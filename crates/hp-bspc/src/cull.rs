use glam::DVec3;

use crate::brush::MapBrush;
use crate::materials::{contents, surf};
use crate::planes::PlaneSet;
use crate::texinfo::TexInfos;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BrushCull {
    pub duplicates: usize,
    pub enclosed: usize,
    pub degenerate: usize,
    pub merged: usize,
    pub hidden: usize,
}

impl BrushCull {
    pub fn total(&self) -> usize {
        self.duplicates + self.enclosed + self.degenerate + self.merged + self.hidden
    }
}

const INSIDE_EPSILON: f64 = 0.01;
const CELL: f64 = 256.0;
const MAX_CELLS: i64 = 256;

pub fn removable(b: &MapBrush) -> bool {
    b.contents & contents::SOLID != 0
        && b.contents & contents::TRANSLUCENT == 0
        && b.sides.iter().all(|s| s.disp.is_none() && s.overlays.is_empty() && !s.no_split)
}

fn faces(b: &MapBrush) -> impl Iterator<Item = &crate::brush::BrushSide> {
    b.sides.iter().filter(|s| !s.bevel)
}

fn plane_set(b: &MapBrush) -> Vec<(usize, i32)> {
    let mut v: Vec<(usize, i32)> = faces(b).map(|s| (s.plane, s.texinfo)).collect();
    v.sort_unstable();
    v
}

fn inside(x: &MapBrush, y: &MapBrush, planes: &PlaneSet) -> bool {
    if x.mins.cmplt(y.mins - INSIDE_EPSILON).any() || x.maxs.cmpgt(y.maxs + INSIDE_EPSILON).any() {
        return false;
    }
    let contents_ok =
        x.contents == y.contents || (y.contents == contents::SOLID && x.contents == contents::SOLID | contents::DETAIL);
    if !contents_ok {
        return false;
    }
    for s in faces(x) {
        if let Some(t) = faces(y).find(|t| t.plane == s.plane)
            && t.texinfo != s.texinfo
        {
            return false;
        }
    }
    faces(y).all(|t| {
        let p = planes.get(t.plane);
        faces(x)
            .filter_map(|s| s.winding.as_ref())
            .flat_map(|w| w.points.iter())
            .all(|q| p.normal.dot(*q) - p.dist <= INSIDE_EPSILON)
    })
}

fn cell_of(v: f64) -> i64 {
    (v / CELL).floor() as i64
}

pub fn enclosed(brushes: &[MapBrush], planes: &PlaneSet, ids: &[usize], culled: &mut [bool], report: &mut BrushCull) {
    let mut grid: ahash::AHashMap<(i64, i64), Vec<usize>> = ahash::AHashMap::new();
    let mut big: Vec<usize> = Vec::new();
    let ok: Vec<usize> = ids.iter().copied().filter(|&i| removable(&brushes[i])).collect();
    for &i in &ok {
        let b = &brushes[i];
        let (x0, x1, y0, y1) = (cell_of(b.mins.x), cell_of(b.maxs.x), cell_of(b.mins.y), cell_of(b.maxs.y));
        if (x1 - x0 + 1) * (y1 - y0 + 1) > MAX_CELLS {
            big.push(i);
            continue;
        }
        for cx in x0..=x1 {
            for cy in y0..=y1 {
                grid.entry((cx, cy)).or_default().push(i);
            }
        }
    }
    for &x in &ok {
        if culled[x] {
            continue;
        }
        let b = &brushes[x];
        let near = grid.get(&(cell_of(b.mins.x), cell_of(b.mins.y))).map_or(&[][..], |v| v.as_slice());
        let found = near
            .iter()
            .chain(big.iter())
            .copied()
            .find(|&y| y != x && !culled[y] && brushes[y].entity == b.entity && inside(b, &brushes[y], planes));
        if let Some(y) = found {
            culled[x] = true;
            if plane_set(b) == plane_set(&brushes[y]) {
                report.duplicates += 1;
            } else {
                report.enclosed += 1;
            }
        }
    }
}

pub fn degenerate(brushes: &[MapBrush], ids: &[usize], culled: &mut [bool], report: &mut BrushCull) {
    for &i in ids {
        let b = &brushes[i];
        if !culled[i] && removable(b) && b.volume().abs() < 1e-3 {
            culled[i] = true;
            report.degenerate += 1;
        }
    }
}

fn nodraw_box(b: &MapBrush, planes: &PlaneSet, tex: &TexInfos) -> Option<i32> {
    if !removable(b) || b.sides.len() != 6 || b.sides.iter().any(|s| s.bevel) {
        return None;
    }
    let t = b.sides[0].texinfo;
    if b.sides.iter().any(|s| s.texinfo != t) {
        return None;
    }
    let flags = tex.infos.get(usize::try_from(t).ok()?)?.flags;
    if flags & surf::NODRAW == 0 {
        return None;
    }
    let mut seen = [false; 6];
    for s in &b.sides {
        let n = planes.get(s.plane).normal;
        let axis = (0..3).find(|&a| (n[a].abs() - 1.0).abs() < 1e-9)?;
        let k = axis * 2 + usize::from(n[axis] > 0.0);
        if seen[k] {
            return None;
        }
        seen[k] = true;
    }
    Some(t)
}

