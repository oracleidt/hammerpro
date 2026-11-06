#![forbid(unsafe_code)]

pub mod dmx;
pub mod editor;
pub mod environment;
pub mod model;
mod parameters;
pub mod random;
pub mod sheet;
pub mod sim;

pub use model::{Child, Operator, SystemDef};
pub use sheet::{Sheet, sheet_from_vtf};
pub use sim::{Effect, Particle, Plan, Points, Render, RenderKind, Sim};

pub const MAX_FILE_SIZE: usize = 64 << 20;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum PcfError {
    #[error("not a binary DMX particle file")]
    NotDmx,
    #[error("unsupported DMX encoding version {0}")]
    Unsupported(u32),
    #[error("damaged particle file")]
    Corrupt,
    #[error("the particle file is too large")]
    TooLarge,
}

pub fn parse_file(data: &[u8]) -> Result<Vec<SystemDef>, PcfError> {
    if data.len() > MAX_FILE_SIZE {
        return Err(PcfError::TooLarge);
    }
    let dmx = dmx::parse(data)?;
    Ok(model::systems(&dmx))
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_ops;
