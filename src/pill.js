// gettype — recording pill: spouští/stopuje timer podle eventů z Rustu.
// Okno samo neschovává — to dělá Rust (show bez focusu, hide při stopu).
// Stav „transcribing“ jen zšedne a přepne timer na tečky; Rust okno skrývá
// v každém koncovém stavu (complete/error).

const timerEl = document.getElementById("timer");
let tick = null;
let startedAt = 0;

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

// Klasické skončení (úspěch i chyba) — class jen uklidí, Rust skrývá okno.
function endTranscribing() {
  stopTimer();
  document.body.classList.remove("transcribing");
}

const Tauri = window.__TAURI__;
if (Tauri && Tauri.event) {
  Tauri.event.listen("recording-started", () => {
    document.body.classList.remove("transcribing");
    startTimer();
  });
  Tauri.event.listen("recording-stopped", stopTimer);
  Tauri.event.listen("transcribing-started", startTranscribing);
  Tauri.event.listen("transcription-complete", endTranscribing);
  Tauri.event.listen("transcription-error", endTranscribing);
  Tauri.event.listen("recording-error", endTranscribing);
} else {
  console.error("gettype pill: Tauri API not available");
}
