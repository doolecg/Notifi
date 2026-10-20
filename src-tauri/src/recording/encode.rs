//! Writes the recording: H.264 and AAC in an MP4 through the Media Foundation sink writer
//! (hardware encoders where the machine has one), or an animated GIF.

use std::{
    fs::File,
    io::BufWriter,
    path::PathBuf,
    sync::mpsc::{channel, sync_channel, Receiver, Sender, SyncSender},
    thread::{self, JoinHandle},
};

use windows::{
    core::HSTRING,
    Win32::{
        Media::MediaFoundation::{
            IMFAttributes, IMFByteStream, IMFMediaType, IMFSinkWriter, MFAudioFormat_AAC, MFAudioFormat_PCM, MFCreateAttributes,
            MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFCreateSinkWriterFromURL, MFMediaType_Audio, MFMediaType_Video,
            MFShutdown, MFStartup, MFVideoFormat_H264, MFVideoFormat_NV12, MFVideoInterlace_Progressive, MFSTARTUP_FULL,
            MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, MF_MT_AAC_PAYLOAD_TYPE, MF_MT_ALL_SAMPLES_INDEPENDENT,
            MF_MT_AUDIO_AVG_BYTES_PER_SECOND, MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS,
            MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE,
            MF_MT_MAJOR_TYPE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS,
            MF_SINK_WRITER_DISABLE_THROTTLING, MF_VERSION,
        },
        System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
    },
};

use super::pure::*;

pub struct Config {
    pub path: PathBuf,
    pub w: u32,
    pub h: u32,
    pub fps: u32,
    pub bitrate: u32,
    pub audio: bool,
    pub gif: bool,
}

pub enum Msg {
    /// A finished BGRA frame and its time in 100 ns units.
    Video { bgra: Vec<u8>, ts: i64 },
    /// Interleaved 48 kHz stereo 16-bit sound and its time in 100 ns units.
    Audio { pcm: Vec<i16>, ts: i64 },
}

pub struct Encoder {
    pub tx: SyncSender<Msg>,
    handle: JoinHandle<Result<(), String>>,
}

impl Encoder {
    /// Opens the output and returns once it is ready to take frames, or says why it cannot.
    pub fn start(cfg: Config) -> Result<Self, String> {
        let (tx, rx) = sync_channel(8);
        let (ready_tx, ready_rx) = channel();
        let handle = thread::Builder::new()
            .name("notifi-recording-encode".into())
            .spawn(move || if cfg.gif { run_gif(&cfg, &rx, &ready_tx) } else { run_mf(&cfg, &rx, &ready_tx) })
            .map_err(|e| e.to_string())?;
        ready_rx.recv().map_err(|_| "The encoder stopped before it started".to_string())??;
        Ok(Self { tx, handle })
    }

    /// Writes what is queued and closes the file. Every clone of `tx` must be dropped first.
    pub fn finish(self) -> Result<(), String> {
        drop(self.tx);
        self.handle.join().map_err(|_| "The encoder crashed".to_string())?
    }
}

fn mf_err(what: &'static str) -> impl Fn(windows::core::Error) -> String {
    move |e| format!("{what}: {e}")
}

fn pack(hi: u32, lo: u32) -> u64 {
    (u64::from(hi) << 32) | u64::from(lo)
}

pub(super) struct Writer {
    pub(super) writer: IMFSinkWriter,
    pub(super) video: u32,
    pub(super) audio: Option<u32>,
}

fn run_mf(cfg: &Config, rx: &Receiver<Msg>, ready: &Sender<Result<(), String>>) -> Result<(), String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let started = unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }.map_err(mf_err("Media Foundation"));
    let result = started.and_then(|()| {
        let w = match open_writer(cfg) {
            Ok(w) => {
                let _ = ready.send(Ok(()));
                w
            }
            Err(e) => {
                let _ = ready.send(Err(e.clone()));
                return Err(e);
            }
        };
        let r = write_all(cfg, &w, rx);
        let f = unsafe { w.writer.Finalize() }.map_err(mf_err("Finishing the file"));
        r.and(f)
    });
    if result.is_err() {
        let _ = ready.send(result.clone());
    }
    unsafe {
        let _ = MFShutdown();
        CoUninitialize();
    }
    result
}

