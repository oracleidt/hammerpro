use hp_bspc::rad::{RadOptions, RadQuality};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Tuning {
    pub prop_polys: bool,
    pub texture_shadows: bool,
    pub no_disp_shadows: bool,
    pub fast_ambient: bool,
    pub sun_samples: usize,
    pub ambient_samples: usize,
    pub supersample: bool,
    pub quick: bool,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            sun_samples: 6,
            ambient_samples: 40,
            supersample: false,
            quick: false,
            prop_polys: false,
            texture_shadows: false,
            no_disp_shadows: false,
            fast_ambient: false,
        }
    }
}

const MIN_AMBIENT_SAMPLES: usize = 16;
const MAX_SAMPLES: usize = 256;
const MAX_AMBIENT_SAMPLES: usize = 162 * 32;

impl Tuning {
    pub fn of(o: &hp_editor::luxels::LightPlus) -> Self {
        Self {
            prop_polys: o.prop_polys,
            texture_shadows: o.texture_shadows,
            no_disp_shadows: o.no_disp_shadows,
            fast_ambient: o.fast_ambient,
            sun_samples: (o.samples_sun as usize).clamp(1, MAX_SAMPLES),
            ambient_samples: (o.samples_ambient as usize).clamp(MIN_AMBIENT_SAMPLES, MAX_AMBIENT_SAMPLES),
            supersample: o.supersample,
            quick: false,
        }
    }

    pub fn quick(self) -> Self {
        Self { quick: true, ..self }
    }

    pub(crate) fn arg(self) -> String {
        format!(
            "{},{},{},{},{},{},{},{}",
            self.sun_samples,
            self.ambient_samples,
            u8::from(self.supersample),
            u8::from(self.quick),
            u8::from(self.prop_polys),
            u8::from(self.texture_shadows),
            u8::from(self.no_disp_shadows),
            u8::from(self.fast_ambient)
        )
    }

    pub(crate) fn parse(s: &str) -> Self {
        let d = Self::default();
        let mut parts = s.split(',').map(str::trim);
        let mut number = |default: usize, lo: usize, hi: usize| {
            parts.next().and_then(|v| v.parse::<usize>().ok()).map_or(default, |v| v.clamp(lo, hi))
        };
        let sun_samples = number(d.sun_samples, 1, MAX_SAMPLES);
        let ambient_samples = number(d.ambient_samples, MIN_AMBIENT_SAMPLES, MAX_AMBIENT_SAMPLES);
        let supersample = number(0, 0, 1) == 1;
        let quick = number(0, 0, 1) == 1;
        Self {
            sun_samples,
            ambient_samples,
            supersample,
            quick,
            prop_polys: number(u8::from(d.prop_polys) as usize, 0, 1) == 1,
            texture_shadows: number(u8::from(d.texture_shadows) as usize, 0, 1) == 1,
            no_disp_shadows: number(u8::from(d.no_disp_shadows) as usize, 0, 1) == 1,
            fast_ambient: number(u8::from(d.fast_ambient) as usize, 0, 1) == 1,
        }
    }
}

pub(crate) const LIGHT_ARG: &str = "--light-preview";

const PREVIEW_BOUNCE_BUDGET: usize = 192 << 20;

pub(crate) fn quality_arg(q: RadQuality) -> &'static str {
    match q {
        RadQuality::Draft => "draft",
        RadQuality::Normal => "normal",
        RadQuality::Final => "final",
    }
}

pub(crate) fn parse_quality(s: &str) -> RadQuality {
    match s {
        "draft" => RadQuality::Draft,
        "final" => RadQuality::Final,
        _ => RadQuality::Normal,
    }
}

pub(crate) fn options(
    hdr: bool,
    quality: RadQuality,
    bounces: u32,
    threads: Option<usize>,
    prop_shadows: bool,
    prop_lighting: bool,
    tuning: Tuning,
) -> RadOptions {
    let mut opts = RadOptions { hdr, ldr: !hdr, bounces, quality, threads, ..RadOptions::default() };
    opts.props.shadows = prop_shadows;
    opts.props.polys = tuning.prop_polys;
    opts.props.texture_shadows = tuning.texture_shadows;
    opts.no_disp_shadows = tuning.no_disp_shadows;
    opts.fast_ambient = tuning.fast_ambient;
    opts.min_sun_samples = tuning.sun_samples;
    if tuning.quick {
        opts.quality = RadQuality::Draft;
        opts.extra_sky = Some(tuning.ambient_samples as f32 / 162.0);
        opts.no_extra = !tuning.supersample;
    } else {
        opts.normal_sampling = true;
    }
    opts.props.lighting = prop_lighting;
    opts.probes = (!tuning.quick).then(hp_bspc::rad::ProbeOptions::default);
    opts.bounce_budget = PREVIEW_BOUNCE_BUDGET;
    opts
}

