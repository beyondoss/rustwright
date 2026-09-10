//! Page video capture helpers: JPEG frame journals and container writers.
//!
//! Chromium already emits JPEG frames through `Page.startScreencast`. This
//! module keeps those frames off the CDP event log and muxes them without
//! ffmpeg. The default container is VP8 WebM (a real video file). `.gif` is
//! an explicit image attachment; `.avi` dumps the source JPEGs as MJPEG.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Frame, ImageFormat};
use oxideav_vp8::encoder::encode_vp8_keyframe;
use oxideav_vp8::Vp8Frame;

use tempfile::NamedTempFile;

use crate::{RwError, RwResult};

pub const DEFAULT_VIDEO_QUALITY: u32 = 80;
pub const DEFAULT_VIDEO_MAX_WIDTH: u32 = 1280;
pub const DEFAULT_VIDEO_EVERY_NTH_FRAME: u32 = 1;
pub const MAX_VIDEO_FRAMES: u32 = 3_600;
pub const MAX_VIDEO_JOURNAL_BYTES: u64 = 256 * 1024 * 1024;
const DEFAULT_FALLBACK_WIDTH: u32 = 1280;
const DEFAULT_FALLBACK_HEIGHT: u32 = 720;
/// VP8 quantizer (0 = best, 127 = worst). Fixed independently of the CDP JPEG
/// quality knob so agents are not asked to pick a codec setting.
const VP8_QINDEX: u8 = 36;
/// SimpleBlock timecodes are signed 16-bit offsets from the cluster timestamp.
const WEBM_CLUSTER_MAX_MS: u64 = 30_000;

/// Options accepted by [`crate::RustwrightPage::start_video`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoStartOptions {
    pub quality: u32,
    pub max_width: u32,
    pub every_nth_frame: u32,
}

impl Default for VideoStartOptions {
    fn default() -> Self {
        Self {
            quality: DEFAULT_VIDEO_QUALITY,
            max_width: DEFAULT_VIDEO_MAX_WIDTH,
            every_nth_frame: DEFAULT_VIDEO_EVERY_NTH_FRAME,
        }
    }
}

impl VideoStartOptions {
    pub fn from_optional(
        quality: Option<u32>,
        max_width: Option<u32>,
        every_nth_frame: Option<u32>,
    ) -> RwResult<Self> {
        let quality = quality.unwrap_or(DEFAULT_VIDEO_QUALITY);
        if quality == 0 || quality > 100 {
            return Err(RwError::InvalidInput(
                "video quality must be in 1..=100".to_string(),
            ));
        }
        let max_width = max_width.unwrap_or(DEFAULT_VIDEO_MAX_WIDTH);
        if max_width == 0 {
            return Err(RwError::InvalidInput(
                "video max_width must be greater than zero".to_string(),
            ));
        }
        let every_nth_frame = every_nth_frame.unwrap_or(DEFAULT_VIDEO_EVERY_NTH_FRAME);
        if every_nth_frame == 0 {
            return Err(RwError::InvalidInput(
                "video every_nth_frame must be greater than zero".to_string(),
            ));
        }
        Ok(Self {
            quality,
            max_width,
            every_nth_frame,
        })
    }
}

/// Finished page recording written to `path`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoRecording {
    pub path: String,
    pub bytes: u64,
    pub frames: u32,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub struct FrameJournal {
    file: NamedTempFile,
    writer: File,
    frames: u32,
    bytes: u64,
    first_ts_us: Option<u64>,
    last_ts_us: Option<u64>,
    width: u32,
    height: u32,
}

#[derive(Debug)]
pub struct FinishedJournal {
    file: NamedTempFile,
    frames: u32,
    first_ts_us: Option<u64>,
    last_ts_us: Option<u64>,
    width: u32,
    height: u32,
}

impl FrameJournal {
    pub fn create() -> RwResult<Self> {
        let file = NamedTempFile::new()
            .map_err(|error| RwError::Message(format!("video journal create failed: {error}")))?;
        let writer = File::options()
            .write(true)
            .open(file.path())
            .map_err(|error| RwError::Message(format!("video journal open failed: {error}")))?;
        Ok(Self {
            file,
            writer,
            frames: 0,
            bytes: 0,
            first_ts_us: None,
            last_ts_us: None,
            width: 0,
            height: 0,
        })
    }

