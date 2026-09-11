// gettype — recording pill: dissolve (motion/mini, vendored) + timer.
// Okno samo neschovává — Rust řídí fade: `pill-fade-out` spustí dissolve out
// a hide() zavolá Rust po 260 ms. Dissolve in přijde s `recording-started` /
// `transcribing-started`. ŽÁDNÝ posun (translateY) — jen opacity + blur + scale.
// Stav „transcribing“ zšedne a přepne timer na tečky.

import { animate } from "./vendor/motion-mini.js";

/* ---------- theme (Light / Dark / System, bez restartu) ---------- */

const THEME_CHOICES = new Set(["light", "dark", "system"]);
let themeChoice = "system";

function systemPrefersDark() {
  try {
    return !!(window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches);
  } catch (_) {
    return false;
  }
}

function applyThemeChoice(choice) {
  themeChoice = THEME_CHOICES.has(choice) ? choice : "system";
  const dark =
    themeChoice === "dark" ? true : themeChoice === "light" ? false : systemPrefersDark();
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

function themeFromEvent(payload) {
  if (typeof payload === "string") return payload;
  if (payload && typeof payload.theme === "string") return payload.theme;
  return null;
}

async function initTheme() {
  try {
    const loaded = await window.__TAURI__.core.invoke("load_settings");
    applyThemeChoice(loaded && loaded.config && loaded.config.theme);
  } catch (_) {
    // Defenzivně: chybějící config → light (žádný flash do tmy).
    document.documentElement.dataset.theme = "light";
  }
}

try {
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  const onSystem = () => {
    if (themeChoice === "system") applyThemeChoice("system");
  };
  if (typeof mq.addEventListener === "function") mq.addEventListener("change", onSystem);
  else if (typeof mq.addListener === "function") mq.addListener(onSystem);
} catch (_) {}

initTheme();

const body = document.body;
const timerEl = document.getElementById("timer");
let tick = null;
let startedAt = 0;

// Běžící dissolve animace + generace (restart uprostřed animace zruší
// předchozí a úklid provede jen nejnovější).
let currentAnim = null;
let currentIsIn = false;
let animGen = 0;

// Viditelnost pilulky: true po dokončeném dissolve-in, false po dokončeném
// dissolve-out. Slouží k přeskočení dissolve-in při recording → transcribing,
// kdy je pilulka už viditelná (jinak by opacity [0,1] problikla).
let shown = false;

// Dissolve parametry: in ~220 ms easeOut, out ~180 ms easeIn (hotový dřív,
// než Rust po 260 ms skryje okno — viz FADE_MS v pill.rs).
const DISSOLVE_IN = { duration: 0.22, ease: "easeOut" };
const DISSOLVE_OUT = { duration: 0.18, ease: "easeIn" };

function playDissolve(keyframes, options, isIn) {
  const gen = ++animGen;
  currentIsIn = isIn;
  if (currentAnim) {
    try {
      currentAnim.stop();
    } catch (_) {
      // stop() na doběhlé animaci — ignorujeme
    }
    currentAnim = null;
  }
  body.style.willChange = "opacity, filter, transform";
  const controls = animate(body, keyframes, options);
  currentAnim = controls;
  controls.finished
    .then(() => {
      if (animGen !== gen) return;
      // Úklid: ať nezůstane blur vrstva ani composited layer.
      body.style.filter = "";
      body.style.willChange = "";
      currentAnim = null;
      // IN → viditelná, OUT → skrytá (pod generačním guardem, takže jen
      // nejnovější animace přepíná stav).
      shown = isIn === true;
    })
    .catch(() => {});
}

function fmt(ms) {
  const total = Math.floor(ms / 1000);
  const m = String(Math.floor(total / 60)).padStart(2, "0");
  const s = String(total % 60).padStart(2, "0");
  return `${m}:${s}`;
}

function startTimer() {
  stopTimer();
  startedAt = performance.now();
  timerEl.textContent = "00:00";
  // 200 ms tick stačí — zobrazení je celé sekundy, tabular-nums drží šířku.
  tick = setInterval(() => {
    timerEl.textContent = fmt(performance.now() - startedAt);
  }, 200);
}

function stopTimer() {
  if (tick) clearInterval(tick);
  tick = null;
}

function startTranscribing() {
  stopTimer();
  document.body.classList.add("transcribing");
}

// Dissolve in: opacity 0→1 + blur(8px)→blur(0) + scale .96→1.
// Dvojité rAF, ať animace proběhne až po probuzení webview (za skrytým
// oknem WebKit rendering přeruší).
// Je-li pilulka už viditelná (shown), dissolve se nepřhrává — obsah přepne
// volající (startTranscribing). Běžící animace se nejdřív dokončí finish(),
// ať neskončí v polovičním stavu (finished .then doběhne asynchronně, proto
// se shown synchronně dorovná podle směru dokončené animace).
function fadeIn() {
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      if (currentAnim) {
        const wasIn = currentIsIn;
        try {
          currentAnim.finish();
        } catch (_) {
          // finish() na doběhlé animaci — ignorujeme
        }
        // finish() vizuálně dokončí hned; .then úklid doběhne až v mikrotasku.
        if (wasIn) shown = true;
      }
      if (shown) return;
      playDissolve(
        {
          opacity: [0, 1],
          filter: ["blur(8px)", "blur(0px)"],
          scale: [0.96, 1],
        },
        DISSOLVE_IN,
        true,
      );
    });
  });
}

