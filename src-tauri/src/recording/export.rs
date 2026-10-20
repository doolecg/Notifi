//! Renders the edited copy of an MP4 recording: the sound tracks are read first and edited as a
//! whole, then the video is decoded frame by frame, the effects are drawn into each output frame and
//! the result goes through the same encoder a recording uses.

use std::path::Path;

use windows::{
    core::HSTRING,
    Win32::{
        Media::MediaFoundation::{
            IMFAttributes, IMFSample, IMFSourceReader, MFAudioFormat_PCM, MFCreateAttributes, MFCreateMediaType, MFCreateSourceReaderFromURL,
            MFMediaType_Audio, MFMediaType_Video, MFShutdown, MFStartup, MFVideoFormat_RGB32, MFSTARTUP_FULL, MF_MT_AUDIO_BITS_PER_SAMPLE,
            MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
            MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READER_ALL_STREAMS, MF_SOURCE_READER_ANY_STREAM,
            MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, MF_VERSION,
        },
        System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
    },
};

use super::{
    edit::{build_track, chunk, decode_labels, render_frame, rows_to_bgra, EditSpec, FrameMapper, LabelImage, TrackBuf},
    encode::{Config, Encoder, Msg},
    pure::{bitrate, frame_ts, SAMPLE_RATE},
};

/// Sound is sent this far ahead of the picture so it never runs late.
const AUDIO_LEAD_MS: f64 = 250.0;
/// Frames of sound in one message to the encoder.
const AUDIO_CHUNK: usize = 4800;
/// Reads that return nothing in a row before the recording is taken to be stuck.
const STALL_READS: u32 = 100_000;

fn err(what: &'static str) -> impl Fn(windows::core::Error) -> String {
    move |e| format!("{what}: {e}")
}

/// Writes the edited video to `dst` (a new file) from `src` (never changed).
pub fn render_mp4(src: &Path, dst: &Path, spec: &EditSpec) -> Result<(), String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let started = unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }.map_err(err("Media Foundation"));
    let result = started.and_then(|()| render(src, dst, spec));
    unsafe {
        let _ = MFShutdown();
        CoUninitialize();
    }
    result
}

fn open_reader(src: &Path) -> Result<IMFSourceReader, String> {
    unsafe {
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 1).map_err(err("Media Foundation"))?;
        let attrs = attrs.ok_or("Media Foundation gave no settings object")?;
        attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1).map_err(err("Media Foundation"))?;
        let url = HSTRING::from(src.to_string_lossy().as_ref());
        let reader: IMFSourceReader = MFCreateSourceReaderFromURL(&url, &attrs).map_err(err("Opening the recording"))?;
        reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false).map_err(err("Opening the recording"))?;
        Ok(reader)
    }
}

/// The reader's stream numbers of the first video stream and of every sound stream.
fn find_streams(reader: &IMFSourceReader) -> (Option<u32>, Vec<u32>) {
    let (mut video, mut sound) = (None, Vec::new());
    for index in 0..64u32 {
        let Ok(kind) = (unsafe { reader.GetNativeMediaType(index, 0).and_then(|t| t.GetGUID(&MF_MT_MAJOR_TYPE)) }) else { break };
        if kind == MFMediaType_Video && video.is_none() {
            video = Some(index);
        } else if kind == MFMediaType_Audio {
            sound.push(index);
        }
    }
    (video, sound)
}

unsafe fn sample_bytes(sample: &IMFSample) -> Result<Vec<u8>, String> {
    let buffer = sample.ConvertToContiguousBuffer().map_err(err("Reading the recording"))?;
    let (mut data, mut len) = (std::ptr::null_mut::<u8>(), 0u32);
    buffer.Lock(&mut data, None, Some(&mut len)).map_err(err("Reading the recording"))?;
    let bytes = std::slice::from_raw_parts(data, len as usize).to_vec();
    let _ = buffer.Unlock();
    Ok(bytes)
}

