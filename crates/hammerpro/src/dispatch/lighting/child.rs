use std::io::{Read, Write};
use std::path::Path;

use hp_bspc::rad::{RadOptions, RadQuality};

use super::bsp::{self, FlatLightmaps};

const PREVIEW_SUN_SAMPLES: usize = 16;

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
) -> RadOptions {
    let mut opts = RadOptions { hdr, ldr: !hdr, bounces, quality, threads, ..RadOptions::default() };
    opts.props.shadows = prop_shadows;
    opts.ao.by_default = true;
    opts.ao.in_draft = true;
    opts.min_sun_samples = PREVIEW_SUN_SAMPLES;
    opts.normal_sampling = true;
    opts.props.lighting = prop_lighting;
    opts.probes = Some(hp_bspc::rad::ProbeOptions::default());
    opts
}

pub(crate) struct LitResult {
    pub flat: FlatLightmaps,
    pub bsp: Vec<u8>,
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
    log: &mut dyn FnMut(&str),
) -> Result<LitResult, String> {
    let mut opts = options(hdr, quality, bounces, threads, prop_shadows, prop_lighting);
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
    Ok(LitResult { flat, bsp: bsp.bytes().to_vec() })
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
    Ok(LitResult { flat: FlatLightmaps { offsets, texels }, bsp })
}

pub(crate) fn run(args: &[String]) -> i32 {
    let (game, prop_shadows, prop_lighting, args) = match args {
        [rest @ .., game, shadows, lighting] if rest.len() == 6 => {
            (Some(game.as_str()), shadows != "0", lighting != "0", rest)
        }
        [rest @ .., game, shadows] if rest.len() == 6 => (Some(game.as_str()), shadows != "0", true, rest),
        [rest @ .., game] if rest.len() == 6 => (Some(game.as_str()), true, true, rest),
        _ => (None, true, true, args),
    };
    let game = game.filter(|g| !g.is_empty());
    let [input, output, hdr, quality, bounces, threads] = args else {
        println!(
            "Error: usage: hammerpro {LIGHT_ARG} <in.bsp> <out> <hdr 0|1> <draft|normal|final> <bounces> <threads> [game] [prop shadows 0|1] [prop lighting 0|1]"
        );
        return 1;
    };
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
        let lit = LitResult { flat: flat.clone(), bsp: b"VBSP and the rest".to_vec() };
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
    fn prop_options_follow_the_settings() {
        for shadows in [true, false] {
            for lighting in [true, false] {
                let o = options(true, RadQuality::Final, 4, Some(2), shadows, lighting);
                assert_eq!((o.props.shadows, o.props.lighting), (shadows, lighting));
                assert!(o.hdr && !o.ldr);
                assert_eq!((o.bounces, o.threads, o.quality), (4, Some(2), RadQuality::Final));
                assert!(o.ao.by_default && o.ao.in_draft, "the preview keeps the ambient occlusion of a compile");
                assert_eq!(o.min_sun_samples, 16, "the sun penumbra of a Normal compile at every quality");
                assert!(o.normal_sampling, "the preview samples the sky and the bounce like a Normal compile");
            }
        }
    }
}
