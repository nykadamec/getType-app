// gettype — Settings Window V2 (sidebar + sekce, plna verze)

// Nouzový fallback, když backend masku (zatím) neumí vrátit.
const FALLBACK_MASK = "gsk_…";

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
  theme: "system",
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
  keyToggle: $("key-toggle"),
  error: $("key-error"),
  saveBtn: $("save-key-btn"),
  testBtn: $("test-key-btn"),
  testResult: $("test-result"),
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
  themeSegmented: $("theme-segmented"),
};

let config = { ...DEFAULT_CONFIG };
let hasApiKey = false;
// Provenience hodnoty v inputu: true jen když hodnotu vložil reveal
// z Keychainu a uživatel ji od té doby needitoval (input event → false,
// pak jde o uživatelský text, který musí zůstat kvůli Save).
let revealedActive = false;
// Skutečná maska z backendu (masked_api_key), např. "gsk_••••XxXx".
let maskedKeyLabel = FALLBACK_MASK;
let listening = false;
let saveTimer = null;
let testResultTimer = null;

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
    els.status.textContent = `Connected · ${maskedKeyLabel}`;
  } else {
    els.dot.classList.remove("on");
    els.status.textContent = "Not connected";
  }
  if (els.testBtn) els.testBtn.disabled = !hasApiKey;
}

// Načte skutečný konec klíče z backendu. Při jakékoli chybě
// (vč. "no such command" ve starším buildu) tiše drží fallback.
async function refreshMaskedKey() {
  if (!hasApiKey) {
    maskedKeyLabel = FALLBACK_MASK;
    return maskedKeyLabel;
  }
  try {
    const m = await invoke("masked_api_key");
    maskedKeyLabel = typeof m === "string" && m ? m : FALLBACK_MASK;
  } catch (_) {
    maskedKeyLabel = FALLBACK_MASK;
  }
  return maskedKeyLabel;
}

function applyMaskToUi() {
  if (hasApiKey && !els.apiKey.value) els.apiKey.placeholder = maskedKeyLabel;
  renderKeyStatus();
}

function showTestResult(msg, ok) {
  if (!els.testResult) return;
  clearTimeout(testResultTimer);
  testResultTimer = null;
  els.testResult.classList.remove("is-fading");
  if (!msg) {
    els.testResult.hidden = true;
    els.testResult.textContent = "";
    els.testResult.classList.remove("ok", "fail");
    return;
  }
  els.testResult.hidden = false;
  els.testResult.textContent = msg;
  els.testResult.classList.toggle("ok", !!ok);
  els.testResult.classList.toggle("fail", !ok);
  // Úspěch po ~4,5 s sám zmizí (fade); neúspěch zůstává, dokud uživatel nezasáhne.
  if (ok) {
    testResultTimer = setTimeout(() => {
      els.testResult.classList.add("is-fading");
      setTimeout(() => {
        if (els.testResult.classList.contains("is-fading")) showTestResult("");
      }, 400);
    }, 4500);
  }
}

function showError(msg) {
  els.error.textContent = msg;
  els.error.hidden = !msg;
}

async function onTestSavedKey() {
  if (!els.testBtn || els.testBtn.disabled) return;
  const original = "Test connection";
  els.testBtn.disabled = true;
  els.testBtn.classList.add("testing");
  els.testBtn.textContent = "Testing…";
  showTestResult("");
  try {
    const res = await invoke("test_saved_key");
    if (res && typeof res === "object") {
      if (res.ok) showTestResult("Connection successful", true);
      else showTestResult(res.message || "Connection failed — check the saved key.", false);
    } else if (typeof res === "string") {
      showTestResult("Connection successful", true);
    } else {
      showTestResult("Connection successful", true);
    }
  } catch (err) {
    const raw = typeof err === "string" ? err : String(err ?? "");
    if (raw.includes("no such command") || raw.includes("test_saved_key")) {
      showTestResult("Test isn't available in this build yet — save & verify instead.", false);
    } else {
      showTestResult(raw || "Test failed.", false);
    }
  } finally {
    els.testBtn.disabled = !hasApiKey;
    els.testBtn.classList.remove("testing");
    els.testBtn.textContent = original;
  }
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
  showTestResult("");
  try {
    await invoke("verify_api_key", { key: value });
    await invoke("save_api_key", { key: value });
    hasApiKey = true;
    els.apiKey.value = "";
    revealedActive = false;
    await refreshMaskedKey();
    els.apiKey.placeholder = maskedKeyLabel;
    els.apiKey.type = "password";
    if (els.keyToggle) els.keyToggle.setAttribute("aria-label", "Show key");
    showTestResult("");
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
  maskedKeyLabel = FALLBACK_MASK;
  els.apiKey.value = "";
  revealedActive = false;
  els.apiKey.placeholder = "gsk_…";
  els.apiKey.type = "password";
  if (els.keyToggle) els.keyToggle.setAttribute("aria-label", "Show key");
  showError("");
  showTestResult("");
  renderKeyStatus();
}

async function onToggleKey() {
  try {
    // Hodnota z revealu (needitoraná uživatelem) → návrat do původního
    // stavu: prázdný input + maska v placeholderu, bez teček v plné délce.
    if (els.apiKey.value.trim() !== "" && revealedActive) {
      els.apiKey.value = "";
      revealedActive = false;
      els.apiKey.type = "password";
      els.apiKey.placeholder = maskedKeyLabel;
      if (els.keyToggle) els.keyToggle.setAttribute("aria-label", "Show key");
      return;
    }
    // Rozepsaný uživatelský text → jen lokální show/hide, text zůstává kvůli Save.
    if (els.apiKey.value.trim() !== "") {
      const show = els.apiKey.type === "password";
      els.apiKey.type = show ? "text" : "password";
      if (els.keyToggle) els.keyToggle.setAttribute("aria-label", show ? "Hide key" : "Show key");
      return;
    }
    // Prázdný input a už je odhaleno (např. po revealu a smazání) → jen skryj zpět.
    if (els.apiKey.type === "text") {
      els.apiKey.type = "password";
      revealedActive = false;
      if (els.keyToggle) els.keyToggle.setAttribute("aria-label", "Show key");
      return;
    }
    // Prázdný input → ukaž MOMENTÁLNĚ POUŽÍVANÝ klíč z Keychainu.
    let saved = null;
    try {
      saved = await invoke("reveal_api_key");
    } catch (err) {
      const raw = typeof err === "string" ? err : String(err ?? "");
      // Starší build bez nového commandu → nic nerozbít, jen tiše skončit.
      if (raw.includes("no such command") || raw.includes("reveal_api_key")) return;
      console.error("reveal_api_key failed", err);
      showTestResult("Couldn't load the saved key.", false);
      return;
    }
    if (typeof saved === "string" && saved) {
      els.apiKey.value = saved;
      revealedActive = true;
      els.apiKey.type = "text";
      if (els.keyToggle) els.keyToggle.setAttribute("aria-label", "Hide key");
      showTestResult("");
    } else {
      showTestResult(
        hasApiKey ? "Couldn't load the saved key." : "No saved key on this Mac.",
        false,
      );
    }
  } catch (_) {
    // Defenzivně: oko nikdy nesmí rozbít okno.
  }
}

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

/* ---------- theme (Light / Dark / System) ---------- */

const THEME_CHOICES = new Set(["light", "dark", "system"]);
let themeChoice = "system";

function systemPrefersDark() {
  try {
    return !!(window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches);
  } catch (_) {
    return false;
  }
}

function normalizeTheme(v) {
  return THEME_CHOICES.has(v) ? v : "system";
}

function applyThemeChoice(choice) {
  themeChoice = normalizeTheme(choice);
  const dark = themeChoice === "dark" ? true : themeChoice === "light" ? false : systemPrefersDark();
  document.documentElement.dataset.theme = dark ? "dark" : "light";
  renderTheme();
}

function renderTheme() {
  if (!els.themeSegmented) return;
  els.themeSegmented.querySelectorAll("button").forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.themeValue === themeChoice);
  });
}

