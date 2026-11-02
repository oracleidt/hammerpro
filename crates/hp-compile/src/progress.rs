use crate::decode::decode_bytes;

const PACIFIER: &[u8] = b"0...1...2...3...4...5...6...7...8...9...10";

const PACIFIER_TICKS: u8 = 40;

const MAX_LINE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Piece {
    Line(String),
    Progress { fraction: f32, phase: Option<String> },
}

#[derive(Debug, Default)]
pub(crate) struct OutputSplitter {
    buf: Vec<u8>,
    pending_cr: bool,
    last_ticks: Option<u8>,
    phase: Option<String>,
}

impl OutputSplitter {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn feed(&mut self, data: &[u8], out: &mut impl FnMut(Piece)) {
        let mut rest = data;
        while !rest.is_empty() {
            if self.pending_cr {
                self.pending_cr = false;
                if rest.first() == Some(&b'\n') {
                    rest = rest.get(1..).unwrap_or_default();
                    self.end_line(out);
                    continue;
                }
                self.reset_line();
            }
            match memchr::memchr2(b'\r', b'\n', rest) {
                Some(pos) => {
                    self.push_bytes(rest.get(..pos).unwrap_or_default(), out);
                    let terminator = rest.get(pos).copied();
                    rest = rest.get(pos + 1..).unwrap_or_default();
                    if terminator == Some(b'\n') {
                        self.end_line(out);
                    } else {
                        self.pending_cr = true;
                    }
                }
                None => {
                    self.push_bytes(rest, out);
                    rest = &[];
                }
            }
        }
        self.report_partial_progress(out);
    }

    pub(crate) fn finish(&mut self, out: &mut impl FnMut(Piece)) {
        self.pending_cr = false;
        if !self.buf.is_empty() {
            self.end_line(out);
        }
    }

    fn push_bytes(&mut self, bytes: &[u8], out: &mut impl FnMut(Piece)) {
        let mut bytes = bytes;
        while !bytes.is_empty() {
            let room = MAX_LINE_BYTES.saturating_sub(self.buf.len());
            if room == 0 {
                self.end_line(out);
                continue;
            }
            let take = room.min(bytes.len());
            let (head, tail) = bytes.split_at(take);
            self.buf.extend_from_slice(head);
            bytes = tail;
        }
    }

    fn reset_line(&mut self) {
        self.buf.clear();
        self.last_ticks = None;
        self.phase = None;
    }

    fn end_line(&mut self, out: &mut impl FnMut(Piece)) {
        let line = decode_bytes(&self.buf).into_owned();
        if let Some(p) = parse_light_progress(&line) {
            out(Piece::Progress { fraction: p.overall, phase: Some(line.trim().to_owned()) });
            out(Piece::Line(line));
            self.reset_line();
            return;
        }
        if self.last_ticks != Some(PACIFIER_TICKS)
            && let Some(pos) = memchr::memmem::find(&self.buf, PACIFIER)
        {
            let phase = self.phase.take().or_else(|| phase_label(self.buf.get(..pos).unwrap_or_default()));
            out(Piece::Progress { fraction: 1.0, phase });
        }
        out(Piece::Line(line));
        self.reset_line();
    }

    fn report_partial_progress(&mut self, out: &mut impl FnMut(Piece)) {
        let Some((ticks, start)) = pacifier_suffix(&self.buf) else {
            return;
        };
        if self.last_ticks.is_some_and(|last| ticks <= last) {
            return;
        }
        if self.phase.is_none() {
            self.phase = phase_label(self.buf.get(..start).unwrap_or_default());
        }
        self.last_ticks = Some(ticks);
        out(Piece::Progress { fraction: f32::from(ticks) / f32::from(PACIFIER_TICKS), phase: self.phase.clone() });
    }
}

fn pacifier_suffix(line: &[u8]) -> Option<(u8, usize)> {
    let window = line.len().saturating_sub(PACIFIER.len() + 1);
    let tail = line.get(window..)?;
    let start = match tail.iter().rposition(|b| !(b.is_ascii_digit() || *b == b'.')) {
        Some(p) => window + p + 1,
        None if window == 0 => 0,
        None => return None,
    };
    let region = line.get(start..)?;
    if region.len() < 3 || !PACIFIER.starts_with(region) {
        return None;
    }
    if let Some(prev) = start.checked_sub(1).and_then(|i| line.get(i))
        && !(prev.is_ascii_whitespace() || *prev == b':')
    {
        return None;
    }
    Some((ticks_for_prefix(region.len()), start))
}

fn ticks_for_prefix(len: usize) -> u8 {
    let mut pos = 0usize;
    let mut tokens = 0u8;
    for n in 0..=PACIFIER_TICKS / 4 {
        let digits = if n >= 10 { 2 } else { 1 };
        if pos + digits > len {
            break;
        }
        pos += digits;
        tokens += 1;
        if n == PACIFIER_TICKS / 4 {
            break;
        }
        for _ in 0..3 {
            if pos + 1 > len {
                return tokens.saturating_sub(1);
            }
            pos += 1;
            tokens += 1;
        }
    }
    tokens.saturating_sub(1)
}

