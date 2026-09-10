// gettype — mikrofonní capture (cpal) → 16 kHz mono WAV (hound).
//
// Stav nahrávání žije v Tauri managed state (Mutex<Recorder>). cpal Stream je
// uložený uvnitř stavu, aby ho Rust nesepadl hned po vrácení ze start().
// Veškeré chyby jdou jako event `recording-error` do frontendu — žádné paniky.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

use crate::log;

/// Cílový formát WAV, který jde rovnou do Whisper API (fáze 4).
pub const TARGET_SAMPLE_RATE: u32 = 16_000;
/// Pod 300 ms záznam nemá smysl posílat — spíš omylek klávesy.
const MIN_DURATION_MS: u128 = 300;

/// Živý stav nahrávání (spec: varianty Idle / Recording).
#[derive(Debug, Default)]
pub enum RecorderState {
    #[default]
    Idle,
    Recording {
        started_at: Instant,
        /// device sample rate, z nějž se při stopu resampluje na 16 kHz
        sample_rate: u32,
    },
}

/// Managed state: `Mutex<Recorder>` (bez Debug — cpal::Stream ho nemá).
#[derive(Default)]
pub struct Recorder {
    pub state: RecorderState,
    pub last_recording: Option<PathBuf>,
    /// Stream musí žít po celou dobu nahrávání — dropne se v stop().
    stream: Option<cpal::Stream>,
    /// Vzorky sdílené s audio callbackem. Callback sahá jen sem, nikdy na
    /// Recorder — audio thread se nemůže vmíchávat do hlavního zámku.
    samples: Arc<Mutex<Vec<i16>>>,
}

/// Generace akce (nahrávání → transkripce → output patří k sobě).
/// `start()` i `cancel()` ji zvyšují; doběhlá transkripce/output s jinou
/// generací svůj výsledek zahodí (Escape-cancel). 0 = žádná akce.
static ACTION_GEN: AtomicU64 = AtomicU64::new(0);
/// Generace právě běžící transkripce, 0 = žádná. Slouží i jako „probíhá
/// akce“ pro gating Escape (mimo akci Escape nic nedělá).
static TRANSCRIBING_GEN: AtomicU64 = AtomicU64::new(0);

/// Aktuální generace akce.
pub fn current_generation() -> u64 {
    ACTION_GEN.load(Ordering::SeqCst)
}

/// True, pokud generace už byla zneplatněna (cancel / novější akce).
pub fn is_stale(generation: u64) -> bool {
    ACTION_GEN.load(Ordering::SeqCst) != generation
}

/// Probíhá nahrávání nebo transkripce (Escape má smysl jen tehdy).
pub fn is_active(app: &AppHandle) -> bool {
    is_recording(app) || TRANSCRIBING_GEN.load(Ordering::SeqCst) != 0
}

/// Probíhá transkripce (TRANSCRIBING_GEN != 0).
pub fn is_transcribing() -> bool {
    TRANSCRIBING_GEN.load(Ordering::SeqCst) != 0
}

/// Transkripce generace `generation` právě odstartovala.
pub fn set_transcribing(generation: u64) {
    TRANSCRIBING_GEN.store(generation, Ordering::SeqCst);
}

/// Transkripce generace `generation` doběhla — vlajku shodí jen vlastník
/// (compare_exchange), aby pozdní konec staré akce neshodil novější.
pub fn clear_transcribing(generation: u64) {
    let _ = TRANSCRIBING_GEN.compare_exchange(
        generation,
        0,
        Ordering::SeqCst,
        Ordering::SeqCst,
    );
}

