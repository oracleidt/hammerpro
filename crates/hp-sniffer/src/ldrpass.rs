use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::hubdata::{HubData, capture_lighting};
use crate::launch::{GmodInstall, LogTail, hijack_command};
use crate::procmem::MemSource;
use crate::report::SniffReport;
use crate::settings::SnifferSettings;

pub struct Request<'a> {
    pub install: &'a GmodInstall,
    pub tail: &'a mut LogTail,
    pub local_map: Option<&'a str>,
    pub connect: Option<&'a str>,
    pub hub_base: u64,
    pub ptr: usize,
    pub surfaces: usize,
    pub timeout: Duration,
    pub settle: Duration,
    pub cancel: &'a AtomicBool,
    pub say: &'a dyn Fn(&str),
}

pub fn strategies(local_map: Option<&str>, connect: Option<&str>, hdr_level: u32) -> Vec<Vec<String>> {
    let level = format!("+mat_hdr_level {hdr_level}");
    match (local_map, connect) {
        (Some(m), _) if !m.is_empty() => vec![vec![level, format!("+map {m}")]],
        (_, Some(addr)) => {
            vec![vec![level, "+retry".to_owned()], vec!["+disconnect".to_owned(), format!("+connect {addr}")]]
        }
        _ => vec![vec![level, "+retry".to_owned()]],
    }
}

fn nap(cancel: &AtomicBool, d: Duration) -> Result<(), String> {
    let end = Instant::now() + d;
    while Instant::now() < end {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_owned());
        }
        std::thread::sleep(Duration::from_millis(200).min(end.saturating_duration_since(Instant::now())));
    }
    Ok(())
}

pub fn world_was_reloaded(before: &HubData, after: &HubData) -> bool {
    let same_block = before.lightdata == after.lightdata;
    let same_bytes = match (before.piece(53), after.piece(8)) {
        (Some(a), Some(b)) => a.bytes == b.bytes,
        _ => false,
    };
    !(same_block && same_bytes)
}

pub fn read<S: MemSource + ?Sized>(src: &S, req: Request<'_>) -> Result<HubData, String> {
    let Request { install, tail, local_map, connect, hub_base, ptr, surfaces, timeout, settle, cancel, say } = req;
    let before = capture_lighting(src, hub_base, ptr, true);
    let plans = strategies(local_map, connect, 0);
    let per_plan = timeout / plans.len().max(1) as u32;
    let mut last = "the game did not load the map again".to_owned();
    for steps in plans {
        tail.poll();
        say(&format!(
            "switching the game to low dynamic range lighting and loading the map again: {}",
            steps.join(", ")
        ));
        for (k, step) in steps.iter().enumerate() {
            if k > 0 {
                nap(cancel, Duration::from_secs(3))?;
            }
            hijack_command(install, step)?;
        }
        let deadline = Instant::now() + per_plan;
        let mut loaded = false;
        while Instant::now() < deadline && !loaded {
            nap(cancel, Duration::from_millis(500))?;
            for line in tail.poll() {
                if line.trim_start().starts_with("Map: ") {
                    loaded = true;
                }
            }
        }
        if !loaded {
            last = "the game did not load the map again in time".to_owned();
            continue;
        }
        say("the map was loaded again: waiting for the engine to finish it");
        nap(cancel, settle.max(Duration::from_secs(4)))?;
        for _ in 0..10 {
            match capture_lighting(src, hub_base, ptr, false) {
                Some(d) if d.faces.len() == surfaces && d.piece(8).is_some() => {
                    if before.as_ref().is_none_or(|b| world_was_reloaded(b, &d)) {
                        return Ok(d);
                    }
                    last =
                        "the game kept the world it had loaded (a reconnect to a local server does not load it again)"
                            .to_owned();
                    break;
                }
                Some(d) => last = format!("{} faces instead of {surfaces}", d.faces.len()),
                None => last = "the world structure is not readable".to_owned(),
            }
            nap(cancel, Duration::from_secs(2))?;
        }
        say(&format!("second reading not obtained: {last}"));
    }
    Err(last)
}

pub fn restore(install: &GmodInstall) {
    let _ = hijack_command(install, "+mat_hdr_level 2");
}

