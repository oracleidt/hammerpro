use hp_bsp::{Bsp, gamelump, lump};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Passthrough {
    pub lumps: Vec<(usize, usize)>,
    pub game_lumps: Vec<(String, usize)>,
    pub refused: Vec<usize>,
    pub lightmapped_faces: usize,
}

impl Passthrough {
    pub fn bytes(&self) -> usize {
        self.lumps.iter().map(|(_, n)| n).sum::<usize>() + self.game_lumps.iter().map(|(_, n)| n).sum::<usize>()
    }

    pub fn names(&self) -> Vec<String> {
        let mut out: Vec<String> = self.lumps.iter().map(|(i, _)| lump_name(*i).to_owned()).collect();
        out.extend(self.game_lumps.iter().map(|(id, _)| id.clone()));
        out
    }
}

const LEAF_RECORD: usize = 32;
const FACE_RECORD: usize = 56;

const LEAF_FIELDS: &[(usize, usize)] = &[(0, 4), (4, 6), (6, 8), (24, 28)];
const FACE_FIELDS: &[(usize, usize)] = &[(8, 12), (12, 14), (14, 16)];
const PREFIX_SLACK: usize = 2;

const MODEL_RECORD: usize = 48;
const MODEL_FIELDS: &[(usize, usize)] = &[(24, 40)];

const FACE_LIGHT_FIELDS: &[(usize, usize)] = &[(20, 24), (24, 28), (28, 44)];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Guard {
    Leafs,
    Faces,
    Models,
}

const CANDIDATES: &[(usize, Guard)] = &[
    (4, Guard::Leafs),
    (8, Guard::Faces),
    (11, Guard::Faces),
    (20, Guard::Leafs),
    (22, Guard::Leafs),
    (23, Guard::Leafs),
    (24, Guard::Leafs),
    (25, Guard::Leafs),
    (29, Guard::Models),
    (34, Guard::Faces),
    (46, Guard::Leafs),
    (51, Guard::Leafs),
    (52, Guard::Leafs),
    (53, Guard::Faces),
    (55, Guard::Leafs),
    (56, Guard::Leafs),
];

fn fields_match(ours: &Bsp, file: &Bsp, index: usize, stride: usize, fields: &[(usize, usize)], prefix: bool) -> bool {
    let (Ok(a), Ok(b)) = (ours.lump(index), file.lump(index)) else { return false };
    let exact = a.len() == b.len();
    let longer = prefix && a.len() > b.len() && a.len() - b.len() <= stride * PREFIX_SLACK;
    if a.is_empty() || b.is_empty() || !(exact || longer) || !a.len().is_multiple_of(stride) {
        return false;
    }
    (0..b.len() / stride).all(|k| {
        fields.iter().all(|(lo, hi)| a[k * stride + lo..k * stride + hi] == b[k * stride + lo..k * stride + hi])
    })
}

fn guard_holds(ours: &Bsp, file: &Bsp, guard: Guard) -> bool {
    match guard {
        Guard::Leafs => fields_match(ours, file, lump::LEAFS, LEAF_RECORD, LEAF_FIELDS, true) && same(ours, file, 17),
        Guard::Faces => fields_match(ours, file, lump::FACES, FACE_RECORD, FACE_FIELDS, false),
        Guard::Models => fields_match(ours, file, 14, MODEL_RECORD, MODEL_FIELDS, false),
    }
}

const GAME_LUMPS: &[u32] =
    &[gamelump::id::DETAIL_PROPS, gamelump::id::DETAIL_PROP_LIGHTING, gamelump::id::DETAIL_PROP_LIGHTING_HDR];

pub fn lump_name(index: usize) -> &'static str {
    hp_bsp::lump_name(index)
}

