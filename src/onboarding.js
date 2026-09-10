// getType — Onboarding (Welcome → API key → Permissions)
// Stejný tvar volání backendu jako settings.js: window.__TAURI__.core.invoke(cmd, args).
// Všechny nové commandy defenzivně (try/catch) — backend se paralelně dodělává.

const $ = (id) => document.getElementById(id);

const els = {
  back: $("back-btn"),
  backPlaceholder: $("back-placeholder"),
  stepLabel: $("step-label"),
  continueBtn: $("continue-btn"),
  footnote: $("footnote"),
  steps: [$("step-1"), $("step-2"), $("step-3")],
  // API key
  keyDot: $("key-dot"),
  keyStatus: $("key-status"),
  keyWrap: $("key-wrap"),
  apiKey: $("api-key"),
  keyToggle: $("key-toggle"),
  keyError: $("key-error"),
  saveBtn: $("save-key-btn"),
  groqLink: $("groq-link"),
  // Permissions
  micStatus: $("mic-status"),
  a11yStatus: $("a11y-status"),
  micBtn: $("mic-btn"),
  a11yBtn: $("a11y-btn"),
  permError: $("perm-error"),
};

const MASKED_TAIL = "gsk_••••8f2a";

let current = 1;
let keyVerified = false;
let finishing = false;

const FOOTNOTES = {
  1: "Requires a free Groq API key — next step",
  2: "You can change it later in Settings.",
  3: "Step 3 of 3 — Microphone & Accessibility",
};

const CONTINUE_LABELS = { 1: "Continue", 2: "Continue", 3: "Finish" };

/* ---------- defenzivní invoke ---------- */

function tauriCore() {
  try {
    return window.__TAURI__ && window.__TAURI__.core ? window.__TAURI__.core : null;
  } catch (err) {
    return null;
  }
}

async function invoke(cmd, args) {
  const core = tauriCore();
  if (!core || typeof core.invoke !== "function") {
    throw new Error("Backend není dostupný (Tauri IPC chybí).");
  }
  return core.invoke(cmd, args);
}

function friendlyError(err) {
  const msg = typeof err === "string" ? err : (err && err.message ? err.message : String(err));
  if (/no such command|not found|unknown command/i.test(msg)) {
    return "Tato funkce zatím není v backendu — zkus to prosím později.";
  }
  return msg;
}

/* ---------- kroky ---------- */

function showStep(n) {
  current = Math.min(3, Math.max(1, n));
  els.steps.forEach((s, i) => s.classList.toggle("active", i === current - 1));
  const first = current === 1;
  els.back.hidden = first;
  els.backPlaceholder.style.display = "none";
  els.stepLabel.textContent = `Step ${current} of 3`;
  els.continueBtn.textContent = CONTINUE_LABELS[current];
  els.footnote.textContent = FOOTNOTES[current];
  if (current === 3) refreshPermissions();
}

function goNext() {
  if (current < 3) showStep(current + 1);
  else onFinish();
}

function goBack() {
  if (current > 1) showStep(current - 1);
}

/* ---------- API key (stejný tvar jako settings.js) ---------- */

function renderKeyStatus() {
  els.keyDot.classList.toggle("on", keyVerified);
  els.keyStatus.textContent = keyVerified ? `Connected · ${MASKED_TAIL}` : "Not connected";
}

function showKeyError(msg) {
  els.keyError.textContent = msg || "";
  els.keyError.hidden = !msg;
  els.keyWrap.classList.toggle("invalid", !!msg);
}

async function onSaveKey() {
  const value = els.apiKey.value.trim();
  if (!value) {
    showKeyError("Enter an API key first");
    return;
  }
  els.saveBtn.disabled = true;
  els.saveBtn.textContent = "Verifying…";
  showKeyError("");
  try {
    await invoke("verify_api_key", { key: value });
    await invoke("save_api_key", { key: value });
    keyVerified = true;
    els.apiKey.value = "";
    els.apiKey.placeholder = MASKED_TAIL;
    renderKeyStatus();
  } catch (err) {
    keyVerified = false;
    renderKeyStatus();
    showKeyError(friendlyError(err));
  } finally {
    els.saveBtn.disabled = false;
    els.saveBtn.textContent = "Save & verify";
  }
}