pub fn merge_nodraw_boxes(
    brushes: &mut [MapBrush],
    planes: &PlaneSet,
    tex: &TexInfos,
    ids: &[usize],
    culled: &mut [bool],
    report: &mut BrushCull,
) {
    loop {
        let mut changed = false;
        for axis in 0..3 {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let mut rows: ahash::AHashMap<(usize, u32, i32, [u64; 4]), Vec<usize>> = ahash::AHashMap::new();
            for &i in ids {
                if culled[i] {
                    continue;
                }
                let b = &brushes[i];
                let Some(t) = nodraw_box(b, planes, tex) else { continue };
                let key = [b.mins[u].to_bits(), b.maxs[u].to_bits(), b.mins[v].to_bits(), b.maxs[v].to_bits()];
                rows.entry((b.entity, b.contents, t, key)).or_default().push(i);
            }
            for row in rows.values_mut() {
                row.sort_by(|a, b| brushes[*a].mins[axis].total_cmp(&brushes[*b].mins[axis]));
                let mut k = 0;
                while k < row.len() {
                    let a = row[k];
                    let mut j = k + 1;
                    while j < row.len() && (brushes[row[j]].mins[axis] - brushes[a].maxs[axis]).abs() < 1e-6 {
                        let b = row[j];
                        let far =
                            brushes[b].sides.iter().find(|s| planes.get(s.plane).normal[axis] > 0.5).map(|s| s.plane);
                        let near = brushes[a].sides.iter().position(|s| planes.get(s.plane).normal[axis] > 0.5);
                        let (Some(far), Some(near)) = (far, near) else { break };
                        let backup = brushes[a].clone();
                        brushes[a].sides[near].plane = far;
                        if !brushes[a].refresh_windings(planes) {
                            brushes[a] = backup;
                            break;
                        }
                        culled[b] = true;
                        report.merged += 1;
                        changed = true;
                        j += 1;
                    }
                    k = j;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

pub fn hidden(
    brushes: &[MapBrush],
    ids: &[usize],
    origins: &[DVec3],
    planes: &PlaneSet,
    culled: &mut [bool],
    report: &mut BrushCull,
) {
    for &i in ids {
        let b = &brushes[i];
        if culled[i] || b.contents & contents::DETAIL != 0 || !removable(b) || b.entity != 0 {
            continue;
        }
        if faces(b).any(|s| s.visible) {
            continue;
        }
        let holds = origins.iter().any(|o| {
            o.cmpge(b.mins - 1.0).all()
                && o.cmple(b.maxs + 1.0).all()
                && faces(b).all(|s| {
                    let p = planes.get(s.plane);
                    p.normal.dot(*o) - p.dist <= 1.0
                })
        });
        if holds {
            continue;
        }
        culled[i] = true;
        report.hidden += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(planes: &mut PlaneSet, lo: DVec3, hi: DVec3) -> MapBrush {
        let mut sides = Vec::new();
        for axis in 0..3 {
            for (dir, d) in [(1.0, hi[axis]), (-1.0, -lo[axis])] {
                let mut n = DVec3::ZERO;
                n[axis] = dir;
                let plane = planes.find(n, d).expect("plane");
                sides.push(crate::brush::BrushSide {
                    plane,
                    texinfo: 0,
                    surf: 0,
                    winding: None,
                    bevel: false,
                    visible: false,
                    no_split: false,
                    side_id: 0,
                    disp: None,
                    contents: contents::SOLID,
                    fwinding: None,
                    smoothing: 0,
                    overlays: Vec::new(),
                });
            }
        }
        let mut b = MapBrush {
            id: 1,
            entity: 0,
            contents: contents::SOLID,
            sides,
            mins: lo,
            maxs: hi,
            surfaceprop: String::new(),
        };
        assert!(b.refresh_windings(planes));
        b
    }

    #[test]
    fn duplicates_and_enclosed_brushes_go_but_their_container_stays() {
        let mut planes = PlaneSet::new();
        let brushes = vec![
            cube(&mut planes, DVec3::ZERO, DVec3::splat(128.0)),
            cube(&mut planes, DVec3::ZERO, DVec3::splat(128.0)),
            cube(&mut planes, DVec3::splat(16.0), DVec3::splat(32.0)),
            cube(&mut planes, DVec3::splat(100.0), DVec3::splat(200.0)),
        ];
        let mut culled = vec![false; brushes.len()];
        let mut report = BrushCull::default();
        enclosed(&brushes, &planes, &[0, 1, 2, 3], &mut culled, &mut report);
        assert_eq!(culled, vec![true, false, true, false]);
        assert_eq!((report.duplicates, report.enclosed), (1, 1));
    }

    #[test]
    fn nodraw_boxes_side_by_side_become_one() {
        let mut planes = PlaneSet::new();
        let mut tex = TexInfos::new();
        tex.infos.push(crate::texinfo::TexInfo {
            texture: [[0.0; 4]; 2],
            lightmap: [[0.0; 4]; 2],
            flags: surf::NODRAW,
            texdata: -1,
        });
        let t = (tex.infos.len() - 1) as i32;
        let mut brushes = vec![
            cube(&mut planes, DVec3::ZERO, DVec3::new(64.0, 64.0, 64.0)),
            cube(&mut planes, DVec3::new(64.0, 0.0, 0.0), DVec3::new(128.0, 64.0, 64.0)),
            cube(&mut planes, DVec3::new(128.0, 0.0, 0.0), DVec3::new(192.0, 64.0, 32.0)),
        ];
        for b in &mut brushes {
            for s in &mut b.sides {
                s.texinfo = t;
            }
        }
        let mut culled = vec![false; brushes.len()];
        let mut report = BrushCull::default();
        merge_nodraw_boxes(&mut brushes, &planes, &tex, &[0, 1, 2], &mut culled, &mut report);
        assert_eq!(culled, vec![false, true, false]);
        assert_eq!(report.merged, 1);
        assert_eq!(brushes[0].maxs, DVec3::new(128.0, 64.0, 64.0));
        assert!((brushes[0].volume() - 128.0 * 64.0 * 64.0).abs() < 1e-6);
    }
}
