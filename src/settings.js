// gettype — settings window logic (vanilla JS, global Tauri API)

const MASK = "gsk_••••••••••";

const MODIFIER_SYMBOLS = { Command: "⌘", Option: "⌥", Control: "⌃", Shift: "⇧" };
const MODIFIER_CODES = new Set([
  "MetaLeft",
  "MetaRight",
  "AltLeft",
  "AltRight",
  "ControlLeft",
  "ControlRight",
  "ShiftLeft",
  "ShiftRight",
]);
const KEY_NAMES = {
  Space: "Space",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Enter: "Enter",
  Tab: "Tab",
  Backspace: "Backspace",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Escape: "Esc",
};

const DEFAULT_CONFIG = {
  hotkey: "Option+Space",
  mode: "push_to_talk",
  auto_paste: true,
  copy_clipboard: true,
  model: "whisper-large-v3-turbo",
  language: "cs",
};

const $ = (id) => document.getElementById(id);

const els = {
  apiKey: $("api-key"),
  keyWrap: $("key-wrap"),
  eyeBtn: $("eye-btn"),
  hintLink: $("hint-link"),
  hotkeyBtn: $("hotkey-btn"),
  segmented: $("mode-segmented"),
  autoPaste: $("toggle-auto-paste"),
  copyClipboard: $("toggle-copy-clipboard"),
  model: $("model"),
  doneBtn: $("done-btn"),
};

let config = null;
let saveTimer = null;
let listening = false;
let hasApiKey = false;
let keyMasked = false;

function invoke(cmd, args) {
  return window.__TAURI__.core.invoke(cmd, args);
}

function scheduleSave() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(async () => {
    try {
      await invoke("save_config", { config });
      // Po uložení přeregistruje globální hotkey, aby změna platila okamžitě.
      await invoke("apply_hotkey");
    } catch (err) {
      console.error("save_config/apply_hotkey failed", err);
    }
  }, 300);
}

/* ---------- hotkey helpers ---------- */

function hotkeyToDisplay(hotkey) {
  let symbols = "";
  const keys = [];
  for (const part of hotkey.split("+")) {
    if (!part) continue;
    if (MODIFIER_SYMBOLS[part]) symbols += MODIFIER_SYMBOLS[part];
    else keys.push(part);
  }
  if (keys.length === 0) return symbols || "—";
  return symbols ? `${symbols} ${keys.join(" ")}` : keys.join(" ");
}

function codeToKeyName(event) {
  const code = event.code;
  if (KEY_NAMES[code]) return KEY_NAMES[code];
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  if (code.startsWith("Numpad")) return "Num" + code.slice(6);
  if (/^F\d+$/.test(code)) return code;
  if (event.key && event.key.length === 1) return event.key.toUpperCase();
  return code;
}

function activeModifiers(event) {
  const mods = [];
  if (event.metaKey) mods.push("Command");
  if (event.altKey) mods.push("Option");
  if (event.ctrlKey) mods.push("Control");
  if (event.shiftKey) mods.push("Shift");
  return mods;
}

function renderHotkey() {
  els.hotkeyBtn.textContent = hotkeyToDisplay(config.hotkey);
}

function renderListening(text) {
  els.hotkeyBtn.classList.add("listening");
  els.hotkeyBtn.innerHTML = `<span class="dot">●</span> ${text}`;
}

function startListening() {
  listening = true;
  renderListening("Listening…");
  els.hotkeyBtn.focus();
}

function stopListening() {
  listening = false;
  els.hotkeyBtn.classList.remove("listening");
  renderHotkey();
}

function onCaptureKeydown(event) {
  if (!listening) return;
  event.preventDefault();
  event.stopPropagation();
  if (event.key === "Escape") {
    stopListening();
    return;
  }
  const mods = activeModifiers(event);
  if (MODIFIER_CODES.has(event.code)) {
    // Live preview while modifiers are held; wait for the main key.
    const symbols = mods.map((m) => MODIFIER_SYMBOLS[m]).join("");
    renderListening(symbols ? `${symbols} …` : "Listening…");
    return;
  }
  // Stored format: Command/Option/Control/Shift + key name, e.g. "Option+Space".
  const stored = [...mods, codeToKeyName(event)].join("+");
  config.hotkey = stored;
  scheduleSave();
  stopListening();
}