/// Lock, který přežije poison (audio callback nesmí shodit UI).
/// `inner()` vrátí referenci s životností AppHandle, takže guard lze vrátit.
fn lock_recorder(app: &AppHandle) -> MutexGuard<'_, Recorder> {
    let mutex = app.state::<Mutex<Recorder>>().inner();
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn emit_error(app: &AppHandle, message: impl Into<String>) {
    // Chybová větev končí vždy viditelným stavem: pilulku fade-outneme tady,
    // ať žádná chyba nezanechá okno viset (hide až po CSS animaci).
    crate::pill::fade_out(app);
    let _ = app.emit("recording-error", json!({ "message": message.into() }));
}

pub fn is_recording(app: &AppHandle) -> bool {
    matches!(lock_recorder(app).state, RecorderState::Recording { .. })
}

pub fn toggle(app: &AppHandle) {
    if is_recording(app) {
        stop(app);
    } else {
        start(app);
    }
}

/// Otevře defaultní vstupní zařízení a začne sbírat i16 vzorky (1. kanál).
/// Vrátí se bez akce, pokud už nahrávání běží.
pub fn start(app: &AppHandle) {
    {
        let recorder = lock_recorder(app);
        if matches!(recorder.state, RecorderState::Recording { .. }) {
            log::info("audio", "start ignored already-recording=true");
            return; // už běží — ignoruj (double-press, toggle vs. push kolize)
        }
    }
    log::info("audio", "start requested");

    let host = cpal::default_host();
    let Some(device) = host.default_input_device() else {
        log::error("audio", "start failed reason=\"no microphone available\"");
        emit_error(app, "No microphone available");
        return;
    };
    let device_name = device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    let config = match device.default_input_config() {
        Ok(config) => config,
        Err(e) => {
            // Typicky TCC: aplikaci nebyl povolen přístup k mikrofonu.
            log::error(
                "audio",
                format!("start failed reason=\"cannot open microphone (mic permission?)\" err=\"{e}\""),
            );
            emit_error(app, format!("Cannot open microphone (mic permission?): {e}"));
            return;
        }
    };

    let sample_rate = config.sample_rate(); // v cpal 0.18 je SampleRate = u32
    let channels = config.channels().max(1) as usize;
    let format = config.sample_format();

    let samples: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    let stream_config: cpal::StreamConfig = config.into();

    let stream = match format {
        SampleFormat::F32 => build_stream::<f32>(app, &device, &stream_config, &samples, channels),
        SampleFormat::I16 => build_stream::<i16>(app, &device, &stream_config, &samples, channels),
        SampleFormat::U16 => build_stream::<u16>(app, &device, &stream_config, &samples, channels),
        other => {
            log::error(
                "audio",
                format!("start failed reason=\"unsupported sample format\" format=\"{other:?}\""),
            );
            emit_error(app, format!("Unsupported microphone sample format: {other:?}"));
            return;
        }
    };
    let stream = match stream {
        Ok(stream) => stream,
        Err(e) => {
            log::error("audio", format!("start failed reason=\"cannot build stream\" err=\"{e}\""));
            emit_error(app, format!("Cannot open microphone: {e}"));
            return;
        }
    };
    if let Err(e) = stream.play() {
        log::error("audio", format!("start failed reason=\"cannot play stream\" err=\"{e}\""));
        emit_error(app, format!("Cannot start microphone stream: {e}"));
        return; // stream se dropne na konci scope → nic nezůstane viset
    }

    {
        let mut recorder = lock_recorder(app);
        // Double-check po async fázi (build/play) — proti závodu při rychlém
        // přepínání hotkey na obou testech výše nestačí.
        if matches!(recorder.state, RecorderState::Recording { .. }) {
            log::info("audio", "start raced already-recording=true dropping-stream=true");
            return; // mezitím začalo jiné nahrávání → tento stream se dropne
        }
        recorder.stream = Some(stream);
        recorder.samples = Arc::clone(&samples);
        recorder.state = RecorderState::Recording {
            started_at: Instant::now(),
            sample_rate,
        };
        // Nová akce — zneplatní případný doběh předchozí transkripce/outputu
        // (uživatel stihl začít znovu dřív, než stará doběhla).
        ACTION_GEN.fetch_add(1, Ordering::SeqCst);
    }
    log::info(
        "audio",
        format!(
            "start ok=true device=\"{device_name}\" rate={sample_rate} channels={channels} format=\"{format:?}\" generation={}",
            current_generation()
        ),
    );

    // Akce běží → zachyť globální Escape pro cancel. V idle stavu zůstává
    // Escape odregistrovaný a propadá do ostatních aplikací.
    crate::hotkey::register_escape(app);

    // Pilulka se jen ukáže — bez focusu, ať nevytrhne uživatele z aplikace.
    // show() zároveň zneplatní čekající fade-hide z předchozího nahrávání.
    crate::pill::show(app);
    crate::sound::play_start();
    let _ = app.emit("recording-started", json!({ "sample_rate": sample_rate }));
}

