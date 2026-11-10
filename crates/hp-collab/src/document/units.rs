use anyhow::{Result, ensure};
use hp_kv::Block;
use hp_vmf::{Entity, Map, Solid, VisGroup};

use super::{Unit, unhide};

fn single(text: &str) -> Result<(Block, bool)> {
    let root = hp_kv::parse_bounded(text, 128)?;
    ensure!(root.props.is_empty(), "Invalid map unit");
    let mut blocks = unhide(root.children)?;
    ensure!(blocks.len() == 1, "Invalid map unit");
    let (mut block, hidden) = blocks.remove(0);
    block.name.make_ascii_lowercase();
    Ok((block, hidden))
}

impl Unit {
    pub fn from_point_entity(entity: &Entity) -> Result<Self> {
        ensure!(entity.solids.is_empty() && entity.groups.is_empty(), "Expected a point entity");
        Self::from_entity(entity, false)
    }

    pub fn from_entity(entity: &Entity, brush: bool) -> Result<Self> {
        ensure!(entity.groups.is_empty(), "Entities cannot own groups");
        let text = if entity.solids.is_empty() && !(brush && hp_vmf::is_world_particle_volume(&entity.classname)) {
            hp_vmf::write_entity(entity)
        } else {
            let mut header = entity.clone();
            header.solids.clear();
            if brush && hp_vmf::is_world_particle_volume(&header.classname) {
                header.props.retain(|(key, _)| !key.eq_ignore_ascii_case("origin"));
            }
            hp_vmf::write_entity(&header)
        };
        let (block, hidden) = single(&text)?;
        ensure!(block.name == "entity", "Expected an entity");
        Ok(Self { owner: 0, hidden, vmf: block.to_kv_string() })
    }

    pub fn from_solid(solid: &Solid, owner: u32) -> Result<Self> {
        let (block, hidden) = single(&hp_vmf::write_solid(solid))?;
        ensure!(block.name == "solid", "Expected a solid");
        Ok(Self { owner, hidden, vmf: block.to_kv_string() })
    }

    pub fn header_units(header: &Map) -> Result<(Self, Self)> {
        ensure!(header.entities.is_empty() && header.world.solids.is_empty(), "Expected a map header");
        let root = hp_kv::parse_bounded(&hp_vmf::write(header), 128)?;
        let mut world = None;
        let mut metadata = Block::default();
        for (mut block, hidden) in unhide(root.children)? {
            match block.name.to_ascii_lowercase().as_str() {
                "world" => {
                    ensure!(!hidden, "The world cannot be hidden");
                    block.name.make_ascii_lowercase();
                    world = Some(Self { owner: 0, hidden: false, vmf: block.to_kv_string() });
                }
                "entity" => anyhow::bail!("Expected a map header"),
                "versioninfo" | "cameras" | "viewsettings" | "cordon" | "cordons" => {}
                _ => metadata.children.push(block),
            }
        }
        let world = world.ok_or_else(|| anyhow::anyhow!("Missing world"))?;
        Ok((world, Self { owner: 0, hidden: false, vmf: hp_kv::write(&metadata) }))
    }

    pub fn to_entity(&self) -> Result<Entity> {
        let (solids, mut entities) = hp_vmf::parse_fragment(&self.vmf)?;
        ensure!(solids.is_empty() && entities.len() == 1, "Expected an entity");
        let mut entity = entities.remove(0);
        entity.hidden = self.hidden;
        Ok(entity)
    }

    pub fn to_solid(&self) -> Result<Solid> {
        let (mut solids, entities) = hp_vmf::parse_fragment(&self.vmf)?;
        ensure!(entities.is_empty() && solids.len() == 1, "Expected a solid");
        let mut solid = solids.remove(0);
        solid.hidden = self.hidden;
        Ok(solid)
    }

    pub fn to_world(&self) -> Result<Entity> {
        let map = hp_vmf::parse(&self.vmf)?;
        ensure!(map.entities.is_empty() && map.world.solids.is_empty(), "Expected the world header");
        Ok(map.world)
    }

    pub fn to_metadata(&self) -> Result<(Vec<VisGroup>, Vec<Block>)> {
        let map = hp_vmf::parse(&format!("{}world\n{{\n\"id\" \"1\"\n}}\n", self.vmf))?;
        ensure!(map.entities.is_empty() && map.world.solids.is_empty(), "Invalid map metadata");
        Ok((map.visgroups, map.extra_blocks))
    }
}
