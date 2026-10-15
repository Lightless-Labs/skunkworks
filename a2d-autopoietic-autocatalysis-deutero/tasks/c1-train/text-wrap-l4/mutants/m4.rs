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
struct Word { chars: Vec<char>, marks: Vec<usize> }
#[derive(Clone)]
struct Line { pieces: Vec<String>, consumed: usize, kind: u8, indent: usize }
impl Line {
    fn text(&self) -> String { " ".repeat(self.indent) + &self.pieces.join(" ") }
    fn used(&self) -> usize { self.indent + self.pieces.iter().map(|p| p.chars().count()).sum::<usize>() + self.pieces.len() - 1 }
}
fn sep(c: char) -> bool { matches!(c, ' '|'\t'|'\n'|'\r'|'\u{a0}') }
fn paragraphs(input: &str) -> Vec<Option<String>> {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let chars: Vec<char> = normalized.chars().collect();
    let mut out = Vec::new();
    let mut segment = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\n' {
            let mut end = i + 1;
            while end < chars.len() && chars[end] == '\n' { end += 1; }
            if end - i >= 2 {
                out.push(Some(std::mem::take(&mut segment)));
                for _ in 0..end-i-1 { out.push(None); }
            } else { segment.push('\n'); }
            i = end;
        } else { segment.push(chars[i]); i += 1; }
    }
    out.push(Some(segment));
    out
}
fn words(segment: &str, markers: bool) -> Vec<Word> {
    segment.split(sep).filter(|s| !s.is_empty()).map(|s| {
        let mut w = Word { chars: Vec::new(), marks: Vec::new() };
        for c in s.chars() {
            if markers && c == '~' { w.marks.push(w.chars.len()); }
            else { w.chars.push(c); }
        }
        w
    }).collect()
}
fn validate(input: &str, o: &WrapOptions) -> Result<(), WrapError> {
    if o.width == 0 { return Err(WrapError::WidthZero); }
    if o.width > 256 { return Err(WrapError::WidthTooLarge { width: o.width }); }
    if o.first_indent >= o.width { return Err(WrapError::FirstIndentTooWide { indent: o.first_indent }); }
    if o.rest_indent >= o.width { return Err(WrapError::RestIndentTooWide { indent: o.rest_indent }); }
    if o.hyphen_cost > 1000000 { return Err(WrapError::HyphenCostTooLarge { cost: o.hyphen_cost }); }
    let count = input.chars().count(); if count > 512 { return Err(WrapError::InputTooLong { chars: count }); }
    for (index, c) in input.char_indices() {
        if c.is_control() && !matches!(c, '\t'|'\r'|'\n') { return Err(WrapError::InvalidControl { index }); }
    }
    
    let cs: Vec<(usize, char)> = input.char_indices().collect();
    for (i, &(index, c)) in cs.iter().enumerate() {
        if c == '~' && (i == 0 || i + 1 == cs.len() || sep(cs[i-1].1) || cs[i-1].1 == '~' || sep(cs[i+1].1) || cs[i+1].1 == '~') {
            return Err(WrapError::InvalidMarker { index });
        }
    }

    Ok(())
}

