// gettype — tray popover: čtení existující konfigurace + přepínač režimu.
//
// Opakovaně používá existující commandy (load_settings / save_config /
// apply_hotkey) — ŽÁDNÁ vlastní hotkey/STT/output logika, stejný postup
// jako settings.js (načti celý config, změň pole mode, ulož, přeregistruj).
// Nové commandy open_settings / hide_popover jen ukazují/skrývají okna.

const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);

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

try {
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  const onSystem = () => {
    if (themeChoice === "system") applyThemeChoice("system");
  };
  if (typeof mq.addEventListener === "function") mq.addEventListener("change", onSystem);
  else if (typeof mq.addListener === "function") mq.addListener(onSystem);
} catch (_) {}

// Most pro frontend logy do Rust stderr logu (diagnostika dvojitého bliknutí).
// Fire-and-forget: nikdy neblokuje animaci, při selhání tichý fallback na console.
function tlog(level, msg) {
  try {
    invoke("log_frontend", { level, msg }).catch(() => {
      console.log(`[popover] ${msg}`);
    });
  } catch (_) {
    try {
      console.log(`[popover] ${msg}`);
    } catch (_) {
      // console nedostupné — ignorujeme
    }
  }
}

const card = document.getElementById("popover");
const gear = document.getElementById("gear");
const hint = document.getElementById("hint");
const modelEl = document.getElementById("model");
const kbdEl = document.getElementById("kbd");
const modeBtns = [...document.querySelectorAll("#mode button")];

let config = null;

const MOD_GLYPH = { Option: "⌥", Command: "⌘", Control: "⌃", Shift: "⇧" };

function hotkeyParts(hotkey) {
  return String(hotkey || "Option+Space")
    .split("+")
    .map((p) => MOD_GLYPH[p] || (p.length === 1 ? p.toUpperCase() : p));
}

function render() {
  if (!config) return;
  for (const btn of modeBtns) {
    btn.classList.toggle("active", btn.dataset.mode === config.mode);
  }
  const parts = hotkeyParts(config.hotkey);
  kbdEl.innerHTML = "";
  for (const part of parts) {
    const key = document.createElement("span");
    key.className = "key";
    key.textContent = part;
    kbdEl.appendChild(key);
  }
  hint.textContent = `Hold ${parts.join(" ")} and speak. Release to insert text where your cursor is.`;
  if (config.model) modelEl.textContent = `Groq · ${config.model}`;
}

async function refresh() {
  try {
    const loaded = await invoke("load_settings");
    config = loaded.config;
    applyThemeChoice(loaded.config && loaded.config.theme);
    render();
  } catch (err) {
    document.documentElement.dataset.theme = "light";
    console.error("gettype popover: load_settings failed", err);
  }
}

for (const btn of modeBtns) {
  btn.addEventListener("click", async () => {
    if (!config || config.mode === btn.dataset.mode) return;
    config.mode = btn.dataset.mode;
    render();
    try {
      await invoke("save_config", { config });
      await invoke("apply_hotkey");
    } catch (err) {
      console.error("gettype popover: save_config/apply_hotkey failed", err);
    }
  });
}

gear.addEventListener("click", () => {
  invoke("open_settings").catch((err) => console.error("gettype popover: open_settings failed", err));
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    tlog("info", "hide event source=escape");
    invoke("hide_popover").catch(() => {});
  }
});

if (window.__TAURI__ && window.__TAURI__.event) {
  window.__TAURI__.event
    .listen("tauri://focus", (event) => {
      tlog("info", `focus event received payload=${JSON.stringify(event.payload ?? null)}`);
    })
    .catch(() => {});
  window.__TAURI__.event
    .listen("tauri://blur", (event) => {
      tlog("info", `blur event received payload=${JSON.stringify(event.payload ?? null)}`);
    })
    .catch(() => {});
  // Konfigurace se mohla změnit v Settings — při návratu focusu přenačíst.
  window.__TAURI__.event.listen("tauri://focus", refresh).catch(() => {});
  window.__TAURI__.event
    .listen("theme-changed", (event) => {
      const payload = event && event.payload;
      const next = themeFromEvent(payload);
      if (next) applyThemeChoice(next);
    })
    .catch(() => {});
} else {
  console.error("gettype popover: Tauri API not available");
}

refresh();
