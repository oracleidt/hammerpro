use hp_bsp::Bsp;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

fn i32_at(d: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(d[at..at + 4].try_into().unwrap())
}
fn i16_at(d: &[u8], at: usize) -> i16 {
    i16::from_le_bytes(d[at..at + 2].try_into().unwrap())
}
fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(d[at..at + 2].try_into().unwrap())
}
fn f32_at(d: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(d[at..at + 4].try_into().unwrap())
}

struct View {
    planes: Vec<u8>,
    brushes: Vec<u8>,
    sides: Vec<u8>,
    texinfo: Vec<u8>,
    texdata: Vec<u8>,
    strings: Vec<u8>,
    table: Vec<u8>,
    nodes: Vec<u8>,
    leafs: Vec<u8>,
    leafbrushes: Vec<u8>,
    models: Vec<u8>,
}

impl View {
    fn new(b: &Bsp) -> View {
        let g = |i| b.lump(i).unwrap().into_owned();
        View {
            planes: g(1),
            brushes: g(18),
            sides: g(19),
            texinfo: g(6),
            texdata: g(2),
            strings: g(43),
            table: g(44),
            nodes: g(5),
            leafs: g(10),
            leafbrushes: g(17),
            models: g(14),
        }
    }

    fn plane_key(&self, p: usize) -> String {
        let at = p * 20;
        let z = |v: f32| if v.abs() < 0.0005 { 0.0 } else { v };
        format!(
            "{:.3},{:.3},{:.3},{:.1}",
            z(f32_at(&self.planes, at)),
            z(f32_at(&self.planes, at + 4)),
            z(f32_at(&self.planes, at + 8)),
            z(f32_at(&self.planes, at + 12))
        )
    }

    fn tex_name(&self, ti: i32) -> String {
        if ti < 0 || (ti as usize + 1) * 72 > self.texinfo.len() {
            return "none".into();
        }
        let td = i32_at(&self.texinfo, ti as usize * 72 + 68);
        if td < 0 || (td as usize + 1) * 32 > self.texdata.len() {
            return "none".into();
        }
        let sid = i32_at(&self.texdata, td as usize * 32 + 12) as usize;
        let o = u32::from_le_bytes(self.table[sid * 4..sid * 4 + 4].try_into().unwrap()) as usize;
        let e = self.strings[o..].iter().position(|&x| x == 0).unwrap();
        String::from_utf8_lossy(&self.strings[o..o + e]).to_ascii_lowercase()
    }

    fn axes(&self, ti: i32) -> String {
        if ti < 0 || (ti as usize + 1) * 72 > self.texinfo.len() {
            return "none".into();
        }
        let a = ti as usize * 72;
        let v: Vec<String> = (0..8).map(|k| format!("{:.2}", f32_at(&self.texinfo, a + k * 4))).collect();
        v.join(",")
    }

    fn brush_sig(&self, b: usize) -> (String, Vec<(String, String, String)>) {
        let (first, num, contents) =
            (i32_at(&self.brushes, b * 12), i32_at(&self.brushes, b * 12 + 4), i32_at(&self.brushes, b * 12 + 8));
        let num = if num == 0 {
            if (b + 1) * 12 < self.brushes.len() { i32_at(&self.brushes, (b + 1) * 12) - first } else { 6 }
        } else {
            num
        };
        let mut v = Vec::new();
        for k in 0..num.max(0) {
            let s = (first + k) as usize * 8;
            if s + 8 > self.sides.len() {
                break;
            }
            let plane = u16_at(&self.sides, s) as usize;
            let ti = i16_at(&self.sides, s + 2) as i32;
            v.push((self.plane_key(plane), self.tex_name(ti), self.axes(ti)));
        }
        v.sort();
        (format!("{contents:x}"), v)
    }

