pub(super) struct Span {
    pub range: std::ops::Range<usize>,
    pub vectorized: bool,
}

pub(super) fn spans(first: usize, count: usize) -> impl Iterator<Item = Span> {
    let ranges = if count < 16 {
        [(count > 0).then_some(Span { range: 0..count, vectorized: false }), None, None]
    } else {
        let head = (4 - (first & 3)) & 3;
        let blocks_end = head + (count - head) / 4 * 4;
        [
            (head > 0).then_some(Span { range: 0..head, vectorized: false }),
            (blocks_end > head).then_some(Span { range: head..blocks_end, vectorized: true }),
            (blocks_end < count).then_some(Span { range: blocks_end..count, vectorized: false }),
        ]
    };
    ranges.into_iter().flatten()
}
