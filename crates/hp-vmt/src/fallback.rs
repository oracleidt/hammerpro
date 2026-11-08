use hp_kv::Block;

use crate::Material;
use crate::info::shader_base_name;
use crate::material::set_unique;

pub const DEFAULT_DX_LEVEL: u32 = 95;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    ActiveConditional,
    ActiveShader { hdr: bool },
    Inactive,
    Unrelated,
}

fn parse_dx_level(s: &str) -> Option<u32> {
    let digits = s.strip_prefix("dx")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u32 = digits.parse().ok()?;
    Some(if n < 10 { n * 10 } else { n })
}

fn eval_conditional(name: &str, dx_level: u32) -> Option<bool> {
    let (op, rest) =
        [">=", "<=", "==", "!=", ">", "<", "="].iter().find_map(|op| name.strip_prefix(op).map(|rest| (*op, rest)))?;
    let level = parse_dx_level(rest.trim())?;
    Some(match op {
        ">=" => dx_level >= level,
        "<=" => dx_level <= level,
        ">" => dx_level > level,
        "<" => dx_level < level,
        "!=" => dx_level != level,
        _ => dx_level == level,
    })
}

fn classify(name: &str, base: &str, dx_level: u32, hdr: bool) -> Section {
    let lower = name.trim().to_ascii_lowercase();
    if let Some(active) = eval_conditional(&lower, dx_level) {
        return if active { Section::ActiveConditional } else { Section::Inactive };
    }
    let Some(suffix) =
        lower.strip_prefix("sdk_").unwrap_or(&lower).strip_prefix(base).and_then(|s| s.strip_prefix('_'))
    else {
        return Section::Unrelated;
    };
    if let Some(level) = suffix
        .strip_prefix("hdr_")
        .and_then(parse_dx_level)
        .or_else(|| suffix.strip_suffix("_hdr").and_then(parse_dx_level))
    {
        return if hdr && level >= 90 && dx_level >= 90 {
            Section::ActiveShader { hdr: true }
        } else {
            Section::Inactive
        };
    }
    let Some(level) = parse_dx_level(suffix) else {
        return Section::Unrelated;
    };
    let (lo, hi) = match level {
        90.. => (90, u32::MAX),
        80..=89 => (80, 90),
        70..=79 => (70, 80),
        _ => (0, 70),
    };
    if (lo..hi).contains(&dx_level) { Section::ActiveShader { hdr: false } } else { Section::Inactive }
}

fn apply_section(out: &mut Material, section: &Block) {
    for (k, v) in &section.props {
        set_unique(&mut out.params, k, v);
    }
    if let Some(proxies) = section.child("proxies") {
        out.proxies = Some(proxies.clone());
    }
}

