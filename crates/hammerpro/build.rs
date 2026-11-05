use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    let seconds = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or_else(|| SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()));
    let (y, m, d) = civil_from_days((seconds / 86_400) as i64);
    println!("cargo:rustc-env=HP_BUILD_DATE={y:04}-{m:02}-{d:02}");

    let manifest = std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from).unwrap_or_default();
    embed_icon(&manifest);
    main_stack();
    let git = manifest.join("..").join("..").join(".git");
    if let Some(dir) = git_dir(&git) {
        let head = dir.join("HEAD");
        println!("cargo:rerun-if-changed={}", head.display());
        for path in [dir.join("refs"), dir.join("packed-refs")] {
            if path.exists() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
        if let Some((commit, reference)) = head_commit(&dir) {
            if let Some(reference) = reference {
                println!("cargo:rerun-if-changed={}", dir.join(reference).display());
            }
            println!("cargo:rustc-env=HP_BUILD_COMMIT={commit}");
        }
    }
}

fn git_dir(dot_git: &Path) -> Option<PathBuf> {
    if dot_git.is_dir() {
        return Some(dot_git.to_path_buf());
    }
    let text = read_small(dot_git)?;
    let target = text.trim().strip_prefix("gitdir:")?.trim();
    let path = Path::new(target);
    let path = if path.is_absolute() { path.to_path_buf() } else { dot_git.parent()?.join(path) };
    path.is_dir().then_some(path)
}

fn head_commit(dir: &Path) -> Option<(String, Option<String>)> {
    let head = read_small(&dir.join("HEAD"))?;
    let head = head.trim();
    let Some(reference) = head.strip_prefix("ref:").map(str::trim) else {
        return short_hash(head).map(|h| (h, None));
    };
    if reference.split('/').any(|part| part.is_empty() || part == "." || part == "..") || reference.contains('\\') {
        return None;
    }
    if let Some(hash) = read_small(&dir.join(reference)).and_then(|t| short_hash(t.trim())) {
        return Some((hash, Some(reference.to_owned())));
    }
    let packed = read_small(&dir.join("packed-refs"))?;
    packed
        .lines()
        .filter_map(|line| line.split_once(' '))
        .find(|(_, name)| name.trim() == reference)
        .and_then(|(hash, _)| short_hash(hash))
        .map(|h| (h, None))
}

fn short_hash(text: &str) -> Option<String> {
    let ok = (text.len() == 40 || text.len() == 64) && text.bytes().all(|b| b.is_ascii_hexdigit());
    ok.then(|| text[..7].to_ascii_lowercase())
}

fn read_small(path: &Path) -> Option<String> {
    let len = std::fs::metadata(path).ok()?.len();
    if len > 1 << 20 {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

const MAIN_STACK: u64 = 8 << 20;

fn main_stack() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("windows") && target.contains("msvc") {
        println!("cargo:rustc-link-arg-bins=/STACK:{MAIN_STACK}");
    } else if target.contains("windows") {
        println!("cargo:rustc-link-arg-bins=-Wl,--stack,{MAIN_STACK}");
    }
}

fn embed_icon(manifest: &Path) {
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") || !target.contains("msvc") {
        return;
    }
    let ico_path = manifest.join("..").join("..").join("assets").join("hammer-pro.ico");
    println!("cargo:rerun-if-changed={}", ico_path.display());
    let Ok(ico) = std::fs::read(&ico_path) else { return };
    let Some(res) = icon_res(&ico) else {
        println!("cargo:warning=assets/hammer-pro.ico could not be read: the exe has no icon");
        return;
    };
    let out = std::env::var_os("OUT_DIR").map(PathBuf::from).unwrap_or_default().join("hammerpro_icon.res");
    if std::fs::write(&out, res).is_ok() {
        println!("cargo:rustc-link-arg-bins={}", out.display());
    }
}

fn icon_res(ico: &[u8]) -> Option<Vec<u8>> {
    const RT_ICON: u16 = 3;
    const RT_GROUP_ICON: u16 = 14;
    let u16_at = |at: usize| Some(u16::from_le_bytes([*ico.get(at)?, *ico.get(at + 1)?]));
    let u32_at = |at: usize| Some(u32::from_le_bytes(ico.get(at..at + 4)?.try_into().ok()?));
    if u16_at(2)? != 1 {
        return None;
    }
    let count = usize::from(u16_at(4)?);
    let mut res = Vec::new();
    resource(&mut res, 0, 0, &[]);
    let mut group = Vec::new();
    group.extend_from_slice(&[0, 0, 1, 0]);
    group.extend_from_slice(&(count as u16).to_le_bytes());
    for i in 0..count {
        let e = 6 + 16 * i;
        let size = u32_at(e + 8)? as usize;
        let offset = u32_at(e + 12)? as usize;
        let image = ico.get(offset..offset.checked_add(size)?)?;
        let id = (i + 1) as u16;
        resource(&mut res, RT_ICON, id, image);
        group.extend_from_slice(ico.get(e..e + 12)?);
        group.extend_from_slice(&id.to_le_bytes());
    }
    resource(&mut res, RT_GROUP_ICON, 1, &group);
    Some(res)
}

fn resource(res: &mut Vec<u8>, kind: u16, name: u16, data: &[u8]) {
    res.extend_from_slice(&(data.len() as u32).to_le_bytes());
    res.extend_from_slice(&32u32.to_le_bytes());
    res.extend_from_slice(&[0xff, 0xff]);
    res.extend_from_slice(&kind.to_le_bytes());
    res.extend_from_slice(&[0xff, 0xff]);
    res.extend_from_slice(&name.to_le_bytes());
    res.extend_from_slice(&0u32.to_le_bytes());
    let flags: u16 = if kind == 0 { 0 } else { 0x1030 };
    res.extend_from_slice(&flags.to_le_bytes());
    res.extend_from_slice(&0u16.to_le_bytes());
    res.extend_from_slice(&0u32.to_le_bytes());
    res.extend_from_slice(&0u32.to_le_bytes());
    res.extend_from_slice(data);
    while !res.len().is_multiple_of(4) {
        res.push(0);
    }
}
