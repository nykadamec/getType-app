// gettype — Settings Window V2 (sidebar + sekce, plna verze)

const MASKED_TAIL = "gsk_••••8f2a";

const MODIFIER_SYMBOLS = { Command: "⌘", Option: "⌥", Control: "⌃", Shift: "⇧" };
const MODIFIER_CODES = new Set([
  "MetaLeft", "MetaRight", "AltLeft", "AltRight",
  "ControlLeft", "ControlRight", "ShiftLeft", "ShiftRight",
]);
const KEY_NAMES = {
  Space: "Space", Minus: "-", Equal: "=", BracketLeft: "[", BracketRight: "]",
  Backslash: "\\", Semicolon: ";", Quote: "'", Backquote: "`", Comma: ",",
  Period: ".", Slash: "/", Enter: "Enter", Tab: "Tab", Backspace: "Backspace",
  ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right", Escape: "Esc",
};

const DEFAULT_CONFIG = {
  hotkey: "Option+Space",
  mode: "push_to_talk",
  auto_paste: true,
  copy_clipboard: true,
  model: "whisper-large-v3-turbo",
  language: "cs",
};

const SECTIONS = {
  apikey: { title: "API key", sub: "Connect getType to Groq — stored only on this Mac." },
  general: { title: "General", sub: "Permissions, startup and app info." },
  shortcut: { title: "Shortcut", sub: "Global hotkey that starts dictation." },
  output: { title: "Output", sub: "How transcripts reach your apps." },
  model: { title: "Model", sub: "Which Groq speech model and language to use." },
  history: { title: "History", sub: "Your recent dictations — click any entry to copy it back." },
};

const LANG_LABELS = {
  "": "Auto-detect",
  cs: "Čeština",
  sk: "Slovenština",
  en: "English",
  de: "Deutsch",
  pl: "Polski",
  uk: "Українська",
  ru: "Русский",
  es: "Español",
  fr: "Français",
};

const PRIVACY_URLS = {
  mic: "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
  a11y: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
  fallback: "x-apple.systempreferences:com.apple.preference.security",
};

const $ = (id) => document.getElementById(id);

const els = {
  search: $("nav-search"),
  navItems: [...document.querySelectorAll(".nav-item")],
  title: $("section-title"),
  sub: $("section-sub"),
  dot: $("key-dot"),
  status: $("key-status"),
  apiKey: $("api-key"),
  error: $("key-error"),
  saveBtn: $("save-key-btn"),
  disconnectBtn: $("disconnect-btn"),
  hintLink: $("hint-link"),
  hotkeyBtn: $("hotkey-btn"),
  segmented: $("mode-segmented"),
  autoPaste: $("toggle-auto-paste"),
  copyClipboard: $("toggle-copy-clipboard"),
  model: $("model"),
  language: $("language"),
  launch: $("toggle-launch"),
  doneBtn: $("done-btn"),
  micDot: $("mic-dot"),
  micStatus: $("mic-status"),
  a11yDot: $("a11y-dot"),
  a11yStatus: $("a11y-status"),
  openMic: $("open-mic-settings"),
  openA11y: $("open-a11y-settings"),
  previewShortcut: $("preview-shortcut"),
  previewOutput: $("preview-output"),
  previewModel: $("preview-model"),
  historyList: $("history-list"),
  historyEmpty: $("history-empty"),
  historyCount: $("history-count"),
  clearHistory: $("clear-history-btn"),
};

let config = { ...DEFAULT_CONFIG };
let hasApiKey = false;
let listening = false;
let saveTimer = null;

function invoke(cmd, args) {
  return window.__TAURI__.core.invoke(cmd, args);
}

function scheduleSave() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(async () => {
    try {
      await invoke("save_config", { config });
      await invoke("apply_hotkey");
      renderPreviews();
    } catch (err) {
      console.error("save_config/apply_hotkey failed", err);
    }
  }, 300);
}

/* ---------- nav + search ---------- */