// Dissolve out: opacity 1→0 + blur(0)→blur(6px) + scale 1→.97.
// Okamžitý start (Rust hide() přijde až po 260 ms).
function fadeOutNow() {
  playDissolve(
    {
      opacity: [1, 0],
      filter: ["blur(0px)", "blur(6px)"],
      scale: [1, 0.97],
    },
    DISSOLVE_OUT,
    false,
  );
}

// Klasické skončení (úspěch i chyba) — uklidí třídy a jistotně do out;
// hide okna dělá Rust (pill-fade-out + zpožděné hide).
function endTranscribing() {
  stopTimer();
  document.body.classList.remove("transcribing");
  fadeOutNow();
}

const Tauri = window.__TAURI__;
if (Tauri && Tauri.event) {
  Tauri.event.listen("recording-started", () => {
    document.body.classList.remove("transcribing");
    startTimer();
    fadeIn();
  });
  Tauri.event.listen("recording-stopped", stopTimer);
  Tauri.event.listen("transcribing-started", () => {
    fadeIn();
    startTranscribing();
  });
  Tauri.event.listen("pill-fade-out", fadeOutNow);
  Tauri.event.listen("transcription-complete", endTranscribing);
  Tauri.event.listen("transcription-error", endTranscribing);
  Tauri.event.listen("recording-error", endTranscribing);
  // Escape-cancel (Rust): timer stop + dissolve out hned, hide okna dělá
  // Rust (pill-fade-out + zpožděné hide). Žádný „cancelled" stav.
  Tauri.event.listen("recording-cancelled", endTranscribing);
  Tauri.event.listen("theme-changed", (event) => {
    const next = themeFromEvent(event && event.payload);
    if (next) applyThemeChoice(next);
  });
  // Cold-start sync (race: první `recording-started` mohl dorazit dřív, než
  // studené webview subscribnulo eventy — body by zůstal na opacity: 0).
  // Po attachi listenerů si vyžádáme aktuální stav; je-li akce aktivní,
  // lokálně doběhneme to, co by udělal zmeškaný event. Generační guardy
  // i dissolve parametry se nemění — voláme stejné fadeIn/startTimer cesty.
  if (Tauri.core && typeof Tauri.core.invoke === "function") {
    Tauri.core
      .invoke("get_recorder_state")
      .then((state) => {
        if (state === "recording") {
          document.body.classList.remove("transcribing");
          startTimer();
          fadeIn();
        } else if (state === "transcribing") {
          fadeIn();
          startTranscribing();
        }
      })
      .catch(() => {});
  }
} else {
  console.error("gettype pill: Tauri API not available");
}
