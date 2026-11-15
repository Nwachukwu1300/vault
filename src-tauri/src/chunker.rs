//! Splits extracted text into overlapping chunks of roughly `TARGET_TOKENS`,
//! preferring paragraph, then sentence, then word boundaries. Each chunk
//! keeps the page and timestamps of the text it came from.

use crate::extract::Segment;

pub const TARGET_TOKENS: usize = 450;
/// Share of `max_tokens` repeated at the start of the next chunk, so a
/// sentence near a boundary is fully inside at least one chunk.
const OVERLAP: f32 = 0.15;

#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub index: u32,
    pub text: String,
    pub page: Option<u32>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
}

/// Smallest piece the packer works with.
struct Unit<'a> {
    text: &'a str,
    tokens: usize,
    /// Whole paragraph (joined with a blank line) vs. sentence or word run.
    paragraph: bool,
    seg: &'a Segment,
}

pub fn chunk(segments: &[Segment], count_tokens: &dyn Fn(&str) -> usize, max_tokens: usize) -> Vec<Chunk> {
    let units: Vec<Unit> = segments.iter().flat_map(|s| split(s, count_tokens, max_tokens)).collect();
    let overlap = (max_tokens as f32 * OVERLAP) as usize;

    let mut chunks = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    let mut cur_tokens = 0;
    for (i, u) in units.iter().enumerate() {
        if !cur.is_empty() && cur_tokens + u.tokens > max_tokens {
            chunks.push(build(&units, &cur, chunks.len() as u32));
            // Carry trailing units into the next chunk, never the whole
            // chunk, and only if the next unit still fits beside them.
            let mut keep = Vec::new();
            let mut kept = 0;
            for &j in cur.iter().rev().take(cur.len() - 1) {
                if kept + units[j].tokens > overlap {
                    break;
                }
                kept += units[j].tokens;
                keep.push(j);
            }
            if kept + u.tokens > max_tokens {
                keep.clear();
                kept = 0;
            }
            keep.reverse();
            cur = keep;
            cur_tokens = kept;
        }
        cur.push(i);
        cur_tokens += u.tokens;
    }
    if !cur.is_empty() {
        chunks.push(build(&units, &cur, chunks.len() as u32));
    }
    chunks
}

fn build(units: &[Unit], idx: &[usize], index: u32) -> Chunk {
    let mut text = String::new();
    for (n, &i) in idx.iter().enumerate() {
        if n > 0 {
            text.push_str(if units[i].paragraph { "\n\n" } else { " " });
        }
        text.push_str(units[i].text);
    }
    let first = units[idx[0]].seg;
    let last = units[*idx.last().unwrap()].seg;
    Chunk { index, text, page: first.page, start_ms: first.start_ms, end_ms: last.end_ms }
}

/// Paragraphs that fit stay whole; bigger ones break into sentences, and a
/// sentence still bigger than `max` breaks into word runs.
fn split<'a>(seg: &'a Segment, count: &dyn Fn(&str) -> usize, max: usize) -> Vec<Unit<'a>> {
    let mut out = Vec::new();
    for para in seg.text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        let tokens = count(para);
        if tokens <= max {
            out.push(Unit { text: para, tokens, paragraph: true, seg });
            continue;
        }
        for sentence in sentences(para) {
            let tokens = count(sentence);
            if tokens <= max {
                out.push(Unit { text: sentence, tokens, paragraph: false, seg });
            } else {
                words(sentence, count, max, seg, &mut out);
            }
        }
    }
    out
}

/// Sentence ends: `.`, `!` or `?` followed by whitespace, or a line break.
fn sentences(text: &str) -> impl Iterator<Item = &str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let end_here = c == '\n'
            || (matches!(c, '.' | '!' | '?') && chars.peek().is_some_and(|(_, n)| n.is_whitespace()));
        if end_here {
            parts.push(&text[start..i + c.len_utf8()]);
            start = i + c.len_utf8();
        }
    }
    parts.push(&text[start..]);
    parts.into_iter().map(str::trim).filter(|s| !s.is_empty())
}

