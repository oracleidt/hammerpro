use std::path::{Path, PathBuf};

use crate::custom::CustomDir;
use crate::mount::{PlanItem, PlanKind, dedupe_items};
use crate::source::SourceOrigin;
use crate::test_util::TempDir;

fn dir_item(path: &Path, label: &str, origin: SourceOrigin) -> PlanItem {
    PlanItem::new(PlanKind::GameDir, path.to_path_buf(), label.to_owned(), origin)
}

#[test]
fn the_same_folder_declared_twice_is_mounted_once_in_first_priority() {
    let tmp = TempDir::new("dedupe-same");
    let pack = tmp.mkdir("pack/materials").parent().unwrap().to_path_buf();
    tmp.write("other/materials/x.vmt", b"");
    let other = tmp.path().join("other");
    let items = vec![
        PlanItem::custom(&CustomDir::new(pack.clone())),
        dir_item(&other, "Other", SourceOrigin::Mounted),
        dir_item(&pack, "Mount: pack", SourceOrigin::Mounted),
        dir_item(&other, "Extra: other", SourceOrigin::Extra),
    ];
    let out = dedupe_items(items);
    let labels: Vec<&str> = out.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels, ["Custom: pack", "Other"]);
}

#[test]
fn spellings_of_one_path_collapse() {
    let tmp = TempDir::new("dedupe-spell");
    let dir = tmp.mkdir("Game Dir/materials").parent().unwrap().to_path_buf();
    let text = dir.to_string_lossy().into_owned();
    let slashes = text.replace('\\', "/");
    let trailing = format!("{text}{}", std::path::MAIN_SEPARATOR);
    let dotted = dir.join("..").join("Game Dir").join(".");
    let mut spellings = vec![dir.clone(), PathBuf::from(&slashes), PathBuf::from(&trailing), dotted];
    if cfg!(windows) {
        spellings.push(PathBuf::from(text.to_uppercase()));
        spellings.push(PathBuf::from(format!("\\\\?\\{text}")));
    }
    let items: Vec<PlanItem> =
        spellings.iter().enumerate().map(|(i, p)| dir_item(p, &format!("n{i}"), SourceOrigin::Extra)).collect();
    let out = dedupe_items(items);
    assert_eq!(out.len(), 1, "{:?}", out.iter().map(|i| i.path.clone()).collect::<Vec<_>>());
    assert_eq!(out[0].label, "n0");
}

#[test]
fn spellings_of_a_missing_path_collapse_too() {
    let tmp = TempDir::new("dedupe-missing");
    let base = tmp.path().join("not").join("there");
    let spelled = tmp.path().join("not").join("x").join("..").join("there").join("");
    let items = vec![dir_item(&base, "a", SourceOrigin::Extra), dir_item(&spelled, "b", SourceOrigin::Extra)];
    assert_eq!(dedupe_items(items).len(), 1);
}

#[cfg(windows)]
#[test]
fn missing_paths_under_a_canonical_ancestor_share_the_same_identity() {
    let tmp = TempDir::new("dedupe-missing-alias");
    let logical = tmp.path().join("missing").join("materials");
    let physical = std::fs::canonicalize(tmp.path()).unwrap().join("missing").join("materials");
    let dotted = tmp.path().join("missing").join("unused").join("..").join("materials");
    let extended = PathBuf::from(format!("\\\\?\\{}", dotted.display()));
    let items = vec![
        dir_item(&logical, "logical", SourceOrigin::Extra),
        dir_item(&physical, "physical", SourceOrigin::Extra),
        dir_item(&extended, "extended", SourceOrigin::Extra),
    ];
    let out = dedupe_items(items);
    assert_eq!(out.len(), 1, "{:?}", out.iter().map(|item| &item.path).collect::<Vec<_>>());
    assert_eq!(out[0].label, "logical");
}

#[test]
fn different_kinds_and_folders_are_kept() {
    let tmp = TempDir::new("dedupe-kinds");
    tmp.write("a/pak01_dir.vpk", b"");
    tmp.mkdir("b");
    let a = tmp.path().join("a");
    let b = tmp.path().join("b");
    let items = vec![
        PlanItem::new(PlanKind::Vpk, a.join("pak01_dir.vpk"), "pak".to_owned(), SourceOrigin::Mounted),
        dir_item(&a, "a", SourceOrigin::Mounted),
        dir_item(&b, "b", SourceOrigin::Mounted),
        PlanItem::new(PlanKind::Vpk, a.join("PAK01_DIR.VPK"), "pak again".to_owned(), SourceOrigin::Extra),
    ];
    let out = dedupe_items(items);
    let labels: Vec<&str> = out.iter().map(|i| i.label.as_str()).collect();
    let expected: &[&str] = if cfg!(windows) { &["pak", "a", "b"] } else { &["pak", "a", "b", "pak again"] };
    assert_eq!(labels, expected);
}

#[test]
fn a_plain_image_folder_keeps_both_views() {
    let tmp = TempDir::new("dedupe-images");
    tmp.write("imgs/wall.png", b"");
    let imgs = tmp.path().join("imgs");
    let items =
        vec![PlanItem::custom(&CustomDir::new(imgs.clone())), dir_item(&imgs, "Mount: imgs", SourceOrigin::Mounted)];
    assert_eq!(dedupe_items(items).len(), 2);
}

#[test]
fn an_enabled_duplicate_re_enables_a_disabled_first_declaration() {
    let tmp = TempDir::new("dedupe-enabled");
    let dir = tmp.mkdir("d/materials").parent().unwrap().to_path_buf();
    let mut off = dir_item(&dir, "first", SourceOrigin::Extra);
    off.enabled = false;
    let out = dedupe_items(vec![off, dir_item(&dir, "second", SourceOrigin::Extra)]);
    assert_eq!(out.len(), 1);
    assert!(out[0].enabled);
    assert_eq!(out[0].label, "first");
}

#[test]
fn adding_a_mounted_folder_again_returns_the_existing_sources() {
    use crate::fs::GameFs;
    let tmp = TempDir::new("dedupe-add");
    tmp.write("pack/materials/a.vmt", b"x");
    tmp.write("imgs/wall.png", b"x");
    let pack = tmp.path().join("pack");
    let fs = GameFs::empty();
    let first = fs.add_game_dir(&pack).unwrap();
    let count = fs.sources().len();
    let name = if cfg!(windows) { "PACK" } else { "pack" };
    let spelled = tmp.path().join("pack").join("..").join(name).join("");
    let second = fs.add_game_dir(&spelled).unwrap();
    assert_eq!(first, second);
    assert_eq!(fs.sources().len(), count);
    let custom = fs.add_custom_dir(CustomDir::new(tmp.path().join("imgs"))).unwrap();
    let again = fs.add_custom_dir(CustomDir::new(tmp.path().join("imgs"))).unwrap();
    assert_eq!(custom, again);
    assert_eq!(fs.sources().len(), count + 1);
}