fn write_all(cfg: &Config, w: &Writer, rx: &Receiver<Msg>) -> Result<(), String> {
    let frame_len = 10_000_000 / i64::from(cfg.fps.max(1));
    let mut nv12 = Vec::new();
    for msg in rx.iter() {
        match msg {
            Msg::Video { bgra, ts } => {
                bgra_to_nv12(&bgra, cfg.w as usize, cfg.h as usize, &mut nv12);
                write_sample(&w.writer, w.video, &nv12, ts, frame_len)?;
            }
            Msg::Audio { pcm, ts } => {
                if let Some(stream) = w.audio {
                    let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
                    let len = (pcm.len() as i64 / 2) * 10_000_000 / i64::from(SAMPLE_RATE);
                    write_sample(&w.writer, stream, &bytes, ts, len)?;
                }
            }
        }
    }
    Ok(())
}

fn write_sample(writer: &IMFSinkWriter, stream: u32, data: &[u8], ts: i64, duration: i64) -> Result<(), String> {
    unsafe {
        let buffer = MFCreateMemoryBuffer(data.len() as u32).map_err(mf_err("Frame buffer"))?;
        let mut dst = std::ptr::null_mut();
        buffer.Lock(&mut dst, None, None).map_err(mf_err("Frame buffer"))?;
        std::ptr::copy_nonoverlapping(data.as_ptr(), dst, data.len());
        buffer.Unlock().map_err(mf_err("Frame buffer"))?;
        buffer.SetCurrentLength(data.len() as u32).map_err(mf_err("Frame buffer"))?;
        let sample = MFCreateSample().map_err(mf_err("Frame"))?;
        sample.AddBuffer(&buffer).map_err(mf_err("Frame"))?;
        sample.SetSampleTime(ts).map_err(mf_err("Frame"))?;
        sample.SetSampleDuration(duration).map_err(mf_err("Frame"))?;
        writer.WriteSample(stream, &sample).map_err(mf_err("Writing the video"))
    }
}

unsafe fn video_type(subtype: &windows::core::GUID, cfg: &Config) -> Result<IMFMediaType, String> {
    let t = MFCreateMediaType().map_err(mf_err("Video format"))?;
    let e = mf_err("Video format");
    t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(&e)?;
    t.SetGUID(&MF_MT_SUBTYPE, subtype).map_err(&e)?;
    t.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).map_err(&e)?;
    t.SetUINT64(&MF_MT_FRAME_SIZE, pack(cfg.w, cfg.h)).map_err(&e)?;
    t.SetUINT64(&MF_MT_FRAME_RATE, pack(cfg.fps, 1)).map_err(&e)?;
    t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1)).map_err(&e)?;
    Ok(t)
}

unsafe fn audio_type(subtype: &windows::core::GUID, bytes_per_second: u32, block_align: u32) -> Result<IMFMediaType, String> {
    let t = MFCreateMediaType().map_err(mf_err("Audio format"))?;
    let e = mf_err("Audio format");
    t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(&e)?;
    t.SetGUID(&MF_MT_SUBTYPE, subtype).map_err(&e)?;
    t.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2).map_err(&e)?;
    t.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, SAMPLE_RATE).map_err(&e)?;
    t.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).map_err(&e)?;
    t.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, bytes_per_second).map_err(&e)?;
    t.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, block_align).map_err(&e)?;
    Ok(t)
}

