pub(crate) mod client;
pub(crate) mod engine;
pub(crate) mod raytrace;
pub(crate) mod server;

use std::io::{Read, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub(crate) const SERVE_ARG: &str = "serve";

const MAGIC: [u8; 4] = *b"HPLH";
const MAX_HEAD: u32 = 256 << 20;
const MAX_BLOB: u64 = 4 << 30;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Mounts {
    pub root: Option<PathBuf>,
    pub game_dir: Option<PathBuf>,
    pub mounts: Vec<hp_fs::SourceMount>,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct LightOptions {
    pub hdr: bool,
    pub quality: String,
    pub bounces: u32,
    pub prop_shadows: bool,
    pub prop_lighting: bool,
    pub tuning: String,
    pub threads: usize,
    #[serde(default)]
    pub raytracing: Option<RayOptions>,
    #[serde(default)]
    pub rad: RadFiles,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct RadFiles {
    pub lights: Option<String>,
    pub map: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct RayOptions {
    pub args: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct RayReport {
    pub device: String,
    pub gpu: bool,
    pub pass: u32,
    pub progress: f32,
    pub samples: u32,
    pub target_samples: u32,
    pub denoised: bool,
    pub converged: bool,
    pub seconds: f32,
    pub faces: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct LitStats {
    pub full: bool,
    pub lights: usize,
    pub changed_lights: usize,
    pub relit_faces: usize,
    pub faces: usize,
    pub luxels: u64,
    pub triangles: usize,
    pub scene_seconds: f32,
    pub direct_seconds: f32,
    pub bounce_seconds: f32,
    pub seconds: f32,
    pub session_mb: u64,
    pub process_mb: u64,
    pub skybox: Option<SkyboxShadeStat>,
}

pub(crate) type SkyboxShadeStat = hp_app::runtime::SkyboxShadeStatus;

pub(crate) fn skybox_stat(s: &hp_bspc::rad::SkyboxShade) -> SkyboxShadeStat {
    SkyboxShadeStat {
        sunlit: s.sunlit,
        shaded: s.shaded,
        camera: s.cameras.first().map_or([0.0; 3], |c| [c.x, c.y, c.z]),
        cameras: s.cameras.len() as u32,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct CostEntry {
    pub hammerid: Option<u32>,
    pub classname: String,
    pub origin: [f32; 3],
    pub faces: usize,
    pub luxels: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) enum Request {
    Setup {
        mounts: Mounts,
        threads: usize,
    },
    Compile {
        job: u64,
        key: u64,
        factor: f64,
        threads: usize,
    },
    Geometry {
        job: u64,
    },
    Relight {
        job: u64,
        removed: Vec<u32>,
        options: LightOptions,
        focus: Option<[f32; 3]>,
        stream: bool,
        all: bool,
        exact: bool,
    },
    Models {
        job: u64,
        options: LightOptions,
    },
    Costs {
        job: u64,
        options: LightOptions,
    },
    Cancel {
        job: u64,
    },
    Pace {
        interactive: bool,
        #[serde(default)]
        camera: Option<[f32; 3]>,
    },
    Release,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) enum Reply {
    Progress {
        job: u64,
        phase: u8,
        fraction: f32,
    },
    Compiled {
        job: u64,
        key: u64,
        leaked: bool,
        warnings: Vec<String>,
        seconds: f32,
        peak_mb: u64,
    },
    Partial {
        job: u64,
        #[serde(default)]
        pass: u32,
    },
    Raytrace {
        job: u64,
        report: RayReport,
    },
    Shafts {
        job: u64,
    },
    Lit {
        job: u64,
        stats: LitStats,
    },
    Models {
        job: u64,
        seconds: f32,
    },
    Costs {
        job: u64,
        costs: Vec<CostEntry>,
    },
    Failed {
        job: u64,
        cancelled: bool,
        message: String,
    },
    Log {
        line: String,
    },
}

impl Reply {
    pub(crate) fn job(&self) -> u64 {
        match self {
            Reply::Progress { job, .. }
            | Reply::Compiled { job, .. }
            | Reply::Partial { job, .. }
            | Reply::Raytrace { job, .. }
            | Reply::Shafts { job }
            | Reply::Lit { job, .. }
            | Reply::Models { job, .. }
            | Reply::Costs { job, .. }
            | Reply::Failed { job, .. } => *job,
            Reply::Log { .. } => 0,
        }
    }
}

pub(crate) const PHASE_SCENE: u8 = 0;
pub(crate) const PHASE_COMPILE: u8 = 1;
pub(crate) const PHASE_DIRECT: u8 = 2;
pub(crate) const PHASE_BOUNCE: u8 = 3;
pub(crate) const PHASE_OUTPUT: u8 = 4;
pub(crate) const PHASE_MODELS: u8 = 5;

pub(crate) fn write_frame<W: Write>(out: &mut W, head: &impl Serialize, blob: &[u8]) -> std::io::Result<()> {
    let head = serde_json::to_vec(head).map_err(std::io::Error::other)?;
    let len = u32::try_from(head.len()).map_err(|_| std::io::Error::other("message too large"))?;
    out.write_all(&MAGIC)?;
    out.write_all(&len.to_le_bytes())?;
    out.write_all(&(blob.len() as u64).to_le_bytes())?;
    out.write_all(&head)?;
    out.write_all(blob)?;
    out.flush()
}

pub(crate) fn read_frame<R: Read, T: for<'de> Deserialize<'de>>(
    input: &mut R,
) -> std::io::Result<Option<(T, Vec<u8>)>> {
    let mut magic = [0u8; 4];
    let mut filled = 0;
    while filled < 4 {
        match input.read(&mut magic[filled..])? {
            0 if filled == 0 => return Ok(None),
            0 => return Err(std::io::ErrorKind::UnexpectedEof.into()),
            n => filled += n,
        }
    }
    while magic != MAGIC {
        magic.copy_within(1.., 0);
        let mut byte = [0u8; 1];
        if input.read(&mut byte)? == 0 {
            return Ok(None);
        }
        magic[3] = byte[0];
    }
    let mut sizes = [0u8; 12];
    input.read_exact(&mut sizes)?;
    let head_len = u32::from_le_bytes([sizes[0], sizes[1], sizes[2], sizes[3]]);
    let blob_len = u64::from_le_bytes(sizes[4..12].try_into().unwrap_or([0; 8]));
    if head_len > MAX_HEAD || blob_len > MAX_BLOB {
        return Err(std::io::Error::other("lighting message too large"));
    }
    let mut head = vec![0u8; head_len as usize];
    input.read_exact(&mut head)?;
    let mut blob = vec![0u8; blob_len as usize];
    input.read_exact(&mut blob)?;
    let value = serde_json::from_slice(&head).map_err(std::io::Error::other)?;
    Ok(Some((value, blob)))
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Layer {
    pub pattern: String,
    pub image: Vec<[f32; 3]>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FaceTexels {
    pub face: u32,
    pub width: u32,
    pub height: u32,
    pub normals: u32,
    pub texels: Vec<u32>,
    pub layers: Vec<Layer>,
}

pub(crate) fn encode_faces(faces: &[FaceTexels]) -> Vec<u8> {
    let size: usize = faces
        .iter()
        .map(|f| {
            16 + f.texels.len() * 4 + f.layers.iter().map(|l| 4 + l.pattern.len() + l.image.len() * 12).sum::<usize>()
        })
        .sum();
    let mut out = Vec::with_capacity(4 + size);
    out.extend_from_slice(&(faces.len() as u32).to_le_bytes());
    for f in faces {
        out.extend_from_slice(&f.face.to_le_bytes());
        out.extend_from_slice(&(f.width as u16).to_le_bytes());
        out.extend_from_slice(&(f.height as u16).to_le_bytes());
        out.extend_from_slice(&(f.normals as u16).to_le_bytes());
        out.extend_from_slice(&(f.layers.len() as u16).to_le_bytes());
        out.extend_from_slice(&(f.texels.len() as u32).to_le_bytes());
        for t in &f.texels {
            out.extend_from_slice(&t.to_le_bytes());
        }
        for l in &f.layers {
            out.extend_from_slice(&(l.pattern.len() as u16).to_le_bytes());
            out.extend_from_slice(&(l.image.len() as u16).to_le_bytes());
            out.extend_from_slice(l.pattern.as_bytes());
            for v in &l.image {
                for c in v {
                    out.extend_from_slice(&c.to_le_bytes());
                }
            }
        }
    }
    out
}

pub(crate) type GlowRecord = ([f32; 3], [f32; 3], f32);

pub(crate) fn encode_shafts(
    vertices: &[([f32; 3], [f32; 2], [f32; 4])],
    indices: &[u32],
    glows: &[GlowRecord],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + vertices.len() * 36 + indices.len() * 4);
    out.extend_from_slice(&(vertices.len() as u32).to_le_bytes());
    for (p, uv, c) in vertices {
        for v in p.iter().chain(uv).chain(c) {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out.extend_from_slice(&(indices.len() as u32).to_le_bytes());
    for i in indices {
        out.extend_from_slice(&i.to_le_bytes());
    }
    out.extend_from_slice(&(glows.len() as u32).to_le_bytes());
    for (p, c, size) in glows {
        for v in p.iter().chain(c).chain(std::iter::once(size)) {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

pub(crate) fn decode_shafts(data: &[u8]) -> Result<hp_render::light_shafts::LightShafts, String> {
    let word = |at: usize| -> Result<[u8; 4], String> {
        data.get(at..at + 4).and_then(|b| b.try_into().ok()).ok_or_else(|| "truncated light shafts".to_owned())
    };
    let count = u32::from_le_bytes(word(0)?) as usize;
    if count > data.len() / 36 {
        return Err("light shafts too large".to_owned());
    }
    let mut at = 4;
    let mut vertices = Vec::with_capacity(count);
    for _ in 0..count {
        let mut f = [0f32; 9];
        for v in &mut f {
            *v = f32::from_le_bytes(word(at)?);
            at += 4;
        }
        vertices.push(hp_render::light_shafts::ShaftVertex {
            pos: [f[0], f[1], f[2]],
            uv: [f[3], f[4]],
            color: [f[5], f[6], f[7], f[8]],
        });
    }
    let n = u32::from_le_bytes(word(at)?) as usize;
    at += 4;
    if n > (data.len() - at) / 4 {
        return Err("truncated light shafts".to_owned());
    }
    let mut indices = Vec::with_capacity(n);
    for _ in 0..n {
        indices.push(u32::from_le_bytes(word(at)?));
        at += 4;
    }
    let mut glows = Vec::new();
    if at < data.len() {
        let g = u32::from_le_bytes(word(at)?) as usize;
        at += 4;
        if g > (data.len() - at) / 28 {
            return Err("truncated glow halos".to_owned());
        }
        for _ in 0..g {
            let mut f = [0f32; 7];
            for v in &mut f {
                *v = f32::from_le_bytes(word(at)?);
                at += 4;
            }
            glows.push(hp_render::light_shafts::GlowHalo {
                center: [f[0], f[1], f[2]],
                color: [f[3], f[4], f[5]],
                size: f[6],
                pad: 0.0,
            });
        }
    }
    Ok(hp_render::light_shafts::LightShafts { vertices, indices, glows })
}

pub(crate) fn decode_faces(data: &[u8]) -> Result<Vec<FaceTexels>, String> {
    struct Cursor<'a>(&'a [u8], usize);
    impl Cursor<'_> {
        fn take(&mut self, n: usize) -> Result<&[u8], String> {
            let end = self.1.checked_add(n).filter(|e| *e <= self.0.len()).ok_or("truncated lightmaps")?;
            let s = &self.0[self.1..end];
            self.1 = end;
            Ok(s)
        }
        fn u16(&mut self) -> Result<u16, String> {
            let b = self.take(2)?;
            Ok(u16::from_le_bytes([b[0], b[1]]))
        }
        fn u32(&mut self) -> Result<u32, String> {
            let b = self.take(4)?;
            Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        }
        fn f32(&mut self) -> Result<f32, String> {
            Ok(f32::from_bits(self.u32()?))
        }
    }
    let mut c = Cursor(data, 0);
    let count = c.u32()? as usize;
    let mut out = Vec::with_capacity(count.min(data.len() / 16));
    for _ in 0..count {
        let face = c.u32()?;
        let width = u32::from(c.u16()?);
        let height = u32::from(c.u16()?);
        let normals = u32::from(c.u16()?);
        let layers = c.u16()? as usize;
        let n = c.u32()? as usize;
        if n != (width * height * normals) as usize {
            return Err("lightmap size mismatch".to_owned());
        }
        let mut texels = Vec::with_capacity(n);
        for _ in 0..n {
            texels.push(c.u32()?);
        }
        let mut list = Vec::with_capacity(layers);
        for _ in 0..layers {
            let plen = c.u16()? as usize;
            let ilen = c.u16()? as usize;
            let pattern = String::from_utf8_lossy(c.take(plen)?).into_owned();
            let mut image = Vec::with_capacity(ilen);
            for _ in 0..ilen {
                image.push([c.f32()?, c.f32()?, c.f32()?]);
            }
            list.push(Layer { pattern, image });
        }
        out.push(FaceTexels { face, width, height, normals, texels, layers: list });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip_and_resync_after_noise() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"stray output\n");
        let req = Request::Relight {
            job: 7,
            removed: vec![3, 4],
            options: LightOptions {
                hdr: true,
                quality: "draft".into(),
                bounces: 100,
                prop_shadows: true,
                prop_lighting: false,
                tuning: "6,40,0,1,0,0,0,0".into(),
                threads: 4,
                raytracing: Some(RayOptions { args: vec!["-raytracing".into(), "-rtbounces".into(), "9".into()] }),
                rad: RadFiles { lights: Some("E:/x/my.rad".into()), map: Some("E:/x/map.vmf".into()) },
            },
            focus: Some([1.0, 2.0, 3.0]),
            stream: true,
            all: false,
            exact: false,
        };
        write_frame(&mut buf, &req, b"entity {}").unwrap();
        write_frame(&mut buf, &Request::Release, &[]).unwrap();
        let mut input = std::io::Cursor::new(buf);
        let (first, blob): (Request, Vec<u8>) = read_frame(&mut input).unwrap().unwrap();
        assert_eq!(first, req);
        assert_eq!(blob, b"entity {}");
        let (second, blob): (Request, Vec<u8>) = read_frame(&mut input).unwrap().unwrap();
        assert_eq!(second, Request::Release);
        assert!(blob.is_empty());
        assert!(read_frame::<_, Request>(&mut input).unwrap().is_none());
    }

    #[test]
    fn lightmaps_round_trip() {
        let faces = vec![
            FaceTexels { face: 3, width: 2, height: 1, normals: 1, texels: vec![7, 9], layers: Vec::new() },
            FaceTexels {
                face: 9,
                width: 1,
                height: 1,
                normals: 4,
                texels: vec![1, 2, 3, 4],
                layers: vec![Layer { pattern: "mmamma".into(), image: vec![[0.5, 0.25, 1.0]; 4] }],
            },
        ];
        assert_eq!(decode_faces(&encode_faces(&faces)).unwrap(), faces);
        let verts = [([1.0, 2.0, 3.0], [0.0, 1.0], [0.5, 0.4, 0.3, 0.9]); 3];
        let halo = ([4.0, 5.0, 6.0], [1.0, 0.5, 0.25], 48.0);
        let shafts = decode_shafts(&encode_shafts(&verts, &[0, 1, 2], &[halo])).unwrap();
        assert_eq!(shafts.glows.len(), 1);
        assert_eq!((shafts.glows[0].center, shafts.glows[0].size), ([4.0, 5.0, 6.0], 48.0));
        assert_eq!(shafts.indices, vec![0, 1, 2]);
        assert_eq!(shafts.vertices[2].color, [0.5, 0.4, 0.3, 0.9]);
        assert_eq!(shafts.vertices[0].pos, [1.0, 2.0, 3.0]);
        assert!(decode_shafts(&encode_shafts(&verts, &[0, 1, 2], &[])[..30]).is_err());
        assert!(decode_shafts(&encode_shafts(&verts, &[0, 1, 2], &[halo])[..130]).is_err());
        assert!(decode_shafts(&[255, 255, 255, 255]).is_err());
        assert!(decode_faces(&encode_faces(&faces)[..20]).is_err());
    }
}