    /// Append one JPEG frame. Returns `false` when the journal is at a cap and
    /// the frame was dropped (the caller should still ACK the screencast).
    pub fn push(&mut self, timestamp_us: u64, jpeg: &[u8]) -> RwResult<bool> {
        if jpeg.is_empty() {
            return Ok(true);
        }
        if self.frames >= MAX_VIDEO_FRAMES || self.bytes >= MAX_VIDEO_JOURNAL_BYTES {
            return Ok(false);
        }
        let framed = 8u64.saturating_add(4).saturating_add(jpeg.len() as u64);
        if self.bytes.saturating_add(framed) > MAX_VIDEO_JOURNAL_BYTES {
            return Ok(false);
        }
        if self.width == 0 || self.height == 0 {
            if let Some((width, height)) = jpeg_dimensions(jpeg) {
                self.width = width;
                self.height = height;
            }
        }
        self.writer
            .write_all(&timestamp_us.to_le_bytes())
            .and_then(|_| self.writer.write_all(&(jpeg.len() as u32).to_le_bytes()))
            .and_then(|_| self.writer.write_all(jpeg))
            .map_err(|error| RwError::Message(format!("video journal write failed: {error}")))?;
        self.frames = self.frames.saturating_add(1);
        self.bytes = self.bytes.saturating_add(framed);
        if self.first_ts_us.is_none() {
            self.first_ts_us = Some(timestamp_us);
        }
        self.last_ts_us = Some(timestamp_us);
        Ok(true)
    }

