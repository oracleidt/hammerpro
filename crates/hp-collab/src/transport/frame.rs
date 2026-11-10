use std::io::{Read, Write};
use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use bytes::Bytes;
use iroh::endpoint::{RecvStream, SendStream};
use serde::Serialize;

use crate::document::{Snapshot, Unit};
use crate::protocol::Wire;

pub(crate) const INITIAL_CAPACITY: usize = 64 * 1024;
pub(crate) const TRUSTED_PREFIX: usize = 1024 * 1024;
pub(crate) const COMPRESSED: u32 = 0x8000_0000;
pub(crate) const COMPRESS_FROM: usize = 4096;
pub(crate) const FAST: u32 = 1;
pub(crate) const DENSE: u32 = 6;
pub(crate) const MAP_PART_BYTES: usize = 1024 * 1024;
const MAP_BATCH: usize = 8;
const PREFIX: usize = 64;
const COMPRESSED_PREFIX: usize = 4096;
const INFLATE_INLINE: usize = 1024 * 1024;

#[derive(Clone)]
pub(crate) struct Frame {
    parts: Vec<Bytes>,
    len: usize,
    compressed: bool,
}

impl Frame {
    pub(crate) fn encode(message: &Wire) -> Result<Self> {
        Self::serialize(message, super::MAX_FRAME)
    }

    pub(crate) fn serialize(message: &impl Serialize, limit: usize) -> Result<Self> {
        let frame = Self::from_parts(vec![compact(serde_json::to_vec(message)?)])?;
        ensure!(frame.len <= limit, "Outgoing message too large");
        Ok(frame)
    }

    pub(crate) fn pack(message: &impl Serialize, level: u32) -> Result<Self> {
        Self::pack_json(serde_json::to_vec(message)?, level)
    }

    pub(crate) fn pack_json(json: Vec<u8>, level: u32) -> Result<Self> {
        ensure!(!json.is_empty() && json.len() <= super::MAX_FRAME, "Outgoing message too large");
        if json.len() >= COMPRESS_FROM {
            let mut packed = Vec::with_capacity(json.len() / 4 + 8);
            packed.extend_from_slice(&(json.len() as u32).to_be_bytes());
            let mut encoder = flate2::write::DeflateEncoder::new(packed, flate2::Compression::new(level));
            encoder.write_all(&json)?;
            let packed = encoder.finish()?;
            if packed.len() < json.len() {
                return Ok(Self { len: packed.len(), parts: vec![compact(packed)], compressed: true });
            }
        }
        Self::from_parts(vec![compact(json)])
    }

    pub(crate) fn from_parts(parts: Vec<Bytes>) -> Result<Self> {
        let parts: Vec<Bytes> = parts.into_iter().filter(|part| !part.is_empty()).collect();
        let len = parts.iter().map(Bytes::len).sum();
        ensure!(len > 0 && len <= super::MAX_FRAME, "Outgoing message too large");
        Ok(Self { parts, len, compressed: false })
    }

    pub(crate) fn wire_bytes(&self) -> u64 {
        self.len as u64 + 4
    }

    #[cfg(test)]
    pub(crate) fn compressed(&self) -> bool {
        self.compressed
    }

    #[cfg(test)]
    pub(crate) fn concat(&self) -> Vec<u8> {
        self.parts.iter().flat_map(|part| part.iter().copied()).collect()
    }

    #[cfg(test)]
    pub(crate) fn payload(&self) -> Vec<u8> {
        let raw = self.concat();
        if !self.compressed {
            return raw;
        }
        let json = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
        inflate(&raw[4..], json).expect("compressed frames inflate")
    }
}

struct PartRef<'a>(&'a [(&'a String, &'a Unit)]);

impl Serialize for PartRef<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.collect_map(self.0.iter().map(|(key, unit)| (*key, *unit)))
    }
}

#[derive(Serialize)]
enum PartWire<'a> {
    MapPart(PartRef<'a>),
}

pub(crate) struct MapTransfer {
    start: Frame,
    map: Arc<Snapshot>,
}

impl MapTransfer {
    pub(crate) fn new(start: &Wire, map: Arc<Snapshot>) -> Result<Self> {
        Ok(Self { start: Frame::encode(start)?, map })
    }

