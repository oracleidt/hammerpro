#![deny(unsafe_code)]

#[cfg(not(test))]
#[allow(unused_macros)]
macro_rules! println {
    () => {{
        let _ = ::std::io::Write::write_all(&mut ::std::io::stdout(), b"\n");
    }};
    ($($arg:tt)*) => {{
        let _ = ::std::io::Write::write_fmt(&mut ::std::io::stdout(), format_args!("{}\n", format_args!($($arg)*)));
    }};
}

#[cfg(not(test))]
#[allow(unused_macros)]
macro_rules! print {
    ($($arg:tt)*) => {{
        let _ = ::std::io::Write::write_fmt(&mut ::std::io::stdout(), format_args!($($arg)*));
    }};
}

#[cfg(not(test))]
#[allow(unused_macros)]
macro_rules! eprintln {
    () => {{
        let _ = ::std::io::Write::write_all(&mut ::std::io::stderr(), b"\n");
    }};
    ($($arg:tt)*) => {{
        let _ = ::std::io::Write::write_fmt(&mut ::std::io::stderr(), format_args!("{}\n", format_args!($($arg)*)));
    }};
}

#[cfg(not(test))]
#[allow(unused_macros)]
macro_rules! eprint {
    ($($arg:tt)*) => {{
        let _ = ::std::io::Write::write_fmt(&mut ::std::io::stderr(), format_args!($($arg)*));
    }};
}

pub mod areas;
pub mod brush;
pub mod brushbsp;
pub mod compile;
pub mod cubemap;
pub mod detail;
pub mod detail_props;
pub mod disp;
pub mod emit;
pub mod faces;
pub mod materials;
pub mod occluder;
pub mod phys;
pub mod planes;
pub mod portals;
pub mod prop_debug;
pub mod props;
pub mod rad;
pub mod stats;
pub mod texinfo;
pub mod tree;
pub mod validate;
pub mod vis;
pub mod water;
pub mod water_overlays;
pub mod winding;

#[cfg(test)]
mod parity_tests;

pub use compile::{CompileError, CompileOptions, CompileOutput, VisLevel, compile};
pub use materials::{MaterialFacts, MaterialSource};
