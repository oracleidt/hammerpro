use std::io::{Read, Write};
use std::path::Path;

use hp_bspc::rad::{RadOptions, RadQuality};

use super::bsp::{self, FlatLightmaps};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Tuning {
    pub sun_samples: usize,
    pub ambient_samples: usize,
    pub supersample: bool,
    pub quick: bool,
}

impl Default for Tuning {
    fn default() -> Self {
        Self { sun_samples: 6, ambient_samples: 40, supersample: false, quick: false }
    }
}

const MIN_AMBIENT_SAMPLES: usize = 16;
const MAX_SAMPLES: usize = 256;
const MAX_AMBIENT_SAMPLES: usize = 162 * 32;

impl Tuning {
    pub fn of(o: &hp_editor::luxels::LightPlus) -> Self {
        Self {
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
        format!("{},{},{},{}", self.sun_samples, self.ambient_samples, u8::from(self.supersample), u8::from(self.quick))
    }

    fn parse(s: &str) -> Self {
        let d = Self::default();
        let mut parts = s.split(',').map(str::trim);
        let mut number = |default: usize, lo: usize, hi: usize| {
            parts.next().and_then(|v| v.parse::<usize>().ok()).map_or(default, |v| v.clamp(lo, hi))
        };
        let sun_samples = number(d.sun_samples, 1, MAX_SAMPLES);
        let ambient_samples = number(d.ambient_samples, MIN_AMBIENT_SAMPLES, MAX_AMBIENT_SAMPLES);
        let supersample = number(0, 0, 1) == 1;
        let quick = number(0, 0, 1) == 1;
        Self { sun_samples, ambient_samples, supersample, quick }
    }
}

pub(crate) const LIGHT_ARG: &str = "--light-preview";

const MAGIC: &[u8; 4] = b"HPLP";
const VERSION: u32 = 3;

pub(crate) fn quality_arg(q: RadQuality) -> &'static str {
    match q {
        RadQuality::Draft => "draft",
        RadQuality::Normal => "normal",
        RadQuality::Final => "final",
    }
}

fn parse_quality(s: &str) -> RadQuality {
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
    opts
}

