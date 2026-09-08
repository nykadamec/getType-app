// gettype — recording pill: dissolve (motion/mini, vendored) + timer.
// Okno samo neschovává — Rust řídí fade: `pill-fade-out` spustí dissolve out
// a hide() zavolá Rust po 260 ms. Dissolve in přijde s `recording-started` /
// `transcribing-started`. ŽÁDNÝ posun (translateY) — jen opacity + blur + scale.
// Stav „transcribing“ zšedne a přepne timer na tečky.

import { animate } from "./vendor/motion-mini.js";

const body = document.body;
const timerEl = document.getElementById("timer");
let tick = null;
let startedAt = 0;

// Běžící dissolve animace + generace (restart uprostřed animace zruší
// předchozí a úklid provede jen nejnovější).
let currentAnim = null;
let animGen = 0;

// Dissolve parametry: in ~220 ms easeOut, out ~180 ms easeIn (hotový dřív,
// než Rust po 260 ms skryje okno — viz FADE_MS v pill.rs).
const DISSOLVE_IN = { duration: 0.22, ease: "easeOut" };
const DISSOLVE_OUT = { duration: 0.18, ease: "easeIn" };

function playDissolve(keyframes, options) {
  const gen = ++animGen;
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
function fadeIn() {
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      playDissolve(
        {
          opacity: [0, 1],
          filter: ["blur(8px)", "blur(0px)"],
          scale: [0.96, 1],
        },
        DISSOLVE_IN,
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
} else {
  console.error("gettype pill: Tauri API not available");
}
