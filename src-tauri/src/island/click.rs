//! Notifi's one sound: a short deep click, made in code so no audio file ships.

const RATE: u32 = 44_100;
const SECONDS: f64 = 0.12;

/// A mono 16-bit WAV: a low sine that falls from about 190 Hz to 60 Hz and dies out in under 100 ms,
/// with a soft low-passed knock at the start so it reads as a click and not a beep.
pub fn wav() -> Vec<u8> {
    let n = (f64::from(RATE) * SECONDS) as usize;
    let mut pcm: Vec<i16> = Vec::with_capacity(n);
    let (mut phase, mut seed, mut soft) = (0.0f64, 0x2545_F491u32, 0.0f64);
    for i in 0..n {
        let t = i as f64 / f64::from(RATE);
        phase += std::f64::consts::TAU * (60.0 + 130.0 * (-t / 0.018).exp()) / f64::from(RATE);
        let body = phase.sin() * (1.0 - (-t / 0.0015).exp()) * (-t / 0.028).exp();
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        soft += 0.15 * (f64::from(seed) / f64::from(u32::MAX) * 2.0 - 1.0 - soft);
        let knock = soft * (-t / 0.004).exp();
        // The last few milliseconds fade out so the end cannot pop.
        let fade = ((n - i) as f64 / 200.0).min(1.0);
        pcm.push(((body * 0.9 + knock * 0.9).tanh() * 0.7 * fade * f64::from(i16::MAX)) as i16);
    }
    let data = (pcm.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + pcm.len() * 2);
    out.extend(b"RIFF");
    out.extend((36 + data).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend([1u8, 0, 1, 0]); // PCM, mono
    out.extend(RATE.to_le_bytes());
    out.extend((RATE * 2).to_le_bytes());
    out.extend([2u8, 0, 16, 0]); // block align, bits per sample
    out.extend(b"data");
    out.extend(data.to_le_bytes());
    out.extend(pcm.iter().flat_map(|s| s.to_le_bytes()));
    out
}

/// Plays the click without waiting for it to finish. Windows only for now.
#[cfg(windows)]
pub fn play() {
    use std::sync::OnceLock;
    use windows::{
        core::PCWSTR,
        Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT},
    };
    // The buffer must outlive the playback, so it is kept for the life of the program.
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    let bytes = BYTES.get_or_init(wav);
    let _ = unsafe { PlaySoundW(PCWSTR(bytes.as_ptr().cast()), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT) };
}

#[cfg(not(windows))]
pub fn play() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_is_a_valid_quiet_enough_wav_that_ends_at_rest() {
        let w = wav();
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        let samples: Vec<i16> = w[44..].chunks(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        assert_eq!(w.len(), 44 + samples.len() * 2);
        assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()) as usize, w.len() - 8);
        let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!((6_000..26_000).contains(&peak), "peak {peak}");
        assert!(samples.last().unwrap().unsigned_abs() < 50);
    }
}