fn same(a: &Bsp, b: &Bsp, index: usize) -> bool {
    match (a.lump(index), b.lump(index)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

pub fn plan(ours: &Bsp, file: &Bsp) -> Passthrough {
    let mut out = Passthrough::default();
    let mut guards: std::collections::HashMap<usize, bool> = std::collections::HashMap::new();
    for (index, guard) in CANDIDATES {
        let have = ours.lump(*index).map(|l| !l.is_empty()).unwrap_or(false);
        let theirs = file.lump(*index).map(|l| l.len()).unwrap_or(0);
        if have || theirs == 0 {
            continue;
        }
        let key = *guard as usize;
        if *guards.entry(key).or_insert_with(|| guard_holds(ours, file, *guard)) {
            out.lumps.push((*index, theirs));
        } else {
            out.refused.push(*index);
        }
    }
    let lighting = out.lumps.iter().any(|(i, _)| *i == 8 || *i == 53);
    if lighting && let Ok(faces) = ours.lump(lump::FACES) {
        out.lightmapped_faces = faces.len() / FACE_RECORD;
    }
    let (Ok(mine), Ok(theirs)) = (ours.game_lumps(), file.game_lumps()) else { return out };
    let props_match = match (mine.get(gamelump::id::STATIC_PROPS), theirs.get(gamelump::id::STATIC_PROPS)) {
        (Some(a), Some(b)) => a.data().ok() == b.data().ok(),
        _ => false,
    };
    for id in GAME_LUMPS {
        let Some(g) = theirs.get(*id) else { continue };
        if mine.get(*id).is_some() {
            continue;
        }
        let lighting = *id != gamelump::id::DETAIL_PROPS;
        if lighting && !props_match {
            out.refused.push(lump::GAME_LUMP);
            continue;
        }
        out.game_lumps.push((gamelump::id_text(*id), g.stored.len()));
    }
    out
}

fn lightmap_faces(ours: &Bsp, file: &Bsp) -> Option<Vec<u8>> {
    let (Ok(a), Ok(b)) = (ours.lump(lump::FACES), file.lump(lump::FACES)) else { return None };
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let mut out = a.into_owned();
    for k in 0..out.len() / FACE_RECORD {
        for (lo, hi) in FACE_LIGHT_FIELDS {
            let at = k * FACE_RECORD;
            out[at + lo..at + hi].copy_from_slice(&b[at + lo..at + hi]);
        }
    }
    Some(out)
}

pub fn apply(ours: &Bsp, file: &Bsp) -> (Option<Bsp>, Passthrough) {
    let report = plan(ours, file);
    if report.lumps.is_empty() && report.game_lumps.is_empty() {
        return (None, report);
    }
    let Ok(copy) = Bsp::from_vec(ours.bytes().to_vec()) else { return (None, report) };
    let mut edit = copy.edit();
    if report.lightmapped_faces > 0
        && let Some(faces) = lightmap_faces(ours, file)
        && edit.replace_lump(lump::FACES, faces).is_err()
    {
        return (None, report);
    }
    for (index, _) in &report.lumps {
        let Ok(data) = file.lump(*index) else { return (None, report) };
        let version = file.header().lumps[*index].version;
        if edit.replace_lump_with_version(*index, data.into_owned(), version).is_err() {
            return (None, report);
        }
    }
    if !report.game_lumps.is_empty() {
        let (Ok(mut mine), Ok(theirs)) = (edit.source().game_lumps(), file.game_lumps()) else {
            return (None, report);
        };
        for id in GAME_LUMPS {
            if !report.game_lumps.iter().any(|(text, _)| *text == gamelump::id_text(*id)) {
                continue;
            }
            let Some(g) = theirs.get(*id) else { continue };
            let Ok(data) = g.data() else { continue };
            if mine.set(*id, g.version, data.into_owned()).is_err() {
                return (None, report);
            }
        }
        edit.set_game_lumps(mine);
    }
    let out = edit.to_vec().ok().and_then(|v| Bsp::from_vec(v).ok());
    (out, report)
}

#[cfg(test)]
mod tests {
    use hp_bsp::GameLumps;

    use super::*;

    fn bsp_with(lumps: &[(usize, Vec<u8>)]) -> Bsp {
        Bsp::from_vec(hp_bsp::synthetic_bsp(20, lumps)).expect("bsp")
    }

    #[test]
    fn a_lump_we_do_not_build_is_copied_when_its_index_lumps_match() {
        let leafs = vec![7u8; 32 * 4];
        let ours = bsp_with(&[(lump::LEAFS, leafs.clone()), (17, vec![1u8; 8])]);
        let file = bsp_with(&[
            (lump::LEAFS, leafs),
            (17, vec![1u8; 8]),
            (4, vec![9u8; 64]),
            (55, vec![3u8; 96]),
            (46, vec![4u8; 16]),
        ]);
        let (out, report) = apply(&ours, &file);
        let out = out.expect("a new bsp");
        assert_eq!(report.lumps.iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![4, 46, 55]);
        assert_eq!(report.bytes(), 64 + 16 + 96);
        assert!(report.refused.is_empty());
        assert_eq!(out.lump(4).expect("visibility").into_owned(), vec![9u8; 64]);
        assert_eq!(out.lump(55).expect("ambient").into_owned(), vec![3u8; 96]);
        assert_eq!(out.lump(lump::LEAFS).expect("leafs"), ours.lump(lump::LEAFS).expect("leafs"));
    }

    #[test]
    fn one_extra_leaf_at_the_end_still_counts_as_the_same_leaf_list() {
        let leaf = |v: u8| vec![v; LEAF_RECORD];
        let mut ours = leaf(1);
        ours.extend(leaf(2));
        ours.extend(leaf(9));
        let mut theirs = leaf(1);
        theirs.extend(leaf(2));
        let ours_bsp = bsp_with(&[(lump::LEAFS, ours), (17, vec![1u8; 8])]);
        let file = bsp_with(&[(lump::LEAFS, theirs.clone()), (17, vec![1u8; 8]), (4, vec![9u8; 64])]);
        let (out, report) = apply(&ours_bsp, &file);
        assert!(out.is_some());
        assert_eq!(report.lumps.iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![4]);
        let mut different = leaf(1);
        different.extend(leaf(3));
        let other = bsp_with(&[(lump::LEAFS, different), (17, vec![1u8; 8]), (4, vec![9u8; 64])]);
        let (_, refused) = apply(&ours_bsp, &other);
        assert_eq!(refused.refused, vec![4], "a leaf that differs inside the common part is refused");
    }

    #[test]
    fn a_lump_is_refused_when_the_indices_it_points_into_differ() {
        let ours = bsp_with(&[(lump::LEAFS, vec![7u8; 64]), (17, vec![1u8; 8])]);
        let file = bsp_with(&[(lump::LEAFS, vec![8u8; 64]), (17, vec![1u8; 8]), (4, vec![9u8; 64])]);
        let (out, report) = apply(&ours, &file);
        assert!(out.is_none());
        assert!(report.lumps.is_empty());
        assert_eq!(report.refused, vec![4]);
    }

    #[test]
    fn a_lump_we_already_rebuilt_is_never_overwritten() {
        let faces = vec![5u8; 56 * 3];
        let ours = bsp_with(&[
            (lump::FACES, faces.clone()),
            (3, vec![2u8; 36]),
            (12, vec![0u8; 8]),
            (13, vec![0u8; 8]),
            (8, vec![1u8; 32]),
        ]);
        let file = bsp_with(&[
            (lump::FACES, faces),
            (3, vec![2u8; 36]),
            (12, vec![0u8; 8]),
            (13, vec![0u8; 8]),
            (8, vec![9u8; 32]),
            (11, vec![4u8; 12]),
        ]);
        let (out, report) = apply(&ours, &file);
        let out = out.expect("a new bsp");
        assert_eq!(
            report.lumps.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![11],
            "only the missing lump is taken"
        );
        assert_eq!(out.lump(8).expect("lighting").into_owned(), vec![1u8; 32], "our own lighting stays");
        assert_eq!(out.lump(11).expect("faceids").into_owned(), vec![4u8; 12]);
    }

    #[test]
    fn the_physics_follows_the_model_list_not_its_bounding_boxes() {
        let model = |origin: u8, head: u8, bounds: u8| {
            let mut r = vec![bounds; MODEL_RECORD];
            r[24..36].fill(origin);
            r[36..40].fill(head);
            r
        };
        let ours = bsp_with(&[(14, [model(5, 7, 0), model(6, 8, 0)].concat())]);
        let file = bsp_with(&[(14, [model(5, 7, 9), model(6, 8, 9)].concat()), (29, vec![4u8; 128])]);
        let (out, report) = apply(&ours, &file);
        assert_eq!(report.lumps.iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![29]);
        assert_eq!(out.expect("bsp").lump(29).expect("physics").into_owned(), vec![4u8; 128]);
        let other = bsp_with(&[(14, [model(5, 7, 9), model(6, 9, 9)].concat()), (29, vec![4u8; 128])]);
        let (_, refused) = apply(&ours, &other);
        assert_eq!(refused.refused, vec![29], "another head node means another model list");
    }

    #[test]
    fn the_detail_prop_lighting_needs_the_same_static_props() {
        let mut mine = GameLumps::default();
        mine.set(gamelump::id::STATIC_PROPS, 10, vec![1u8; 20]).expect("props");
        let mut theirs = GameLumps::default();
        theirs.set(gamelump::id::STATIC_PROPS, 10, vec![2u8; 20]).expect("props");
        theirs.set(gamelump::id::DETAIL_PROPS, 4, vec![3u8; 40]).expect("detail");
        theirs.set(gamelump::id::DETAIL_PROP_LIGHTING, 1, vec![4u8; 40]).expect("lighting");
        let build = |g: GameLumps| {
            let b = bsp_with(&[(lump::LEAFS, vec![7u8; 32])]);
            let mut edit = b.edit();
            edit.set_game_lumps(g);
            Bsp::from_vec(edit.to_vec().expect("bytes")).expect("bsp")
        };
        let ours = build(mine);
        let file = build(theirs);
        let (out, report) = apply(&ours, &file);
        let names = report.names();
        assert!(names.contains(&"dprp".to_owned()), "{names:?}");
        assert!(!names.contains(&"dplt".to_owned()), "the lighting follows the prop order and is refused");
        let out = out.expect("a new bsp");
        let game = out.game_lumps().expect("game lumps");
        assert!(game.get(gamelump::id::DETAIL_PROPS).is_some());
        assert_eq!(
            game.get(gamelump::id::STATIC_PROPS).expect("props").data().expect("data").into_owned(),
            vec![1u8; 20],
            "our own static props stay"
        );
    }
}