    pub fn finish(mut self) -> RwResult<FinishedJournal> {
        self.writer
            .flush()
            .map_err(|error| RwError::Message(format!("video journal flush failed: {error}")))?;
        drop(self.writer);
        Ok(FinishedJournal {
            file: self.file,
            frames: self.frames,
            first_ts_us: self.first_ts_us,
            last_ts_us: self.last_ts_us,
            width: self.width,
            height: self.height,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VideoContainer {
    Webm,
    Gif,
    Avi,
}

impl FinishedJournal {
    pub fn frames(&self) -> u32 {
        self.frames
    }

    pub fn duration_us(&self) -> u64 {
        match (self.first_ts_us, self.last_ts_us) {
            (Some(first), Some(last)) if last >= first => last - first,
            _ => 0,
        }
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (
            if self.width == 0 {
                DEFAULT_FALLBACK_WIDTH
            } else {
                self.width
            },
            if self.height == 0 {
                DEFAULT_FALLBACK_HEIGHT
            } else {
                self.height
            },
        )
    }
}

pub fn write_recording(
    journal: &FinishedJournal,
    output: impl AsRef<Path>,
) -> RwResult<VideoRecording> {
    let output = output.as_ref();
    match video_container(output)? {
        VideoContainer::Webm => write_webm(journal, output),
        VideoContainer::Gif => write_gif(journal, output),
        VideoContainer::Avi => write_mjpeg_avi(journal, output),
    }
}

fn video_container(output: &Path) -> RwResult<VideoContainer> {
    match output
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("webm") | None => Ok(VideoContainer::Webm),
        Some("gif") => Ok(VideoContainer::Gif),
        Some("avi") => Ok(VideoContainer::Avi),
        Some(other) => Err(RwError::InvalidInput(format!(
            "unsupported video extension .{other}; use .webm, .gif, or .avi"
        ))),
    }
}

fn prepare_video_output(journal: &FinishedJournal, output: &Path) -> RwResult<(u32, u32, u64, u32)> {
    if journal.frames == 0 {
        return Err(RwError::Message(
            "video recording captured no frames".to_string(),
        ));
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|error| {
                RwError::Message(format!("video output directory create failed: {error}"))
            })?;
        }
    }
    let (width, height) = journal.dimensions();
    let duration_us = journal.duration_us();
    let fps = recording_fps(journal.frames, duration_us);
    Ok((width, height, duration_us, fps))
}

fn finished_recording(
    output: &Path,
    journal: &FinishedJournal,
    duration_us: u64,
    fps: u32,
    width: u32,
    height: u32,
) -> RwResult<VideoRecording> {
    let metadata = fs::metadata(output)
        .map_err(|error| RwError::Message(format!("video output stat failed: {error}")))?;
    let path = output
        .to_str()
        .ok_or_else(|| RwError::Message("video output path is not valid UTF-8".to_string()))?
        .to_string();
    Ok(VideoRecording {
        path,
        bytes: metadata.len(),
        frames: journal.frames,
        duration_ms: duration_us.div_ceil(1000).max(if journal.frames <= 1 {
            1_000 / fps as u64
        } else {
            0
        }),
        width,
        height,
    })
}

fn write_webm(journal: &FinishedJournal, output: &Path) -> RwResult<VideoRecording> {
    let (width, height, duration_us, fps) = prepare_video_output(journal, output)?;
    let mut source = File::open(journal.file.path())
        .map_err(|error| RwError::Message(format!("video journal reopen failed: {error}")))?;
    let origin_us = journal.first_ts_us.unwrap_or(0);
    let mut frames = Vec::with_capacity(journal.frames as usize);
    let mut remaining = journal.frames;
    while remaining > 0 {
        let (timestamp_us, jpeg) = read_journal_frame(&mut source)?;
        let payload = encode_jpeg_vp8_keyframe(&jpeg, width, height)?;
        let ts_ms = timestamp_us.saturating_sub(origin_us) / 1000;
        frames.push((ts_ms, payload));
        remaining -= 1;
    }
    let bytes = mux_webm(width, height, &frames)?;
    fs::write(output, bytes)
        .map_err(|error| RwError::Message(format!("video webm write failed: {error}")))?;
    finished_recording(output, journal, duration_us, fps, width, height)
}

fn encode_jpeg_vp8_keyframe(jpeg: &[u8], width: u32, height: u32) -> RwResult<Vec<u8>> {
    let decoded = image::load_from_memory_with_format(jpeg, ImageFormat::Jpeg)
        .map_err(|error| RwError::Message(format!("video jpeg decode failed: {error}")))?;
    let mut rgb = decoded.to_rgb8();
    if rgb.width() != width || rgb.height() != height {
        rgb = image::imageops::resize(&rgb, width, height, image::imageops::FilterType::Triangle);
    }
    let frame = rgb_to_vp8_frame(&rgb, width, height);
    encode_vp8_keyframe(width, height, VP8_QINDEX, &frame)
        .map_err(|error| RwError::Message(format!("video vp8 encode failed: {error}")))
}

fn rgb_to_vp8_frame(rgb: &[u8], width: u32, height: u32) -> Vp8Frame {
    let w = width as usize;
    let h = height as usize;
    let chroma_w = w.div_ceil(2);
    let chroma_h = h.div_ceil(2);
    let mut y = vec![0_u8; w * h];
    let mut u = vec![0_u8; chroma_w * chroma_h];
    let mut v = vec![0_u8; chroma_w * chroma_h];
    for row in 0..h {
        for col in 0..w {
            let i = (row * w + col) * 3;
            let r = rgb[i] as i32;
            let g = rgb[i + 1] as i32;
            let b = rgb[i + 2] as i32;
            y[row * w + col] = ((66 * r + 129 * g + 25 * b + 128) >> 8).clamp(0, 255) as u8 + 16;
            if row % 2 == 0 && col % 2 == 0 {
                let chroma = (row / 2) * chroma_w + (col / 2);
                u[chroma] = ((-38 * r - 74 * g + 112 * b + 128) >> 8).clamp(0, 255) as u8 + 128;
                v[chroma] = ((112 * r - 94 * g - 18 * b + 128) >> 8).clamp(0, 255) as u8 + 128;
            }
        }
    }
    Vp8Frame {
        width,
        height,
        pts: None,
        y,
        u,
        v,
        y_stride: width,
        uv_stride: chroma_w as u32,
    }
}

fn mux_webm(width: u32, height: u32, frames: &[(u64, Vec<u8>)]) -> RwResult<Vec<u8>> {
    let mut ebml_body = Vec::new();
    ebml_body.extend(ebml_elem(&[0x42, 0x86], &[1])?);
    ebml_body.extend(ebml_elem(&[0x42, 0xF7], &[1])?);
    ebml_body.extend(ebml_elem(&[0x42, 0xF2], &[4])?);
    ebml_body.extend(ebml_elem(&[0x42, 0xF3], &[8])?);
    ebml_body.extend(ebml_elem(&[0x42, 0x82], b"webm")?);
    ebml_body.extend(ebml_elem(&[0x42, 0x87], &[4])?);
    ebml_body.extend(ebml_elem(&[0x42, 0x85], &[2])?);
    let ebml = ebml_elem(&[0x1A, 0x45, 0xDF, 0xA3], &ebml_body)?;

    let mut info = Vec::new();
    info.extend(ebml_elem(&[0x2A, 0xD7, 0xB1], &1_000_000u64.to_be_bytes()[4..])?);
    info.extend(ebml_elem(&[0x4D, 0x80], b"rustwright")?);
    info.extend(ebml_elem(&[0x57, 0x41], b"rustwright")?);
    let info = ebml_elem(&[0x15, 0x49, 0xA9, 0x66], &info)?;

    let mut video = Vec::new();
    video.extend(ebml_elem(
        &[0xB0],
        &u16::try_from(width).unwrap_or(u16::MAX).to_be_bytes(),
    )?);
    video.extend(ebml_elem(
        &[0xBA],
        &u16::try_from(height).unwrap_or(u16::MAX).to_be_bytes(),
    )?);
    let video = ebml_elem(&[0xE0], &video)?;

    let mut track = Vec::new();
    track.extend(ebml_elem(&[0xD7], &[1])?);
    track.extend(ebml_elem(&[0x73, 0xC5], &[1])?);
    track.extend(ebml_elem(&[0x83], &[1])?);
    track.extend(ebml_elem(&[0x86], b"V_VP8")?);
    track.extend(video);
    let tracks = ebml_elem(&[0x16, 0x54, 0xAE, 0x6B], &ebml_elem(&[0xAE], &track)?)?;

    let mut clusters = Vec::new();
    let mut cluster_origin = 0_u64;
    let mut cluster_body = Vec::new();
    cluster_body.extend(ebml_elem(&[0xE7], &ebml_uint(0))?);
    for (ts_ms, payload) in frames {
        if ts_ms.saturating_sub(cluster_origin) > WEBM_CLUSTER_MAX_MS {
            clusters.extend(ebml_elem(&[0x1F, 0x43, 0xB6, 0x75], &cluster_body)?);
            cluster_origin = *ts_ms;
            cluster_body.clear();
            cluster_body.extend(ebml_elem(&[0xE7], &ebml_uint(cluster_origin))?);
        }
        let rel = ts_ms.saturating_sub(cluster_origin).min(i16::MAX as u64) as u16;
        let mut block = Vec::with_capacity(4 + payload.len());
        block.push(0x81);
        block.extend(rel.to_be_bytes());
        block.push(0x80);
        block.extend(payload);
        cluster_body.extend(ebml_elem(&[0xA3], &block)?);
    }
    clusters.extend(ebml_elem(&[0x1F, 0x43, 0xB6, 0x75], &cluster_body)?);

    let mut segment = Vec::new();
    segment.extend(info);
    segment.extend(tracks);
    segment.extend(clusters);
    let segment = ebml_elem(&[0x18, 0x53, 0x80, 0x67], &segment)?;

    let mut out = ebml;
    out.extend(segment);
    Ok(out)
}

fn ebml_elem(id: &[u8], body: &[u8]) -> RwResult<Vec<u8>> {
    let mut out = Vec::with_capacity(id.len() + 8 + body.len());
    out.extend_from_slice(id);
    out.extend(ebml_vint(body.len() as u64)?);
    out.extend_from_slice(body);
    Ok(out)
}

fn ebml_uint(value: u64) -> Vec<u8> {
    if value == 0 {
        return vec![0];
    }
    let bytes = value.to_be_bytes();
    let skip = bytes.iter().position(|byte| *byte != 0).unwrap_or(7);
    bytes[skip..].to_vec()
}

fn ebml_vint(value: u64) -> RwResult<Vec<u8>> {
    if value < 0x80 {
        Ok(vec![0x80 | value as u8])
    } else if value < 0x4000 {
        Ok(vec![0x40 | ((value >> 8) as u8), value as u8])
    } else if value < 0x20_0000 {
        Ok(vec![
            0x20 | ((value >> 16) as u8),
            (value >> 8) as u8,
            value as u8,
        ])
    } else if value < 0x1000_0000 {
        Ok(vec![
            0x10 | ((value >> 24) as u8),
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
        ])
    } else if value < 0x8_0000_0000 {
        Ok(vec![
            0x08 | ((value >> 32) as u8),
            (value >> 24) as u8,
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
        ])
    } else {
        Err(RwError::Message(
            "video webm element exceeds supported EBML size".to_string(),
        ))
    }
}

pub fn write_mjpeg_avi(
    journal: &FinishedJournal,
    output: impl AsRef<Path>,
) -> RwResult<VideoRecording> {
    let output = output.as_ref();
    let (width, height, duration_us, fps) = prepare_video_output(journal, output)?;
    let micros_per_frame = 1_000_000 / fps;
    let mut source = File::open(journal.file.path())
        .map_err(|error| RwError::Message(format!("video journal reopen failed: {error}")))?;
    let mut dest = File::create(output)
        .map_err(|error| RwError::Message(format!("video output create failed: {error}")))?;
    dest.write_all(&[0_u8; AVI_HEADER_PREFIX])
        .map_err(avi_io_error)?;

    let mut index = Vec::with_capacity(journal.frames as usize);
    let mut max_frame = 0_u32;
    let mut movi_payload = 4_u32; // 'movi' fourcc
    let mut remaining = journal.frames;
    while remaining > 0 {
        let (timestamp_us, jpeg) = read_journal_frame(&mut source)?;
        let _ = timestamp_us;
        let frame_len = jpeg.len() as u32;
        max_frame = max_frame.max(frame_len);
        let padded = frame_len + (frame_len & 1);
        dest.write_all(b"00dc").map_err(avi_io_error)?;
        dest.write_all(&frame_len.to_le_bytes())
            .map_err(avi_io_error)?;
        dest.write_all(&jpeg).map_err(avi_io_error)?;
        if frame_len & 1 == 1 {
            dest.write_all(&[0]).map_err(avi_io_error)?;
        }
        index.push(AviIndexEntry {
            offset: movi_payload,
            size: frame_len,
        });
        movi_payload = movi_payload.saturating_add(8).saturating_add(padded);
        remaining -= 1;
    }

    dest.write_all(b"idx1").map_err(avi_io_error)?;
    dest.write_all(&(index.len() as u32 * 16).to_le_bytes())
        .map_err(avi_io_error)?;
    for entry in &index {
        dest.write_all(b"00dc").map_err(avi_io_error)?;
        dest.write_all(&0x10u32.to_le_bytes())
            .map_err(avi_io_error)?; // AVIIF_KEYFRAME
        dest.write_all(&entry.offset.to_le_bytes())
            .map_err(avi_io_error)?;
        dest.write_all(&entry.size.to_le_bytes())
            .map_err(avi_io_error)?;
    }

    let movi_list_size = movi_payload; // includes 'movi'
    let idx1_size = index.len() as u32 * 16;
    let riff_size = 4 // 'AVI '
        + AVI_HDRL_LIST_SIZE
        + 8
        + movi_list_size
        + 8
        + idx1_size;
    dest.seek(SeekFrom::Start(0)).map_err(avi_io_error)?;
    write_avi_header(
        &mut dest,
        riff_size,
        journal.frames,
        micros_per_frame,
        fps,
        max_frame,
        width,
        height,
        movi_list_size,
    )?;
    dest.flush().map_err(avi_io_error)?;
    drop(dest);
    finished_recording(output, journal, duration_us, fps, width, height)
}

fn write_gif(journal: &FinishedJournal, output: &Path) -> RwResult<VideoRecording> {
    let (width, height, duration_us, fps) = prepare_video_output(journal, output)?;
    let delay = Delay::from_saturating_duration(Duration::from_millis(
        u64::from(1_000 / fps.max(1)).max(10),
    ));
    let mut source = File::open(journal.file.path())
        .map_err(|error| RwError::Message(format!("video journal reopen failed: {error}")))?;
    let dest = File::create(output)
        .map_err(|error| RwError::Message(format!("video output create failed: {error}")))?;
    let mut encoder = GifEncoder::new_with_speed(dest, 10);
    encoder
        .set_repeat(Repeat::Infinite)
        .map_err(|error| RwError::Message(format!("video gif header failed: {error}")))?;

    let mut remaining = journal.frames;
    while remaining > 0 {
        let (_timestamp_us, jpeg) = read_journal_frame(&mut source)?;
        let decoded = image::load_from_memory_with_format(&jpeg, ImageFormat::Jpeg)
            .map_err(|error| RwError::Message(format!("video jpeg decode failed: {error}")))?;
        let mut rgba = decoded.to_rgba8();
        if rgba.width() != width || rgba.height() != height {
            rgba = image::imageops::resize(
                &rgba,
                width,
                height,
                image::imageops::FilterType::Triangle,
            );
        }
        encoder
            .encode_frame(Frame::from_parts(rgba, 0, 0, delay))
            .map_err(|error| RwError::Message(format!("video gif frame encode failed: {error}")))?;
        remaining -= 1;
    }
    drop(encoder);
    finished_recording(output, journal, duration_us, fps, width, height)
}

pub fn jpeg_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    let mut index = 0;
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    index += 2;
    while index + 3 < data.len() {
        if data[index] != 0xFF {
            index += 1;
            continue;
        }
        while index < data.len() && data[index] == 0xFF {
            index += 1;
        }
        if index >= data.len() {
            break;
        }
        let marker = data[index];
        index += 1;
        if marker == 0xD8 || marker == 0xD9 || (0xD0..=0xD7).contains(&marker) {
            continue;
        }
        if index + 1 >= data.len() {
            break;
        }
        let length = u16::from_be_bytes([data[index], data[index + 1]]) as usize;
        if length < 2 || index + length > data.len() {
            break;
        }
        // SOF0 / SOF1 / SOF2
        if matches!(marker, 0xC0 | 0xC1 | 0xC2) && length >= 7 {
            let height = u16::from_be_bytes([data[index + 3], data[index + 4]]) as u32;
            let width = u16::from_be_bytes([data[index + 5], data[index + 6]]) as u32;
            if width > 0 && height > 0 {
                return Some((width, height));
            }
        }
        index += length;
    }
    None
}

