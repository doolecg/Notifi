//! Sound for the recording: what the speakers play (WASAPI loopback) and the microphone, each
//! brought to 48 kHz stereo and written as one steady stream timed by the recording clock.

use std::{
    ffi::c_void,
    ptr,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use windows::{
    core::PCWSTR,
    Win32::{
        Media::Audio::{
            eCapture, eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
            AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
        },
        System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED},
    },
};

use super::{encode::Msg, pure::*, Session};

fn err(what: &'static str) -> impl Fn(windows::core::Error) -> String {
    move |e| format!("{what}: {e}")
}

/// One capture stream, already converted to 48 kHz stereo in `out`.
pub struct Source {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    channels: usize,
    bits: u16,
    float: bool,
    resampler: Resampler,
    pub out: Vec<Stereo>,
}

impl Source {
    fn open(device: &IMMDevice, loopback: bool) -> Result<Self, String> {
        unsafe {
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(err("Audio device"))?;
            let fmt = client.GetMixFormat().map_err(err("Audio format"))?;
            let f = fmt.read_unaligned();
            let (tag, channels, rate, bits) = (f.wFormatTag, f.nChannels, f.nSamplesPerSec, f.wBitsPerSample);
            // Extensible formats carry the real sample type in the sub-format; 3 is IEEE float.
            let float = tag == 3 || (tag == 0xFFFE && (fmt as *const WAVEFORMATEXTENSIBLE).read_unaligned().SubFormat.data1 == 3);
            let flags = if loopback { AUDCLNT_STREAMFLAGS_LOOPBACK } else { 0 };
            let init = client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 10_000_000, 0, fmt as *const WAVEFORMATEX, None);
            CoTaskMemFree(Some(fmt as *const c_void));
            init.map_err(err("Audio start"))?;
            let capture: IAudioCaptureClient = client.GetService().map_err(err("Audio capture"))?;
            client.Start().map_err(err("Audio start"))?;
            Ok(Self {
                client,
                capture,
                channels: usize::from(channels),
                bits,
                float,
                resampler: Resampler::new(rate, SAMPLE_RATE),
                out: Vec::new(),
            })
        }
    }

    /// Takes everything the device has ready. An error means the device went away.
    fn poll(&mut self) -> Result<(), String> {
        unsafe {
            loop {
                if self.capture.GetNextPacketSize().map_err(err("Audio read"))? == 0 {
                    break;
                }
                let (mut data, mut frames, mut flags) = (ptr::null_mut(), 0u32, 0u32);
                self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None).map_err(err("Audio read"))?;
                let stereo = if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                    vec![(0.0, 0.0); frames as usize]
                } else {
                    let len = frames as usize * self.channels * usize::from(self.bits / 8);
                    decode_frames(std::slice::from_raw_parts(data, len), self.channels, self.bits, self.float)
                };
                self.capture.ReleaseBuffer(frames).map_err(err("Audio read"))?;
                self.resampler.push(&stereo, &mut self.out);
            }
        }
        // A source running ahead of the clock for a long time would only add delay.
        let limit = SAMPLE_RATE as usize / 2;
        if self.out.len() > limit {
            self.out.drain(..self.out.len() - limit);
        }
        Ok(())
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}

pub struct Streams {
    pub system: Option<Source>,
    pub mic: Option<Source>,
    /// What was asked for but could not be opened, for a notice.
    pub warning: Option<String>,
}

// The streams are opened on one thread and then used only by the audio thread; both are in the
// multithreaded COM apartment, where the interfaces may be called from any thread.
unsafe impl Send for Streams {}

impl Streams {
    pub fn any(&self) -> bool {
        self.system.is_some() || self.mic.is_some()
    }

    pub fn track_count(&self) -> u8 {
        track_count(self.system.is_some(), self.mic.is_some())
    }
}

