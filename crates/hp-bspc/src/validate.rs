use crate::emit::lump;

fn i32_at(d: &[u8], o: usize) -> Option<i32> {
    d.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn i16_at(d: &[u8], o: usize) -> Option<i16> {
    d.get(o..o + 2).map(|b| i16::from_le_bytes([b[0], b[1]]))
}

fn f32_at(d: &[u8], o: usize) -> Option<f32> {
    d.get(o..o + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

pub fn validate(bytes: &[u8]) -> Vec<String> {
    let mut errors = Vec::new();
    let bsp = match hp_bsp::Bsp::from_vec(bytes.to_vec()) {
        Ok(b) => b,
        Err(e) => return vec![format!("the BSP cannot be read: {e}")],
    };
    let get = |i: usize| bsp.lump(i).map(|c| c.into_owned()).unwrap_or_default();
    let planes = get(lump::PLANES);
    let nodes = get(lump::NODES);
    let leafs = get(lump::LEAFS);
    let faces = get(lump::FACES);
    let edges = get(lump::EDGES);
    let surfedges = get(lump::SURFEDGES);
    let verts = get(lump::VERTEXES);
    let texinfo = get(lump::TEXINFO);
    let texdata = get(lump::TEXDATA);
    let table = get(lump::TEXDATA_STRING_TABLE);
    let strings = get(lump::TEXDATA_STRING_DATA);
    let models = get(lump::MODELS);
    let leaffaces = get(lump::LEAFFACES);
    let leafbrushes = get(lump::LEAFBRUSHES);
    let brushes = get(lump::BRUSHES);
    let sides = get(lump::BRUSHSIDES);
    let dispinfo = get(lump::DISPINFO);
    let dispverts = get(lump::DISP_VERTS);
    let disptris = get(lump::DISP_TRIS);
    let vis = get(lump::VISIBILITY);
    let areas = get(lump::AREAS);

    let (np, nn, nl, nf, ne, nse, nv) = (
        planes.len() / 20,
        nodes.len() / 32,
        leafs.len() / 32,
        faces.len() / 56,
        edges.len() / 4,
        surfedges.len() / 4,
        verts.len() / 12,
    );
    let (nti, ntd, nm, nlf, nlb, nb, nbs, nd) = (
        texinfo.len() / 72,
        texdata.len() / 32,
        models.len() / 48,
        leaffaces.len() / 2,
        leafbrushes.len() / 2,
        brushes.len() / 12,
        sides.len() / 8,
        dispinfo.len() / 176,
    );
    let clusters = i32_at(&vis, 0).unwrap_or(0).max(0) as usize;
    let n_areas = areas.len() / 8;
    let mut err = |m: String| {
        if errors.len() < 200 {
            errors.push(m);
        }
    };
    use hp_bsp::limits as lim;
    for (name, used, max) in [
        ("planes", np, lim::PLANES.max_usize()),
        ("nodes", nn, lim::NODES.max_usize()),
        ("leaves", nl, lim::LEAFS.max_usize()),
        ("faces", nf, lim::FACES.max_usize()),
        ("vertices", nv, lim::VERTEXES.max_usize()),
        ("edges", ne, lim::EDGES.max_usize()),
        ("surface edges", nse, lim::SURFEDGES.max_usize()),
        ("texture infos", nti, lim::TEXINFO.max_usize()),
        ("leaf faces", nlf, lim::LEAFFACES.max_usize()),
        ("leaf brushes", nlb, lim::LEAFBRUSHES.max_usize()),
        ("brushes", nb, lim::BRUSHES.max_usize()),
        ("brush sides", nbs, lim::BRUSHSIDES.max_usize()),
    ] {
        if used > max {
            err(format!("too many {name}: {used} of {max}"));
        }
    }
    for i in 0..np {
        let n = [f32_at(&planes, i * 20), f32_at(&planes, i * 20 + 4), f32_at(&planes, i * 20 + 8)];
        let len: f32 = n.iter().map(|v| v.unwrap_or(0.0).powi(2)).sum::<f32>().sqrt();
        if (len - 1.0).abs() > 0.01 {
            err(format!("plane {i} has a bad normal"));
        }
    }
    let child_ok = |c: i32| if c >= 0 { (c as usize) < nn } else { ((-1 - c) as usize) < nl };
    for i in 0..nn {
        let o = i * 32;
        let p = i32_at(&nodes, o).unwrap_or(-1);
        if p < 0 || p as usize >= np {
            err(format!("node {i}: bad plane {p}"));
        }
        for c in [i32_at(&nodes, o + 4).unwrap_or(0), i32_at(&nodes, o + 8).unwrap_or(0)] {
            if !child_ok(c) {
                err(format!("node {i}: bad child {c}"));
            }
        }
        let (ff, fc) = (u16_at(&nodes, o + 24).unwrap_or(0) as usize, u16_at(&nodes, o + 26).unwrap_or(0) as usize);
        if ff + fc > nf {
            err(format!("node {i}: faces {ff}+{fc} out of {nf}"));
        }
    }
    for i in 0..nl {
        let o = i * 32;
        let cluster = i16_at(&leafs, o + 4).unwrap_or(-1);
        let area = (u16_at(&leafs, o + 6).unwrap_or(0) & 0x1ff) as usize;
        if cluster >= 0 && clusters > 0 && cluster as usize >= clusters {
            err(format!("leaf {i}: cluster {cluster} of {clusters}"));
        }
        if area >= n_areas.max(1) {
            err(format!("leaf {i}: area {area} of {n_areas}"));
        }
        let (lf, lc) = (u16_at(&leafs, o + 20).unwrap_or(0) as usize, u16_at(&leafs, o + 22).unwrap_or(0) as usize);
        if lf + lc > nlf {
            err(format!("leaf {i}: leaf faces {lf}+{lc} out of {nlf}"));
        }
        let (bf, bc) = (u16_at(&leafs, o + 24).unwrap_or(0) as usize, u16_at(&leafs, o + 26).unwrap_or(0) as usize);
        if bf + bc > nlb {
            err(format!("leaf {i}: leaf brushes {bf}+{bc} out of {nlb}"));
        }
    }
    for i in 0..nlf {
        if u16_at(&leaffaces, i * 2).unwrap_or(0) as usize >= nf {
            err(format!("leaf face {i} points past the faces"));
        }
    }
    for i in 0..nlb {
        if u16_at(&leafbrushes, i * 2).unwrap_or(0) as usize >= nb {
            err(format!("leaf brush {i} points past the brushes"));
        }
    }
    for i in 0..ne {
        let (a, b) = (u16_at(&edges, i * 4).unwrap_or(0) as usize, u16_at(&edges, i * 4 + 2).unwrap_or(0) as usize);
        if a >= nv || b >= nv {
            err(format!("edge {i}: vertex out of range"));
        }
    }
    for i in 0..nf {
        let o = i * 56;
        let plane = u16_at(&faces, o).unwrap_or(0) as usize;
        let first = i32_at(&faces, o + 4).unwrap_or(-1);
        let count = i16_at(&faces, o + 8).unwrap_or(0);
        let ti = i16_at(&faces, o + 10).unwrap_or(-1);
        let di = i16_at(&faces, o + 12).unwrap_or(-1);
        if plane >= np {
            err(format!("face {i}: bad plane"));
        }
        if first < 0 || count < 3 || first as usize + count as usize > nse {
            err(format!("face {i}: bad edges {first}+{count}"));
            continue;
        }
        if ti < 0 || ti as usize >= nti {
            err(format!("face {i}: bad texture info {ti}"));
        }
        if di >= 0 && di as usize >= nd {
            err(format!("face {i}: bad displacement {di}"));
        }
        let vert = |k: usize| -> Option<(u16, u16)> {
            let se = i32_at(&surfedges, (first as usize + k) * 4)?;
            let e = se.unsigned_abs() as usize;
            if e >= ne {
                return None;
            }
            let (a, b) = (u16_at(&edges, e * 4)?, u16_at(&edges, e * 4 + 2)?);
            Some(if se < 0 { (b, a) } else { (a, b) })
        };
        let n = count as usize;
        for k in 0..n {
            match (vert(k), vert((k + 1) % n)) {
                (Some((_, end)), Some((start, _))) if end == start => {}
                _ => {
                    err(format!("face {i}: its edges do not close"));
                    break;
                }
            }
        }
    }
    for i in 0..nti {
        let td = i32_at(&texinfo, i * 72 + 68).unwrap_or(-1);
        if td < 0 || td as usize >= ntd {
            err(format!("texture info {i}: bad texture data {td}"));
        }
    }
    for i in 0..ntd {
        let id = i32_at(&texdata, i * 32 + 12).unwrap_or(-1);
        let ofs = (id >= 0).then(|| i32_at(&table, id as usize * 4)).flatten();
        if ofs.is_none_or(|o| o < 0 || o as usize >= strings.len()) {
            err(format!("texture data {i}: bad name"));
        }
    }
    for i in 0..nm {
        let o = i * 48;
        let head = i32_at(&models, o + 36).unwrap_or(-1);
        let (ff, fc) = (i32_at(&models, o + 40).unwrap_or(0), i32_at(&models, o + 44).unwrap_or(0));
        if head < 0 || head as usize >= nn {
            err(format!("model {i}: bad head node {head}"));
        }
        if ff < 0 || fc < 0 || (ff + fc) as usize > nf {
            err(format!("model {i}: faces {ff}+{fc} out of {nf}"));
        }
    }
    for i in 0..nb {
        let (f, c) = (i32_at(&brushes, i * 12).unwrap_or(-1), i32_at(&brushes, i * 12 + 4).unwrap_or(0));
        if f < 0 || c < 4 || (f + c) as usize > nbs {
            err(format!("brush {i}: bad sides {f}+{c}"));
        }
    }
    for i in 0..nbs {
        let p = u16_at(&sides, i * 8).unwrap_or(0) as usize;
        let t = i16_at(&sides, i * 8 + 2).unwrap_or(-1);
        if p >= np || t as i64 >= nti as i64 {
            err(format!("brush side {i}: bad plane or texture info"));
        }
    }
    let mut multiblend_verts = 0usize;
    for i in 0..nd {
        let o = i * 176;
        let vs = i32_at(&dispinfo, o + 12).unwrap_or(-1);
        let ts = i32_at(&dispinfo, o + 16).unwrap_or(-1);
        let power = i32_at(&dispinfo, o + 20).unwrap_or(0);
        let face = u16_at(&dispinfo, o + 36).unwrap_or(0) as usize;
        if !(1..=4).contains(&power) {
            err(format!("displacement {i}: bad power {power}"));
            continue;
        }
        let n = (1usize << power) + 1;
        if vs < 0 || vs as usize + n * n > dispverts.len() / 20 {
            err(format!("displacement {i}: vertices out of range"));
        }
        if ts < 0 || ts as usize + 2 * (n - 1) * (n - 1) > disptris.len() / 2 {
            err(format!("displacement {i}: triangles out of range"));
        }
        if face >= nf || i16_at(&faces, face * 56 + 12) != Some(i as i16) {
            err(format!("displacement {i}: its face does not point back to it"));
        }
        let tess = i32_at(&dispinfo, o + 24).unwrap_or(0);
        if tess < 0 && tess & crate::disp::HAS_MULTIBLEND_BIT != 0 {
            multiblend_verts += n * n;
        }
    }
    let multiblend = bsp.lump(hp_bsp::lump::DISP_MULTIBLEND).map_or(0, |c| c.len());
    if multiblend_verts > 0 && multiblend != multiblend_verts * crate::disp::MULTIBLEND_ENTRY_SIZE {
        err(format!(
            "displacement multiblend: {multiblend} bytes for {multiblend_verts} vertices of {} bytes",
            crate::disp::MULTIBLEND_ENTRY_SIZE
        ));
    }
    if nl > 0 && clusters == 0 && vis.len() >= 4 {}
    if let Ok(g) = bsp.game_lumps()
        && let Some(sp) = g.get(u32::from_be_bytes(*b"sprp"))
        && let Ok(d) = sp.data()
    {
        let names = i32_at(&d, 0).unwrap_or(0).max(0) as usize;
        let at = 4 + names * 128;
        let leaves = i32_at(&d, at).unwrap_or(0).max(0) as usize;
        for l in 0..leaves {
            if u16_at(&d, at + 4 + l * 2).unwrap_or(0) as usize >= nl {
                err(format!("static prop leaf {l} out of range"));
                break;
            }
        }
        match hp_bsp::staticprops::StaticPropLump::parse(&d, sp.version, None) {
            Err(e) => err(format!("static props: {e}")),
            Ok((lump, _)) => {
                if let Some(k) = lump.props.iter().position(|p| {
                    usize::from(p.prop_type) >= names || usize::from(p.first_leaf) + usize::from(p.leaf_count) > leaves
                }) {
                    err(format!("static prop {k}: bad model or leaves"));
                }
            }
        }
    }
    let areaportals = get(lump::AREAPORTALS);
    let clipverts = get(lump::CLIPPORTALVERTS);
    let (nap, ncv) = (areaportals.len() / 12, clipverts.len() / 12);
    if n_areas > 256 {
        err(format!("too many areas: {n_areas} of 256"));
    }
    if nap > 1024 {
        err(format!("too many areaportals: {nap} of 1024"));
    }
    for a in 0..n_areas {
        let (num, first) = (i32_at(&areas, a * 8).unwrap_or(-1), i32_at(&areas, a * 8 + 4).unwrap_or(-1));
        if num < 0 || first < 0 || (first + num) as usize > nap {
            err(format!("area {a}: areaportals {first}+{num} out of {nap}"));
        }
    }
    for p in 1..nap {
        let o = p * 12;
        let other = u16_at(&areaportals, o + 2).unwrap_or(0) as usize;
        let (fv, cv) =
            (u16_at(&areaportals, o + 4).unwrap_or(0) as usize, u16_at(&areaportals, o + 6).unwrap_or(0) as usize);
        let plane = i32_at(&areaportals, o + 8).unwrap_or(-1);
        if other == 0 || other >= n_areas {
            err(format!("areaportal {p}: bad area {other} of {n_areas}"));
        }
        if fv + cv > ncv {
            err(format!("areaportal {p}: clip vertices {fv}+{cv} out of {ncv}"));
        }
        if plane < 0 || plane as usize >= np {
            err(format!("areaportal {p}: bad plane {plane}"));
        }
    }
    let occlusion = get(lump::OCCLUSION);
    let mut n_occluders = 0usize;
    if !occlusion.is_empty() {
        let n = i32_at(&occlusion, 0).unwrap_or(-1);
        let at_polys = 4 + n.max(0) as usize * 40;
        let np_occ = i32_at(&occlusion, at_polys).unwrap_or(-1);
        let at_idx = at_polys + 4 + np_occ.max(0) as usize * 12;
        let ni = i32_at(&occlusion, at_idx).unwrap_or(-1);
        if n < 0 || np_occ < 0 || ni < 0 || at_idx + 4 + ni as usize * 4 != occlusion.len() {
            err("occlusion lump: bad counts".to_owned());
        } else {
            n_occluders = n as usize;
            let (np_occ, ni) = (np_occ as usize, ni as usize);
            for k in 0..n_occluders {
                let o = 4 + k * 40;
                let (fp, pc) = (i32_at(&occlusion, o + 4).unwrap_or(-1), i32_at(&occlusion, o + 8).unwrap_or(-1));
                let area = i32_at(&occlusion, o + 36).unwrap_or(0);
                if fp < 0 || pc < 0 || (fp + pc) as usize > np_occ {
                    err(format!("occluder {k}: polygons {fp}+{pc} out of {np_occ}"));
                }
                if area < -1 || area >= n_areas as i32 {
                    err(format!("occluder {k}: bad area {area}"));
                }
            }
            for k in 0..np_occ {
                let o = at_polys + 4 + k * 12;
                let (fv, vc, plane) = (
                    i32_at(&occlusion, o).unwrap_or(-1),
                    i32_at(&occlusion, o + 4).unwrap_or(-1),
                    i32_at(&occlusion, o + 8).unwrap_or(-1),
                );
                if fv < 0 || vc < 3 || (fv + vc) as usize > ni {
                    err(format!("occluder polygon {k}: vertices {fv}+{vc} out of {ni}"));
                }
                if plane < 0 || plane as usize >= np {
                    err(format!("occluder polygon {k}: bad plane {plane}"));
                }
            }
            for k in 0..ni {
                let v = i32_at(&occlusion, at_idx + 4 + k * 4).unwrap_or(-1);
                if v < 0 || v as usize >= nv {
                    err(format!("occluder vertex index {k}: vertex {v} out of {nv}"));
                    break;
                }
            }
        }
    }
    if let Ok(g) = bsp.game_lumps() {
        use crate::detail_props as dp;
        let mut style_records = Vec::new();
        for (id, name) in [(dp::DPLT, "LDR"), (dp::DPLH, "HDR")] {
            let Some(l) = g.get(id) else { continue };
            if l.version != dp::DPL_VERSION {
                err(format!("{name} detail light styles: version {}", l.version));
            }
            match l.data().map_err(|e| e.to_string()).and_then(|d| dp::parse_styles(&d)) {
                Ok(s) => {
                    if s.iter().any(|(_, style)| *style >= 64) {
                        err(format!("{name} detail light styles: style past 63"));
                    }
                    style_records.push(s.len());
                }
                Err(e) => err(format!("{name} {e}")),
            }
        }
        if let Some(l) = g.get(dp::DPRP) {
            if l.version != dp::DPRP_VERSION {
                err(format!("detail props: version {} (the engine reads {} only)", l.version, dp::DPRP_VERSION));
            } else {
                match l.data().map_err(|e| e.to_string()).and_then(|d| dp::DetailLump::parse(&d)) {
                    Err(e) => err(e),
                    Ok(lump) => {
                        if lump.models.iter().any(String::is_empty) {
                            err("detail props: a model without a name".to_owned());
                        }
                        let records = style_records.iter().copied().max().unwrap_or(0);
                        let mut last = 0u16;
                        for (k, o) in lump.objects.iter().enumerate() {
                            let dict = match o.kind {
                                dp::kind::MODEL => lump.models.len(),
                                dp::kind::SPRITE | dp::kind::SHAPE_CROSS | dp::kind::SHAPE_TRI => lump.sprites.len(),
                                _ => 0,
                            };
                            let problem = if o.leaf < last {
                                Some("not sorted by leaf".to_owned())
                            } else if usize::from(o.leaf) >= nl {
                                Some(format!("leaf {} of {nl}", o.leaf))
                            } else if usize::from(o.dict) >= dict {
                                Some(format!("type {} model {} of {dict}", o.kind, o.dict))
                            } else if o.orientation > 2 {
                                Some(format!("orientation {}", o.orientation))
                            } else if o.light_style_count > 0
                                && o.light_styles as usize + usize::from(o.light_style_count) > records
                            {
                                Some(format!("light styles {}+{} of {records}", o.light_styles, o.light_style_count))
                            } else if !(o.origin.is_finite() && o.angles.is_finite() && o.scale.is_finite()) {
                                Some("bad position".to_owned())
                            } else {
                                None
                            };
                            if let Some(p) = problem {
                                err(format!("detail prop {k}: {p}"));
                                break;
                            }
                            last = o.leaf;
                        }
                    }
                }
            }
        }
    }
    for e in bsp.entities().unwrap_or_default() {
        if let Some(m) = e.get("model").and_then(|m| m.strip_prefix('*'))
            && m.parse::<usize>().is_ok_and(|k| k >= nm)
        {
            err(format!("entity {}: model *{m} does not exist", e.classname()));
        }
        if let Some(k) = e.get("portalnumber").and_then(|v| v.trim().parse::<i64>().ok())
            && (k < 0 || k as usize > nap)
        {
            err(format!("entity {}: portalnumber {k} out of {nap} areaportals", e.classname()));
        }
        if let Some(k) = e.get("occludernumber").and_then(|v| v.trim().parse::<i64>().ok())
            && (k < 0 || k as usize >= n_occluders)
        {
            err(format!("entity {}: occludernumber {k} out of {n_occluders} occluders", e.classname()));
        }
    }
    let cubemaps = get(lump::CUBEMAPS);
    let nc = cubemaps.len() / 16;
    if !cubemaps.len().is_multiple_of(16) {
        err(format!("cubemaps: {} bytes is not a whole number of samples", cubemaps.len()));
    }
    if nc > crate::cubemap::MAX_SAMPLES {
        err(format!("too many cubemaps: {nc} of {}", crate::cubemap::MAX_SAMPLES));
    }
    let mut suffixes = Vec::with_capacity(nc);
    for i in 0..nc {
        let o: Vec<i32> = (0..4).map(|k| i32_at(&cubemaps, i * 16 + k * 4).unwrap_or(0)).collect();
        if o[..3].iter().any(|v| v.abs() > 32768) || !(0..=16).contains(&o[3]) {
            err(format!("cubemap {i}: bad origin or size ({} {} {}, {})", o[0], o[1], o[2], o[3]));
        }
        suffixes.push(format!("_{}_{}_{}", o[0], o[1], o[2]));
    }
    if nc > 0 {
        let pak = bsp.pakfile().ok();
        for i in 0..ntd {
            let id = i32_at(&texdata, i * 32 + 12).unwrap_or(-1);
            let Some(ofs) = (id >= 0).then(|| i32_at(&table, id as usize * 4)).flatten().filter(|o| *o >= 0) else {
                continue;
            };
            let Some(bytes) = strings.get(ofs as usize..) else { continue };
            let name = String::from_utf8_lossy(&bytes[..bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len())])
                .to_ascii_lowercase()
                .replace('\\', "/");
            if !name.starts_with("maps/") || !suffixes.iter().any(|s| name.ends_with(s.as_str())) {
                continue;
            }
            let vmt = format!("materials/{name}.vmt");
            if !pak.as_ref().is_some_and(|p| p.contains(&vmt)) {
                err(format!("texture data {i}: the cubemap material {vmt} is not in the pakfile"));
            }
        }
    }
    if nm == 0 {
        err("no world model".to_owned());
    }
    errors
}
