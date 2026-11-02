mod tool;
mod tr;

#[cfg(test)]
mod tests;

pub use tool::{BevelOptions, BevelTool, EDGE_TOLERANCE, edge_at, edge_exists};
pub use tr::BEVEL_TR;
