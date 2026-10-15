#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrapError {
    WidthZero,
    WidthTooLarge { width: usize },
    FirstIndentTooWide { indent: usize },
    RestIndentTooWide { indent: usize },
    InvalidControl { index: usize },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapOptions {
    pub width: usize,
    pub first_indent: usize,
    pub rest_indent: usize,
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
    
    
    for (index, c) in input.char_indices() {
        if c.is_control() && !matches!(c, '\t'|'\r'|'\n') { return Err(WrapError::InvalidControl { index: input[..index].chars().count() }); }
    }
    
    Ok(())
}

fn greedy(ws: &[Word], o: &WrapOptions) -> Vec<Line> {
    let mut result = Vec::new();
    let mut pieces = Vec::<String>::new();
    let mut used = 0;
    for w in ws {
        let mut at = 0;
        while at < w.chars.len() {
            let indent = if result.is_empty() { o.first_indent } else { o.rest_indent };
            let cap = o.width - indent;
            if used == cap {
                result.push(Line { pieces: std::mem::take(&mut pieces), consumed: 0, kind: 0, indent });
                used = 0;
                continue;
            }
            let gap = usize::from(!pieces.is_empty());
            let room = cap - used - gap;
            let left = w.chars.len() - at;
            if left <= cap && left > room || left > cap && !pieces.is_empty() && room < 2 {
                result.push(Line { pieces: std::mem::take(&mut pieces), consumed: 0, kind: 0, indent });
                used = 0;
                continue;
            }
            let take = if left <= cap { left } else { room };
            pieces.push(w.chars[at..at+take].iter().collect());
            used += gap + take;
            at += take;
            if at < w.chars.len() {
                result.push(Line { pieces: std::mem::take(&mut pieces), consumed: 0, kind: 0, indent });
                used = 0;
            }
        }
    }
    if !pieces.is_empty() {
        let indent = if result.is_empty() { o.first_indent } else { o.rest_indent };
        result.push(Line { pieces, consumed: 0, kind: 0, indent });
    }
    result
}

pub fn wrap(input: &str, options: &WrapOptions) -> Result<Vec<String>, WrapError> {
    validate(input, options)?;
    let mut out = Vec::new();
    for segment in paragraphs(input) {
        if let Some(s) = segment {
            let ws = words(&s, false);
            let lines = greedy(&ws, options);
            out.extend(lines.iter().map(Line::text));
        } else { out.push(String::new()); }
    }
    Ok(out)
}