#[derive(Clone)]
struct Plan { cost: u64, hyphens: usize, lines: Vec<Line> }
fn better(a: &Plan, b: &Plan) -> bool {
    let ka = (a.cost, a.hyphens, a.lines.len());
    let kb = (b.cost, b.hyphens, b.lines.len());
    if ka != kb { return ka < kb; }
    let ac: Vec<usize> = a.lines.iter().map(|l| l.consumed).collect();
    let bc: Vec<usize> = b.lines.iter().map(|l| l.consumed).collect();
    if ac != bc { return ac > bc; }
    let at: Vec<String> = a.lines.iter().map(Line::text).collect();
    let bt: Vec<String> = b.lines.iter().map(Line::text).collect();
    at < bt
}
fn solve(ws: &[Word], o: &WrapOptions, wi: usize, at: usize, first: bool,
    memo: &mut std::collections::HashMap<(usize,usize,bool), Plan>) -> Plan {
    if wi == ws.len() { return Plan { cost: 0, hyphens: 0, lines: Vec::new() }; }
    if let Some(p) = memo.get(&(wi,at,first)) { return p.clone(); }
    let indent = if first { o.first_indent } else { o.rest_indent };
    let cap = o.width - indent;
    let mut candidates: Vec<(Line,usize,usize)> = Vec::new();
    let mut pieces = Vec::<String>::new();
    let mut used = 0;
    let mut consumed = 0;
    for i in wi..ws.len() {
        let w = &ws[i];
        let offset = if i == wi { at } else { 0 };
        let gap = usize::from(!pieces.is_empty());
        if used + gap >= cap { break; }
        let room = cap - used - gap;
        let fits: Vec<usize> = w.marks.iter().copied().filter(|&m| m > offset && m-offset+1 <= room).collect();
        for &m in &fits {
            let mut ps = pieces.clone();
            ps.push(w.chars[offset..m].iter().collect::<String>() + "-");
            candidates.push((Line { pieces: ps, consumed: consumed + m-offset, kind: 1, indent }, i, m));
        }
        let left = w.chars.len() - offset;
        if pieces.is_empty() && left > cap && fits.is_empty() {
            candidates.push((Line { pieces: vec![w.chars[offset..offset+cap].iter().collect()], consumed: cap, kind: 2, indent }, i, offset+cap));
        }
        if left > room { break; }
        pieces.push(w.chars[offset..].iter().collect());
        used += gap + left;
        consumed += left;
        candidates.push((Line { pieces: pieces.clone(), consumed, kind: 0, indent }, i+1, 0));
    }
    let mut best: Option<Plan> = None;
    for (line, ni, na) in candidates {
        let tail = solve(ws, o, ni, na, false, memo);
        let final_line = ni == ws.len();
        let gap = (o.width - line.used()) as u64;
        let mut local = if final_line { gap * gap / 4 } else { gap * gap };
        if line.kind == 1 { local += o.hyphen_cost as u64; }
        if line.kind == 2 { local += (2 * o.width + 1) as u64; }
        if !final_line && line.pieces.len() == 1 { local += 3; }
        let mut p = Plan { cost: local + tail.cost, hyphens: tail.hyphens + usize::from(line.kind == 1), lines: vec![line] };
        p.lines.extend(tail.lines);
        if best.as_ref().is_none_or(|b| better(&p,b)) { best = Some(p); }
    }
    let best = best.unwrap();
    memo.insert((wi,at,first), best.clone());
    best
}

fn render(lines: &[Line], o: &WrapOptions) -> Vec<String> {
    lines.iter().enumerate().map(|(index, l)| {
        let gaps = l.pieces.len() - 1;
        if !o.justify || index + 1 == lines.len() || gaps == 0 { return l.text(); }
        let extra = o.width - l.used();
        let mut spaces = vec![1 + extra / gaps; gaps];
        let start = 0;
        for k in 0..extra % gaps { spaces[(start + k) % gaps] += 1; }
        let mut s = " ".repeat(l.indent);
        for (i, p) in l.pieces.iter().enumerate() {
            if i > 0 { s.push_str(&" ".repeat(spaces[i-1])); }
            s.push_str(p);
        }
        s
    }).collect()
}

pub fn wrap(input: &str, options: &WrapOptions) -> Result<Vec<String>, WrapError> {
    validate(input, options)?;
    let mut out = Vec::new();
    for segment in paragraphs(input) {
        if let Some(s) = segment {
            let ws = words(&s, true);
            let lines = solve(&ws, options, 0, 0, true, &mut std::collections::HashMap::new()).lines;
            out.extend(render(&lines, options));
        } else { out.push(String::new()); }
    }
    Ok(out)
}
