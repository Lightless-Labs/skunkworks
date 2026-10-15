#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrapError {
    WidthZero,
    WidthTooLarge { width: usize },
    FirstIndentTooWide { indent: usize },
    RestIndentTooWide { indent: usize },
    HyphenCostTooLarge { cost: usize },
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

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00A0}')
}

#[derive(Clone)]
struct Line {
    pieces: Vec<String>,
    indent: usize,
}

impl Line {
    fn content_len(&self) -> usize {
        self.pieces.iter().map(|s| s.chars().count()).sum::<usize>()
            + self.pieces.len().saturating_sub(1)
    }

    fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }
}

fn render_lines(lines: Vec<Line>, options: &WrapOptions) -> Vec<String> {
    let count = lines.len();
    lines.into_iter().enumerate().map(|(line_index, line)| {
        let gaps = line.pieces.len().saturating_sub(1);
        let pre_width = line.indent + line.content_len();
        let extra = if options.justify && line_index + 1 < count && gaps > 0 {
            options.width - pre_width
        } else {
            0
        };
        let base = if gaps > 0 { extra / gaps } else { 0 };
        let remainder = if gaps > 0 { extra % gaps } else { 0 };
        let start = if gaps > 0 { line_index % gaps } else { 0 };

        let mut out = " ".repeat(line.indent);
        for (i, piece) in line.pieces.iter().enumerate() {
            out.push_str(piece);
            if i < gaps {
                let cyclic = if gaps == 0 { 0 } else { (i + gaps - start) % gaps };
                out.push_str(&" ".repeat(1 + base + usize::from(cyclic < remainder)));
            }
        }
        out
    }).collect()
}

fn layout_paragraph(words: &[Vec<char>], options: &WrapOptions) -> Vec<String> {
    let mut lines = Vec::<Line>::new();
    let mut current = Line { pieces: Vec::new(), indent: options.first_indent };

    for word in words {
        let mut offset = 0usize;
        while offset < word.len() {
            let capacity = options.width - current.indent;
            let current_len = current.content_len();
            let room = capacity - current_len;
            let suffix_len = word.len() - offset;

            if suffix_len + usize::from(!current.is_empty()) <= room {
                current.pieces.push(word[offset..].iter().filter(|&&c| c != '~').collect());
                offset = word.len();
                continue;
            }

            let available = if current.is_empty() { capacity } else { room };
            let rightmost = ((offset + 1)..word.len())
                .filter(|&i| word[i] == '~' && i < word.len() - 1 && i - offset + 1 <= available)
                .last()
                .map(|i| (i, available - (i - offset) - 1));

            if let Some((marker, unused)) = rightmost {
                let accepted = current.is_empty()
                    || unused * unused + options.hyphen_cost < available * available;
                if accepted {
                    let mut piece: String = word[offset..marker].iter().filter(|&&c| c != '~').collect();
                    piece.push('-');
                    current.pieces.push(piece);
                    offset = marker + 1;
                    lines.push(current);
                    current = Line { pieces: Vec::new(), indent: options.rest_indent };
                    continue;
                }
            }

            if !current.is_empty() {
                lines.push(current);
                current = Line { pieces: Vec::new(), indent: options.rest_indent };
                continue;
            }

            let visible: Vec<char> = word[offset..].iter().copied().filter(|&c| c != '~').collect();
            if visible.len() <= capacity {
                current.pieces.push(visible.into_iter().collect());
                offset = word.len();
            } else {
                current.pieces.push(visible[..capacity].iter().collect());
                let mut consumed = 0usize;
                while offset < word.len() && consumed < capacity {
                    if word[offset] != '~' {
                        consumed += 1;
                    }
                    offset += 1;
                }
                lines.push(current);
                current = Line { pieces: Vec::new(), indent: options.rest_indent };
            }
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }
    render_lines(lines, options)
}

fn split_words(chars: &[char]) -> Vec<Vec<char>> {
    let mut words = Vec::new();
    let mut current = Vec::new();
    for &c in chars {
        if is_separator(c) {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

pub fn wrap(input: &str, options: &WrapOptions) -> Result<Vec<String>, WrapError> {
    if options.width == 0 {
        return Err(WrapError::WidthZero);
    }
    if options.width > 256 {
        return Err(WrapError::WidthTooLarge { width: options.width });
    }
    if options.first_indent >= options.width {
        return Err(WrapError::FirstIndentTooWide { indent: options.first_indent });
    }
    if options.rest_indent >= options.width {
        return Err(WrapError::RestIndentTooWide { indent: options.rest_indent });
    }
    if options.hyphen_cost > 1_000_000 {
        return Err(WrapError::HyphenCostTooLarge { cost: options.hyphen_cost });
    }

    for (index, c) in input.char_indices() {
        if c.is_control() && !matches!(c, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index });
        }
    }

    let chars: Vec<(usize, char)> = input.char_indices().collect();
    for i in 0..chars.len() {
        if chars[i].1 == '~' {
            let valid_side = |j: isize| {
                if j < 0 || j as usize >= chars.len() {
                    return false;
                }
                let c = chars[j as usize].1;
                c != '~' && !is_separator(c)
            };
            if !valid_side(i as isize - 1) || !valid_side(i as isize + 1) {
                return Err(WrapError::InvalidMarker { index: chars[i].0 });
            }
        }
    }

    let mut normalized = Vec::new();
    let mut iter = input.chars().peekable();
    while let Some(c) = iter.next() {
        if c == '\r' {
            if iter.peek() == Some(&'\n') {
                iter.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(c);
        }
    }
    if normalized.is_empty() {
        return Ok(Vec::new());
    }

    let mut output = Vec::new();
    let mut segment = Vec::new();
    let mut i = 0;
    while i < normalized.len() {
        if normalized[i] == '\n' {
            let mut end = i;
            while end < normalized.len() && normalized[end] == '\n' {
                end += 1;
            }
            if end - i >= 2 {
                output.extend(layout_paragraph(&split_words(&segment), options));
                segment.clear();
                for _ in 1..(end - i) {
                    output.push(String::new());
                }
                i = end;
                continue;
            }
        }
        segment.push(normalized[i]);
        i += 1;
    }
    output.extend(layout_paragraph(&split_words(&segment), options));
    Ok(output)
}

fn main() {}