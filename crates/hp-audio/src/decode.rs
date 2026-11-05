use std::io::Cursor;

use rodio::Source;

use crate::{AudioError, MAX_CHANNELS, Sound};

const MAX_SAMPLES: usize = 96 << 20;

pub fn decode(bytes: &[u8], hint: &str) -> Result<Sound, AudioError> {
    let sound = match decode_wav(bytes) {
        Some(sound) => sound,
        None => decode_any(bytes, hint)?,
    };
    if sound.samples.is_empty() {
        return Err(AudioError::Empty);
    }
    if sound.channels == 0 || sound.channels > MAX_CHANNELS {
        return Err(AudioError::Channels(sound.channels));
    }
    Ok(sound)
}

fn decode_any(bytes: &[u8], hint: &str) -> Result<Sound, AudioError> {
    let len = bytes.len() as u64;
    let hint = hint.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let decoder = rodio::Decoder::builder()
        .with_data(Cursor::new(bytes.to_vec()))
        .with_byte_len(len)
        .with_hint(&hint)
        .with_seekable(true)
        .build()
        .map_err(|e| AudioError::Unreadable(e.to_string()))?;
    let channels = decoder.channels().get();
    let rate = decoder.sample_rate().get();
    if channels > MAX_CHANNELS {
        return Err(AudioError::Channels(channels));
    }
    let mut samples = Vec::new();
    for s in decoder {
        if samples.len() >= MAX_SAMPLES {
            return Err(AudioError::TooLong((MAX_SAMPLES * 4) >> 20));
        }
        samples.push(if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 });
    }
    let width = usize::from(channels.max(1));
    let whole = samples.len() / width * width;
    samples.truncate(whole);
    Ok(Sound { rate, channels, samples, looped: false, loop_start: 0 })
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

struct Format {
    tag: u16,
    channels: u16,
    rate: u32,
    bits: u16,
}

fn decode_wav(b: &[u8]) -> Option<Sound> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return None;
    }
    let mut at = 12;
    let mut format: Option<Format> = None;
    let mut data: Option<&[u8]> = None;
    let mut smpl_start: Option<u32> = None;
    let mut cue_start: Option<u32> = None;
    while at + 8 <= b.len() {
        let id = &b[at..at + 4];
        let size = u32_at(b, at + 4)? as usize;
        let body_at = at + 8;
        let body_end = body_at.saturating_add(size).min(b.len());
        let body = &b[body_at..body_end];
        match id {
            b"fmt " if body.len() >= 16 => {
                let mut tag = u16_at(body, 0)?;
                if tag == 0xFFFE && body.len() >= 26 {
                    tag = u16_at(body, 24)?;
                }
                format =
                    Some(Format { tag, channels: u16_at(body, 2)?, rate: u32_at(body, 4)?, bits: u16_at(body, 14)? });
            }
            b"data" => data = Some(body),
            b"smpl" if body.len() >= 36 + 24 && u32_at(body, 28)? > 0 => smpl_start = u32_at(body, 36 + 8),
            b"cue " if body.len() >= 4 + 24 && u32_at(body, 0)? > 0 => cue_start = u32_at(body, 4 + 20),
            _ => {}
        }
        at = body_at.saturating_add(size).saturating_add(size & 1);
    }
    let f = format?;
    let data = data?;
    if f.channels == 0 || f.rate == 0 {
        return None;
    }
    let samples = pcm_samples(&f, data)?;
    let loop_start = smpl_start.or(cue_start);
    Some(Sound {
        rate: f.rate,
        channels: f.channels,
        samples,
        looped: loop_start.is_some(),
        loop_start: loop_start.unwrap_or(0),
    })
}

fn pcm_samples(f: &Format, data: &[u8]) -> Option<Vec<f32>> {
    let width = usize::from(f.bits / 8);
    let ok = matches!((f.tag, f.bits), (1, 8 | 16 | 24 | 32) | (3, 32 | 64));
    if !ok || width == 0 {
        return None;
    }
    let channels = usize::from(f.channels);
    let count = data.len() / width / channels * channels;
    if count > MAX_SAMPLES {
        return None;
    }
    let mut out = Vec::with_capacity(count);
    for chunk in data.chunks_exact(width).take(count) {
        let v = match (f.tag, f.bits) {
            (1, 8) => (f32::from(chunk[0]) - 128.0) / 128.0,
            (1, 16) => f32::from(i16::from_le_bytes([chunk[0], chunk[1]])) / 32768.0,
            (1, 24) => (i32::from_le_bytes([0, chunk[0], chunk[1], chunk[2]]) >> 8) as f32 / 8_388_608.0,
            (1, 32) => i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as f32 / 2_147_483_648.0,
            (3, 32) => f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
            (3, 64) => f64::from_le_bytes(chunk.try_into().ok()?) as f32,
            _ => 0.0,
        };
        out.push(if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 });
    }
    Some(out)
}