function showSection(name) {
  els.navItems.forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.section === name);
  });
  document.querySelectorAll(".pane").forEach((pane) => {
    pane.classList.toggle("active", pane.id === `pane-${name}`);
  });
  const meta = SECTIONS[name];
  if (meta) {
    els.title.textContent = meta.title;
    els.sub.textContent = meta.sub;
  }
  if (name === "shortcut" && listening) stopListening();
  if (name === "history") loadHistory();
}

function filterNav() {
  const q = els.search.value.trim().toLowerCase();
  els.navItems.forEach((btn) => {
    btn.hidden = q !== "" && !btn.dataset.label.includes(q);
  });
}

/* ---------- hotkey ---------- */

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
    const symbols = mods.map((m) => MODIFIER_SYMBOLS[m]).join("");
    renderListening(symbols ? `${symbols} …` : "Listening…");
    return;
  }
  config.hotkey = [...mods, codeToKeyName(event)].join("+");
  scheduleSave();
  stopListening();
}

/* ---------- API key ---------- */

function renderKeyStatus() {
  if (hasApiKey) {
    els.dot.classList.add("on");
    els.status.textContent = `Connected · ${MASKED_TAIL}`;
  } else {
    els.dot.classList.remove("on");
    els.status.textContent = "Not connected";
  }
}

function showError(msg) {
  els.error.textContent = msg;
  els.error.hidden = !msg;
}

async function onSaveKey() {
  const value = els.apiKey.value.trim();
  if (!value) {
    showError("Enter an API key first");
    return;
  }
  els.saveBtn.disabled = true;
  els.saveBtn.textContent = "Verifying…";
  showError("");
  try {
    await invoke("verify_api_key", { key: value });
    await invoke("save_api_key", { key: value });
    hasApiKey = true;
    els.apiKey.value = "";
    els.apiKey.placeholder = MASKED_TAIL;
    renderKeyStatus();
  } catch (err) {
    showError(typeof err === "string" ? err : String(err));
  } finally {
    els.saveBtn.disabled = false;
    els.saveBtn.textContent = "Save & verify";
  }
}

async function onDisconnect() {
  try {
    await invoke("save_api_key", { key: "" });
  } catch (err) {
    console.error("save_api_key failed", err);
    return;
  }
  hasApiKey = false;
  els.apiKey.value = "";
  els.apiKey.placeholder = "gsk_…";
  showError("");
  renderKeyStatus();
}

/* ---------- previews ---------- */

function modeLabel() {
  return config.mode === "toggle" ? "Toggle" : "Push to talk";
}

function onOff(v) {
  return v ? "ON" : "OFF";
}

function renderPreviews() {
  els.previewShortcut.textContent = `${hotkeyToDisplay(config.hotkey)} · ${modeLabel()}`;
  els.previewOutput.textContent = `Auto-paste ${onOff(config.auto_paste)} · Clipboard ${onOff(config.copy_clipboard)}`;
  els.previewModel.textContent = `${config.model} · ${LANG_LABELS[config.language] ?? LANG_LABELS["cs"]}`;
}

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

/* ---------- permissions (read-only status) ---------- */

function renderPill(dotEl, textEl, allowed, label) {
  dotEl.classList.toggle("on", allowed);
  textEl.textContent = label;
}

async function refreshPermissions() {
  try {
    const mic = await invoke("get_mic_permission");
    if (mic === "granted") renderPill(els.micDot, els.micStatus, true, "Allowed");
    else if (mic === "not-determined") renderPill(els.micDot, els.micStatus, false, "Not asked yet");
    else renderPill(els.micDot, els.micStatus, false, "Not allowed");
  } catch (err) {
    console.error("get_mic_permission failed", err);
    renderPill(els.micDot, els.micStatus, false, "Not allowed");
  }
  try {
    const trusted = await invoke("get_accessibility_permission");
    renderPill(els.a11yDot, els.a11yStatus, !!trusted, trusted ? "Allowed" : "Not allowed");
  } catch (err) {
    console.error("get_accessibility_permission failed", err);
    renderPill(els.a11yDot, els.a11yStatus, false, "Not allowed");
  }
}

