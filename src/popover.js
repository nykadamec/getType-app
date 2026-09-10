// gettype — tray popover: čtení existující konfigurace + přepínač režimu.
//
// Opakovaně používá existující commandy (load_settings / save_config /
// apply_hotkey) — ŽÁDNÁ vlastní hotkey/STT/output logika, stejný postup
// jako settings.js (načti celý config, změň pole mode, ulož, přeregistruj).
// Nové commandy open_settings / hide_popover jen ukazují/skrývají okna.

const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);

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
    render();
  } catch (err) {
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
  if (event.key === "Escape") invoke("hide_popover").catch(() => {});
});

// Jemný nástup při každém otevření (okno se jen ukazuje/skrývá,
// webview žije dál — animace patří na focus, ne na load).
// Respektuje reduced-motion; bez WAAPI se jen neanimuje.
function playEntrance() {
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  try {
    card.animate(
      { opacity: [0, 1], transform: ["scale(0.98) translateY(-4px)", "scale(1) translateY(0)"] },
      { duration: 140, easing: "ease-out" },
    );
  } catch (_) {
    // Starší webview bez WAAPI — popover se ukáže bez animace.
  }
}

if (window.__TAURI__ && window.__TAURI__.event) {
  window.__TAURI__.event.listen("tauri://focus", playEntrance).catch(() => {});
  // Konfigurace se mohla změnit v Settings — při návratu focusu přenačíst.
  window.__TAURI__.event.listen("tauri://focus", refresh).catch(() => {});
} else {
  console.error("gettype popover: Tauri API not available");
}

refresh();