#[derive(Clone, Debug, PartialEq)]
pub struct LightProgress {
    pub overall: f32,
    pub stage: String,
    pub stage_fraction: f32,
    pub index: u32,
    pub count: u32,
    pub elapsed_s: u64,
    pub left_s: Option<u64>,
}

pub fn parse_light_progress(line: &str) -> Option<LightProgress> {
    let rest = line.trim().strip_prefix("Lighting ")?;
    let (all, rest) = rest.split_once("%: ")?;
    let overall: f32 = all.trim().parse().ok()?;
    let (head, tail) = rest.split_once(" (stage ")?;
    let (stage, pct) = head.rsplit_once(' ')?;
    let pct: f32 = pct.strip_suffix('%')?.parse().ok()?;
    let (index, tail) = tail.split_once(" of ")?;
    let (count, tail) = tail.split_once("), ")?;
    let (elapsed, left) = tail.split_once(" elapsed, ")?;
    let left_s = left.strip_prefix("about ").and_then(|l| l.strip_suffix(" left")).and_then(parse_duration);
    if stage.is_empty() || !(0.0..=100.0).contains(&overall) || !(0.0..=100.0).contains(&pct) {
        return None;
    }
    Some(LightProgress {
        overall: overall / 100.0,
        stage: stage.to_owned(),
        stage_fraction: pct / 100.0,
        index: index.trim().parse().ok()?,
        count: count.trim().parse().ok()?,
        elapsed_s: parse_duration(elapsed)?,
        left_s,
    })
}

fn parse_duration(text: &str) -> Option<u64> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() || !words.len().is_multiple_of(2) {
        return None;
    }
    words.chunks(2).try_fold(0u64, |total, pair| {
        let n: u64 = pair[0].parse().ok()?;
        let unit = match pair[1] {
            "h" => 3600,
            "min" => 60,
            "s" => 1,
            _ => return None,
        };
        Some(total + n * unit)
    })
}