/// Greedy word runs of at most `max` tokens, as slices of the original text.
fn words<'a>(text: &'a str, count: &dyn Fn(&str) -> usize, max: usize, seg: &'a Segment, out: &mut Vec<Unit<'a>>) {
    let mut start: Option<usize> = None;
    let mut end = 0;
    let mut tokens = 0;
    for (pos, word) in text.split_whitespace().map(|w| (w.as_ptr() as usize - text.as_ptr() as usize, w)) {
        let t = count(word);
        if let Some(s) = start {
            if tokens + t > max {
                out.push(Unit { text: &text[s..end], tokens, paragraph: false, seg });
                start = None;
                tokens = 0;
            }
        }
        start.get_or_insert(pos);
        end = pos + word.len();
        tokens += t;
    }
    if let Some(s) = start {
        out.push(Unit { text: &text[s..end], tokens, paragraph: false, seg });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wc(s: &str) -> usize {
        s.split_whitespace().count()
    }

    fn seg(text: &str) -> Segment {
        Segment { text: text.into(), ..Default::default() }
    }

    /// `n` numbered sentences of 5 words each: "s0 a b c d." ...
    fn sentences_text(n: usize) -> String {
        (0..n).map(|i| format!("s{i} alpha beta gamma delta.")).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn empty_input_gives_no_chunks() {
        assert!(chunk(&[], &wc, 450).is_empty());
        assert!(chunk(&[seg("  \n\n ")], &wc, 450).is_empty());
    }

    #[test]
    fn short_text_is_one_chunk() {
        let c = chunk(&[seg("Hello there.\n\nSecond paragraph.")], &wc, 450);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].text, "Hello there.\n\nSecond paragraph.");
        assert_eq!(c[0].index, 0);
    }

    #[test]
    fn long_text_splits_within_budget_with_overlap() {
        let text = sentences_text(100); // 500 words in one paragraph
        let c = chunk(&[seg(&text)], &wc, 100);
        assert!(c.len() >= 5, "{}", c.len());
        for ch in &c {
            assert!(wc(&ch.text) <= 100, "{}", wc(&ch.text));
            // Whole sentences only: every chunk ends at a full stop.
            assert!(ch.text.ends_with('.'), "{}", ch.text);
        }
        for pair in c.windows(2) {
            let last_sentence = pair[0].text.rsplit(". ").next().unwrap();
            assert!(pair[1].text.contains(last_sentence), "no overlap");
        }
        assert_eq!(c.iter().map(|ch| ch.index).collect::<Vec<_>>(), (0..c.len() as u32).collect::<Vec<_>>());
    }

    #[test]
    fn giant_sentence_falls_back_to_words() {
        let text = vec!["word"; 250].join(" ");
        let c = chunk(&[seg(&text)], &wc, 100);
        assert!(c.iter().all(|ch| wc(&ch.text) <= 100));
        assert!(c.iter().all(|ch| ch.text.split_whitespace().all(|w| w == "word")));
    }

    #[test]
    fn pages_carry_first_page() {
        let pages: Vec<Segment> = (1..=3)
            .map(|p| Segment { text: sentences_text(12), page: Some(p), ..Default::default() })
            .collect(); // 60 words per page
        let c = chunk(&pages, &wc, 50);
        assert_eq!(c.first().unwrap().page, Some(1));
        assert_eq!(c.last().unwrap().page, Some(3));
        // Pages never go backwards.
        assert!(c.windows(2).all(|w| w[0].page <= w[1].page));
    }

    #[test]
    fn timestamps_span_first_to_last_segment() {
        let segs: Vec<Segment> = (0..4)
            .map(|i| Segment {
                text: format!("Sentence number {i} here."),
                start_ms: Some(i * 1000),
                end_ms: Some(i * 1000 + 900),
                ..Default::default()
            })
            .collect();
        let c = chunk(&segs, &wc, 450);
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].start_ms, c[0].end_ms), (Some(0), Some(3900)));
    }
}