/* ---------- Permissions ---------- */

function showPermError(msg) {
  els.permError.textContent = msg || "";
  els.permError.hidden = !msg;
}

function setPermLine(el, state) {
  // state: "granted" | "pending" | "denied" | "unknown"
  el.classList.toggle("ok", state === "granted");
  if (state === "granted") el.textContent = "Allowed";
  else if (state === "pending") el.textContent = "Not asked yet";
  else if (state === "denied") el.textContent = "Not allowed — allow in System Settings";
  else el.textContent = "Checking…";
}

async function refreshPermissions() {
  showPermError("");
  try {
    const mic = await invoke("get_mic_permission");
    if (mic === "granted") setPermLine(els.micStatus, "granted");
    else if (mic === "not-determined") setPermLine(els.micStatus, "pending");
    else setPermLine(els.micStatus, "denied");
  } catch (err) {
    setPermLine(els.micStatus, "unknown");
    showPermError(friendlyError(err));
  }
  try {
    const trusted = await invoke("get_accessibility_permission");
    setPermLine(els.a11yStatus, trusted ? "granted" : "denied");
  } catch (err) {
    setPermLine(els.a11yStatus, "unknown");
    showPermError(friendlyError(err));
  }
}

async function onMicEnable() {
  showPermError("");
  els.micBtn.disabled = true;
  const original = els.micBtn.textContent;
  els.micBtn.textContent = "Waiting…";
  // Backend má timeout 60 s — frontend jistí o kus delším (65 s),
  // aby se tlačítko odemklo i kdyby IPC nikdy nedorazilo.
  const MIC_TIMEOUT_MS = 65000;
  try {
    await Promise.race([
      invoke("request_mic_permission"),
      new Promise((_, reject) =>
        setTimeout(() => reject(new Error("Microphone request timed out — try again.")), MIC_TIMEOUT_MS)
      ),
    ]);
    await refreshPermissions();
  } catch (err) {
    showPermError(friendlyError(err));
  } finally {
    els.micBtn.disabled = false;
    els.micBtn.textContent = original;
    await refreshPermissions();
  }
}

async function onA11yOpen() {
  showPermError("");
  try {
    await invoke("open_accessibility_settings");
  } catch (err) {
    showPermError(friendlyError(err));
  }
}

/* ---------- Finish — volá jen finish_onboarding(), nic víc ---------- */

async function onFinish() {
  if (finishing) return;
  finishing = true;
  els.continueBtn.disabled = true;
  showPermError("");
  try {
    await invoke("finish_onboarding");
  } catch (err) {
    showPermError(friendlyError(err));
    finishing = false;
    els.continueBtn.disabled = false;
  }
}

/* ---------- init ---------- */

(function init() {
  renderKeyStatus();
  showStep(1);

  els.continueBtn.addEventListener("click", goNext);
  els.back.addEventListener("click", goBack);

  els.saveBtn.addEventListener("click", onSaveKey);
  els.apiKey.addEventListener("keydown", (e) => {
    if (e.key === "Enter") onSaveKey();
  });
  els.keyToggle.addEventListener("click", () => {
    const show = els.apiKey.type === "password";
    els.apiKey.type = show ? "text" : "password";
    els.keyToggle.setAttribute("aria-label", show ? "Hide key" : "Show key");
  });

  els.groqLink.addEventListener("click", (e) => {
    const opener = window.__TAURI__ && window.__TAURI__.opener;
    if (opener && opener.openUrl) {
      e.preventDefault();
      opener.openUrl("https://console.groq.com/keys").catch(() => {
        window.open("https://console.groq.com/keys", "_blank");
      });
    }
  });

  els.micBtn.addEventListener("click", onMicEnable);
  els.a11yBtn.addEventListener("click", onA11yOpen);

  window.addEventListener("focus", () => {
    if (current === 3) refreshPermissions();
  });
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden && current === 3) refreshPermissions();
  });
})();