fn phase_label(prefix: &[u8]) -> Option<String> {
    let text = decode_bytes(prefix);
    let label = text.trim().trim_end_matches(':').trim_end();
    (!label.is_empty()).then(|| label.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_chunks(chunks: &[&[u8]]) -> Vec<Piece> {
        let mut splitter = OutputSplitter::new();
        let mut pieces = Vec::new();
        for chunk in chunks {
            splitter.feed(chunk, &mut |p| pieces.push(p));
        }
        splitter.finish(&mut |p| pieces.push(p));
        pieces
    }

    fn lines(pieces: &[Piece]) -> Vec<String> {
        pieces
            .iter()
            .filter_map(|p| match p {
                Piece::Line(l) => Some(l.clone()),
                Piece::Progress { .. } => None,
            })
            .collect()
    }

    fn fractions(pieces: &[Piece]) -> Vec<f32> {
        pieces
            .iter()
            .filter_map(|p| match p {
                Piece::Progress { fraction, .. } => Some(*fraction),
                Piece::Line(_) => None,
            })
            .collect()
    }

    #[test]
    fn tick_table() {
        assert_eq!(ticks_for_prefix(1), 0);
        assert_eq!(ticks_for_prefix(3), 2);
        assert_eq!(ticks_for_prefix(5), 4);
        assert_eq!(ticks_for_prefix(PACIFIER.len() - 1), 39);
        assert_eq!(ticks_for_prefix(PACIFIER.len()), 40);
    }

    #[test]
    fn crlf_lines_split_anywhere() {
        let text = b"12 threads\r\nLoading c:\\maps\\a.vmf\r\n\r\nlast";
        for split in 0..text.len() {
            let (a, b) = text.split_at(split);
            let pieces = run_chunks(&[a, b]);
            assert_eq!(lines(&pieces), vec!["12 threads", "Loading c:\\maps\\a.vmf", "", "last"], "split at {split}");
        }
    }

    #[test]
    fn lone_cr_overwrites_line() {
        let pieces = run_chunks(&[b"progress 10%\rprogress 20%\rdone\n"]);
        assert_eq!(lines(&pieces), vec!["done"]);
    }

    #[test]
    fn byte_by_byte_pacifier() {
        let text = b"BuildFacelights:     0...1...2...3...4...5...6...7...8...9...10 (0)\r\nnext\r\n";
        let chunks: Vec<&[u8]> = text.chunks(1).collect();
        let pieces = run_chunks(&chunks);
        let f = fractions(&pieces);
        assert_eq!(f.len(), 39);
        assert!(f.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(f.first().copied(), Some(2.0 / 40.0));
        assert_eq!(f.last().copied(), Some(1.0));
        assert!(pieces.iter().any(|p| matches!(
            p,
            Piece::Progress { phase: Some(ph), .. } if ph == "BuildFacelights"
        )));
        assert_eq!(lines(&pieces), vec!["BuildFacelights:     0...1...2...3...4...5...6...7...8...9...10 (0)", "next"]);
    }

    #[test]
    fn pacifier_at_arbitrary_boundaries() {
        let text = b"PortalFlow:          0...1...2...3...4...5...6...7...8...9...10 (3)\r\n";
        for a in 0..text.len() {
            for b in a..text.len() {
                let pieces = run_chunks(&[&text[..a], &text[a..b], &text[b..]]);
                let f = fractions(&pieces);
                assert_eq!(f.last().copied(), Some(1.0), "split {a} {b}");
                assert!(f.windows(2).all(|w| w[0] < w[1]), "split {a} {b}: {f:?}");
                assert_eq!(lines(&pieces).len(), 1);
            }
        }
    }

    #[test]
    fn partial_ten_does_not_regress() {
        let mut splitter = OutputSplitter::new();
        let mut got = Vec::new();
        splitter.feed(b"GatherLight: 0...1...2...3...4...5...6...7...8...9...1", &mut |p| got.push(p));
        assert_eq!(fractions(&got), vec![39.0 / 40.0]);
        got.clear();
        splitter.feed(b"0", &mut |p| got.push(p));
        assert_eq!(fractions(&got), vec![1.0]);
    }

    #[test]
    fn whole_line_pacifier_reports_completion() {
        let pieces = run_chunks(&[b"Displacement Alpha : 0...1...2...3...4...5...6...7...8...9...10\r\n"]);
        assert_eq!(fractions(&pieces), vec![1.0]);
        assert!(matches!(
            pieces.first(),
            Some(Piece::Progress { phase: Some(p), .. }) if p == "Displacement Alpha"
        ));
    }

    #[test]
    fn numbers_are_not_progress() {
        for text in [
            &b"Reduced 36 texinfos to 5"[..],
            b"scale 0.5",
            b"version 1.0.",
            b"Found 0",
            b"value0...1",
            b"   4 numportals",
        ] {
            let mut s = OutputSplitter::new();
            let mut got = Vec::new();
            s.feed(text, &mut |p| got.push(p));
            assert!(fractions(&got).is_empty(), "{}", String::from_utf8_lossy(text));
        }
    }

    #[test]
    fn huge_line_is_split() {
        let data = vec![b'a'; MAX_LINE_BYTES * 2 + 10];
        let pieces = run_chunks(&[&data]);
        let l = lines(&pieces);
        assert_eq!(l.len(), 3);
        assert_eq!(l.iter().map(String::len).sum::<usize>(), data.len());
    }

    #[test]
    fn long_lines_byte_by_byte_stay_fast() {
        let data: Vec<u8> = b"0...1...".iter().copied().cycle().take(MAX_LINE_BYTES - 1).collect();
        let mut splitter = OutputSplitter::new();
        let mut progress = 0usize;
        let t = std::time::Instant::now();
        for b in &data {
            splitter.feed(std::slice::from_ref(b), &mut |p| {
                if matches!(p, Piece::Progress { .. }) {
                    progress += 1;
                }
            });
        }
        assert!(t.elapsed() < std::time::Duration::from_secs(2), "took {:?}", t.elapsed());
        assert!(progress <= 8, "{progress}");
    }

    #[test]
    fn light_progress_lines() {
        let line = "Lighting 37%: direct lighting 45% (stage 2 of 7), 1 min 05 s elapsed, about 1 h 02 min left";
        let p = parse_light_progress(line).expect("parsed");
        assert_eq!(
            p,
            LightProgress {
                overall: 0.37,
                stage: "direct lighting".to_owned(),
                stage_fraction: 0.45,
                index: 2,
                count: 7,
                elapsed_s: 65,
                left_s: Some(3720),
            }
        );
        let early =
            parse_light_progress("Lighting 0%: scene 12% (stage 1 of 5), 1 s elapsed, estimating the time left")
                .expect("parsed");
        assert_eq!((early.stage.as_str(), early.left_s), ("scene", None));
        for bad in [
            "Lighting stages: direct lighting",
            "Lighting 140%: x 5% (stage 1 of 2), 1 s elapsed, ",
            "direct light in 5 ms",
        ] {
            assert!(parse_light_progress(bad).is_none(), "{bad}");
        }
        let pieces = run_chunks(&[line.as_bytes(), b"\r\nnext\r\n"]);
        assert_eq!(fractions(&pieces), vec![0.37]);
        assert!(matches!(&pieces[0], Piece::Progress { phase: Some(ph), .. } if ph == line));
        assert_eq!(lines(&pieces), vec![line, "next"]);
    }

    #[test]
    fn invalid_utf8_is_decoded() {
        let pieces = run_chunks(&[b"Loading C:\\Cl\xE9ment\\map.vmf\r\n"]);
        assert_eq!(lines(&pieces), vec!["Loading C:\\Clément\\map.vmf"]);
    }
}