fn recording_fps(frames: u32, duration_us: u64) -> u32 {
    if frames <= 1 || duration_us == 0 {
        return 10;
    }
    let fps = f64::from(frames - 1) * 1_000_000.0 / duration_us as f64;
    fps.round().clamp(1.0, 60.0) as u32
}

fn read_journal_frame(source: &mut File) -> RwResult<(u64, Vec<u8>)> {
    let mut header = [0_u8; 12];
    source
        .read_exact(&mut header)
        .map_err(|error| RwError::Message(format!("video journal read failed: {error}")))?;
    let timestamp_us = u64::from_le_bytes(header[0..8].try_into().expect("timestamp bytes"));
    let len = u32::from_le_bytes(header[8..12].try_into().expect("length bytes")) as usize;
    let mut jpeg = vec![0_u8; len];
    source
        .read_exact(&mut jpeg)
        .map_err(|error| RwError::Message(format!("video journal frame read failed: {error}")))?;
    Ok((timestamp_us, jpeg))
}

const AVI_HDRL_LIST_SIZE: u32 = 200;
const AVI_HEADER_PREFIX: usize = 12 + AVI_HDRL_LIST_SIZE as usize + 12;

struct AviIndexEntry {
    offset: u32,
    size: u32,
}

fn avi_io_error(error: io::Error) -> RwError {
    RwError::Message(format!("video AVI write failed: {error}"))
}