/// Stopne stream, resampluje na 16 kHz mono a zapíše WAV. Resetuje stav.
/// Úspěch předává dokonalý WAV do `stt::transcribe` (pilulka přechází do
/// stavu „překládám“); chyby jdou přes `emit_error` (schovává pilulku).
pub fn stop(app: &AppHandle) {
    // Stream vyjmeme pod zámkem, ale dropujeme až po odemčení (Drop
    // streamu může chvíli blokovat; audio thread mezitím klidně dokončí callback).
    let (stream, samples, started_at, sample_rate) = {
        let mut recorder = lock_recorder(app);
        let RecorderState::Recording {
            started_at,
            sample_rate,
        } = recorder.state
        else {
            return; // nenahráváme — nic
        };
        recorder.state = RecorderState::Idle;
        (
            recorder.stream.take(),
            std::mem::take(&mut recorder.samples),
            started_at,
            sample_rate,
        )
    };
    drop(stream);

    // Zvuk konce nahrávání — fire-and-forget, best-effort (viz sound.rs).
    // Hraje při každém skutečném stopu (i too-short / preprocess / wav
    // fail — nahrávání tím skončilo), ne při no-opu bez nahrávání.
    crate::sound::play_stop();

    let duration_ms = started_at.elapsed().as_millis();

    if duration_ms < MIN_DURATION_MS {
        log::warn(
            "audio",
            format!("stop rejected reason=\"too short\" duration_ms={duration_ms} min_ms={MIN_DURATION_MS}"),
        );
        emit_error(app, "Recording too short");
        // Akce končí bez transkripce → Escape zpět ostatním aplikacím
        // (jen když mezitím nezačala nová akce).
        crate::hotkey::release_escape_if_idle(app);
        return;
    }

    let raw = samples
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    log::info(
        "audio",
        format!("stop duration_ms={duration_ms} samples={} rate={sample_rate}", raw.len()),
    );
    let resampled = resample_linear(&raw, sample_rate, TARGET_SAMPLE_RATE);

    // DSP preprocessing (trim → HPF → normalizace); ticho → Err, ne WAV.
    let processed = match crate::preprocess::process(&resampled) {
        Ok(processed) => processed,
        Err(e) => {
            log::warn("audio", format!("stop failed reason=\"preprocess rejected\" err=\"{e}\""));
            emit_error(app, e);
            crate::hotkey::release_escape_if_idle(app);
            return;
        }
    };

    let path = match write_wav(&processed) {
        Ok(path) => path,
        Err(e) => {
            log::error("audio", format!("stop failed reason=\"wav write failed\" err=\"{e}\""));
            emit_error(app, format!("WAV write failed: {e}"));
            crate::hotkey::release_escape_if_idle(app);
            return;
        }
    };

    lock_recorder(app).last_recording = Some(path.clone());
    let generation = current_generation();
    log::info(
        "audio",
        format!(
            "stop ok=true path=\"{}\" duration_ms={duration_ms} generation={generation}",
            path.to_string_lossy()
        ),
    );
    let _ = app.emit(
        "recording-stopped",
        json!({
            "duration_ms": duration_ms,
            "path": path.to_string_lossy(),
        }),
    );

    // Pilulka zůstává viditelná (timer už stopnutý) a přechází do stavu
    // „překládám“ — skrývá ji až stt/output v koncovém stavu (úspěch i chyba).
    crate::stt::transcribe(app.clone(), path, generation);
}