/// Every sound track of the file as 48 kHz stereo, in file order. A track that cannot be decoded is left out.
fn read_sound(src: &Path, streams: &[u32]) -> Result<Vec<TrackBuf>, String> {
    if streams.is_empty() {
        return Ok(Vec::new());
    }
    let reader = open_reader(src)?;
    let mut active: Vec<u32> = Vec::new();
    for &index in streams {
        let ok = unsafe {
            reader.SetStreamSelection(index, true).is_ok() && {
                let pcm = MFCreateMediaType().map_err(err("Audio format"))?;
                pcm.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(err("Audio format"))?;
                pcm.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).map_err(err("Audio format"))?;
                pcm.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2).map_err(err("Audio format"))?;
                pcm.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, SAMPLE_RATE).map_err(err("Audio format"))?;
                pcm.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).map_err(err("Audio format"))?;
                reader.SetCurrentMediaType(index, None, &pcm).is_ok()
            }
        };
        if ok {
            active.push(index);
        } else {
            unsafe { reader.SetStreamSelection(index, false) }.ok();
        }
    }
    let mut tracks: Vec<TrackBuf> = active.iter().map(|_| TrackBuf::default()).collect();
    let mut ended = vec![false; active.len()];
    let mut empty = 0u32;
    while !ended.iter().all(|e| *e) {
        let (mut index, mut flags, mut stamp) = (0u32, 0u32, 0i64);
        let mut sample: Option<IMFSample> = None;
        unsafe { reader.ReadSample(MF_SOURCE_READER_ANY_STREAM.0 as u32, 0, Some(&mut index), Some(&mut flags), Some(&mut stamp), Some(&mut sample)) }
            .map_err(err("Reading the recording"))?;
        let Some(which) = active.iter().position(|a| *a == index) else {
            empty += 1;
            if empty > STALL_READS {
                return Err("The recording stopped giving sound.".into());
            }
            continue;
        };
        if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
            ended[which] = true;
        }
        let Some(sample) = sample else {
            empty += 1;
            if empty > STALL_READS {
                return Err("The recording stopped giving sound.".into());
            }
            continue;
        };
        empty = 0;
        let bytes = unsafe { sample_bytes(&sample) }?;
        let pcm: Vec<i16> = bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        tracks[which].place(stamp, &pcm);
    }
    Ok(tracks)
}

/// Sends frames to the encoder, with the edited sound kept just ahead of them.
struct Out<'a> {
    tx: std::sync::mpsc::SyncSender<Msg>,
    spec: &'a EditSpec,
    labels: Vec<LabelImage>,
    sound: Vec<Vec<i16>>,
    sound_frames: usize,
    sent: usize,
    w: usize,
    h: usize,
    fps: u32,
    frames: u32,
    scratch: Vec<u8>,
}

impl Out<'_> {
    fn gone(_: std::sync::mpsc::SendError<Msg>) -> String {
        "The encoder stopped unexpectedly".to_string()
    }

    /// Draws `base` once for each of `shown` (output frame number and original time) and sends it.
    fn show(&mut self, base: &[u8], shown: &[(u64, f64)]) -> Result<(), String> {
        for &(index, src_ms) in shown {
            let mut frame = base.to_vec();
            render_frame(self.spec, &self.labels, &mut frame, &mut self.scratch, self.w, self.h, src_ms);
            self.tx.send(Msg::Video { bgra: frame, ts: frame_ts(index, self.fps) }).map_err(Self::gone)?;
            self.frames += 1;
            self.sound_to((index + 1) as f64 * 1000.0 / f64::from(self.fps) + AUDIO_LEAD_MS)?;
        }
        Ok(())
    }

    /// Sends the sound up to `out_ms` of the edited video.
    fn sound_to(&mut self, out_ms: f64) -> Result<(), String> {
        if self.sound.is_empty() {
            return Ok(());
        }
        let target = ((out_ms * f64::from(SAMPLE_RATE) / 1000.0) as usize).min(self.sound_frames);
        while self.sent < target {
            let n = (target - self.sent).min(AUDIO_CHUNK);
            let pcm = chunk(&self.sound[0], self.sent, n);
            let second = self.sound.get(1).map(|t| chunk(t, self.sent, n));
            let ts = (self.sent as u128 * 10_000_000 / u128::from(SAMPLE_RATE)) as i64;
            self.tx.send(Msg::Audio { pcm, second, ts }).map_err(Self::gone)?;
            self.sent += n;
        }
        Ok(())
    }
}

