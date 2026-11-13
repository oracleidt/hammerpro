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

pub mod b64;
pub mod bridge;
pub mod install;
pub mod protocol;
pub mod registry;
pub mod server;
pub mod tools;

mod process;

pub const SERVER_NAME: &str = "hammer-pro";
pub const PRODUCT_NAME: &str = "Hammer Pro";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const BRIDGE_ARG: &str = "--mcp";
pub const INSTANCE_ENV: &str = "HAMMERPRO_INSTANCE";
pub const DIR_ENV: &str = "HAMMERPRO_MCP_DIR";
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

pub const NO_INSTANCE_TEXT: &str = "No Hammer Pro window has its MCP server switched on. Open Hammer Pro, tick MCP > \
     MCP server in the menu bar to switch it on (it is off by default), then try again. Call list_instances to see \
     the windows with MCP on.";

pub const INSTRUCTIONS: &str = "Hammer Pro is a Source engine (Garry's Mod) map editor. These tools edit the map \
     open in a Hammer Pro window: coordinates are Source units (Z up), materials are names like \
     \"dev/dev_measuregeneric01b\" (search_materials), every edit is one undo step labelled \"MCP: <tool>\" that the \
     user can undo. Start with get_map_info. Use screenshot_view to look at the result. Object ids are the VMF ids; \
     when an entity and a solid share an id, ids names the entity: use solid_ids for the solid. To place things \
     precisely, never guess from bounding boxes or origins: get_object_shape (or get_model_info) gives the real \
     size, footprint and how high the origin sits above the lowest point of any object or model, render_shape \
     draws it with dimensions, place_object and drop_to_surface put the real mesh exactly on, against or next to \
     brushes, displacements and other props (no gap, no overlap), check_placement and find_floating_objects \
     verify (floating, sunk, interpenetrating). The tools that create or move props and brushes add a placement_note \
     to their result when one ends floating, sunk or overlapping another object: fix it with drop_to_surface or \
     place_object before going on. Before you finish a map, run optimize_map (or analyze_visibility) \
     and act on its report. Use func_detail for small and decorative brushes, hint brushes where they measure \
     a gain, areaportals in door frames and narrow openings, give every light a visible lamp, and give every \
     room with reflective materials its env_cubemap (suggest_cubemaps).";
