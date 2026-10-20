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

fn sep(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00A0}')
}

fn validate(s: &str) -> Result<(), WrapError> {
    let v: Vec<(usize, char)> = s.char_indices().collect();
    for &(index, c) in &v {
        if c.is_control() && c != '\t' && c != '\r' && c != '\n' {
            return Err(WrapError::InvalidControl { index });
        }
    }
    for i in 0..v.len() {
        if v[i].1 == '~'
            && (i == 0 || i + 1 == v.len()
                || v[i - 1].1 == '~' || v[i + 1].1 == '~'
                || sep(v[i - 1].1) || sep(v[i + 1].1))
        {
            return Err(WrapError::InvalidMarker { index: v[i].0 });
        }
    }
    Ok(())
}

#[derive(Clone)]
struct Word {
    chars: Vec<char>,
    breaks: Vec<usize>,
}

fn parse_words(chars: &[char]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut current = Vec::new();
    let mut breaks = Vec::new();
    let mut marked = false;

    for &c in chars {
        if sep(c) {
            if !current.is_empty() {
                words.push(Word {
                    chars: std::mem::take(&mut current),
                    breaks: std::mem::take(&mut breaks),
                });
            }
            marked = false;
        } else if c == '~' {
            marked = true;
        } else {
            if marked {
                breaks.push(current.len());
            }
            current.push(c);
            marked = false;
        }
    }
    if !current.is_empty() {
        words.push(Word { chars: current, breaks });
    }
    words
}

#[derive(Default)]
struct CurrentLine {
    parts: Vec<String>,
}

impl CurrentLine {
    fn content_len(&self) -> usize {
        self.parts.iter().map(|p| p.chars().count()).sum::<usize>()
            + self.parts.len().saturating_sub(1)
    }

    fn append_word(&mut self, text: String) {
        if !text.is_empty() {
            self.parts.push(text);
        }
    }
}

fn output_line(
    line: &CurrentLine,
    indent: usize,
    width: usize,
    justify: bool,
    final_line: bool,
    line_index: usize,
) -> String {
    let gaps = line.parts.len().saturating_sub(1);
    let content = line.content_len();
    let extra_total = width.saturating_sub(indent + content);
    let each_extra = if gaps == 0 { 0 } else { extra_total / gaps };
    let remainder = if gaps == 0 { 0 } else { extra_total % gaps };
    let start = if gaps == 0 { 0 } else { line_index % gaps };

    let mut out = " ".repeat(indent);
    for (i, part) in line.parts.iter().enumerate() {
        out.push_str(part);
        if i < gaps {
            let spaces = if justify && !final_line && gaps > 0 {
                1 + each_extra + usize::from((i + gaps - start) % gaps < remainder)
            } else {
                1
            };
            out.push_str(&" ".repeat(spaces));
        }
    }
    out
}

fn paragraph(chars: &[char], opt: &WrapOptions, out: &mut Vec<String>) {
    let words = parse_words(chars);
    if words.is_empty() {
        return;
    }

    let mut line = CurrentLine::default();
    let mut first = true;
    let mut line_index = 0usize;

    let emit = |line: &mut CurrentLine, first: &mut bool, line_index: &mut usize, out: &mut Vec<String>, final_line: bool| {
        let indent = if *first { opt.first_indent } else { opt.rest_indent };
        out.push(output_line(line, indent, opt.width, opt.justify, final_line, *line_index));
        *first = false;
        *line_index += 1;
        *line = CurrentLine::default();
    };

    for word in words {
        let mut pos = 0usize;
        while pos < word.chars.len() {
            let indent = if first { opt.first_indent } else { opt.rest_indent };
            let cap = opt.width - indent;
            let line_len = line.content_len();
            let gap = usize::from(!line.parts.is_empty());
            let room = cap.saturating_sub(line_len + gap);
            let remaining = word.chars.len() - pos;

            if remaining <= room {
                if gap != 0 {
                    line.append_word(word.chars[pos..].iter().collect());
                } else {
                    line.append_word(word.chars[pos..].iter().collect());
                }
                break;
            }

            let available = if line.parts.is_empty() { cap } else { room };
            let opportunity = word.breaks.iter().copied()
                .filter(|&b| b > pos && b < word.chars.len())
                .filter(|&b| b - pos + 1 <= available)
                .last();

            let accept = opportunity.and_then(|b| {
                if line.parts.is_empty() {
                    Some(b)
                } else {
                    let used = b - pos + 1;
                    let unused = available - used;
                    let baseline = cap - line_len;
                    let lhs = (unused as u128) * (unused as u128) + opt.hyphen_cost as u128;
                    let rhs = (baseline as u128) * (baseline as u128);
                    (lhs < rhs).then_some(b)
                }
            });

            if let Some(b) = accept {
                let mut fragment: String = word.chars[pos..b].iter().collect();
                fragment.push('-');
                line.append_word(fragment);
                pos = b;
                emit(&mut line, &mut first, &mut line_index, out, false);
                continue;
            }

            if !line.parts.is_empty() {
                emit(&mut line, &mut first, &mut line_index, out, false);
                continue;
            }

            if remaining > cap {
                let end = pos + cap;
                line.append_word(word.chars[pos..end].iter().collect());
                pos = end;
                emit(&mut line, &mut first, &mut line_index, out, false);
            } else {
                line.append_word(word.chars[pos..].iter().collect());
                pos = word.chars.len();
            }
        }
    }

    if !line.parts.is_empty() {
        emit(&mut line, &mut first, &mut line_index, out, true);
    }
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

    validate(input)?;

    let mut normalized = Vec::new();
    let mut it = input.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\r' {
            if it.peek() == Some(&'\n') {
                it.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(c);
        }
    }

    let mut result = Vec::new();
    let mut segment = Vec::new();
    let mut i = 0usize;

    while i < normalized.len() {
        if normalized[i] == '\n' {
            let begin = i;
            while i < normalized.len() && normalized[i] == '\n' {
                i += 1;
            }
            let count = i - begin;
            if count >= 2 {
                paragraph(&segment, options, &mut result);
                segment.clear();
                for _ in 1..count {
                    result.push(String::new());
                }
            } else {
                segment.push('\n');
            }
        } else {
            segment.push(normalized[i]);
            i += 1;
        }
    }
    paragraph(&segment, options, &mut result);
    Ok(result)
}