    fn ranges(entries: &[(&String, &Unit)]) -> Vec<Range<usize>> {
        let mut ranges = Vec::new();
        let (mut first, mut size) = (0, 0);
        for (index, (key, unit)) in entries.iter().enumerate() {
            size += key.len() + unit.vmf.len() + 64;
            if size >= MAP_PART_BYTES {
                ranges.push(first..index + 1);
                first = index + 1;
                size = 0;
            }
        }
        if first < entries.len() {
            ranges.push(first..entries.len());
        }
        ranges
    }

    pub(crate) fn produce(self, mut send: impl FnMut(Result<Frame>) -> bool) {
        use rayon::prelude::*;
        if !send(Ok(self.start.clone())) {
            return;
        }
        let entries: Vec<(&String, &Unit)> = self.map.0.iter().collect();
        for batch in Self::ranges(&entries).chunks(MAP_BATCH) {
            let frames: Vec<Result<Frame>> = batch
                .par_iter()
                .map(|range| Frame::pack(&PartWire::MapPart(PartRef(&entries[range.clone()])), DENSE))
                .collect();
            for frame in frames {
                let failed = frame.is_err();
                if !send(frame) || failed {
                    return;
                }
            }
        }
        send(Frame::encode(&Wire::MapEnd));
    }

    #[cfg(test)]
    pub(crate) fn frames(self) -> Result<Vec<Frame>> {
        let mut frames = Vec::new();
        let mut failure = None;
        self.produce(|frame| match frame {
            Ok(frame) => {
                frames.push(frame);
                true
            }
            Err(error) => {
                failure = Some(error);
                false
            }
        });
        failure.map_or(Ok(frames), Err)
    }
}

pub(crate) fn compact(mut bytes: Vec<u8>) -> Bytes {
    if bytes.len() >= TRUSTED_PREFIX && bytes.capacity() - bytes.len() > bytes.len() / 16 {
        bytes.shrink_to_fit();
    }
    Bytes::from(bytes)
}