#[allow(clippy::too_many_arguments)]
fn write_avi_header(
    dest: &mut File,
    riff_size: u32,
    frames: u32,
    micros_per_frame: u32,
    fps: u32,
    max_frame: u32,
    width: u32,
    height: u32,
    movi_list_size: u32,
) -> RwResult<()> {
    dest.write_all(b"RIFF").map_err(avi_io_error)?;
    dest.write_all(&riff_size.to_le_bytes())
        .map_err(avi_io_error)?;
    dest.write_all(b"AVI ").map_err(avi_io_error)?;
    dest.write_all(b"LIST").map_err(avi_io_error)?;
    dest.write_all(&(AVI_HDRL_LIST_SIZE - 8).to_le_bytes())
        .map_err(avi_io_error)?;
    dest.write_all(b"hdrl").map_err(avi_io_error)?;
    dest.write_all(b"avih").map_err(avi_io_error)?;
    dest.write_all(&56u32.to_le_bytes()).map_err(avi_io_error)?;
    dest.write_all(&micros_per_frame.to_le_bytes())
        .map_err(avi_io_error)?;
    dest.write_all(&0u32.to_le_bytes()).map_err(avi_io_error)?; // dwMaxBytesPerSec
    dest.write_all(&0u32.to_le_bytes()).map_err(avi_io_error)?; // dwPaddingGranularity
    dest.write_all(&0x10u32.to_le_bytes())
        .map_err(avi_io_error)?; // AVIF_HASINDEX
    dest.write_all(&frames.to_le_bytes())
        .map_err(avi_io_error)?;
    dest.write_all(&0u32.to_le_bytes()).map_err(avi_io_error)?; // dwInitialFrames
    dest.write_all(&1u32.to_le_bytes()).map_err(avi_io_error)?; // dwStreams
    dest.write_all(&max_frame.saturating_add(8).to_le_bytes())
        .map_err(avi_io_error)?;
    dest.write_all(&width.to_le_bytes()).map_err(avi_io_error)?;
    dest.write_all(&height.to_le_bytes())
        .map_err(avi_io_error)?;
    dest.write_all(&[0_u8; 16]).map_err(avi_io_error)?;

    dest.write_all(b"LIST").map_err(avi_io_error)?;
    dest.write_all(&116u32.to_le_bytes())
        .map_err(avi_io_error)?;
    dest.write_all(b"strl").map_err(avi_io_error)?;
    dest.write_all(b"strh").map_err(avi_io_error)?;
    dest.write_all(&56u32.to_le_bytes()).map_err(avi_io_error)?;
    dest.write_all(b"vids").map_err(avi_io_error)?;
    dest.write_all(b"MJPG").map_err(avi_io_error)?;
    dest.write_all(&0u32.to_le_bytes()).map_err(avi_io_error)?; // dwFlags
    dest.write_all(&0u32.to_le_bytes()).map_err(avi_io_error)?; // wPriority + wLanguage
    dest.write_all(&0u32.to_le_bytes()).map_err(avi_io_error)?; // dwInitialFrames
    dest.write_all(&1u32.to_le_bytes()).map_err(avi_io_error)?; // dwScale
    dest.write_all(&fps.to_le_bytes()).map_err(avi_io_error)?; // dwRate
    dest.write_all(&0u32.to_le_bytes()).map_err(avi_io_error)?; // dwStart
    dest.write_all(&frames.to_le_bytes())
        .map_err(avi_io_error)?; // dwLength
    dest.write_all(&max_frame.saturating_add(8).to_le_bytes())
        .map_err(avi_io_error)?;
    dest.write_all(&(-1i32).to_le_bytes())
        .map_err(avi_io_error)?; // dwQuality
    dest.write_all(&0u32.to_le_bytes()).map_err(avi_io_error)?; // dwSampleSize
    dest.write_all(&0u16.to_le_bytes()).map_err(avi_io_error)?; // left
    dest.write_all(&0u16.to_le_bytes()).map_err(avi_io_error)?; // top
    dest.write_all(&(width as u16).to_le_bytes())
        .map_err(avi_io_error)?;
    dest.write_all(&(height as u16).to_le_bytes())
        .map_err(avi_io_error)?;

    dest.write_all(b"strf").map_err(avi_io_error)?;
    dest.write_all(&40u32.to_le_bytes()).map_err(avi_io_error)?;
    dest.write_all(&40u32.to_le_bytes()).map_err(avi_io_error)?; // biSize
    dest.write_all(&width.to_le_bytes()).map_err(avi_io_error)?;
    dest.write_all(&height.to_le_bytes())
        .map_err(avi_io_error)?;
    dest.write_all(&1u16.to_le_bytes()).map_err(avi_io_error)?; // biPlanes
    dest.write_all(&24u16.to_le_bytes()).map_err(avi_io_error)?; // biBitCount
    dest.write_all(b"MJPG").map_err(avi_io_error)?;
    dest.write_all(&max_frame.to_le_bytes())
        .map_err(avi_io_error)?;
    dest.write_all(&0u32.to_le_bytes()).map_err(avi_io_error)?;
    dest.write_all(&0u32.to_le_bytes()).map_err(avi_io_error)?;
    dest.write_all(&0u32.to_le_bytes()).map_err(avi_io_error)?;
    dest.write_all(&0u32.to_le_bytes()).map_err(avi_io_error)?;

    dest.write_all(b"LIST").map_err(avi_io_error)?;
    dest.write_all(&movi_list_size.to_le_bytes())
        .map_err(avi_io_error)?;
    dest.write_all(b"movi").map_err(avi_io_error)?;
    Ok(())
}