function themeFromEvent(payload) {
  if (typeof payload === "string") return payload;
  if (payload && typeof payload.theme === "string") return payload.theme;
  return null;
}

async function setTheme(choice) {
  const next = normalizeTheme(choice);
  if (config.theme === next && themeChoice === next) return;
  config.theme = next;
  applyThemeChoice(next);
  try {
    await invoke("save_config", { config });
  } catch (err) {
    console.error("save_config (theme) failed", err);
  }
  try {
    const Tauri = window.__TAURI__;
    if (Tauri && Tauri.event && typeof Tauri.event.emit === "function") {
      await Tauri.event.emit("theme-changed", next);
    }
  } catch (err) {
    console.error("theme-changed emit failed", err);
  }
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

  // prvotni render — skutečný konec klíče z backendu, fallback "gsk_…"
  await refreshMaskedKey();
  applyThemeChoice(config.theme);
  renderKeyStatus();
  els.apiKey.value = "";
  els.apiKey.placeholder = hasApiKey ? maskedKeyLabel : "gsk_…";
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
  if (els.testBtn) els.testBtn.addEventListener("click", onTestSavedKey);
  if (els.keyToggle) {
    els.keyToggle.addEventListener("click", onToggleKey);
  }
  els.apiKey.addEventListener("input", () => {
    // Jakýkoli ruční zásah do hodnoty po revealu → jde o uživatelský text.
    revealedActive = false;
    showTestResult("");
  });
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

  if (els.themeSegmented) {
    els.themeSegmented.querySelectorAll("button").forEach((btn) => {
      btn.addEventListener("click", () => setTheme(btn.dataset.themeValue));
    });
  }

  // System vzhled se může změnit za běhu — reagovat jen při volbě System.
  try {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onSystem = () => {
      if (themeChoice === "system") applyThemeChoice("system");
    };
    if (typeof mq.addEventListener === "function") mq.addEventListener("change", onSystem);
    else if (typeof mq.addListener === "function") mq.addListener(onSystem);
  } catch (_) {}

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

  window.addEventListener("focus", async () => {
    refreshPermissions();
    loadHistory();
    try {
      const loaded = await invoke("load_settings");
      hasApiKey = loaded.has_api_key;
    } catch (_) {}
    await refreshMaskedKey();
    applyMaskToUi();
  });
  document.addEventListener("visibilitychange", async () => {
    if (!document.hidden) {
      refreshPermissions();
      loadHistory();
      await refreshMaskedKey();
      applyMaskToUi();
    }
  });

  refreshPermissions();
  loadHistory();

  const Tauri = window.__TAURI__;
  if (Tauri && Tauri.event && typeof Tauri.event.listen === "function") {
    Tauri.event.listen("transcription-complete", () => loadHistory());
    Tauri.event.listen("theme-changed", (event) => {
      const next = themeFromEvent(event && event.payload);
      if (next) applyThemeChoice(next);
    });
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
      // close() místo hide(): projde přes Rust CloseRequested handler
      // (prevent_close + hide + návrat na Accessory) — přímé hide() by
      // obešlo reset activation policy a v Docku by zůstala ikona.
      window.__TAURI__.window.getCurrentWindow().close();
    } catch (err) {
      console.error("close failed", err);
    }
  });
})();