pub(crate) struct LitResult {
    pub flat: FlatLightmaps,
    pub bsp: Vec<u8>,
    pub peak: Option<super::memory::Usage>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn light(
    bytes: &[u8],
    hdr: bool,
    quality: RadQuality,
    bounces: u32,
    threads: Option<usize>,
    game: Option<&Path>,
    prop_shadows: bool,
    prop_lighting: bool,
    tuning: Tuning,
    log: &mut dyn FnMut(&str),
) -> Result<LitResult, String> {
    let mut opts = options(hdr, quality, bounces, threads, prop_shadows, prop_lighting, tuning);
    let world = crate::bspc_cli::world_lights_rad(bytes);
    opts.texlights.files = crate::bspc_cli::light_files(game, None, None, world.as_deref(), None, log);
    let lit = if prop_shadows || prop_lighting {
        let models = crate::bspc_cli::LazyModels::of_game(game.map(Path::to_path_buf));
        hp_bspc::rad::light_bsp_with(bytes, &opts, &models, log)?
    } else {
        hp_bspc::rad::light_bsp(bytes, &opts, log)?
    };
    let bsp = hp_bsp::Bsp::from_vec(lit).map_err(|e| e.to_string())?;
    let flat = bsp::flat_lightmaps(&bsp, hdr)?;
    Ok(LitResult { flat, bsp: for_models(bsp)?, peak: None })
}

fn for_models(bsp: hp_bsp::Bsp) -> Result<Vec<u8>, String> {
    use hp_bsp::lump;
    let keep = |i: usize| {
        matches!(i, lump::ENTITIES | lump::GAME_LUMP | lump::PAKFILE)
            || i == hp_light::probes::LUMP
            || crate::dispatch::health::portals::PROP_LIGHTING_LUMPS.contains(&i)
    };
    let mut edit = bsp.edit();
    for i in (0..hp_bsp::LUMP_COUNT).filter(|i| !keep(*i)) {
        edit.remove_lump(i).map_err(|e| e.to_string())?;
    }
    edit.to_vec().map_err(|e| e.to_string())
}

pub(crate) fn write_result(path: &Path, lit: &LitResult) -> std::io::Result<()> {
    let flat = &lit.flat;
    let bsp_len = u32::try_from(lit.bsp.len()).map_err(|_| std::io::Error::other("the lit map is too large"))?;
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    out.write_all(MAGIC)?;
    for v in [VERSION, flat.offsets.len() as u32, flat.texels.len() as u32, bsp_len] {
        out.write_all(&v.to_le_bytes())?;
    }
    for v in flat.offsets.iter().chain(&flat.texels) {
        out.write_all(&v.to_le_bytes())?;
    }
    out.write_all(&lit.bsp)?;
    out.flush()
}

pub(crate) fn read_result(path: &Path) -> Result<LitResult, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut head = [0u8; 20];
    file.read_exact(&mut head).map_err(|e| e.to_string())?;
    let word = |i: usize| u32::from_le_bytes([head[i], head[i + 1], head[i + 2], head[i + 3]]);
    if &head[..4] != MAGIC || word(4) != VERSION {
        return Err("not a lighting preview result".to_owned());
    }
    let (faces, texels, bsp_len) = (word(8) as usize, word(12) as usize, word(16) as usize);
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    if len != 20 + 4 * (faces as u64 + texels as u64) + bsp_len as u64 {
        return Err("truncated lighting preview result".to_owned());
    }
    let mut read = |n: usize| -> Result<Vec<u32>, String> {
        let mut bytes = vec![0u8; n * 4];
        file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
        Ok(bytes.chunks_exact(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
    };
    let offsets = read(faces)?;
    let texels = read(texels)?;
    let mut bsp = vec![0u8; bsp_len];
    file.read_exact(&mut bsp).map_err(|e| e.to_string())?;
    Ok(LitResult { flat: FlatLightmaps { offsets, texels }, bsp, peak: None })
}

pub(crate) fn run(args: &[String]) -> i32 {
    let (Some(args), extra) = (args.get(..6), args.get(6..).unwrap_or_default()) else {
        println!(
            "Error: usage: hammerpro {LIGHT_ARG} <in.bsp> <out> <hdr 0|1> <draft|normal|final> <bounces> <threads> [game] [prop shadows 0|1] [prop lighting 0|1] [sun samples,ambient samples,supersample 0|1,quick 0|1]"
        );
        return 1;
    };
    let [input, output, hdr, quality, bounces, threads] = args else { return 1 };
    let game = extra.first().map(String::as_str).filter(|g| !g.is_empty());
    let prop_shadows = extra.get(1).is_none_or(|v| v != "0");
    let prop_lighting = extra.get(2).is_none_or(|v| v != "0");
    let tuning = extra.get(3).map_or_else(Tuning::default, |t| Tuning::parse(t));
    let _ = std::thread::Builder::new().name("hp-light-watch".to_owned()).spawn(|| {
        let mut buf = [0u8; 64];
        let mut stdin = std::io::stdin();
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => std::process::exit(3),
                Ok(_) => {}
            }
        }
    });
    let data = match std::fs::read(input) {
        Ok(d) => d,
        Err(e) => {
            println!("Error: cannot read {input}: {e}");
            return 1;
        }
    };
    let threads = threads.parse::<usize>().ok().filter(|n| *n > 0);
    let mut log = |l: &str| {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{l}");
        let _ = out.flush();
    };
    let lit = light(
        &data,
        hdr == "1",
        parse_quality(quality),
        bounces.parse().unwrap_or(100),
        threads,
        game.map(Path::new),
        prop_shadows,
        prop_lighting,
        tuning,
        &mut log,
    );
    drop(data);
    let lit = match lit {
        Ok(f) => f,
        Err(e) => {
            println!("Error: {e}");
            return 1;
        }
    };
    if let Err(e) = write_result(Path::new(output), &lit) {
        println!("Error: cannot write {output}: {e}");
        return 1;
    }
    println!("done");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_round_trip() {
        let dir = std::env::temp_dir().join(format!("hp-light-child-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("r.bin");
        let flat = FlatLightmaps {
            offsets: vec![0, u32::MAX, 4 | FlatLightmaps::BUMPED],
            texels: vec![1, 2, 3, 4, 5, 6, 7, 8],
        };
        let lit = LitResult { flat: flat.clone(), bsp: b"VBSP and the rest".to_vec(), peak: None };
        write_result(&path, &lit).expect("write");
        let read = read_result(&path).expect("read");
        assert_eq!(read.bsp, lit.bsp);
        let back = read.flat;
        assert_eq!(back, flat);
        assert_eq!((back.face(0), back.face(1), back.face(2), back.face(3)), (Some((0, 1)), None, Some((4, 4)), None));
        std::fs::write(&path, b"HPLP\x03\0\0\0\x05\0\0\0\0\0\0\0\0\0\0\0").expect("write");
        assert!(read_result(&path).is_err(), "truncated");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(parse_quality(quality_arg(RadQuality::Final)), RadQuality::Final);
        assert_eq!(parse_quality(quality_arg(RadQuality::Draft)), RadQuality::Draft);
    }

    #[test]
    fn tuning_follows_the_lighting_options() {
        let t = Tuning { sun_samples: 12, ambient_samples: 40, supersample: true, quick: false };
        assert_eq!(Tuning::parse(&t.arg()), t);
        assert_eq!(Tuning::parse(&t.quick().arg()), t.quick());
        assert_eq!(Tuning::parse("x"), Tuning::default());
        assert_eq!(Tuning::parse("9").sun_samples, 9, "a lone sun sample count");
        assert_eq!(options(true, RadQuality::Normal, 1, None, true, true, t).min_sun_samples, 12);
        let plus = hp_editor::luxels::LightPlus { samples_ambient: 3, ..Default::default() };
        assert_eq!(Tuning::of(&plus).ambient_samples, MIN_AMBIENT_SAMPLES);
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
                assert_eq!((o.bounces, o.threads, o.quality), (4, Some(2), RadQuality::Final));
                assert_eq!(o.min_sun_samples, 6, "the Sun Samples of Hammer++");
                assert!(o.normal_sampling, "the preview samples the sky and the bounce like a Normal compile");
            }
        }
    }
}
