#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrapError {
    WidthZero,
    WidthTooLarge { width: usize },
    FirstIndentTooWide { indent: usize },
    RestIndentTooWide { indent: usize },
    HyphenCostTooLarge { cost: usize },
    InputTooLong { chars: usize },
    InvalidControl { index: usize },
    InvalidMarker { index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapOptions {
    pub width: usize,
    pub first_indent: usize,
    pub rest_indent: usize,
    pub hyphen_cost: usize,
    pub justify: bool,
}

#[derive(Clone)]
struct Word {
    chars: Vec<char>,
    markers: Vec<usize>,
}

#[derive(Clone)]
struct Line {
    indent: usize,
    pieces: Vec<String>,
    consumed: usize,
    marker: bool,
    raw: bool,
    gap: usize,
}

#[derive(Clone)]
struct Candidate {
    cost: u64,
    marker_count: usize,
    lines: Vec<Line>,
}

fn line_text(l: &Line) -> String {
    let mut s = " ".repeat(l.indent);
    for (i, p) in l.pieces.iter().enumerate() {
        if i > 0 { s.push(' '); }
        s.push_str(p);
    }
    s
}

fn preferable(a: &Candidate, b: &Candidate) -> bool {
    if a.cost != b.cost { return a.cost < b.cost; }
    if a.marker_count != b.marker_count { return a.marker_count < b.marker_count; }
    if a.lines.len() != b.lines.len() { return a.lines.len() < b.lines.len(); }
    for (x, y) in a.lines.iter().zip(&b.lines) {
        if x.consumed != y.consumed { return x.consumed > y.consumed; }
    }
    for (x, y) in a.lines.iter().zip(&b.lines) {
        let sx = line_text(x);
        let sy = line_text(y);
        if sx != sy { return sx < sy; }
    }
    false
}

fn separators(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
}

fn parse_words(cs: &[char]) -> Vec<Word> {
    let mut out = Vec::new();
    let mut spelling = Vec::new();
    let mut marks = Vec::new();
    for &c in cs {
        match c {
            c if separators(c) => {
                if !spelling.is_empty() {
                    out.push(Word { chars: std::mem::take(&mut spelling), markers: std::mem::take(&mut marks) });
                }
            }
            '~' => marks.push(spelling.len()),
            _ => spelling.push(c),
        }
    }
    if !spelling.is_empty() { out.push(Word { chars: spelling, markers: marks }); }
    out
}

fn solve(words: &[Word], opt: &WrapOptions) -> Vec<Line> {
    use std::collections::HashMap;
    fn key(w: usize, o: usize, f: bool) -> (usize, usize, bool) { (w, o, f) }
    let mut memo: HashMap<(usize, usize, bool), Candidate> = HashMap::new();

    fn rec(
        w: usize, off: usize, first: bool, words: &[Word], opt: &WrapOptions,
        memo: &mut std::collections::HashMap<(usize, usize, bool), Candidate>,
    ) -> Candidate {
        if w == words.len() {
            return Candidate { cost: 0, marker_count: 0, lines: Vec::new() };
        }
        let k = key(w, off, first);
        if let Some(v) = memo.get(&k) { return v.clone(); }
        let indent = if first { opt.first_indent } else { opt.rest_indent };
        let cap = opt.width - indent;
        let word = &words[w];
        let rem = word.chars.len() - off;
        let mut choices: Vec<(Line, usize, usize)> = Vec::new();

        // marker endings in the current word
        for &m in &word.markers {
            if m > off && m < word.chars.len() && m - off + 1 <= cap {
                let mut p: String = word.chars[off..m].iter().collect();
                p.push('-');
                choices.push((Line {
                    indent, pieces: vec![p], consumed: m - off, marker: true, raw: false,
                    gap: cap - (m - off + 1),
                }, w, m));
            }
        }

        // Completed words, optionally several on a line.
        let mut display = 0usize;
        let mut pieces = Vec::new();
        let mut x = w;
        loop {
            if x >= words.len() { break; }
            let begin = if x == w { off } else { 0 };
            let len = words[x].chars.len() - begin;
            let needed = display + usize::from(!pieces.is_empty()) + len;
            if needed > cap { break; }
            display = needed;
            pieces.push(words[x].chars[begin..].iter().collect::<String>());
            choices.push((Line {
                indent, pieces: pieces.clone(), consumed: len, marker: false, raw: false,
                gap: cap - display,
            }, x + 1, 0));
            x += 1;
        }

        // Raw split only on an empty line and only when no marker can be used.
        if rem > cap && !word.markers.iter().any(|&m| m > off && m < word.chars.len() && m - off + 1 <= cap) {
            choices.push((Line {
                indent, pieces: vec![word.chars[off..off + cap].iter().collect()],
                consumed: cap, marker: false, raw: true, gap: 0,
            }, if off + cap == word.chars.len() { w + 1 } else { w }, if off + cap == word.chars.len() { 0 } else { off + cap }));
        }

        let mut best: Option<Candidate> = None;
        for (line, nw, no) in choices {
            let tail = rec(nw, no, false, words, opt, memo);
            let final_line = nw == words.len();
            let sq = (line.gap as u64) * (line.gap as u64);
            let mut cand = Candidate {
                cost: tail.cost + if final_line { sq / 4 } else { sq },
                marker_count: tail.marker_count + usize::from(line.marker),
                lines: std::iter::once(line.clone()).chain(tail.lines.into_iter()).collect(),
            };
            if line.marker { cand.cost += opt.hyphen_cost as u64; }
            if line.raw { cand.cost += (2 * opt.width + 1) as u64; }
            if !final_line && line.pieces.len() == 1 { cand.cost += 3; }
            if best.as_ref().map_or(true, |b| preferable(&cand, b)) { best = Some(cand); }
        }
        let answer = best.unwrap_or(Candidate { cost: 0, marker_count: 0, lines: Vec::new() });
        memo.insert(k, answer.clone());
        answer
    }

    rec(0, 0, true, words, opt, &mut memo).lines
}

fn render(lines: &[Line], opt: &WrapOptions) -> Vec<String> {
    let mut out = Vec::new();
    for (li, line) in lines.iter().enumerate() {
        let g = line.pieces.len().saturating_sub(1);
        let do_justify = opt.justify && li + 1 < lines.len() && g > 0;
        let mut s = " ".repeat(line.indent);
        for i in 0..line.pieces.len() {
            if i > 0 {
                let base = if do_justify { line.gap / g } else { 0 };
                let rem = if do_justify { line.gap % g } else { 0 };
                let start = if g == 0 { 0 } else { li % g };
                let rank = (i - 1 + g - start) % g;
                s.push_str(&" ".repeat(1 + base + usize::from(do_justify && rank < rem)));
            }
            s.push_str(&line.pieces[i]);
        }
        out.push(s);
    }
    out
}

pub fn wrap(input: &str, options: &WrapOptions) -> Result<Vec<String>, WrapError> {
    if options.width == 0 { return Err(WrapError::WidthZero); }
    if options.width > 256 { return Err(WrapError::WidthTooLarge { width: options.width }); }
    if options.first_indent >= options.width { return Err(WrapError::FirstIndentTooWide { indent: options.first_indent }); }
    if options.rest_indent >= options.width { return Err(WrapError::RestIndentTooWide { indent: options.rest_indent }); }
    if options.hyphen_cost > 1_000_000 { return Err(WrapError::HyphenCostTooLarge { cost: options.hyphen_cost }); }

    let original: Vec<(usize, char)> = input.char_indices().collect();
    if original.len() > 512 { return Err(WrapError::InputTooLong { chars: original.len() }); }
    for &(index, c) in &original {
        if c.is_control() && !matches!(c, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index });
        }
    }
    for i in 0..original.len() {
        if original[i].1 == '~' {
            let left = i > 0 && original[i - 1].1 != '~' && !separators(original[i - 1].1);
            let right = i + 1 < original.len() && original[i + 1].1 != '~' && !separators(original[i + 1].1);
            if !left || !right { return Err(WrapError::InvalidMarker { index: original[i].0 }); }
        }
    }

    let mut normalized = Vec::new();
    let mut i = 0;
    while i < original.len() {
        if original[i].1 == '\r' {
            normalized.push('\n');
            if i + 1 < original.len() && original[i + 1].1 == '\n' { i += 1; }
        } else {
            normalized.push(original[i].1);
        }
        i += 1;
    }

    let mut result = Vec::new();
    let mut segment = Vec::new();
    i = 0;
    while i < normalized.len() {
        if normalized[i] == '\n' {
            let mut j = i;
            while j < normalized.len() && normalized[j] == '\n' { j += 1; }
            if j - i >= 2 {
                let words = parse_words(&segment);
                if !words.is_empty() { result.extend(render(&solve(&words, options), options)); }
                segment.clear();
                result.extend(std::iter::repeat(String::new()).take(j - i - 1));
                i = j;
                continue;
            }
        }
        segment.push(normalized[i]);
        i += 1;
    }
    let words = parse_words(&segment);
    if !words.is_empty() { result.extend(render(&solve(&words, options), options)); }
    Ok(result)
}
