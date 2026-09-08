// gettype — DSP preprocessing mikrofonního audia před Whisper API.
//
// Pořadí: trim ticha → high-pass filtr → RMS normalizace.
// Vstup: &[i16] na 16 kHz mono. Výstup: Vec<i16>. Jen std, žádné závislosti.
// Whisper na čistém tichu halucinuje („Titulky vytvořil…“), proto ticho
// nikdy neposíláme — místo toho vracíme Err.

/// Vzorkovací kmitočet — `process` očekává výstup z `resample_linear` (16 kHz).
const SAMPLE_RATE: f32 = 16_000.0;
/// Práh trimu −30 dBFS: 20·log10(0.0316) ≈ −30 dB. Amplituda pod ~0.032
/// (≈ 1036 v i16) se bere jako ticho.
const TRIM_THRESHOLD_NORM: f32 = 0.032;
/// Trimovací chunk 10 ms při 16 kHz = 160 vzorků.
const TRIM_CHUNK: usize = 160;
/// Minimum 300 ms (stejná hranice jako MIN_DURATION_MS v audio.rs):
/// 0.3 s · 16 000 = 4800 vzorků. Kratší zbytek → Err, ne ticho do Whisperu.
const MIN_SAMPLES: usize = 4800;
/// High-pass 80 Hz — odstraňuje HVAC bručení / rumble / stejnosměrný offset.
const HPF_FREQ: f32 = 80.0;
/// Q 0.707 (Butterworth, maximálně plochá propust bez rezonance).
const HPF_Q: f32 = 0.70710678;
/// Cílová hlasitost −16 dBFS v RMS: 10^(−16/20) ≈ 0.1585 (normalizovaně).
const TARGET_RMS_DB: f32 = -16.0;
/// Cap zisku na +20 dB (lineárně 10×) — ticho/šepot nesmí vyletět do šumu.
const MAX_GAIN_DB: f32 = 20.0;
/// Hranice soft-clipu: vzorky nad 0.95 se zaoblí přes tanh místo ostrého řezu.
const SOFT_CLIP_THRESHOLD: f32 = 0.95;

/// Trim → HPF → RMS normalizace. Prázdný vstup → Err, nikdy panic.
pub fn process(samples: &[i16]) -> Result<Vec<i16>, String> {
    if samples.is_empty() {
        return Err("Recording too short".to_string());
    }
    let in_ms = samples.len() as u64 * 1000 / SAMPLE_RATE as u64;

    // 1. Trim ticha na začátku/konci (chunk-aligned).
    let trimmed = trim_silence(samples)?;
    // 2. High-pass 80 Hz (biquad, 2. řád).
    let filtered = apply_high_pass(&trimmed);
    // 3. RMS normalizace na −16 dBFS + clip-guard.
    let (out, gain_db) = normalize(&filtered)?;

    let out_ms = out.len() as u64 * 1000 / SAMPLE_RATE as u64;
    eprintln!("preprocess: in_ms={in_ms} out_ms={out_ms} gain_db={gain_db:.1}");
    Ok(out)
}

/// Ořeže úvodní/koncové 10ms chunky, jejichž špička je pod −30 dBFS.
/// Chunk je „hlasitý“, když aspoň jeden vzorek překročí práh.
fn trim_silence(samples: &[i16]) -> Result<Vec<i16>, String> {
    // Práh v i16: 0.032 · 32767 ≈ 1049.
    let threshold = (TRIM_THRESHOLD_NORM * i16::MAX as f32).round() as i16;
    if samples.is_empty() {
        return Err("Recording too short".to_string());
    }
    // Počet chunků (poslední může být neúplný — hodnotí se stejně).
    let n_chunks = samples.len().div_ceil(TRIM_CHUNK);
    let is_loud = |chunk: &[i16]| chunk.iter().any(|&s| s.abs() >= threshold);

    let mut first_loud: Option<usize> = None;
    let mut last_loud: Option<usize> = None;
    for i in 0..n_chunks {
        let start = i * TRIM_CHUNK;
        let end = (start + TRIM_CHUNK).min(samples.len());
        if is_loud(&samples[start..end]) {
            if first_loud.is_none() {
                first_loud = Some(i);
            }
            last_loud = Some(i);
        }
    }
    let (first, last) = match (first_loud, last_loud) {
        (Some(f), Some(l)) => (f, l),
        // Žádný hlasitý chunk → čisté ticho. Whisper by halucinoval.
        _ => return Err("Recording too short".to_string()),
    };
    let start = first * TRIM_CHUNK;
    let end = ((last + 1) * TRIM_CHUNK).min(samples.len());
    let trimmed = &samples[start..end];
    if trimmed.len() < MIN_SAMPLES {
        return Err("Recording too short".to_string());
    }
    Ok(trimmed.to_vec())
}

