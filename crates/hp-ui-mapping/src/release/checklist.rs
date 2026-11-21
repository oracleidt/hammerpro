use std::path::{Path, PathBuf};
use std::time::SystemTime;

use hp_app::{t, tf};
use hp_gma::{AddonJson, AddonProblem};

use super::pipeline::BspFacts;
use super::profiles::{missing_groups, profile_for};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Ok,
    Warn,
    Fail,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fix {
    Build,
    Capture,
    Addon,
    GameSession,
    Report,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Row {
    pub label: String,
    pub status: Status,
    pub detail: String,
    pub fix: Option<Fix>,
}

impl Row {
    pub(crate) fn shared(&self) -> hp_app::runtime::ChecklistRow {
        let status = match self.status {
            Status::Ok => "ok",
            Status::Warn => "warning",
            Status::Fail => "failed",
            Status::Unknown => "unknown",
        };
        hp_app::runtime::ChecklistRow { label: self.label.clone(), status, detail: self.detail.clone() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum NavState {
    #[default]
    Missing,
    Current,
    Stale,
    Unknown,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Facts {
    pub map: String,
    pub saved: bool,
    pub vmf_mtime: Option<SystemTime>,
    pub compiled: Option<BspFacts>,
    pub compiled_mtime: Option<SystemTime>,
    pub shipped: Option<BspFacts>,
    pub leak: bool,
    pub nav: NavState,
    pub missing_deps: Option<usize>,
    pub thumbnail: bool,
    pub icon: bool,
    pub addon: AddonJson,
    pub name_taken_by: Option<String>,
    pub env_cubemaps: usize,
    pub classes: Vec<String>,
    pub gmpublish: Option<PathBuf>,
    pub gma: bool,
}

#[derive(Clone)]
pub(crate) struct GatherInput {
    pub vmf: Option<PathBuf>,
    pub map: String,
    pub out_bsp: Option<PathBuf>,
    pub out_icon: Option<PathBuf>,
    pub thumbnail: bool,
    pub addon: AddonJson,
    pub fs: Option<hp_fs::GameFs>,
    pub nav_fingerprint: Option<u64>,
    pub missing_deps: Option<usize>,
    pub env_cubemaps: usize,
    pub classes: Vec<String>,
    pub gmod_root: Option<PathBuf>,
    pub out_gma: Option<PathBuf>,
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

pub(crate) fn gather(input: GatherInput) -> Facts {
    let mut f = Facts {
        map: input.map.clone(),
        saved: input.vmf.is_some(),
        missing_deps: input.missing_deps,
        thumbnail: input.thumbnail,
        addon: input.addon.clone(),
        env_cubemaps: input.env_cubemaps,
        classes: input.classes.clone(),
        ..Facts::default()
    };
    if let Some(vmf) = &input.vmf {
        f.vmf_mtime = mtime(vmf);
        let bsp = vmf.with_extension("bsp");
        f.compiled = BspFacts::read(&bsp, &input.map);
        f.compiled_mtime = mtime(&bsp);
        let lin = vmf.with_extension("lin");
        f.leak = match (mtime(&lin), f.compiled_mtime) {
            (Some(l), Some(b)) => l >= b,
            (Some(_), None) => true,
            _ => false,
        };
    }
    if let Some(out) = &input.out_bsp {
        f.shipped = BspFacts::read(out, &input.map);
        let nav = out.with_extension("nav");
        f.nav = if !nav.is_file() {
            NavState::Missing
        } else {
            let geometry = f.shipped.as_ref().and_then(|b| b.walkable);
            match (input.nav_fingerprint, geometry) {
                (Some(a), Some(b)) if a == b => NavState::Current,
                (Some(_), Some(_)) => NavState::Stale,
                _ => NavState::Unknown,
            }
        };
    }
    f.gmpublish = input.gmod_root.as_deref().and_then(super::publish::gmpublish_exe);
    f.gma = input.out_gma.as_deref().is_some_and(Path::is_file);
    f.icon = input
        .out_icon
        .as_deref()
        .and_then(|p| super::files::read_limited(p, 8 << 20))
        .is_some_and(|b| super::thumbnail::icon_ok(&b));
    if let Some(fs) = &input.fs {
        let vpath = format!("maps/{}.bsp", input.map);
        if let Some(id) = fs.which_source(&vpath)
            && let Some(src) = fs.source(id)
            && !matches!(src.origin, hp_fs::SourceOrigin::GameLoose | hp_fs::SourceOrigin::Custom)
        {
            f.name_taken_by = Some(src.label);
        }
    }
    f
}

fn row(label: &str, status: Status, detail: String, fix: Option<Fix>) -> Row {
    Row { label: label.to_owned(), status, detail, fix }
}

pub(crate) fn addon_problem_text(p: &AddonProblem) -> String {
    match p {
        AddonProblem::EmptyTitle => t!("release.addon_empty_title").to_owned(),
        AddonProblem::TitleNotAscii => t!("release.addon_title_ascii").to_owned(),
        AddonProblem::TitleTooLong => t!("release.addon_title_long").to_owned(),
        AddonProblem::BadType(x) => tf!("release.addon_bad_type", x),
        AddonProblem::TooManyTags => t!("release.addon_too_many_tags").to_owned(),
        AddonProblem::BadTag(x) => tf!("release.addon_bad_tag", x),
        AddonProblem::DuplicateTag(x) => tf!("release.addon_dup_tag", x),
    }
}

pub(crate) fn rows(f: &Facts) -> Vec<Row> {
    let mut out = Vec::new();
    out.push(match (&f.compiled, f.compiled_mtime, f.vmf_mtime) {
        _ if !f.saved => row(t!("release.check.compile"), Status::Fail, t!("release.check.not_saved").to_owned(), None),
        (None, _, _) => {
            row(t!("release.check.compile"), Status::Fail, t!("release.check.no_bsp").to_owned(), Some(Fix::Build))
        }
        (Some(b), Some(bt), Some(vt)) if bt < vt => {
            let _ = b;
            row(t!("release.check.compile"), Status::Fail, t!("release.check.stale_bsp").to_owned(), Some(Fix::Build))
        }
        (Some(b), _, _) if !b.has_vis => {
            row(t!("release.check.compile"), Status::Fail, t!("release.check.no_vis").to_owned(), Some(Fix::Build))
        }
        _ => row(t!("release.check.compile"), Status::Ok, t!("release.check.compile_ok").to_owned(), None),
    });
    out.push(match &f.compiled {
        None => row(t!("release.check.lighting"), Status::Unknown, String::new(), Some(Fix::Build)),
        Some(b) if b.ldr && b.hdr => {
            row(t!("release.check.lighting"), Status::Ok, t!("release.check.both_ranges").to_owned(), None)
        }
        Some(b) if b.ldr || b.hdr => {
            row(t!("release.check.lighting"), Status::Warn, t!("release.check.one_range").to_owned(), Some(Fix::Build))
        }
        Some(_) => row(
            t!("release.check.lighting"),
            Status::Fail,
            t!("release.check.no_lighting").to_owned(),
            Some(Fix::Build),
        ),
    });
    out.push(if f.leak {
        row(t!("release.check.leak"), Status::Fail, t!("release.check.leaks").to_owned(), Some(Fix::Build))
    } else if f.compiled.is_some() {
        row(t!("release.check.leak"), Status::Ok, t!("release.check.no_leak").to_owned(), None)
    } else {
        row(t!("release.check.leak"), Status::Unknown, String::new(), None)
    });
    out.push(match &f.shipped {
        _ if f.env_cubemaps == 0 => {
            row(t!("release.check.cubemaps"), Status::Warn, t!("release.check.no_env_cubemap").to_owned(), None)
        }
        None => row(t!("release.check.cubemaps"), Status::Unknown, t!("release.check.not_released").to_owned(), None),
        Some(b) if b.cubemap_samples > 0 && b.built_cubemaps >= b.cubemap_samples => row(
            t!("release.check.cubemaps"),
            Status::Ok,
            tf!("release.check.cubemaps_built", b.built_cubemaps, b.cubemap_samples),
            None,
        ),
        Some(b) => row(
            t!("release.check.cubemaps"),
            Status::Fail,
            tf!("release.check.cubemaps_built", b.built_cubemaps, b.cubemap_samples),
            Some(Fix::GameSession),
        ),
    });
    let needs_nav = profile_for(&f.map).is_some_and(|p| p.needs_nav);
    out.push(match f.nav {
        NavState::Current => row(t!("release.check.nav"), Status::Ok, t!("release.check.nav_current").to_owned(), None),
        NavState::Stale => {
            row(t!("release.check.nav"), Status::Fail, t!("release.check.nav_stale").to_owned(), Some(Fix::GameSession))
        }
        NavState::Unknown => row(
            t!("release.check.nav"),
            Status::Warn,
            t!("release.check.nav_unknown").to_owned(),
            Some(Fix::GameSession),
        ),
        NavState::Missing => row(
            t!("release.check.nav"),
            if needs_nav { Status::Fail } else { Status::Warn },
            t!("release.check.nav_missing").to_owned(),
            Some(Fix::GameSession),
        ),
    });
    out.push(match f.missing_deps {
        None => row(
            t!("release.check.deps"),
            Status::Unknown,
            t!("release.check.deps_unknown").to_owned(),
            Some(Fix::Report),
        ),
        Some(0) => row(t!("release.check.deps"), Status::Ok, t!("release.check.deps_ok").to_owned(), None),
        Some(n) => row(t!("release.check.deps"), Status::Warn, tf!("release.check.deps_missing", n), Some(Fix::Report)),
    });
    out.push(if f.thumbnail {
        row(t!("release.check.thumbnail"), Status::Ok, String::new(), None)
    } else {
        row(
            t!("release.check.thumbnail"),
            Status::Fail,
            t!("release.check.no_thumbnail").to_owned(),
            Some(Fix::Capture),
        )
    });
    out.push(if f.icon {
        row(t!("release.check.icon"), Status::Ok, String::new(), None)
    } else {
        row(t!("release.check.icon"), Status::Warn, t!("release.check.no_icon").to_owned(), Some(Fix::Capture))
    });
    let problems = f.addon.problems();
    out.push(if problems.is_empty() {
        row(t!("release.check.addon"), Status::Ok, String::new(), None)
    } else {
        let text: Vec<String> = problems.iter().map(addon_problem_text).collect();
        row(t!("release.check.addon"), Status::Fail, text.join(", "), Some(Fix::Addon))
    });
    out.push(if !super::placement::map_name_ok(&f.map) {
        row(t!("release.check.name"), Status::Fail, t!("release.check.name_bad").to_owned(), None)
    } else if let Some(by) = &f.name_taken_by {
        row(t!("release.check.name"), Status::Fail, tf!("release.check.name_taken", by), None)
    } else {
        row(t!("release.check.name"), Status::Ok, String::new(), None)
    });
    out.push(match f.shipped.as_ref().or(f.compiled.as_ref()) {
        None => row(t!("release.check.limits"), Status::Unknown, String::new(), None),
        Some(b) => {
            let worst = b.limits.iter().max_by(|a, b| a.fraction().total_cmp(&b.fraction()));
            match worst {
                Some(w) if w.fraction() >= 1.0 => row(
                    t!("release.check.limits"),
                    Status::Fail,
                    tf!("release.check.limit_line", w.name, (w.fraction() * 100.0).round()),
                    None,
                ),
                Some(w) if w.fraction() >= 0.9 => row(
                    t!("release.check.limits"),
                    Status::Warn,
                    tf!("release.check.limit_line", w.name, (w.fraction() * 100.0).round()),
                    None,
                ),
                Some(w) => row(
                    t!("release.check.limits"),
                    Status::Ok,
                    tf!("release.check.limit_line", w.name, (w.fraction() * 100.0).round()),
                    None,
                ),
                None => row(t!("release.check.limits"), Status::Unknown, String::new(), None),
            }
        }
    });
    if let Some(p) = profile_for(&f.map) {
        let missing = missing_groups(p, f.classes.iter().map(String::as_str));
        out.push(if missing.is_empty() {
            row(t!("release.check.profile"), Status::Ok, p.name.to_owned(), None)
        } else {
            row(
                t!("release.check.profile"),
                Status::Fail,
                tf!("release.check.profile_missing", p.name, missing.join(", ")),
                None,
            )
        });
    }
    out
}

pub(crate) fn ready(rows: &[Row]) -> bool {
    rows.iter().all(|r| r.status != Status::Fail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_reflect_the_facts() {
        hp_app::register_translations();
        crate::register_translations();
        let mut f = Facts { map: "ttt_test".into(), saved: true, ..Facts::default() };
        let r = rows(&f);
        assert!(!ready(&r));
        assert_eq!(r[0].status, Status::Fail);
        assert_eq!(r[0].fix, Some(Fix::Build));
        let now = SystemTime::now();
        f.vmf_mtime = Some(now - std::time::Duration::from_secs(10));
        f.compiled_mtime = Some(now);
        let bsp = BspFacts {
            has_vis: true,
            ldr: true,
            hdr: true,
            cubemap_samples: 2,
            built_cubemaps: 2,
            ..BspFacts::default()
        };
        f.compiled = Some(bsp.clone());
        f.shipped = Some(bsp);
        f.env_cubemaps = 2;
        f.nav = NavState::Current;
        f.missing_deps = Some(0);
        f.thumbnail = true;
        f.icon = true;
        f.addon = AddonJson { title: "Test".into(), ..AddonJson::default() };
        f.classes = vec!["info_player_start".into(), "weapon_ttt_m16".into()];
        let r = rows(&f);
        assert!(ready(&r), "{r:#?}");
        assert!(r.iter().all(|x| x.status == Status::Ok || x.status == Status::Unknown), "{r:#?}");
        f.leak = true;
        f.classes.pop();
        let r = rows(&f);
        assert_eq!(r.iter().filter(|x| x.status == Status::Fail).count(), 2);
    }
}