pub(super) fn open_writer(cfg: &Config) -> Result<Writer, String> {
    unsafe {
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 2).map_err(mf_err("Media Foundation"))?;
        let attrs = attrs.ok_or("Media Foundation gave no settings object")?;
        attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1).map_err(mf_err("Media Foundation"))?;
        attrs.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1).map_err(mf_err("Media Foundation"))?;
        let url = HSTRING::from(cfg.path.to_string_lossy().as_ref());
        let writer = MFCreateSinkWriterFromURL(&url, None::<&IMFByteStream>, &attrs).map_err(mf_err("Creating the file"))?;

        let out = video_type(&MFVideoFormat_H264, cfg)?;
        out.SetUINT32(&MF_MT_AVG_BITRATE, cfg.bitrate).map_err(mf_err("Video format"))?;
        let video = writer.AddStream(&out).map_err(mf_err("This computer has no H.264 encoder"))?;
        let input = video_type(&MFVideoFormat_NV12, cfg)?;
        writer.SetInputMediaType(video, &input, None::<&IMFAttributes>).map_err(mf_err("Video input"))?;

        let audio = if cfg.audio {
            // 24000 bytes a second is 192 kbit/s AAC, one of the rates the Windows encoder takes.
            let out = audio_type(&MFAudioFormat_AAC, 24_000, 1)?;
            out.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0).map_err(mf_err("Audio format"))?;
            out.SetUINT32(&MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, 0x29).map_err(mf_err("Audio format"))?;
            let stream = writer.AddStream(&out).map_err(mf_err("This computer has no AAC encoder"))?;
            let input = audio_type(&MFAudioFormat_PCM, SAMPLE_RATE * 4, 4)?;
            input.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1).map_err(mf_err("Audio format"))?;
            writer.SetInputMediaType(stream, &input, None::<&IMFAttributes>).map_err(mf_err("Audio input"))?;
            Some(stream)
        } else {
            None
        };
        writer.BeginWriting().map_err(mf_err("Starting the file"))?;
        Ok(Writer { writer, video, audio })
    }
}

/// GIFs are capped at this width and 15 frames a second to keep the file a sane size.
const GIF_MAX_WIDTH: u32 = 960;
const GIF_MAX_FPS: u32 = 15;

fn run_gif(cfg: &Config, rx: &Receiver<Msg>, ready: &Sender<Result<(), String>>) -> Result<(), String> {
    let open = || -> Result<(gif::Encoder<BufWriter<File>>, u32, u32), String> {
        let (gw, gh) = gif_dims(cfg.w, cfg.h, GIF_MAX_WIDTH);
        let file = File::create(&cfg.path).map_err(|e| format!("Creating the file: {e}"))?;
        let mut enc = gif::Encoder::new(BufWriter::new(file), gw as u16, gh as u16, &[]).map_err(|e| e.to_string())?;
        enc.set_repeat(gif::Repeat::Infinite).map_err(|e| e.to_string())?;
        Ok((enc, gw, gh))
    };
    let (mut enc, gw, gh) = match open() {
        Ok(v) => {
            let _ = ready.send(Ok(()));
            v
        }
        Err(e) => {
            let _ = ready.send(Err(e.clone()));
            return Err(e);
        }
    };
    let interval = 10_000_000 / i64::from(cfg.fps.min(GIF_MAX_FPS).max(1));
    let mut next_ts = 0i64;
    let mut pending: Option<(Vec<u8>, i64)> = None;
    let put = |enc: &mut gif::Encoder<BufWriter<File>>, mut rgba: Vec<u8>, delay: u16| -> Result<(), String> {
        let mut frame = gif::Frame::from_rgba_speed(gw as u16, gh as u16, &mut rgba, 10);
        frame.delay = delay;
        enc.write_frame(&frame).map_err(|e| e.to_string())
    };
    for msg in rx.iter() {
        let Msg::Video { bgra, ts } = msg else { continue };
        if ts < next_ts {
            continue;
        }
        next_ts = ts + interval;
        let rgba = bgra_to_rgba_scaled(&bgra, cfg.w as usize, cfg.h as usize, gw as usize, gh as usize);
        if let Some((prev, prev_ts)) = pending.replace((rgba, ts)) {
            put(&mut enc, prev, gif_delay_cs(prev_ts, ts))?;
        }
    }
    if let Some((last, _)) = pending {
        put(&mut enc, last, 10)?;
    }
    Ok(())
}