function openPrivacy(url) {
  const opener = window.__TAURI__ && window.__TAURI__.opener;
  if (opener && opener.openUrl) {
    opener.openUrl(url).catch(() => opener.openUrl(PRIVACY_URLS.fallback).catch((err) => {
      console.error("openUrl failed", err);
    }));
  }
}

/* ---------- history ---------- */

let historyItems = [];
let copyFeedbackTimer = null;

function formatHistoryTime(createdAt) {
  const d = new Date(createdAt * 1000);
  if (Number.isNaN(d.getTime())) return "";
  const now = new Date();
  const sameDay = (a, b) =>
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate();
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  if (sameDay(d, now)) return `today ${time}`;
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (sameDay(d, yesterday)) return `yesterday ${time}`;
  return d.toLocaleDateString([], { day: "numeric", month: "numeric", year: "numeric" });
}

function historyMetaLabel(entry) {
  const parts = [formatHistoryTime(entry.created_at)];
  if (typeof entry.chars === "number" && entry.chars > 0) parts.push(`${entry.chars} chars`);
  return parts.filter(Boolean).join(" · ");
}

function showCopyFeedback(itemEl) {
  document.querySelectorAll(".hist-item.copied").forEach((el) => {
    if (el !== itemEl) el.classList.remove("copied");
  });
  document.querySelectorAll(".hist-copied").forEach((el) => {
    if (el.closest(".hist-item") !== itemEl) el.hidden = true;
  });
  const flag = itemEl.querySelector(".hist-copied");
  if (flag) flag.hidden = false;
  itemEl.classList.add("copied");
  clearTimeout(copyFeedbackTimer);
  copyFeedbackTimer = setTimeout(() => {
    itemEl.classList.remove("copied");
    if (flag) flag.hidden = true;
  }, 1000);
}

async function copyHistoryEntry(id, itemEl) {
  try {
    await invoke("copy_history_entry", { id });
  } catch (err) {
    console.error("copy_history_entry failed", err);
    return;
  }
  if (itemEl) showCopyFeedback(itemEl);
}

function renderHistory(list) {
  historyItems = Array.isArray(list) ? list : [];
  els.historyList.textContent = "";
  const empty = historyItems.length === 0;
  els.historyEmpty.hidden = !empty;
  els.historyList.style.display = empty ? "none" : "";
  els.clearHistory.disabled = empty;
  els.historyCount.textContent = empty
    ? ""
    : `${historyItems.length} dictation${historyItems.length === 1 ? "" : "s"}`;

  for (const entry of historyItems) {
    const item = document.createElement("div");
    item.className = "hist-item";
    item.dataset.id = String(entry.id);

    const main = document.createElement("button");
    main.className = "hist-main";
    main.type = "button";
    main.title = "Click to copy";

    const text = document.createElement("span");
    text.className = "hist-text";
    text.textContent = entry.text || "";
    text.title = entry.text || "";

    const meta = document.createElement("span");
    meta.className = "hist-meta";
    meta.textContent = historyMetaLabel(entry);

    const copied = document.createElement("span");
    copied.className = "hist-copied";
    copied.textContent = "Copied";
    copied.hidden = true;

    main.append(text, meta, copied);
    main.addEventListener("click", () => copyHistoryEntry(entry.id, item));

    const actions = document.createElement("div");
    actions.className = "hist-actions";

    const copyBtn = document.createElement("button");
    copyBtn.className = "icon-btn hist-copy";
    copyBtn.type = "button";
    copyBtn.title = "Copy";
    copyBtn.setAttribute("aria-label", "Copy entry");
    copyBtn.innerHTML =
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect width="14" height="14" x="8" y="8" rx="2" ry="2" /><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" /></svg>';
    copyBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      copyHistoryEntry(entry.id, item);
    });

    const delBtn = document.createElement("button");
    delBtn.className = "icon-btn hist-del";
    delBtn.type = "button";
    delBtn.title = "Delete";
    delBtn.setAttribute("aria-label", "Delete entry");
    delBtn.innerHTML =
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18" /><path d="m6 6 12 12" /></svg>';
    delBtn.addEventListener("click", async (e) => {
      e.stopPropagation();
      try {
        const next = await invoke("delete_history_entry", { id: entry.id });
        renderHistory(next);
      } catch (err) {
        console.error("delete_history_entry failed", err);
      }
    });

    actions.append(copyBtn, delBtn);
    item.append(main, actions);
    els.historyList.append(item);
  }
}

