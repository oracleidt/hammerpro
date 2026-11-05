use crate::materials::MaterialSource;

pub(super) fn face(source: &dyn MaterialSource, sky: &str, side: &str, hdr: bool) -> Option<hp_vtf::VtfInfo> {
    let regular = format!("skybox/{sky}{side}");
    let names = if hdr { vec![format!("skybox/{sky}_hdr{side}"), regular] } else { vec![regular] };
    names.iter().find_map(|name| {
        let data = source.read_file(&format!("materials/{name}.vmt"))?;
        let mut material = hp_vmt::parse(&super::text_of(&data)).ok()?;
        if material.is_patch() {
            let load = |path: &str| source.read_file(path).map(|bytes| super::text_of(&bytes));
            material = hp_vmt::resolve_patch(&material, &load).ok()?;
        }
        let material = material.resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, hdr);
        let keys: &[&str] =
            if hdr { &["$hdrcompressedtexture", "$hdrbasetexture", "$basetexture"] } else { &["$basetexture"] };
        keys.iter().find_map(|key| {
            let texture = material.get(key)?.trim();
            let path = hp_vmt::normalize_texture_path(texture);
            let data = source.read_file(&format!("materials/{path}.vtf"))?;
            let info = hp_vtf::read_info(&data).ok()?;
            hp_vtf::raw_level(&data, info.mip_for_max_dimension(4), 0, 0).ok()?;
            Some(info)
        })
    })
}

pub(super) fn renderable(source: &dyn MaterialSource, sky: &str) -> bool {
    [false, true]
        .into_iter()
        .any(|hdr| ["rt", "lf", "bk", "ft", "up", "dn"].iter().all(|side| face(source, sky, side, hdr).is_some()))
}