/* ---------- API key ---------- */

function renderApiKeyField() {
  els.apiKey.value = hasApiKey ? MASK : "";
  keyMasked = hasApiKey;
  els.keyWrap.classList.remove("shown");
  els.apiKey.type = "password";
}

async function persistApiKey() {
  const value = els.apiKey.value.trim();
  if (keyMasked && value === MASK) return; // untouched mask
  if (value === MASK) {
    renderApiKeyField(); // literal mask typed — ignore
    return;
  }
  if (value === "") {
    // Nothing to save on blur; restore display to the stored state.
    renderApiKeyField();
    return;
  }
  try {
    await invoke("save_api_key", { key: value });
    hasApiKey = true;
    renderApiKeyField();
  } catch (err) {
    console.error("save_api_key failed", err);
  }
}

/* ---------- init ---------- */

(async () => {
  try {
    const loaded = await invoke("load_settings");
    config = loaded.config;
    hasApiKey = loaded.has_api_key;
  } catch (err) {
    console.error("load_settings failed", err);
    config = { ...DEFAULT_CONFIG };
    hasApiKey = false;
  }

  renderApiKeyField();
  renderHotkey();
  renderMode();
  renderSwitch(els.autoPaste, config.auto_paste);
  renderSwitch(els.copyClipboard, config.copy_clipboard);
  els.model.value = config.model;

  /* bindings */

  els.eyeBtn.addEventListener("mousedown", (e) => e.preventDefault());
  els.eyeBtn.addEventListener("click", () => {
    const show = els.apiKey.type === "password";
    els.apiKey.type = show ? "text" : "password";
    els.keyWrap.classList.toggle("shown", show);
  });

  // Typing over the mask must replace it, not append to it.
  els.apiKey.addEventListener("mousedown", (e) => {
    if (keyMasked && document.activeElement !== els.apiKey) {
      e.preventDefault();
      els.apiKey.focus();
      els.apiKey.select();
    }
  });
  els.apiKey.addEventListener("focus", () => {
    if (keyMasked) els.apiKey.select();
  });
  els.apiKey.addEventListener("input", () => {
    keyMasked = false;
  });
  els.apiKey.addEventListener("blur", persistApiKey);

  els.hintLink.addEventListener("click", (e) => {
    const opener = window.__TAURI__ && window.__TAURI__.opener;
    if (opener && opener.openUrl) {
      e.preventDefault();
      opener.openUrl("https://console.groq.com/keys").catch((err) => {
        console.error("openUrl failed", err);
        window.open("https://console.groq.com/keys", "_blank");
      });
    }
  });

  els.hotkeyBtn.addEventListener("click", () => {
    if (!listening) startListening();
  });
  document.addEventListener("keydown", onCaptureKeydown, true);
  window.addEventListener("blur", () => {
    if (listening) stopListening();
  });

  els.segmented.querySelectorAll("button").forEach((btn) => {
    btn.addEventListener("click", () => {
      const mode = btn.dataset.mode;
      if (config.mode === mode) return;
      config.mode = mode;
      renderMode();
      scheduleSave();
    });
  });

  bindSwitch(els.autoPaste, "auto_paste");
  bindSwitch(els.copyClipboard, "copy_clipboard");

  els.model.addEventListener("change", () => {
    config.model = els.model.value;
    scheduleSave();
  });

  els.doneBtn.addEventListener("click", () => {
    try {
      window.__TAURI__.window.getCurrent().hide();
    } catch (err) {
      console.error("hide failed", err);
    }
  });
})();

/* ---------- render helpers ---------- */

function renderMode() {
  els.segmented.querySelectorAll("button").forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.mode === config.mode);
  });
}

function renderSwitch(el, on) {
  el.classList.toggle("on", on);
  el.setAttribute("aria-checked", String(on));
}

function bindSwitch(el, key) {
  el.addEventListener("click", () => {
    config[key] = !config[key];
    renderSwitch(el, config[key]);
    scheduleSave();
  });
}