/// Opens system sound and/or the microphone. `mic_id` is a capture device id, or `None` for the default.
/// The calling thread must be in the multithreaded COM apartment.
pub fn open(system: bool, mic: bool, mic_id: Option<&str>) -> Streams {
    let none = |warning: &str| Streams { system: None, mic: None, warning: Some(warning.to_string()) };
    unsafe {
        let en: IMMDeviceEnumerator = match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
            Ok(e) => e,
            Err(_) => return none("No sound was recorded: audio devices are not available"),
        };
        let system_src = system.then(|| en.GetDefaultAudioEndpoint(eRender, eConsole).ok().and_then(|d| Source::open(&d, true).ok())).flatten();
        let mic_src = mic
            .then(|| {
                let chosen = mic_id.and_then(|id| {
                    let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
                    en.GetDevice(PCWSTR(wide.as_ptr())).ok()
                });
                chosen
                    .or_else(|| en.GetDefaultAudioEndpoint(eCapture, eConsole).ok())
                    .and_then(|d| Source::open(&d, false).ok())
            })
            .flatten();
        let warning = match (system && system_src.is_none(), mic && mic_src.is_none()) {
            (true, true) => Some("No sound was recorded: system sound and the microphone are not available"),
            (true, false) => Some("System sound is not available, so it was not recorded"),
            (false, true) => Some("The microphone is not available (check the Windows microphone privacy setting)"),
            (false, false) => None,
        };
        Streams { system: system_src, mic: mic_src, warning: warning.map(str::to_string) }
    }
}

/// Writes sound until `stop` is set, then writes what is left. Runs on its own thread.
pub fn spawn(streams: Streams, session: Arc<Session>, stop: Arc<AtomicBool>, tx: SyncSender<Msg>) -> Option<JoinHandle<()>> {
    thread::Builder::new().name("notifi-recording-audio".into()).spawn(move || run(streams, &session, &stop, &tx)).ok()
}

fn run(mut s: Streams, session: &Session, stop: &AtomicBool, tx: &SyncSender<Msg>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let mut written = 0u64;
    // Fixed at the start: a source that drops out later leaves its track silent instead of moving the other.
    let split = track_count(s.system.is_some(), s.mic.is_some()) == 2;
    loop {
        let stopping = stop.load(Ordering::SeqCst);
        for slot in [&mut s.system, &mut s.mic] {
            if slot.as_mut().is_some_and(|src| src.poll().is_err()) {
                // The device changed or went away; keep recording without it.
                *slot = None;
            }
        }
        let (elapsed, paused) = {
            let clock = session.clock.lock().unwrap();
            (clock.elapsed(Instant::now()), clock.is_paused())
        };
        if paused {
            for src in [&mut s.system, &mut s.mic].into_iter().flatten() {
                src.out.clear();
            }
        } else {
            let due = audio_frames_due(elapsed, written) as usize;
            if due >= 960 || (stopping && due > 0) {
                let mut take = |slot: &mut Option<Source>| -> Vec<Stereo> {
                    slot.as_mut().map_or_else(Vec::new, |src| {
                        let n = due.min(src.out.len());
                        src.out.drain(..n).collect()
                    })
                };
                let system = take(&mut s.system);
                let mic = take(&mut s.mic);
                let mut pcm = Vec::with_capacity(due * 2);
                let second = if split {
                    let mut mic_pcm = Vec::with_capacity(due * 2);
                    mix_pcm(&system, &[], due, &mut pcm);
                    mix_pcm(&[], &mic, due, &mut mic_pcm);
                    Some(mic_pcm)
                } else {
                    mix_pcm(&system, &mic, due, &mut pcm);
                    None
                };
                let ts = (u128::from(written) * 10_000_000 / u128::from(SAMPLE_RATE)) as i64;
                if tx.send(Msg::Audio { pcm, second, ts }).is_err() {
                    break;
                }
                written += due as u64;
            }
        }
        if stopping {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    drop(s);
    unsafe {
        CoUninitialize();
    }
}