/// High-pass biquad 2. řádu (Direct Form I).
///
/// Koeficienty podle audio EQ cookbooku R. Bristowa-Johnsona
/// (https://webaudio.github.io/Audio-EQ-Cookbook/audio-eq-cookbook.html),
/// typ HPF, f0 = 80 Hz, Q = 0.707:
///   w0 = 2π·f0/Fs,  alpha = sin(w0)/(2·Q)
///   b0 = (1+cos w0)/2,  b1 = −(1+cos w0),  b2 = (1+cos w0)/2
///   a0 = 1+alpha,  a1 = −2·cos w0,  a2 = 1−alpha
/// normalizováno dělením a0. Počítáno za běhu (ne hardcodováno),
/// aby změna HPF_FREQ/HPF_Q nepotřebovala přepočet konstant.
fn apply_high_pass(samples: &[i16]) -> Vec<f32> {
    let w0 = 2.0 * std::f32::consts::PI * HPF_FREQ / SAMPLE_RATE;
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    let alpha = sin_w0 / (2.0 * HPF_Q);
    let b0 = (1.0 + cos_w0) / 2.0;
    let b1 = -(1.0 + cos_w0);
    let b2 = (1.0 + cos_w0) / 2.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha;
    // Normalizace a0:
    let b0 = b0 / a0;
    let b1 = b1 / a0;
    let b2 = b2 / a0;
    let a1 = a1 / a0;
    let a2 = a2 / a0;

    let mut out = Vec::with_capacity(samples.len());
    let (mut x1, mut x2, mut y1, mut y2) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for &s in samples {
        let x0 = s as f32 / i16::MAX as f32;
        // y[n] = b0·x0 + b1·x1 + b2·x2 − a1·y1 − a2·y2
        let y0 = b0 * x0 + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
        x2 = x1;
        x1 = x0;
        y2 = y1;
        y1 = y0;
        out.push(y0);
    }
    out
}

