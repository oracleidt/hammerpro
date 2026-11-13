mod decode;
pub mod dsp;
mod encode;
pub mod mixer;
mod play;
mod process;

pub use decode::decode;
pub use encode::encode_wav;
pub use mixer::{LoopVoice, VoiceParams};
pub use play::{is_playing, play, position, stop};
pub use process::{Edit, apply, normalize_gain_db, peaks};

pub const MAX_CHANNELS: u16 = 2;
pub const SOURCE_RATES: [u32; 3] = [11_025, 22_050, 44_100];

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("not a sound file this editor can read: {0}")]
    Unreadable(String),
    #[error("the sound is empty")]
    Empty,
    #[error("the sound has {0} channels, only mono and stereo are supported")]
    Channels(u16),
    #[error("the sound is too long to edit ({0} MB of samples)")]
    TooLong(usize),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sound {
    pub rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
    pub looped: bool,
    pub loop_start: u32,
}

impl Sound {
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels.max(1))
    }

    pub fn duration(&self) -> f64 {
        if self.rate == 0 { 0.0 } else { self.frames() as f64 / f64::from(self.rate) }
    }

    pub fn peak(&self) -> f32 {
        self.samples.iter().fold(0.0_f32, |m, s| if s.is_finite() { m.max(s.abs()) } else { m })
    }

    pub fn is_source_friendly(&self) -> bool {
        SOURCE_RATES.contains(&self.rate) && self.channels <= MAX_CHANNELS
    }
}

pub fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

pub fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.0 { f32::NEG_INFINITY } else { 20.0 * gain.log10() }
}
