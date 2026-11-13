use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};

use hp_app::extend::FloodVerdict;

const STACK: usize = 64 << 20;

struct ByName;

impl hp_bspc::MaterialSource for ByName {
    fn facts(&self, name: &str) -> hp_bspc::MaterialFacts {
        hp_bspc::materials::facts_by_name(name)
    }
}

fn pool() -> Option<&'static rayon::ThreadPool> {
    static POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(hp_app::checks::threads())
            .thread_name(|i| format!("hp-leak-flood-{i}"))
            .stack_size(STACK)
            .start_handler(|_| hp_app::checks::lowest_priority())
            .build()
            .ok()
    })
    .as_ref()
}

fn flood(
    map: &hp_vmf::Map,
    fs: Option<&hp_fs::GameFs>,
    cancel: &Arc<AtomicBool>,
) -> Result<Option<FloodVerdict>, String> {
    let mut log = |_: &str| {};
    let stop = Some(Arc::clone(cancel));
    let found = match fs {
        Some(fs) => {
            let materials = crate::bspc_cli::GameMaterials { fs: fs.clone(), opts: Default::default() };
            hp_bspc::flood_check(map, &materials, stop, &mut log)
        }
        None => hp_bspc::flood_check(map, &ByName, stop, &mut log),
    };
    match found {
        Ok(leak) => Ok(Some(leak.map(|l| (l.entity, l.path)))),
        Err(hp_bspc::CompileError::Cancelled) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub(crate) fn confirm(
    map: &hp_vmf::Map,
    fs: Option<&hp_fs::GameFs>,
    cancel: &Arc<AtomicBool>,
) -> Result<Option<FloodVerdict>, String> {
    match pool() {
        Some(p) => p.install(|| flood(map, fs, cancel)),
        None => flood(map, fs, cancel),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(id: u32, min: glam::DVec3, max: glam::DVec3) -> hp_vmf::Solid {
        let mut next = id * 10;
        let mut ids = || {
            next += 1;
            next
        };
        let corners = hp_geom::Aabb::new(min, max).corners();
        let sides = hp_geom::hull_solid(&corners, "BRICK/BRICKWALL001A", 0.25, 16, &mut ids).expect("cube");
        hp_vmf::Solid { id, sides, ..hp_vmf::Solid::default() }
    }

    fn room(open: bool) -> hp_vmf::Map {
        let v = glam::DVec3::new;
        let mut walls = vec![
            cube(1, v(-80.0, -80.0, -80.0), v(80.0, 80.0, -64.0)),
            cube(2, v(-80.0, -80.0, 64.0), v(80.0, 80.0, 80.0)),
            cube(3, v(-80.0, -80.0, -64.0), v(-64.0, 80.0, 64.0)),
            cube(4, v(64.0, -80.0, -64.0), v(80.0, 80.0, 64.0)),
            cube(5, v(-64.0, -80.0, -64.0), v(64.0, -64.0, 64.0)),
            cube(6, v(-64.0, 64.0, -64.0), v(64.0, 80.0, 64.0)),
        ];
        if open {
            walls.pop();
        }
        let mut map = hp_vmf::Map::default();
        map.world.classname = "worldspawn".to_owned();
        map.world.solids = walls;
        let mut e = hp_vmf::Entity { id: 50, classname: "info_player_start".to_owned(), ..hp_vmf::Entity::default() };
        e.set("origin", "0 0 -63");
        map.entities.push(e);
        map
    }

    #[test]
    fn the_compiler_flood_answers_sealed_or_leaked() {
        let go = Arc::new(AtomicBool::new(false));
        assert_eq!(confirm(&room(false), None, &go), Ok(Some(None)));
        let (entity, path) = confirm(&room(true), None, &go).expect("flood").expect("not cancelled").expect("a leak");
        assert_eq!(entity, Some(50));
        assert!(path.len() >= 2, "{path:?}");
        assert_eq!(*path.last().expect("end"), glam::DVec3::new(0.0, 0.0, -63.0), "the path ends at the entity");
        let stop = Arc::new(AtomicBool::new(true));
        assert_eq!(confirm(&room(true), None, &stop), Ok(None), "a stale check stops at once");
    }
}