#[allow(clippy::too_many_arguments)]
pub fn after_produce<S: MemSource + ?Sized>(
    src: &S,
    install: &GmodInstall,
    tail: &mut LogTail,
    settings: &SnifferSettings,
    local_map: Option<&str>,
    connect: Option<&str>,
    report: &mut SniffReport,
    bsp_path: Option<&Path>,
    report_path: &Path,
    cancel: &AtomicBool,
    say: &dyn Fn(&str),
) {
    let h = &report.hub;
    let Some(bsp_path) = bsp_path else { return };
    if !h.found || !h.hdr || h.base == 0 {
        return;
    }
    let request = Request {
        install,
        tail,
        local_map,
        connect,
        hub_base: h.base,
        ptr: h.ptr,
        surfaces: h.faces,
        timeout: Duration::from_secs(u64::from(settings.load_timeout_secs)),
        settle: Duration::from_secs(u64::from(settings.settle_secs)),
        cancel,
        say,
    };
    let outcome = read(src, request).and_then(|ldr| {
        let bytes = std::fs::read(bsp_path).map_err(|e| format!("cannot read {}: {e}", bsp_path.display()))?;
        let base = hp_bsp::Bsp::from_vec(bytes).map_err(|e| e.to_string())?;
        let (merged, summary) = crate::ldr::apply(&base, &ldr)?;
        std::fs::write(bsp_path, merged.bytes()).map_err(|e| format!("cannot write {}: {e}", bsp_path.display()))?;
        Ok((ldr, summary))
    });
    match outcome {
        Ok((ldr, summary)) => {
            let rows = ldr.report();
            report.hub.lumps.extend(rows.lumps.into_iter().filter(|r| summary.lumps.iter().any(|l| l.0 == r.lump)));
            report.hub.lumps.sort_by_key(|l| l.lump);
            report.notes.push(if summary.same_lighting {
                "the low dynamic range lighting read from the game is identical to the high dynamic range one: the map was compiled with a single lighting set".to_owned()
            } else {
                "both lighting sets were read from the game memory: the game was switched to LDR and the map loaded a second time".to_owned()
            });
        }
        Err(e) => report.notes.push(format!("the low dynamic range lighting was not read: {e}")),
    }
    restore(install);
    let _ = std::fs::write(report_path, report.render_text());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hubdata::Fidelity;

    #[test]
    fn the_reload_commands_depend_on_the_kind_of_run() {
        let local = strategies(Some("gm_construct"), None, 0);
        assert_eq!(local, vec![vec!["+mat_hdr_level 0".to_owned(), "+map gm_construct".to_owned()]]);
        let server = strategies(None, Some("1.2.3.4:27015"), 0);
        assert_eq!(server.len(), 2, "a reconnect is tried first, then a fresh connection");
        assert_eq!(server[0], vec!["+mat_hdr_level 0".to_owned(), "+retry".to_owned()]);
        assert_eq!(server[1], vec!["+disconnect".to_owned(), "+connect 1.2.3.4:27015".to_owned()]);
        assert_eq!(strategies(Some(""), None, 2), vec![vec!["+mat_hdr_level 2".to_owned(), "+retry".to_owned()]]);
        assert_eq!(strategies(Some("x"), Some("1.2.3.4:1"), 0).len(), 1, "a local map is always loaded with map");
    }

    fn data(index: usize, bytes: &[u8], lightdata: u64) -> HubData {
        let mut d = HubData { lightdata, ..HubData::default() };
        d.push(index, bytes.to_vec(), 1, Fidelity::Exact, "test");
        d
    }

    #[test]
    fn a_world_that_was_not_loaded_again_is_recognised() {
        let hdr = data(53, &[1, 2, 3], 0x1000);
        assert!(!world_was_reloaded(&hdr, &data(8, &[1, 2, 3], 0x1000)), "same block, same bytes");
        assert!(world_was_reloaded(&hdr, &data(8, &[1, 2, 3], 0x2000)), "another block");
        assert!(world_was_reloaded(&hdr, &data(8, &[9, 9, 9], 0x1000)), "other bytes");
    }
}