    fn model_brushes(&self, model: usize) -> BTreeSet<usize> {
        let head = i32_at(&self.models, model * 48 + 36);
        let mut out = BTreeSet::new();
        let mut stack = vec![head];
        let mut seen = 0;
        while let Some(n) = stack.pop() {
            seen += 1;
            if seen > 2_000_000 {
                break;
            }
            if n >= 0 {
                let a = n as usize * 32;
                if a + 32 > self.nodes.len() {
                    continue;
                }
                stack.push(i32_at(&self.nodes, a + 4));
                stack.push(i32_at(&self.nodes, a + 8));
            } else {
                let leaf = (-1 - n) as usize;
                let a = leaf * 32;
                if a + 32 > self.leafs.len() {
                    continue;
                }
                let (fb, nb) = (u16_at(&self.leafs, a + 24) as usize, u16_at(&self.leafs, a + 26) as usize);
                for k in 0..nb {
                    if (fb + k) * 2 + 2 <= self.leafbrushes.len() {
                        out.insert(u16_at(&self.leafbrushes, (fb + k) * 2) as usize);
                    }
                }
            }
        }
        out
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ours = Bsp::read(&a[1]).unwrap();
    let theirs = Bsp::read(&a[2]).unwrap();
    let (o, t) = (View::new(&ours), View::new(&theirs));
    let ob = o.brushes.len() / 12;
    let tb = t.brushes.len() / 12;
    println!("brushes ours {ob} ref {tb}");
    let mut tmap: HashMap<String, Vec<usize>> = HashMap::new();
    for i in 0..tb {
        let (c, s) = t.brush_sig(i);
        let planes: Vec<String> = s.iter().map(|x| x.0.clone()).collect();
        tmap.entry(format!("{c}|{}", planes.join(";"))).or_default().push(i);
    }
    let (mut matched, mut mat_same, mut sides_total, mut axes_same, mut unmatched) = (0, 0, 0, 0, 0);
    let mut examples = Vec::new();
    let mut dbg_shown = 0;
    let mut axes_shown = 0;
    let mut our_to_ref: HashMap<usize, usize> = HashMap::new();
    let mut used: HashSet<usize> = HashSet::new();
    for i in 0..ob {
        let (c, s) = o.brush_sig(i);
        let planes: Vec<String> = s.iter().map(|x| x.0.clone()).collect();
        let key = format!("{c}|{}", planes.join(";"));
        let Some(list) = tmap.get(&key) else {
            unmatched += 1;
            continue;
        };
        let Some(&j) = list.iter().find(|j| !used.contains(*j)) else { continue };
        used.insert(j);
        matched += 1;
        our_to_ref.insert(i, j);
        let (_, rs) = t.brush_sig(j);
        if std::env::var("DBG").is_ok()
            && dbg_shown < 3
            && s.iter().any(|x| rs.iter().any(|y| y.0 == x.0 && y.1 != x.1))
        {
            dbg_shown += 1;
            println!("brush ours#{i} ref#{j} contents {c}");
            for x in &s {
                let y = rs.iter().find(|y| y.0 == x.0);
                println!("   {} ours {} | ref {}", x.0, x.1, y.map(|y| y.1.clone()).unwrap_or_default());
            }
        }
        let rmap: BTreeMap<&String, &(String, String, String)> = rs.iter().map(|x| (&x.0, x)).collect();
        for x in &s {
            if let Some(y) = rmap.get(&x.0) {
                sides_total += 1;
                if x.1 == y.1 {
                    mat_same += 1;
                } else if examples.len() < 8 && !y.1.starts_with("hpcustom") {
                    examples.push(format!("{} ours {} ref {}", x.0, x.1, y.1));
                }
                if x.2 == y.2 {
                    axes_same += 1;
                } else if std::env::var("AXES").is_ok() && axes_shown < 6 {
                    axes_shown += 1;
                    println!("  axes diff {} {} ours {} ref {}", x.0, x.1, x.2, y.2);
                }
            }
        }
    }
    println!(
        "brushes matched by planes+contents {matched} unmatched {unmatched}; sides {sides_total}: material same {mat_same}, axes same {axes_same}"
    );
    for e in examples {
        println!("  material diff: {e}");
    }
    let ents = ours.entities().unwrap();
    let mut model_ok = 0;
    let mut model_bad = 0;
    let mut bad_examples = Vec::new();
    for e in &ents {
        let Some(m) = e.get("model").and_then(|m| m.strip_prefix('*')).and_then(|m| m.parse::<usize>().ok()) else {
            continue;
        };
        if (m + 1) * 48 > o.models.len() || (m + 1) * 48 > t.models.len() {
            continue;
        }
        let obs: BTreeSet<usize> = o.model_brushes(m);
        let tbs: BTreeSet<usize> = t.model_brushes(m);
        let mapped: BTreeSet<usize> = obs.iter().filter_map(|b| our_to_ref.get(b).copied()).collect();
        if mapped == tbs && obs.len() == tbs.len() {
            model_ok += 1;
        } else {
            model_bad += 1;
            if bad_examples.len() < 5 {
                bad_examples.push(format!("*{m} {} ours {} ref {}", e.classname(), obs.len(), tbs.len()));
            }
        }
    }
    println!("brush models: same brush sets {model_ok}, different {model_bad} {:?}", bad_examples);
    let og = ours.game_lumps().unwrap();
    let tg = theirs.game_lumps().unwrap();
    let props = |g: &hp_bsp::GameLumps| -> Vec<String> {
        let Some(l) = g.get(hp_bsp::gamelump::id::STATIC_PROPS) else { return Vec::new() };
        let data = l.data().unwrap().into_owned();
        let Ok((p, _)) = hp_bsp::staticprops::StaticPropLump::parse(&data, l.version, None) else {
            return Vec::new();
        };
        p.props
            .iter()
            .map(|r| {
                format!(
                    "{}|{:.1},{:.1},{:.1}|{:.1},{:.1},{:.1}|{}|{}|{}|{:.0}|{:.0}",
                    p.names[r.prop_type as usize].to_ascii_lowercase(),
                    r.origin[0],
                    r.origin[1],
                    r.origin[2],
                    r.angles[0],
                    r.angles[1],
                    r.angles[2],
                    r.skin,
                    r.solid,
                    r.flags,
                    r.fade_min,
                    r.fade_max
                )
            })
            .collect()
    };
    let (po, pt) = (props(&og), props(&tg));
    let mut cnt: HashMap<&String, i32> = HashMap::new();
    for p in &pt {
        *cnt.entry(p).or_default() += 1;
    }
    let mut same = 0;
    for p in &po {
        if let Some(c) = cnt.get_mut(p)
            && *c > 0
        {
            *c -= 1;
            same += 1;
        }
    }
    let uniq: HashSet<&String> = pt.iter().collect();
    if std::env::var("PROPS").is_ok() {
        let mut have: HashMap<&String, i32> = HashMap::new();
        for p in &po {
            *have.entry(p).or_default() += 1;
        }
        let mut groups: BTreeMap<String, usize> = BTreeMap::new();
        for p in &pt {
            if have.get_mut(p).is_some_and(|c| {
                if *c > 0 {
                    *c -= 1;
                    true
                } else {
                    false
                }
            }) {
                continue;
            }
            let f: Vec<&str> = p.split('|').collect();
            *groups.entry(format!("solid {} flags {} fade {}-{}", f[4], f[5], f[6], f[7])).or_default() += 1;
        }
        for (k, v) in groups.iter().take(12) {
            println!("  missing prop group: {k}: {v}");
        }
    }
    println!("static props ours {} ref {} (distinct {}) identical records {same}", po.len(), pt.len(), uniq.len());
    let class_count = |e: &[hp_bsp::Entity]| {
        let mut m: BTreeMap<String, i32> = BTreeMap::new();
        for x in e {
            *m.entry(x.classname().to_owned()).or_default() += 1;
        }
        m
    };
    let (co, ct) = (class_count(&ents), class_count(&theirs.entities().unwrap()));
    let diffs: Vec<String> = ct
        .keys()
        .chain(co.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|k| co.get(*k) != ct.get(*k))
        .map(|k| format!("{k}: ours {} ref {}", co.get(k).unwrap_or(&0), ct.get(k).unwrap_or(&0)))
        .collect();
    println!("entity class differences: {:?}", diffs);
    let dz = |b: &Bsp| b.lump(26).unwrap().len() / 176;
    println!("dispinfo ours {} ref {}", dz(&ours), dz(&theirs));
}