/// Zruší celou probíhající akci (Escape): drop streamu + zahození bufferu
/// bez WAV write, stav → Idle, zneplatnění doběhlé transkripce/outputu
/// (generační guard), best-effort smazání WAV z transcribing fáze, okamžitý
/// fade-out pilulky + event `recording-cancelled`.
///
/// Mimo probíhající akci (Idle, nic se nepřepisuje) je no-op — žádný event,
/// žádný fade. Krátká nahrávka (<300 ms) zrušená Escapem jde touto cestou,
/// nikdy error path `stop()`.
///
/// PTT guard: stav je po cancelu Idle, takže doběhnuvší `Released` původní
/// zkratky v hotkey handleru přes `is_recording()` nic nespustí.
pub fn cancel(app: &AppHandle) {
    // Stream vyjmeme pod zámkem, dropneme až po odemčení (stejně jako stop).
    let (stream, was_recording) = {
        let mut recorder = lock_recorder(app);
        match recorder.state {
            RecorderState::Recording { .. } => {
                recorder.state = RecorderState::Idle;
                let stream = recorder.stream.take();
                // Buffer zahodit — žádný WAV z něj nikdy nevznikne.
                recorder.samples = Arc::new(Mutex::new(Vec::new()));
                (stream, true)
            }
            RecorderState::Idle => (None, false),
        }
    };
    drop(stream);

    // Transcribing vlajku shodíme hned (doběhlý task si svůj konec pohlídá
    // přes clear_transcribing + is_stale a výsledek zahodí).
    let was_transcribing = TRANSCRIBING_GEN.swap(0, Ordering::SeqCst) != 0;
    if !was_recording && !was_transcribing {
        log::info("audio", "cancel ignored idle=true");
        return; // idle — Escape nic nedělá
    }
    // Zneplatní doběhlou transkripci i output (generační guard ve stt/output).
    ACTION_GEN.fetch_add(1, Ordering::SeqCst);

    // Stejný stop zvuk jako stop() — cancel je jen jiná cesta konce akce.
    // Hraje i při cancelu během transcribing (bez nahrávání), ne při
    // idle no-opu (ten se vrátil už výše).
    crate::sound::play_stop();

    // WAV z transcribing fáze best-effort smazat (při cancelu během
    // nahrávání žádný soubor neexistuje — write přichází až ve stopu).
    if was_transcribing {
        if let Some(path) = lock_recorder(app).last_recording.take() {
            let _ = std::fs::remove_file(&path);
        }
    }

    log::info(
        "audio",
        format!("cancel ok=true was_recording={was_recording} was_transcribing={was_transcribing}"),
    );
    crate::pill::fade_out(app);
    let _ = app.emit("recording-cancelled", json!({}));
    // Akce skončila → Escape zpět ostatním aplikacím (jen když mezitím
    // nezačala nová akce).
    crate::hotkey::release_escape_if_idle(app);
}