fn render(src: &Path, dst: &Path, spec: &EditSpec) -> Result<(), String> {
    let (video_index, sound_streams) = find_streams(&open_reader(src)?);
    let video_index = video_index.ok_or("The recording has no video")?;

    // Sound first: a track with volume 0 is dropped, and a file holds at most two.
    let raw = read_sound(src, &sound_streams)?;
    let sound: Vec<Vec<i16>> = raw
        .iter()
        .enumerate()
        .filter(|(i, _)| spec.volumes.get(*i).copied().unwrap_or(1.0) > 0.001)
        .take(2)
        .map(|(i, t)| build_track(t, &spec.segments, spec.volumes.get(i).copied().unwrap_or(1.0)))
        .collect();
    drop(raw);
    let sound_frames = sound.iter().map(|t| t.len() / 2).max().unwrap_or(0);

    let reader = open_reader(src)?;
    unsafe { reader.SetStreamSelection(video_index, true) }.map_err(|_| "The recording has no video".to_string())?;
    let native = unsafe { reader.GetNativeMediaType(video_index, 0) }.map_err(err("Reading the video format"))?;
    let size = unsafe { native.GetUINT64(&MF_MT_FRAME_SIZE) }.map_err(err("Reading the video size"))?;
    let rate = unsafe { native.GetUINT64(&MF_MT_FRAME_RATE) }.unwrap_or((30u64 << 32) | 1);
    let (w, h) = ((size >> 32) as u32, size as u32);
    let fps = (((rate >> 32) as f64) / f64::from((rate as u32).max(1))).round().clamp(1.0, 120.0) as u32;

    let want = unsafe { MFCreateMediaType() }.map_err(err("Video format"))?;
    unsafe {
        want.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(err("Video format"))?;
        want.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).map_err(err("Video format"))?;
        // A positive stride asks for the picture top-down.
        want.SetUINT32(&MF_MT_DEFAULT_STRIDE, w * 4).map_err(err("Video format"))?;
        reader.SetCurrentMediaType(video_index, None, &want).map_err(err("Reading the video"))?;
    }
    let stride = unsafe { reader.GetCurrentMediaType(video_index).and_then(|t| t.GetUINT32(&MF_MT_DEFAULT_STRIDE)) }
        .map_or((w * 4) as i32, |v| v as i32);

    let labels = decode_labels(spec)?;
    let enc = Encoder::start(Config {
        path: dst.to_path_buf(),
        w,
        h,
        fps,
        bitrate: bitrate("balanced", w, h, fps),
        audio_tracks: sound.len() as u8,
        gif: false,
    })?;
    let mut out = Out { tx: enc.tx.clone(), spec, labels, sound, sound_frames, sent: 0, w: w as usize, h: h as usize, fps, frames: 0, scratch: Vec::new() };

    let piped = pump(&reader, video_index, stride, &mut out);
    let flushed = piped.and_then(|()| out.sound_to(f64::MAX));
    let frames = out.frames;
    drop(out);
    let finished = enc.finish();
    flushed.and(finished)?;
    if frames == 0 {
        return Err("No frames are left in that part.".into());
    }
    Ok(())
}

/// Decodes the video in order and hands each source frame to the output frames that show it.
fn pump(reader: &IMFSourceReader, video_index: u32, stride: i32, out: &mut Out) -> Result<(), String> {
    let spec = out.spec;
    let mut mapper = FrameMapper::new(&spec.segments, out.fps);
    // A frame is shown until the next one arrives, so it waits here for its end time.
    let mut held: Option<Vec<u8>> = None;
    let mut empty = 0u32;
    while !mapper.done() {
        let (mut flags, mut stamp) = (0u32, 0i64);
        let mut sample: Option<IMFSample> = None;
        unsafe { reader.ReadSample(video_index, 0, None, Some(&mut flags), Some(&mut stamp), Some(&mut sample)) }.map_err(err("Reading the recording"))?;
        let ended = flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0;
        if let Some(sample) = sample {
            empty = 0;
            let bytes = unsafe { sample_bytes(&sample) }?;
            let mut bgra = Vec::new();
            if !rows_to_bgra(&bytes, stride, out.w, out.h, &mut bgra) {
                return Err("The recording has a picture format Notifi cannot read.".into());
            }
            if let Some(frame) = held.take() {
                let shown = mapper.take_until(stamp as f64 / 10_000.0);
                out.show(&frame, &shown)?;
            }
            held = Some(bgra);
        } else if !ended {
            empty += 1;
            if empty > STALL_READS {
                return Err("The recording stopped giving frames.".into());
            }
        }
        if ended {
            break;
        }
    }
    if let Some(frame) = held {
        let shown = mapper.take_until(f64::INFINITY);
        out.show(&frame, &shown)?;
    }
    Ok(())
}
