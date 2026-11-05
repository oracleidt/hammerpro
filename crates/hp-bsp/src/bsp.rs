use std::borrow::Cow;
use std::fs::File;
use std::path::{Path, PathBuf};

use crate::entities::{Entity, parse_entities};
use crate::error::{BspError, Result};
use crate::gamelump::GameLumps;
use crate::header::{HEADER_LEN, Header, LUMP_COUNT, LumpInfo, lump};
use crate::lzma;
use crate::pak::PakArchive;

enum Data {
    Mapped(memmap2::Mmap),
    Owned(Vec<u8>),
}

impl Data {
    fn bytes(&self) -> &[u8] {
        match self {
            Data::Mapped(m) => m,
            Data::Owned(v) => v,
        }
    }
}

pub struct Bsp {
    data: Data,
    header: Header,
    path: Option<PathBuf>,
}

impl std::fmt::Debug for Bsp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bsp")
            .field("path", &self.path)
            .field("version", &self.header.version)
            .field("len", &self.len())
            .finish()
    }
}

impl Bsp {
    pub fn open(path: impl AsRef<Path>) -> Result<Bsp> {
        let path = path.as_ref();
        let file = File::open(path).map_err(|e| BspError::io(path, e))?;
        let len = file.metadata().map_err(|e| BspError::io(path, e))?.len();
        if len < HEADER_LEN as u64 {
            let data = std::fs::read(path).map_err(|e| BspError::io(path, e))?;
            let header = Header::parse(&data)?;
            return Ok(Bsp { data: Data::Owned(data), header, path: Some(path.to_owned()) });
        }
        let map = unsafe { memmap2::Mmap::map(&file) }.map_err(|e| BspError::io(path, e))?;
        let header = Header::parse(&map)?;
        Ok(Bsp { data: Data::Mapped(map), header, path: Some(path.to_owned()) })
    }

    pub fn read(path: impl AsRef<Path>) -> Result<Bsp> {
        let path = path.as_ref();
        let data = std::fs::read(path).map_err(|e| BspError::io(path, e))?;
        let mut bsp = Bsp::from_vec(data)?;
        bsp.path = Some(path.to_owned());
        Ok(bsp)
    }

    pub fn from_vec(data: Vec<u8>) -> Result<Bsp> {
        let header = Header::parse(&data)?;
        Ok(Bsp { data: Data::Owned(data), header, path: None })
    }

    pub fn into_vec(self) -> Vec<u8> {
        match self.data {
            Data::Owned(v) => v,
            Data::Mapped(m) => m.to_vec(),
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn bytes(&self) -> &[u8] {
        self.data.bytes()
    }

    pub fn len(&self) -> usize {
        self.bytes().len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes().is_empty()
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn version(&self) -> i32 {
        self.header.version
    }

    pub fn map_revision(&self) -> i32 {
        self.header.map_revision
    }

    pub fn lump_info(&self, index: usize) -> Result<&LumpInfo> {
        self.header.lumps.get(index).ok_or(BspError::BadLump(index))
    }

    pub fn is_compressed(&self, index: usize) -> bool {
        self.header.lumps.get(index).is_some_and(|l| l.uncompressed_len().is_some() && l.len > 0)
    }

    pub fn any_compressed(&self) -> bool {
        (0..LUMP_COUNT).any(|i| self.is_compressed(i))
    }

    pub fn lump_raw(&self, index: usize) -> Result<&[u8]> {
        let info = self.lump_info(index)?;
        self.bytes()
            .get(info.range())
            .ok_or_else(|| BspError::corrupt(format!("lump {index} lies outside of the file")))
    }

    pub fn lump(&self, index: usize) -> Result<Cow<'_, [u8]>> {
        let raw = self.lump_raw(index)?;
        let info = self.lump_info(index)?;
        match info.uncompressed_len() {
            Some(n) if lzma::is_valve_lzma(raw) => {
                let data = lzma::decompress_valve(raw)?;
                if data.len() != n as usize {
                    return Err(BspError::corrupt(format!("lump {index} has the wrong size once decompressed")));
                }
                Ok(Cow::Owned(data))
            }
            _ => Ok(Cow::Borrowed(raw)),
        }
    }

    pub fn game_lumps(&self) -> Result<GameLumps> {
        let info = *self.lump_info(lump::GAME_LUMP)?;
        if info.uncompressed_len().is_some() && info.len > 0 {
            return GameLumps::parse(&self.lump(lump::GAME_LUMP)?, info.offset);
        }
        GameLumps::parse(self.lump_raw(lump::GAME_LUMP)?, info.offset)
    }

    pub fn entity_text(&self) -> Result<String> {
        let data = self.lump(lump::ENTITIES)?;
        let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
        Ok(String::from_utf8_lossy(&data[..end]).into_owned())
    }

    pub fn entities(&self) -> Result<Vec<Entity>> {
        Ok(parse_entities(&self.entity_text()?))
    }

    pub fn pakfile(&self) -> Result<PakArchive<'_>> {
        PakArchive::parse(self.lump(lump::PAKFILE)?)
    }

    pub fn texture_names(&self) -> Result<Vec<String>> {
        let data = self.lump(lump::TEXDATA_STRING_DATA)?;
        let table = self.lump(lump::TEXDATA_STRING_TABLE)?;
        let mut out = Vec::with_capacity(table.len() / 4);
        for c in table.chunks_exact(4) {
            let ofs = u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize;
            let Some(rest) = data.get(ofs..) else { continue };
            let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len()).min(1024);
            out.push(String::from_utf8_lossy(&rest[..end]).into_owned());
        }
        Ok(out)
    }

    pub fn cubemaps(&self) -> Result<Vec<([i32; 3], i32)>> {
        let data = self.lump(lump::CUBEMAPS)?;
        Ok(data
            .chunks_exact(16)
            .map(|c| {
                let v = |i: usize| i32::from_le_bytes([c[i], c[i + 1], c[i + 2], c[i + 3]]);
                ([v(0), v(4), v(8)], v(12))
            })
            .collect())
    }

    pub fn edit(self) -> crate::BspEdit {
        crate::BspEdit::new(self)
    }
}
