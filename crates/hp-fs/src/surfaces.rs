use std::collections::BTreeSet;

use crate::GameFs;

impl GameFs {
    pub fn surface_property_files(&self) -> Vec<(String, Vec<u8>)> {
        let manifest_path = "scripts/surfaceproperties_manifest.txt";
        let mut manifest = self.read(manifest_path);
        let mut paths = BTreeSet::new();
        if let Some(root) = manifest.as_ref().and_then(|bytes| hp_kv::parse_bytes(bytes).ok()) {
            for block in std::iter::once(&root).chain(root.children.iter()) {
                for (key, file) in &block.props {
                    if key.eq_ignore_ascii_case("file") {
                        paths.insert(crate::normalize_path(file).into_owned());
                    }
                }
            }
        }
        if paths.is_empty() {
            manifest = None;
            paths.insert("scripts/surfaceproperties.txt".to_owned());
        }
        let mut files: Vec<_> =
            paths.into_iter().filter_map(|path| self.read(&path).map(|bytes| (path, bytes))).collect();
        if !files.is_empty() {
            let bytes = manifest
                .unwrap_or_else(|| b"surfaceproperties_manifest { file scripts/surfaceproperties.txt }".to_vec());
            files.push((manifest_path.to_owned(), bytes));
        }
        files
    }

    pub fn surface_properties(&self) -> Vec<String> {
        let mut names = BTreeSet::from(["default".to_owned()]);
        for (path, bytes) in self.surface_property_files() {
            if path == "scripts/surfaceproperties_manifest.txt" {
                continue;
            }
            let Ok(root) = hp_kv::parse_bytes(&bytes) else { continue };
            for block in root.children {
                let name = block.name.trim();
                if !name.is_empty() && !name.chars().any(|c| c.is_control() || c == '"') {
                    names.insert(name.to_ascii_lowercase());
                }
            }
        }
        names.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TempDir;

    #[test]
    fn mounted_manifest_includes_custom_surfaces_and_deduplicates_case() {
        let dir = TempDir::new("surface-catalog");
        dir.write(
            "scripts/surfaceproperties_manifest.txt",
            br#"surfaceproperties_manifest {
            file "scripts/surfaceproperties.txt"
            file "scripts/custom_surfaces.txt"
            file "SCRIPTS/CUSTOM_SURFACES.TXT"
            file "scripts/unavailable.txt"
        }"#,
        );
        dir.write("scripts/surfaceproperties.txt", br#"default { density 2000 } Metal { base default }"#);
        dir.write(
            "scripts/custom_surfaces.txt",
            br#"metal { friction 0.3 } custom_foam { base default } custom_bell { base metal }"#,
        );
        let fs = GameFs::empty();
        fs.add_game_dir(dir.path()).unwrap();
        assert_eq!(fs.surface_properties(), ["custom_bell", "custom_foam", "default", "metal"]);
        assert_eq!(GameFs::empty().surface_properties(), ["default"]);
    }

    #[test]
    fn invalid_manifest_is_replaced_by_a_valid_default_manifest() {
        let dir = TempDir::new("surface-invalid-manifest");
        dir.write("scripts/surfaceproperties_manifest.txt", b"broken {");
        dir.write("scripts/surfaceproperties.txt", b"default { density 2000 } custom { base default }");
        let fs = GameFs::empty();
        fs.add_game_dir(dir.path()).unwrap();
        let files = fs.surface_property_files();
        let (_, manifest) = files.iter().find(|(name, _)| name.ends_with("_manifest.txt")).unwrap();
        assert!(hp_kv::parse_bytes(manifest).is_ok());
        assert_eq!(fs.surface_properties(), ["custom", "default"]);
    }
}
