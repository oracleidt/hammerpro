use crate::header::LumpLayout;
use crate::limits::LimitSet;
use crate::staticprops::StaticPropFormat;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Branch {
    #[default]
    GarrysMod,
    Source2013,
    Source2007,
    BlackMesa,
    TheShip,
    BloodyGoodTime,
    Left4Dead,
    Left4Dead2,
    AlienSwarm,
    Portal2,
    Csgo,
    Insurgency,
}

impl Branch {
    pub const ALL: [Branch; 12] = [
        Branch::GarrysMod,
        Branch::Source2013,
        Branch::Source2007,
        Branch::BlackMesa,
        Branch::TheShip,
        Branch::BloodyGoodTime,
        Branch::Left4Dead,
        Branch::Left4Dead2,
        Branch::AlienSwarm,
        Branch::Portal2,
        Branch::Csgo,
        Branch::Insurgency,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Branch::GarrysMod => "gmod",
            Branch::Source2013 => "source2013",
            Branch::Source2007 => "source2007",
            Branch::BlackMesa => "blackmesa",
            Branch::TheShip => "theship",
            Branch::BloodyGoodTime => "bgt",
            Branch::Left4Dead => "l4d",
            Branch::Left4Dead2 => "l4d2",
            Branch::AlienSwarm => "alienswarm",
            Branch::Portal2 => "portal2",
            Branch::Csgo => "csgo",
            Branch::Insurgency => "insurgency",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Branch::GarrysMod => "Garry's Mod",
            Branch::Source2013 => "Source 2013",
            Branch::Source2007 => "Source 2007",
            Branch::BlackMesa => "Black Mesa",
            Branch::TheShip => "The Ship",
            Branch::BloodyGoodTime => "Bloody Good Time",
            Branch::Left4Dead => "Left 4 Dead",
            Branch::Left4Dead2 => "Left 4 Dead 2",
            Branch::AlienSwarm => "Alien Swarm",
            Branch::Portal2 => "Portal 2",
            Branch::Csgo => "CS:GO",
            Branch::Insurgency => "Insurgency",
        }
    }

    pub fn from_id(id: &str) -> Option<Branch> {
        Self::ALL.into_iter().find(|b| b.id().eq_ignore_ascii_case(id.trim()))
    }

    pub const fn format(self) -> BspFormat {
        let base = BspFormat {
            branch: self,
            version: 20,
            layout: LumpLayout::Standard,
            static_props: StaticPropFormat::V10,
            detail_props_version: 4,
            worldlights_version: 0,
            overlay_system_levels: false,
            disp_multiblend_lump: None,
            limits: LimitSet::Sdk,
            max_lightmap_dim: 32,
        };
        match self {
            Branch::GarrysMod => BspFormat {
                limits: LimitSet::Gmod,
                max_lightmap_dim: 125,
                disp_multiblend_lump: Some(crate::header::lump::DISP_MULTIBLEND),
                ..base
            },
            Branch::Source2013 => {
                BspFormat { disp_multiblend_lump: Some(crate::header::lump::DISP_MULTIBLEND), ..base }
            }
            Branch::Source2007 => BspFormat { static_props: StaticPropFormat::V6, ..base },
            Branch::BlackMesa => BspFormat { static_props: StaticPropFormat::V11BlackMesa, ..base },
            Branch::TheShip => BspFormat { static_props: StaticPropFormat::V5Ship, ..base },
            Branch::BloodyGoodTime => BspFormat { static_props: StaticPropFormat::V6Bgt, ..base },
            Branch::Left4Dead => BspFormat { static_props: StaticPropFormat::V8, ..base },
            Branch::Left4Dead2 => {
                BspFormat { version: 21, layout: LumpLayout::VersionFirst, static_props: StaticPropFormat::V9, ..base }
            }
            Branch::AlienSwarm | Branch::Portal2 => BspFormat {
                version: 21,
                static_props: StaticPropFormat::V9,
                worldlights_version: 1,
                overlay_system_levels: true,
                ..base
            },
            Branch::Csgo => BspFormat {
                version: 21,
                static_props: StaticPropFormat::V11Csgo,
                worldlights_version: 1,
                overlay_system_levels: true,
                disp_multiblend_lump: Some(crate::header::lump::DISP_MULTIBLEND),
                ..base
            },
            Branch::Insurgency => BspFormat {
                version: 21,
                static_props: StaticPropFormat::V10Csgo,
                worldlights_version: 1,
                overlay_system_levels: true,
                ..base
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BspFormat {
    pub branch: Branch,
    pub version: i32,
    pub layout: LumpLayout,
    pub static_props: StaticPropFormat,
    pub detail_props_version: u16,
    pub worldlights_version: u32,
    pub overlay_system_levels: bool,
    pub disp_multiblend_lump: Option<usize>,
    pub limits: LimitSet,
    pub max_lightmap_dim: u32,
}

impl Default for BspFormat {
    fn default() -> Self {
        Branch::GarrysMod.format()
    }
}

pub const fn worldlight_size(version: u32) -> usize {
    if version >= 1 { 100 } else { 88 }
}

pub fn worldlights_v0_to_v1(data: &[u8]) -> Vec<u8> {
    if !data.len().is_multiple_of(88) {
        return data.to_vec();
    }
    let mut out = Vec::with_capacity(data.len() / 88 * 100);
    for r in data.chunks_exact(88) {
        out.extend_from_slice(&r[..36]);
        out.extend_from_slice(&[0u8; 12]);
        out.extend_from_slice(&r[36..]);
    }
    out
}

pub fn worldlights_v1_to_v0(data: &[u8]) -> Vec<u8> {
    if !data.len().is_multiple_of(100) {
        return data.to_vec();
    }
    let mut out = Vec::with_capacity(data.len() / 100 * 88);
    for r in data.chunks_exact(100) {
        out.extend_from_slice(&r[..36]);
        out.extend_from_slice(&r[48..]);
    }
    out
}

pub fn worldlights_as_v0(data: &[u8], version: u32) -> std::borrow::Cow<'_, [u8]> {
    if version >= 1 { std::borrow::Cow::Owned(worldlights_v1_to_v0(data)) } else { std::borrow::Cow::Borrowed(data) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_gmod_is_the_default() {
        for b in Branch::ALL {
            assert_eq!(Branch::from_id(b.id()), Some(b));
            assert_eq!(b.format().branch, b);
        }
        assert_eq!(BspFormat::default(), Branch::GarrysMod.format());
        assert_eq!(Branch::default(), Branch::GarrysMod);
    }

    #[test]
    fn formats_of_the_branches() {
        let gmod = Branch::GarrysMod.format();
        assert_eq!(
            (gmod.version, gmod.layout, gmod.static_props, gmod.worldlights_version),
            (20, LumpLayout::Standard, StaticPropFormat::V10, 0)
        );
        assert_eq!((gmod.limits, gmod.max_lightmap_dim), (LimitSet::Gmod, 125));
        let tf2 = Branch::Source2013.format();
        assert_eq!(
            (tf2.version, tf2.static_props, tf2.limits, tf2.max_lightmap_dim),
            (20, StaticPropFormat::V10, LimitSet::Sdk, 32)
        );
        let l4d2 = Branch::Left4Dead2.format();
        assert_eq!(
            (l4d2.version, l4d2.layout, l4d2.static_props, l4d2.worldlights_version),
            (21, LumpLayout::VersionFirst, StaticPropFormat::V9, 0)
        );
        let p2 = Branch::Portal2.format();
        assert_eq!(
            (p2.version, p2.layout, p2.static_props.size(), p2.worldlights_version),
            (21, LumpLayout::Standard, 72, 1)
        );
        assert!(p2.overlay_system_levels);
        assert_eq!(Branch::Csgo.format().static_props.size(), 80);
        assert_eq!(Branch::Insurgency.format().static_props.size(), 76);
        assert_eq!(Branch::BlackMesa.format().static_props, StaticPropFormat::V11BlackMesa);
        assert!(!Branch::BlackMesa.format().overlay_system_levels, "lump 61 is the multiblend of Black Mesa");
        let multiblend: Vec<Branch> =
            Branch::ALL.into_iter().filter(|b| b.format().disp_multiblend_lump == Some(63)).collect();
        assert_eq!(multiblend, [Branch::GarrysMod, Branch::Source2013, Branch::Csgo]);
        assert_eq!(Branch::Left4Dead.format().static_props.size(), 68);
        for b in Branch::ALL {
            assert_eq!(b.format().detail_props_version, 4);
        }
    }

    #[test]
    fn world_lights_convert_between_versions() {
        let mut v0 = Vec::new();
        for k in 0..2u8 {
            v0.extend((0..88u8).map(|i| i.wrapping_add(k)));
        }
        let v1 = worldlights_v0_to_v1(&v0);
        assert_eq!(v1.len(), 200);
        assert_eq!(&v1[36..48], &[0u8; 12]);
        assert_eq!(&v1[..36], &v0[..36]);
        assert_eq!(&v1[48..100], &v0[36..88]);
        assert_eq!(worldlights_v1_to_v0(&v1), v0);
        assert_eq!(worldlights_as_v0(&v1, 1).as_ref(), v0.as_slice());
        assert_eq!(worldlights_as_v0(&v0, 0).as_ref(), v0.as_slice());
        assert_eq!((worldlight_size(0), worldlight_size(1)), (88, 100));
        assert_eq!(worldlights_v0_to_v1(&[1, 2, 3]), vec![1, 2, 3]);
    }
}