pub(crate) fn for_models(bsp: hp_bsp::Bsp) -> Result<Vec<u8>, String> {
    use hp_bsp::lump;
    let animated = super::style_maps::has_animation(&bsp);
    let keep = |i: usize| {
        matches!(i, lump::ENTITIES | lump::GAME_LUMP | lump::PAKFILE)
            || i == hp_light::probes::LUMP
            || crate::dispatch::health::portals::PROP_LIGHTING_LUMPS.contains(&i)
            || (animated
                && matches!(
                    i,
                    lump::FACES | lump::FACES_HDR | lump::LIGHTING | lump::LIGHTING_HDR | super::bsp::lump::TEXINFO
                ))
    };
    let mut edit = bsp.edit();
    for i in (0..hp_bsp::LUMP_COUNT).filter(|i| !keep(*i)) {
        edit.remove_lump(i).map_err(|e| e.to_string())?;
    }
    edit.to_vec().map_err(|e| e.to_string())
}

pub(crate) fn run(args: &[String]) -> i32 {
    if args.first().is_some_and(|argument| argument == super::host::SERVE_ARG) {
        return super::host::server::run(&args[1..]);
    }
    println!("Error: usage: hammerpro {LIGHT_ARG} {} [threads]", super::host::SERVE_ARG);
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tuning_follows_the_lighting_options() {
        let t = Tuning { sun_samples: 12, ambient_samples: 40, supersample: true, quick: false, ..Default::default() };
        assert_eq!(Tuning::parse(&t.arg()), t);
        assert_eq!(Tuning::parse(&t.quick().arg()), t.quick());
        assert_eq!(Tuning::parse("x"), Tuning::default());
        assert_eq!(Tuning::parse("9").sun_samples, 9, "a lone sun sample count");
        assert_eq!(options(true, RadQuality::Normal, 1, None, true, true, t).min_sun_samples, 12);
        let plus = hp_editor::luxels::LightPlus { samples_ambient: 3, ..Default::default() };
        assert_eq!(Tuning::of(&plus).ambient_samples, MIN_AMBIENT_SAMPLES);
        let advanced =
            Tuning { prop_polys: true, texture_shadows: true, no_disp_shadows: true, fast_ambient: true, ..t };
        assert_eq!(Tuning::parse(&advanced.arg()), advanced);
        let mapped = options(true, RadQuality::Normal, 3, None, true, true, advanced);
        assert!(mapped.props.polys && mapped.props.texture_shadows && mapped.no_disp_shadows && mapped.fast_ambient);
        assert_eq!(mapped.bounces, 3);
        assert_eq!(Tuning::parse("12,40,1,0"), t);
    }

    #[test]
    fn the_quick_pass_follows_hammer_plus_plus() {
        let t = Tuning::of(&hp_editor::luxels::LightPlus::default()).quick();
        let o = options(true, RadQuality::Final, 100, None, true, true, t);
        assert_eq!(o.quality, RadQuality::Draft);
        assert!(!o.normal_sampling && o.no_extra, "no supersampling, like the Hammer++ default");
        assert_eq!(o.extra_sky.map(|n| (162.0 * n).round() as usize), Some(40), "the 40 ambient samples");
        assert_eq!(o.min_sun_samples, 6);
        assert!(o.probes.is_none(), "the light probes of the Lit mode wait for the refined pass");
        let slow = options(true, RadQuality::Final, 100, None, true, true, Tuning { quick: false, ..t });
        assert_eq!(
            (slow.quality, slow.normal_sampling, slow.no_extra, slow.extra_sky),
            (RadQuality::Final, true, false, None)
        );
        assert!(slow.probes.is_some());
    }

    #[test]
    fn prop_options_follow_the_settings() {
        for shadows in [true, false] {
            for lighting in [true, false] {
                let o = options(true, RadQuality::Final, 4, Some(2), shadows, lighting, Tuning::default());
                assert_eq!((o.props.shadows, o.props.lighting), (shadows, lighting));
                assert!(o.hdr && !o.ldr);
                assert!(
                    o.bounce_budget < hp_bspc::rad::BOUNCE_BUDGET,
                    "the preview bounces within a smaller memory budget"
                );
                assert_eq!((o.bounces, o.threads, o.quality), (4, Some(2), RadQuality::Final));
                assert_eq!(o.min_sun_samples, 6, "the Sun Samples of Hammer++");
                assert!(o.normal_sampling, "the preview samples the sky and the bounce like a Normal compile");
            }
        }
    }
}
