const SHEET_TAG: [u8; 3] = [0x10, 0, 0];
const MAX_SEQUENCES: usize = 256;
const MAX_FRAMES: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SheetFrame {
    pub duration: f32,
    pub uv: [f32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct SheetSequence {
    pub number: u32,
    pub clamp: bool,
    pub frames: Vec<SheetFrame>,
    pub total: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sheet {
    pub sequences: Vec<SheetSequence>,
}

pub const FULL_UV: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

struct Cur<'a> {
    d: &'a [u8],
    p: usize,
}

impl Cur<'_> {
    fn u32(&mut self) -> Option<u32> {
        let b = self.d.get(self.p..self.p.checked_add(4)?)?;
        self.p += 4;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn f32(&mut self) -> Option<f32> {
        self.u32().map(f32::from_bits)
    }
    fn left(&self) -> usize {
        self.d.len().saturating_sub(self.p)
    }
}

pub fn sheet_from_vtf(data: &[u8]) -> Option<Sheet> {
    if data.get(..4)? != b"VTF\0" {
        return None;
    }
    let mut c = Cur { d: data, p: 4 };
    let major = c.u32()?;
    let minor = c.u32()?;
    if major != 7 || minor < 3 {
        return None;
    }
    let mut c = Cur { d: data, p: 68 };
    let count = c.u32()? as usize;
    if count > 32 {
        return None;
    }
    for i in 0..count {
        let at = 80 + i * 8;
        let entry = data.get(at..at + 8)?;
        if entry[..3] != SHEET_TAG || entry[3] & 2 != 0 {
            continue;
        }
        let offset = u32::from_le_bytes([entry[4], entry[5], entry[6], entry[7]]) as usize;
        let mut c = Cur { d: data, p: offset };
        let len = c.u32()? as usize;
        let body = data.get(offset + 4..(offset + 4).checked_add(len)?)?;
        return parse_sheet(body);
    }
    None
}

pub fn parse_sheet(data: &[u8]) -> Option<Sheet> {
    let mut c = Cur { d: data, p: 0 };
    let version = c.u32()?;
    let rects = if version > 0 { 4 } else { 1 };
    let n = c.u32()? as usize;
    if n > MAX_SEQUENCES || n.saturating_mul(16) > c.left() {
        return None;
    }
    let mut sequences = Vec::with_capacity(n);
    for _ in 0..n {
        let number = c.u32()?;
        let clamp = c.u32()? != 0;
        let frames = c.u32()? as usize;
        let _total = c.f32()?;
        let frame_size = 4 + 16 * rects;
        if frames > MAX_FRAMES || frames.saturating_mul(frame_size) > c.left() {
            return None;
        }
        let mut list = Vec::with_capacity(frames);
        for _ in 0..frames {
            let duration = c.f32()?;
            let mut uv = [0.0f32; 4];
            for r in 0..rects {
                let v = [c.f32()?, c.f32()?, c.f32()?, c.f32()?];
                if r == 0 {
                    uv = v;
                }
            }
            let duration = if duration.is_finite() { duration.max(0.0) } else { 0.0 };
            let uv = if uv.iter().all(|x| x.is_finite()) { uv.map(|x| x.clamp(0.0, 1.0)) } else { FULL_UV };
            list.push(SheetFrame { duration, uv });
        }
        let total = list.iter().map(|f| f.duration).sum::<f32>();
        if !list.is_empty() {
            sequences.push(SheetSequence { number, clamp, frames: list, total });
        }
    }
    (!sequences.is_empty()).then_some(Sheet { sequences })
}

impl Sheet {
    pub fn sequence(&self, number: u32) -> Option<&SheetSequence> {
        self.sequences.iter().find(|s| s.number == number).or_else(|| {
            let n = self.sequences.len();
            (n > 0).then(|| &self.sequences[number as usize % n])
        })
    }

    pub fn frame_uv(&self, number: u32, cycle: f32) -> [f32; 4] {
        let Some(seq) = self.sequence(number) else { return FULL_UV };
        let cycle = if !cycle.is_finite() || cycle < 0.0 {
            0.0
        } else if seq.clamp {
            cycle.min(1.0)
        } else {
            cycle.fract()
        };
        if seq.total <= 0.0 {
            let i = ((cycle * seq.frames.len() as f32) as usize).min(seq.frames.len() - 1);
            return seq.frames[i].uv;
        }
        let mut t = cycle * seq.total;
        for f in &seq.frames {
            if t < f.duration {
                return f.uv;
            }
            t -= f.duration;
        }
        seq.frames.last().map_or(FULL_UV, |f| f.uv)
    }

    pub fn frame_blend(&self, number: u32, cycle: f32) -> ([f32; 4], [f32; 4], f32) {
        let Some(seq) = self.sequence(number) else { return (FULL_UV, FULL_UV, 0.0) };
        let n = seq.frames.len();
        let cycle = if !cycle.is_finite() || cycle < 0.0 {
            0.0
        } else if seq.clamp {
            cycle.min(1.0)
        } else {
            cycle.fract()
        };
        let (index, frac);
        if seq.total <= 0.0 {
            let x = cycle * n as f32;
            index = (x as usize).min(n - 1);
            frac = x - index as f32;
        } else {
            let mut t = cycle * seq.total;
            let mut found = (n - 1, 0.0f32);
            for (i, f) in seq.frames.iter().enumerate() {
                if t < f.duration {
                    found = (i, if f.duration > 0.0 { t / f.duration } else { 0.0 });
                    break;
                }
                t -= f.duration;
            }
            (index, frac) = found;
        }
        let next = if index + 1 < n {
            index + 1
        } else if seq.clamp {
            index
        } else {
            0
        };
        let frac = if next == index { 0.0 } else { frac.clamp(0.0, 1.0) };
        (seq.frames[index].uv, seq.frames[next].uv, frac)
    }

    pub fn frame_count(&self, number: u32) -> usize {
        self.sequence(number).map_or(0, |s| s.frames.len())
    }
}