async function loadHistory() {
  try {
    const list = await invoke("list_history");
    renderHistory(list);
  } catch (err) {
    console.error("list_history failed", err);
  }
}

/* ---------- init ---------- */

(async () => {
  try {
    const loaded = await invoke("load_settings");
    config = { ...DEFAULT_CONFIG, ...loaded.config };
    hasApiKey = loaded.has_api_key;
  } catch (err) {
    console.error("load_settings failed", err);
    config = { ...DEFAULT_CONFIG };
    hasApiKey = false;
  }

  let launchAtLogin = false;
  try {
    launchAtLogin = await invoke("get_launch_at_login");
  } catch (err) {
    console.error("get_launch_at_login failed", err);
  }

  // prvotni render
  renderKeyStatus();
  els.apiKey.value = "";
  els.apiKey.placeholder = hasApiKey ? MASKED_TAIL : "gsk_…";
  renderHotkey();
  renderMode();
  renderSwitch(els.autoPaste, config.auto_paste);
  renderSwitch(els.copyClipboard, config.copy_clipboard);
  renderSwitch(els.launch, launchAtLogin);
  els.model.value = config.model;
  els.language.value = config.language || "";
  renderPreviews();
  showSection("apikey");

  /* bindings */

  els.navItems.forEach((btn) => {
    btn.addEventListener("click", () => showSection(btn.dataset.section));
  });
  els.search.addEventListener("input", filterNav);

  document.querySelectorAll(".preview").forEach((card) => {
    card.addEventListener("click", () => showSection(card.dataset.goto));
  });

  els.saveBtn.addEventListener("click", onSaveKey);
  els.apiKey.addEventListener("keydown", (e) => {
    if (e.key === "Enter") onSaveKey();
  });
  els.disconnectBtn.addEventListener("click", onDisconnect);

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

  els.launch.addEventListener("click", async () => {
    const next = els.launch.getAttribute("aria-checked") !== "true";
    renderSwitch(els.launch, next);
    try {
      await invoke("set_launch_at_login", { enabled: next });
    } catch (err) {
      console.error("set_launch_at_login failed", err);
      renderSwitch(els.launch, !next);
    }
  });

  els.openMic.addEventListener("click", () => openPrivacy(PRIVACY_URLS.mic));
  els.openA11y.addEventListener("click", () => openPrivacy(PRIVACY_URLS.a11y));

  els.clearHistory.addEventListener("click", async () => {
    if (historyItems.length === 0) return;
    try {
      await invoke("clear_history");
      renderHistory([]);
    } catch (err) {
      console.error("clear_history failed", err);
    }
  });

  window.addEventListener("focus", () => {
    refreshPermissions();
    loadHistory();
  });
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) {
      refreshPermissions();
      loadHistory();
    }
  });

  refreshPermissions();
  loadHistory();

  const Tauri = window.__TAURI__;
  if (Tauri && Tauri.event && typeof Tauri.event.listen === "function") {
    Tauri.event.listen("transcription-complete", () => loadHistory());
  }

  els.model.addEventListener("change", () => {
    config.model = els.model.value;
    scheduleSave();
  });

  els.language.addEventListener("change", () => {
    config.language = els.language.value;
    scheduleSave();
  });

  els.doneBtn.addEventListener("click", () => {
    try {
      // Tauri v2: getCurrentWindow (v1 getCurrent neexistuje → TypeError).
      window.__TAURI__.window.getCurrentWindow().hide();
    } catch (err) {
      console.error("hide failed", err);
    }
  });
})();