impl Material {
    pub fn resolve_fallbacks(&self, dx_level: u32, hdr: bool) -> Material {
        let base = shader_base_name(&self.shader);
        let mut out = Material {
            shader: self.shader.clone(),
            params: self.params.clone(),
            proxies: self.proxies.clone(),
            other_blocks: Vec::new(),
        };
        let mut shader_sections: Vec<(&Block, bool)> = Vec::new();
        for block in &self.other_blocks {
            match classify(&block.name, &base, dx_level, hdr) {
                Section::ActiveConditional => apply_section(&mut out, block),
                Section::ActiveShader { hdr } => shader_sections.push((block, hdr)),
                Section::Inactive => {}
                Section::Unrelated => out.other_blocks.push(block.clone()),
            }
        }
        shader_sections.sort_by_key(|&(_, hdr)| hdr);
        for (block, _) in shader_sections {
            apply_section(&mut out, block);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    const WATER: &str = r#"
"Water"
{
	"$refractamount" "1"
	"$fogend" "500"
	">=dx90"
	{
		"$reflecttexture" "_rt_WaterReflection"
		"$refractamount" "2"
	}
	"<dx90"
	{
		"$fogend" "100"
	}
	"Water_DX81"
	{
		"$refractamount" ".6"
	}
	"Water_DX60"
	{
		"$fallbackmaterial" "nature/water_dx70"
	}
	"gpu>=1"
	{
		"$x" "1"
	}
	"Proxies"
	{
		"WaterLOD" { }
	}
}
"#;

    #[test]
    fn dx95_water() {
        let m = parse(WATER).unwrap();
        let r = m.resolve_fallbacks(DEFAULT_DX_LEVEL, true);
        assert_eq!(r.get("$refractamount"), Some("2"));
        assert_eq!(r.get("$reflecttexture"), Some("_rt_WaterReflection"));
        assert_eq!(r.get("$fogend"), Some("500"));
        assert_eq!(r.get("$fallbackmaterial"), None);
        assert_eq!(r.other_blocks.len(), 1);
        assert_eq!(r.other_blocks[0].name, "gpu>=1");
        assert!(r.proxies.is_some());
    }

    #[test]
    fn dx81_water() {
        let m = parse(WATER).unwrap();
        let r = m.resolve_fallbacks(81, false);
        assert_eq!(r.get("$refractamount"), Some(".6"));
        assert_eq!(r.get("$fogend"), Some("100"));
        assert_eq!(r.get("$reflecttexture"), None);
        let r60 = m.resolve_fallbacks(60, false);
        assert_eq!(r60.get("$fallbackmaterial"), Some("nature/water_dx70"));
    }

    #[test]
    fn shader_and_hdr_sections() {
        let m = parse(
            r#"SDK_LightmappedGeneric
            {
                $envmaptint "[1 1 1]"
                LightmappedGeneric_HDR_DX9 { $envmaptint "[.2 .2 .2]" }
                lightmappedgeneric_dx9 { $envmaptint "[.5 .5 .5]" $extra 1 Proxies { Sine { } } }
                Other { a b }
            }"#,
        )
        .unwrap();
        let hdr = m.resolve_fallbacks(95, true);
        assert_eq!(hdr.get("$envmaptint"), Some("[.2 .2 .2]"));
        assert_eq!(hdr.get("$extra"), Some("1"));
        assert!(hdr.proxies.as_ref().unwrap().child("sine").is_some());
        assert_eq!(hdr.other_blocks.len(), 1);
        let ldr = m.resolve_fallbacks(95, false);
        assert_eq!(ldr.get("$envmaptint"), Some("[.5 .5 .5]"));
        let dx8 = m.resolve_fallbacks(80, true);
        assert_eq!(dx8.get("$envmaptint"), Some("[1 1 1]"));
    }

    #[test]
    fn conditionals() {
        assert_eq!(eval_conditional(">=dx90", 95), Some(true));
        assert_eq!(eval_conditional(">dx90", 90), Some(false));
        assert_eq!(eval_conditional("<=dx9", 90), Some(true));
        assert_eq!(eval_conditional("<dx80", 81), Some(false));
        assert_eq!(eval_conditional("==dx95", 95), Some(true));
        assert_eq!(eval_conditional("=dx95", 90), Some(false));
        assert_eq!(eval_conditional("!=dx95", 90), Some(true));
        assert_eq!(eval_conditional(">=gpu1", 95), None);
        assert_eq!(eval_conditional(">=dx", 95), None);
        assert_eq!(eval_conditional("proxies", 95), None);
        assert_eq!(parse_dx_level("dx9"), Some(90));
        assert_eq!(parse_dx_level("dx81"), Some(81));
        assert_eq!(parse_dx_level("dx99999999999"), None);
    }

    #[test]
    fn water_dx9_hdr_alias_and_sections_match_the_native_fallback_chain() {
        let alias = parse(r#"Water_DX9_HDR { $normalmap dev/water_normal }"#).unwrap();
        assert_eq!(alias.shader_kind(), crate::ShaderKind::Water);
        assert!(alias.is_water());
        let water = parse(
            r#"Water {
                $reflectamount .87 $refractamount .91
                Water_DX9_HDR { $reflectamount .23 $refractamount .63 }
                Water_DX90 { $reflectamount .31 $refractamount .41 }
            }"#,
        )
        .unwrap();
        let ldr = water.resolve_fallbacks(95, false);
        assert_eq!(ldr.get_f32("$reflectamount"), Some(0.31));
        assert_eq!(ldr.get_f32("$refractamount"), Some(0.41));
        assert!(ldr.other_blocks.is_empty());
        let hdr = water.resolve_fallbacks(95, true);
        assert_eq!(hdr.get_f32("$reflectamount"), Some(0.23));
        assert_eq!(hdr.get_f32("$refractamount"), Some(0.63));
        let dx8 = water.resolve_fallbacks(81, true);
        assert_eq!(dx8.get_f32("$reflectamount"), Some(0.87));
    }
}