pub(crate) async fn write_frame(stream: &mut SendStream, frame: &Frame, stall: Duration) -> Result<u64> {
    let mut chunks = Vec::with_capacity(frame.parts.len() + 1);
    let header = frame.len as u32 | if frame.compressed { COMPRESSED } else { 0 };
    chunks.push(Bytes::copy_from_slice(&header.to_be_bytes()));
    chunks.extend(frame.parts.iter().cloned());
    let mut remaining: &mut [Bytes] = &mut chunks;
    while !remaining.is_empty() {
        let written = tokio::time::timeout(stall, stream.write_many_chunks(&mut remaining))
            .await
            .context("Session send stalled")??;
        ensure!(written > 0 || remaining.is_empty(), "Session stream closed while sending data");
    }
    Ok(frame.wire_bytes())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Header {
    pub(crate) size: usize,
    pub(crate) json: usize,
    pub(crate) compressed: bool,
}

impl Header {
    pub(crate) fn wire_bytes(&self) -> u64 {
        self.size as u64 + 4
    }

    pub(crate) fn largest(&self) -> usize {
        self.size.max(self.json)
    }

    fn body(&self) -> usize {
        if self.compressed { self.size - 4 } else { self.size }
    }
}

async fn read_head(stream: &mut RecvStream, limit: usize, stall: Duration) -> Result<Header> {
    let mut head = [0; 4];
    tokio::time::timeout(stall, stream.read_exact(&mut head)).await.context("Session header receive stalled")??;
    let raw = u32::from_be_bytes(head);
    let compressed = raw & COMPRESSED != 0;
    let size = (raw & !COMPRESSED) as usize;
    ensure!(size > usize::from(compressed) * 4, "Invalid session message");
    ensure!(size <= limit, "Incoming message too large");
    if !compressed {
        return Ok(Header { size, json: size, compressed });
    }
    tokio::time::timeout(stall, stream.read_exact(&mut head)).await.context("Session header receive stalled")??;
    let json = u32::from_be_bytes(head) as usize;
    ensure!(json > 0, "Invalid session message");
    ensure!(json <= limit, "Incoming message too large");
    Ok(Header { size, json, compressed })
}

pub(crate) async fn read_size(stream: &mut RecvStream, stall: Duration) -> Result<Header> {
    read_head(stream, usize::MAX, stall).await
}

pub(crate) async fn read_header(stream: &mut RecvStream, limit: usize, stall: Duration) -> Result<Header> {
    read_head(stream, limit, stall).await
}

pub(crate) async fn discard_payload(
    stream: &mut RecvStream,
    size: usize,
    keep: usize,
    stall: Duration,
) -> Result<Vec<u8>> {
    let mut prefix = Vec::with_capacity(keep);
    let mut left = size;
    while left > 0 {
        let chunk = tokio::time::timeout(stall, stream.read_chunk(left.min(INITIAL_CAPACITY)))
            .await
            .context("Session payload receive stalled")??
            .ok_or_else(|| anyhow::anyhow!("Session stream closed before the complete message was received"))?;
        ensure!(!chunk.is_empty(), "Session stream stopped receiving data");
        let wanted = keep.saturating_sub(prefix.len()).min(chunk.len());
        prefix.extend_from_slice(&chunk[..wanted]);
        left -= chunk.len();
    }
    Ok(prefix)
}

pub(crate) async fn discard_body(stream: &mut RecvStream, header: &Header, stall: Duration) -> Result<Vec<u8>> {
    let keep = if header.compressed { COMPRESSED_PREFIX } else { PREFIX };
    let raw = discard_payload(stream, header.body(), keep, stall).await?;
    Ok(if header.compressed { inflate_prefix(&raw) } else { raw })
}

pub(crate) fn inflate(bytes: &[u8], json: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(json.min(bytes.len().saturating_mul(16).max(INITIAL_CAPACITY)));
    flate2::read::DeflateDecoder::new(bytes)
        .take(json as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|_| anyhow::anyhow!("Invalid session message"))?;
    ensure!(out.len() == json, "Invalid session message");
    Ok(out)
}

pub(crate) fn inflate_prefix(bytes: &[u8]) -> Vec<u8> {
    let mut decoder = flate2::read::DeflateDecoder::new(bytes);
    let mut out = [0; PREFIX];
    let mut filled = 0;
    while filled < PREFIX {
        match decoder.read(&mut out[filled..]) {
            Ok(0) | Err(_) => break,
            Ok(count) => filled += count,
        }
    }
    out[..filled].to_vec()
}

pub(crate) fn edit_sequence(prefix: &[u8]) -> Option<u64> {
    let rest = prefix.strip_prefix(b"{\"Edit\":{\"sequence\":")?;
    let digits = rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
    if digits == 0 || digits > 20 || rest.get(digits) != Some(&b',') {
        return None;
    }
    std::str::from_utf8(&rest[..digits]).ok()?.parse().ok()
}

pub(crate) fn grow(bytes: &mut Vec<u8>, incoming: usize, size: usize) {
    let needed = bytes.len() + incoming;
    if needed > bytes.capacity() {
        let target = if needed > TRUSTED_PREFIX {
            size.max(needed)
        } else {
            needed.max(bytes.capacity().saturating_mul(2)).min(size.max(needed))
        };
        bytes.reserve_exact(target - bytes.len());
    }
}

pub(crate) async fn read_payload(stream: &mut RecvStream, size: usize, stall: Duration) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(size.min(INITIAL_CAPACITY));
    while bytes.len() < size {
        let chunk = tokio::time::timeout(stall, stream.read_chunk(size - bytes.len()))
            .await
            .context("Session payload receive stalled")??
            .ok_or_else(|| anyhow::anyhow!("Session stream closed before the complete message was received"))?;
        ensure!(!chunk.is_empty(), "Session stream stopped receiving data");
        grow(&mut bytes, chunk.len(), size);
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(crate) async fn read_body(stream: &mut RecvStream, header: &Header, stall: Duration) -> Result<Vec<u8>> {
    let bytes = read_payload(stream, header.body(), stall).await?;
    if !header.compressed {
        return Ok(bytes);
    }
    let json = header.json;
    if json < INFLATE_INLINE {
        return inflate(&bytes, json);
    }
    tokio::task::spawn_blocking(move || inflate(&bytes, json)).await?
}

pub(crate) async fn read_frame(
    stream: &mut RecvStream,
    limit: usize,
    header_stall: Duration,
    payload_stall: Duration,
) -> Result<Vec<u8>> {
    let header = read_header(stream, limit, header_stall).await?;
    read_body(stream, &header, payload_stall).await
}

pub(crate) fn decode(bytes: &[u8]) -> Result<Wire> {
    serde_json::from_slice(bytes).map_err(|_| anyhow::anyhow!("Invalid session message"))
}