/// Postaví input stream pro konkrétní sample typ; callback zapisuje do bufferu.
fn build_stream<T: SizedSample>(
    app: &AppHandle,
    device: &cpal::Device,
    stream_config: &cpal::StreamConfig,
    samples: &Arc<Mutex<Vec<i16>>>,
    channels: usize,
) -> Result<cpal::Stream, cpal::Error>
where
    f32: FromSample<T>,
{
    let buf = Arc::clone(samples);
    let handle = app.clone();
    device.build_input_stream(
        stream_config.clone(),
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            push_first_channel(&buf, data, channels);
        },
        // Runtime chyby streamu: TCC zamítnutí přichází až sem, ne do build().
        move |err| {
            use cpal::ErrorKind::{DeviceNotAvailable, PermissionDenied};
            match err.kind() {
                PermissionDenied => {
                    log::error(
                        "audio",
                        format!("stream fatal reason=\"mic permission denied\" err=\"{err}\""),
                    );
                    emit_error(
                        &handle,
                        "Microphone access denied — allow mic in System Settings",
                    );
                    stop(&handle);
                }
                DeviceNotAvailable => {
                    log::error("audio", format!("stream fatal reason=\"mic not available\" err=\"{err}\""));
                    emit_error(&handle, "Microphone is not available");
                    stop(&handle);
                }
                // Underrun/overrun je běžný šum CoreAudio — jen warning,
                // nahrávání tím nekončí a stav se nemění.
                _ => log::warn("audio", format!("stream warning ignored=true err=\"{err}\"")),
            }
        },
        None, // bez timeoutu — čekáme, až CoreAudio postaví unit
    )
}

/// Ze vstupního bufferu vezme jen první kanál a převede na i16.
fn push_first_channel<T: SizedSample>(buf: &Arc<Mutex<Vec<i16>>>, data: &[T], channels: usize)
where
    f32: FromSample<T>,
{
    // Blokující lock — stop() drží zámek jen na mikrosekundy (clone),
    // případná krátká prodleva audio threadu je neškodná.
    let mut out = buf
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    out.reserve(data.len() / channels + 1);
    for frame in data.chunks(channels) {
        // cpal dává normalizované f32 (-1.0..1.0) i pro i16/u16 vstup —
        // škálovat na i16 rozsah, jinak vznikne ticho (-1/0/1) a Whisper
        // halucinuje ("Titulky vytvořil JohnyX" na tichu).
        let f32_sample: f32 = frame[0].to_sample();
        out.push(from_normalized_f32(f32_sample));
    }
}

/// Normalizovaný vzorek -1.0..1.0 → i16 (vstup z mikrofonu přes cpal).
fn from_normalized_f32(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

fn to_i16(sample: f32) -> i16 {
    sample
        .clamp(i16::MIN as f32, i16::MAX as f32)
        .round() as i16
}

/// Jednoduchý převzorkovač na 16 kHz: lineární interpolace mezi sousedními
/// vzorky. Kvalita bohatě stačí pro řeč → Whisper.
fn resample_linear(samples: &[i16], from_rate: u32, to_rate: u32) -> Vec<i16> {
    if samples.is_empty() || from_rate == to_rate {
        return samples.to_vec();
    }
    let out_len = ((samples.len() as u64 * to_rate as u64) / from_rate as u64) as usize;
    if out_len == 0 {
        return Vec::new();
    }
    let step = from_rate as f64 / to_rate as f64;
    let last_idx = samples.len() - 1;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * step;
        let idx = (pos as usize).min(last_idx);
        let frac = (pos - idx as f64) as f32;
        let s0 = samples[idx] as f32;
        let s1 = samples[(idx + 1).min(last_idx)] as f32;
        let v = s0 + (s1 - s0) * frac;
        out.push(to_i16(v));
    }
    out
}

/// `$HOME/Library/Application Support/gettype/recordings`
fn recordings_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("gettype")
            .join("recordings"),
    )
}

/// Zapíše 16-bit PCM mono WAV, vrátí cestu k souboru.
fn write_wav(samples: &[i16]) -> Result<PathBuf, String> {
    let dir = recordings_dir().ok_or_else(|| "$HOME is not set".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("create recordings dir failed: {e}"))?;
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = dir.join(format!("rec-{ms}.wav"));

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: TARGET_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec).map_err(|e| e.to_string())?;
    for &sample in samples {
        writer.write_sample(sample).map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())?;
    Ok(path)
}