pub fn video_output_path(path: &str) -> RwResult<PathBuf> {
    if path.is_empty() {
        return Err(RwError::InvalidInput(
            "video output path must not be empty".to_string(),
        ));
    }
    Ok(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg_with_size(width: u16, height: u16) -> Vec<u8> {
        let mut out = vec![0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x0B, 0x08];
        out.extend_from_slice(&height.to_be_bytes());
        out.extend_from_slice(&width.to_be_bytes());
        out.extend_from_slice(&[0x01, 0x11, 0x00, 0xFF, 0xD9]);
        out
    }

    fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn jpeg_dimensions_read_sof0() {
        assert_eq!(jpeg_dimensions(&jpeg_with_size(320, 200)), Some((320, 200)));
        assert_eq!(jpeg_dimensions(&[0, 1, 2, 3]), None);
    }

    #[test]
    fn video_options_reject_zero_dimensions() {
        assert!(VideoStartOptions::from_optional(Some(0), None, None).is_err());
        assert!(VideoStartOptions::from_optional(None, Some(0), None).is_err());
        assert!(VideoStartOptions::from_optional(None, None, Some(0)).is_err());
        assert_eq!(
            VideoStartOptions::from_optional(Some(70), Some(800), Some(2)).unwrap(),
            VideoStartOptions {
                quality: 70,
                max_width: 800,
                every_nth_frame: 2,
            }
        );
    }

    #[test]
    fn muxes_journal_frames_into_mjpeg_avi() {
        let mut journal = FrameJournal::create().expect("journal");
        let jpeg = jpeg_with_size(64, 48);
        assert!(journal.push(1_000, &jpeg).unwrap());
        assert!(journal.push(101_000, &jpeg).unwrap());
        let finished = journal.finish().expect("finish");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.avi");
        let recording = write_mjpeg_avi(&finished, &path).expect("mux");
        assert_eq!(recording.frames, 2);
        assert_eq!(recording.width, 64);
        assert_eq!(recording.height, 48);
        assert!(recording.bytes > 0);
        assert!(recording.duration_ms >= 100);
        let bytes = fs::read(&path).expect("read avi");
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"AVI ");
        assert_eq!(&bytes[12..16], b"LIST");
        assert_eq!(&bytes[20..24], b"hdrl");
        assert!(bytes.windows(4).any(|window| window == b"MJPG"));
        assert!(bytes.windows(4).any(|window| window == b"00dc"));
        assert!(bytes.windows(4).any(|window| window == b"idx1"));
        assert_eq!(read_u32_le(&bytes, 4) as usize + 8, bytes.len());
    }

    #[test]
    fn mux_rejects_empty_journal() {
        let journal = FrameJournal::create().expect("journal").finish().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let error = write_mjpeg_avi(&journal, dir.path().join("empty.avi")).unwrap_err();
        assert!(error.to_string().contains("no frames"));
    }

    fn real_jpeg(width: u32, height: u32) -> Vec<u8> {
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            width,
            height,
            image::Rgb([16, 48, 96]),
        ));
        let mut jpeg = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut jpeg), ImageFormat::Jpeg)
            .expect("encode jpeg");
        jpeg
    }

    #[test]
    fn muxes_journal_frames_into_webm() {
        let mut journal = FrameJournal::create().expect("journal");
        let jpeg = real_jpeg(32, 24);
        assert!(journal.push(1_000, &jpeg).unwrap());
        assert!(journal.push(101_000, &jpeg).unwrap());
        let finished = journal.finish().expect("finish");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.webm");
        let recording = write_recording(&finished, &path).expect("mux");
        assert_eq!(recording.frames, 2);
        assert_eq!(recording.width, 32);
        assert_eq!(recording.height, 24);
        let bytes = fs::read(&path).expect("read webm");
        assert_eq!(&bytes[0..4], &[0x1A, 0x45, 0xDF, 0xA3]);
        assert!(bytes.windows(4).any(|window| window == b"webm"));
        assert!(bytes.windows(5).any(|window| window == b"V_VP8"));
    }

    #[test]
    fn write_recording_defaults_extensionless_path_to_webm() {
        let mut journal = FrameJournal::create().expect("journal");
        let jpeg = real_jpeg(16, 16);
        assert!(journal.push(1_000, &jpeg).unwrap());
        let finished = journal.finish().expect("finish");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip");
        write_recording(&finished, &path).expect("mux");
        let bytes = fs::read(&path).expect("read default webm");
        assert_eq!(&bytes[0..4], &[0x1A, 0x45, 0xDF, 0xA3]);
    }

    #[test]
    fn muxes_journal_frames_into_gif() {
        let mut journal = FrameJournal::create().expect("journal");
        let jpeg = real_jpeg(32, 24);
        assert!(journal.push(1_000, &jpeg).unwrap());
        assert!(journal.push(101_000, &jpeg).unwrap());
        let finished = journal.finish().expect("finish");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.gif");
        let recording = write_recording(&finished, &path).expect("mux");
        assert_eq!(recording.frames, 2);
        assert_eq!(recording.width, 32);
        assert_eq!(recording.height, 24);
        let bytes = fs::read(&path).expect("read gif");
        assert_eq!(&bytes[0..6], b"GIF89a");
    }

    #[test]
    fn write_recording_rejects_mp4_extension() {
        let mut journal = FrameJournal::create().expect("journal");
        assert!(journal.push(1_000, &jpeg_with_size(8, 8)).unwrap());
        let finished = journal.finish().expect("finish");
        let dir = tempfile::tempdir().unwrap();
        let error = write_recording(&finished, dir.path().join("clip.mp4")).unwrap_err();
        assert!(error.to_string().contains(".webm"));
    }
}