/// RMS normalizace filtrovaného signálu (normalizované f32) na −16 dBFS.
///
/// gain = 10^(−16/20) / rms, cap na +20 dB (10×). Tiché RMS pod
/// −60 dBFS (~0.001) bereme jako zbytkové ticho → Err.
/// Clip-guard: po zisku hard-clip do [−1, 1]; vzorky nad 0.95 navíc
/// prochází jemným tanh zaoblením
///   y = sign·(T + (1−T)·tanh((|y|−T)/(1−T))), T = 0.95,
/// které asymptoticky míří k 1.0 místo ostrého řezu (méně slyšitelný clipping).
fn normalize(filtered: &[f32]) -> Result<(Vec<i16>, f32), String> {
    if filtered.is_empty() {
        return Err("Recording too short".to_string());
    }
    let sum_sq: f64 = filtered.iter().map(|&x| (x as f64) * (x as f64)).sum();
    let rms = (sum_sq / filtered.len() as f64).sqrt() as f32;
    // Pod −60 dBFS už není řeč, jen šum po HPF → nezesilovat, vrátit Err.
    if !rms.is_finite() || rms < 0.001 {
        return Err("Recording too short".to_string());
    }
    let target_rms = 10f32.powf(TARGET_RMS_DB / 20.0);
    let max_gain = 10f32.powf(MAX_GAIN_DB / 20.0); // +20 dB = 10×
    let gain = (target_rms / rms).min(max_gain);
    let gain_db = 20.0 * gain.log10();

    let mut out = Vec::with_capacity(filtered.len());
    for &x in filtered {
        let y = x * gain;
        let clipped = if y.abs() <= SOFT_CLIP_THRESHOLD {
            y
        } else {
            // Soft-clip: lineární do 0.95, nad ním tanh zaoblení k 1.0.
            let sign = y.signum();
            let excess = (y.abs() - SOFT_CLIP_THRESHOLD) / (1.0 - SOFT_CLIP_THRESHOLD);
            sign * (SOFT_CLIP_THRESHOLD + (1.0 - SOFT_CLIP_THRESHOLD) * excess.tanh())
        };
        // Hard-clip pojistka + převod do i16 (nikdy panic mimo rozsah).
        let clamped = clipped.clamp(-1.0, 1.0);
        out.push((clamped * i16::MAX as f32).round() as i16);
    }
    Ok((out, gain_db))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sinus daného kmitočtu/amplitudy (normalizovaná amplituda 0..1), 16 kHz.
    fn sine(freq_hz: f32, amp_norm: f32, secs: f32) -> Vec<i16> {
        let n = (secs * SAMPLE_RATE) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE;
                let v = (2.0 * std::f32::consts::PI * freq_hz * t).sin() * amp_norm;
                (v.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
            })
            .collect()
    }

    fn rms_dbfs(samples: &[i16]) -> f32 {
        let sum_sq: f64 = samples
            .iter()
            .map(|&s| {
                let x = s as f64 / i16::MAX as f64;
                x * x
            })
            .sum();
        let rms = (sum_sq / samples.len() as f64).sqrt();
        20.0 * rms.log10() as f32
    }

    #[test]
    fn trims_leading_and_trailing_silence() {
        // 1 s ticha + 0.5 s tónu (440 Hz, −6 dBFS) + 1 s ticha.
        let mut input = vec![0i16; SAMPLE_RATE as usize];
        input.extend(sine(440.0, 0.5, 0.5));
        input.extend(vec![0i16; SAMPLE_RATE as usize]);
        let out = process(&input).expect("tone must survive trim");
        // Ořez: výstup citelně kratší než 2.5 s vstupu, ale drží tón (~0.5 s).
        assert!(out.len() < input.len(), "output not trimmed");
        assert!(
            out.len() >= MIN_SAMPLES,
            "trimmed too aggressively: {}",
            out.len()
        );
        // Tón 0.5 s = 8000 vzorků ± chunk granularita (2×160).
        assert!(
            out.len() <= 8000 + 2 * TRIM_CHUNK,
            "trailing silence remains: {}",
            out.len()
        );
        assert!(
            out.len() >= 8000 - 2 * TRIM_CHUNK,
            "tone cut short: {}",
            out.len()
        );
    }

    #[test]
    fn high_pass_keeps_1khz_and_attenuates_50hz() {
        // Přímo na filtru (bez normalizace, která by zisk vyrovnala).
        let tone_1k = sine(1000.0, 0.5, 1.0);
        let hum_50 = sine(50.0, 0.5, 1.0);
        let peak = |v: &[f32]| v.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        let in_peak_1k = peak(
            &tone_1k
                .iter()
                .map(|&s| s as f32 / i16::MAX as f32)
                .collect::<Vec<_>>(),
        );
        let in_peak_50 = peak(
            &hum_50
                .iter()
                .map(|&s| s as f32 / i16::MAX as f32)
                .collect::<Vec<_>>(),
        );
        // Ustálený stav: měříme druhou polovinu (bez náběhu filtru).
        let out_1k = apply_high_pass(&tone_1k);
        let out_50 = apply_high_pass(&hum_50);
        let half = out_1k.len() / 2;
        let out_peak_1k = peak(&out_1k[half..]);
        let out_peak_50 = peak(&out_50[half..]);
        // 1 kHz projde téměř beze změny (do −1 dB).
        assert!(
            out_peak_1k > in_peak_1k * 0.89,
            "1kHz attenuated too much: {out_peak_1k} vs {in_peak_1k}"
        );
        // 50 Hz zeslaben aspoň o ~6 dB (HPF 80 Hz, 12 dB/okt → reálně ~−9 dB).
        assert!(
            out_peak_50 < in_peak_50 * 0.5,
            "50Hz not attenuated: {out_peak_50} vs {in_peak_50}"
        );
    }

    #[test]
    fn normalizes_quiet_signal_toward_minus_16_dbfs() {
        // Tichý tón −32 dBFS (amp ≈ 0.025) — nad trim prahem? Ne: peak 0.025
        // < 0.032 → trim by ho ořezal. Proto hlasitější −22 dBFS (amp 0.08).
        let tone = sine(440.0, 0.08, 1.0);
        let out = process(&tone).expect("quiet tone must process");
        let db = rms_dbfs(&out);
        // RMS sinu −22 dBFS peak → −25 dBFS RMS; cíl −16 dBFS RMS ±2 dB.
        assert!(
            (-18.0..=-14.0).contains(&db),
            "RMS not normalized to -16 dBFS: {db:.1} dBFS"
        );
    }

    #[test]
    fn pure_silence_is_error() {
        let silence = vec![0i16; SAMPLE_RATE as usize];
        assert!(process(&silence).is_err());
        assert!(process(&[]).is_err());
    }
}
