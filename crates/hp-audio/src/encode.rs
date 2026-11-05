use crate::Sound;

fn push_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub fn encode_wav(sound: &Sound) -> Vec<u8> {
    let channels = sound.channels.max(1);
    let frames = sound.frames();
    let data_len = frames * usize::from(channels) * 2;
    let loop_start = (sound.loop_start as usize).min(frames.saturating_sub(1)) as u32;
    let loop_end = frames.saturating_sub(1) as u32;
    let extra = if sound.looped { 8 + 4 + 24 + 8 + 36 + 24 } else { 0 };
    let mut out = Vec::with_capacity(44 + data_len + extra);
    out.extend_from_slice(b"RIFF");
    push_u32(&mut out, (4 + 8 + 16 + extra + 8 + data_len) as u32);
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    push_u32(&mut out, 16);
    push_u16(&mut out, 1);
    push_u16(&mut out, channels);
    push_u32(&mut out, sound.rate);
    push_u32(&mut out, sound.rate * u32::from(channels) * 2);
    push_u16(&mut out, channels * 2);
    push_u16(&mut out, 16);
    if sound.looped {
        out.extend_from_slice(b"cue ");
        push_u32(&mut out, 4 + 24);
        push_u32(&mut out, 1);
        push_u32(&mut out, 1);
        push_u32(&mut out, loop_start);
        out.extend_from_slice(b"data");
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
        push_u32(&mut out, loop_start);
        out.extend_from_slice(b"smpl");
        push_u32(&mut out, 36 + 24);
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
        push_u32(&mut out, 1_000_000_000 / sound.rate.max(1));
        push_u32(&mut out, 60);
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
        push_u32(&mut out, 1);
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
        push_u32(&mut out, loop_start);
        push_u32(&mut out, loop_end);
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
    }
    out.extend_from_slice(b"data");
    push_u32(&mut out, data_len as u32);
    for s in &sound.samples[..frames * usize::from(channels)] {
        let v = if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 };
        out.extend_from_slice(&((v * 32767.0).round() as i16).to_le_bytes());
    }
    out
}
