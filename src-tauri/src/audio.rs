// gettype — mikrofonní capture (cpal) → 16 kHz mono WAV (hound).
//
// Stav nahrávání žije v Tauri managed state (Mutex<Recorder>). cpal Stream je
// uložený uvnitř stavu, aby ho Rust nesepadl hned po vrácení ze start().
// Veškeré chyby jdou jako event `recording-error` do frontendu — žádné paniky.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

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

/// Lock, který přežije poison (audio callback nesmí shodit UI).
/// `inner()` vrátí referenci s životností AppHandle, takže guard lze vrátit.
fn lock_recorder(app: &AppHandle) -> MutexGuard<'_, Recorder> {
    let mutex = app.state::<Mutex<Recorder>>().inner();
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn emit_error(app: &AppHandle, message: impl Into<String>) {
    // Chybová větev končí vždy viditelným stavem: pilulku schováváme tady,
    // ať žádná chyba nezanechá okno viset.
    if let Some(pill) = pill_window(app) {
        let _ = pill.hide();
    }
    let _ = app.emit("recording-error", json!({ "message": message.into() }));
}

fn pill_window(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    app.get_webview_window("pill")
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
            return; // už běží — ignoruj (double-press, toggle vs. push kolize)
        }
    }

    let host = cpal::default_host();
    let Some(device) = host.default_input_device() else {
        emit_error(app, "No microphone available");
        return;
    };
    let config = match device.default_input_config() {
        Ok(config) => config,
        Err(e) => {
            // Typicky TCC: aplikaci nebyl povolen přístup k mikrofonu.
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
            emit_error(app, format!("Unsupported microphone sample format: {other:?}"));
            return;
        }
    };
    let stream = match stream {
        Ok(stream) => stream,
        Err(e) => {
            emit_error(app, format!("Cannot open microphone: {e}"));
            return;
        }
    };
    if let Err(e) = stream.play() {
        emit_error(app, format!("Cannot start microphone stream: {e}"));
        return; // stream se dropne na konci scope → nic nezůstane viset
    }

    {
        let mut recorder = lock_recorder(app);
        // Double-check po async fázi (build/play) — proti závodu při rychlém
        // přepínání hotkey na obou testech výše nestačí.
        if matches!(recorder.state, RecorderState::Recording { .. }) {
            return; // mezitím začalo jiné nahrávání → tento stream se dropne
        }
        recorder.stream = Some(stream);
        recorder.samples = Arc::clone(&samples);
        recorder.state = RecorderState::Recording {
            started_at: Instant::now(),
            sample_rate,
        };
    }

    // Pilulka se jen ukáže — bez focusu, ať nevytrhne uživatele z aplikace.
    if let Some(pill) = pill_window(app) {
        let _ = pill.show();
    }
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

    let duration_ms = started_at.elapsed().as_millis();

    if duration_ms < MIN_DURATION_MS {
        emit_error(app, "Recording too short");
        return;
    }

    let raw = samples
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let resampled = resample_linear(&raw, sample_rate, TARGET_SAMPLE_RATE);

    let path = match write_wav(&resampled) {
        Ok(path) => path,
        Err(e) => {
            emit_error(app, format!("WAV write failed: {e}"));
            return;
        }
    };

    lock_recorder(app).last_recording = Some(path.clone());
    let _ = app.emit(
        "recording-stopped",
        json!({
            "duration_ms": duration_ms,
            "path": path.to_string_lossy(),
        }),
    );

    // Pilulka zůstává viditelná (timer už stopnutý) a přechází do stavu
    // „překládám“ — skrývá ji až stt/output v koncovém stavu (úspěch i chyba).
    crate::stt::transcribe(app.clone(), path);
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
                    emit_error(
                        &handle,
                        "Microphone access denied — allow mic in System Settings",
                    );
                    stop(&handle);
                }
                DeviceNotAvailable => {
                    emit_error(&handle, "Microphone is not available");
                    stop(&handle);
                }
                _ => eprintln!("gettype audio stream error: {err}"),
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
        let f32_sample: f32 = frame[0].to_sample();
        out.push(to_i16(f32_sample));
    }
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
