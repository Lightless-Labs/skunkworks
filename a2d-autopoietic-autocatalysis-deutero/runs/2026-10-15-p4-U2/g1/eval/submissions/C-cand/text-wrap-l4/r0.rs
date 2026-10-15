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
struct CandidateLine {
    end_word: usize,
    end_offset: usize,
    split: SplitKind,
    text: String,
    consumed: usize,
    pieces: usize,
    gap: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SplitKind {
    Complete,
    Marker,
    Raw,
}

#[derive(Clone)]
struct Solution {
    cost: u64,
    marker_lines: usize,
    lines: Vec<String>,
    consumed: Vec<usize>,
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00A0}')
}

fn parse_segment(chars: &[char]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut current_chars = Vec::new();
    let mut markers = Vec::new();

    for &c in chars {
        if is_separator(c) {
            if !current_chars.is_empty() {
                words.push(Word {
                    chars: std::mem::take(&mut current_chars),
                    markers: std::mem::take(&mut markers),
                });
            }
        } else if c == '~' {
            markers.push(current_chars.len());
        } else {
            current_chars.push(c);
        }
    }

    if !current_chars.is_empty() {
        words.push(Word {
            chars: current_chars,
            markers,
        });
    }

    words
}

fn append_chars(out: &mut String, chars: &[char]) {
    for &c in chars {
        out.push(c);
    }
}

fn enumerate_lines(
    words: &[Word],
    word_index: usize,
    offset: usize,
    indent: usize,
    width: usize,
) -> Vec<CandidateLine> {
    let capacity = width - indent;
    let mut result = Vec::new();
    let mut used = 0usize;
    let mut pieces = 0usize;
    let mut text = " ".repeat(indent);
    let mut wi = word_index;
    let mut off = offset;

    while wi < words.len() {
        let word = &words[wi];
        let remaining = word.chars.len() - off;
        let separator = if pieces == 0 { 0 } else { 1 };
        let available = capacity.saturating_sub(used + separator);

        if remaining <= available {
            if separator != 0 {
                text.push(' ');
                used += 1;
            }
            append_chars(&mut text, &word.chars[off..]);
            used += remaining;
            pieces += 1;
            result.push(CandidateLine {
                end_word: wi + 1,
                end_offset: 0,
                split: SplitKind::Complete,
                text: text.clone(),
                consumed: used,
                pieces,
                gap: width - indent - used,
            });
            wi += 1;
            off = 0;
            if wi == words.len() {
                break;
            }
            continue;
        }

        let marker_fits = word
            .markers
            .iter()
            .copied()
            .filter(|&m| m > off && m < word.chars.len())
            .filter(|&m| m - off + 1 <= available)
            .collect::<Vec<_>>();

        for &marker in &marker_fits {
            if separator != 0 {
                let mut marker_text = text.clone();
                marker_text.push(' ');
                append_chars(&mut marker_text, &word.chars[off..marker]);
                marker_text.push('-');
                let displayed = used + 1 + (marker - off) + 1;
                result.push(CandidateLine {
                    end_word: wi,
                    end_offset: marker,
                    split: SplitKind::Marker,
                    text: marker_text,
                    consumed: used + 1 + (marker - off),
                    pieces: pieces + 1,
                    gap: width - indent - displayed,
                });
            } else {
                let mut marker_text = text.clone();
                append_chars(&mut marker_text, &word.chars[off..marker]);
                marker_text.push('-');
                let displayed = marker - off + 1;
                result.push(CandidateLine {
                    end_word: wi,
                    end_offset: marker,
                    split: SplitKind::Marker,
                    text: marker_text,
                    consumed: marker - off,
                    pieces: 1,
                    gap: width - indent - displayed,
                });
            }
        }

        if pieces == 0 && remaining > capacity && marker_fits.is_empty() {
            let mut raw_text = text.clone();
            append_chars(&mut raw_text, &word.chars[off..off + capacity]);
            result.push(CandidateLine {
                end_word: wi,
                end_offset: off + capacity,
                split: SplitKind::Raw,
                text: raw_text,
                consumed: capacity,
                pieces: 1,
                gap: 0,
            });
        }

        break;
    }

    result
}

fn lex_better(a: &[usize], b: &[usize]) -> std::cmp::Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        match x.cmp(y) {
            std::cmp::Ordering::Equal => {}
            other => return other.reverse(),
        }
    }
    a.len().cmp(&b.len()).reverse()
}

fn better(a: &Solution, b: &Solution) -> bool {
    if a.cost != b.cost {
        return a.cost < b.cost;
    }
    if a.marker_lines != b.marker_lines {
        return a.marker_lines < b.marker_lines;
    }
    if a.lines.len() != b.lines.len() {
        return a.lines.len() < b.lines.len();
    }
    match lex_better(&a.consumed, &b.consumed) {
        std::cmp::Ordering::Less => return true,
        std::cmp::Ordering::Greater => return false,
        std::cmp::Ordering::Equal => {}
    }
    a.lines < b.lines
}

fn solve_paragraph(words: &[Word], options: &WrapOptions) -> Vec<String> {
    if words.is_empty() {
        return Vec::new();
    }

    let mut memo: Vec<Vec<Option<Solution>>> = words
        .iter()
        .map(|w| vec![None; w.chars.len() + 1])
        .collect();

    fn rec(
        words: &[Word],
        wi: usize,
        off: usize,
        first: bool,
        options: &WrapOptions,
        memo: &mut [Vec<Option<Solution>>],
    ) -> Option<Solution> {
        if wi >= words.len() {
            return Some(Solution {
                cost: 0,
                marker_lines: 0,
                lines: Vec::new(),
                consumed: Vec::new(),
            });
        }

        if off == 0 && !first {
            if let Some(cached) = &memo[wi][0] {
                return Some(cached.clone());
            }
        }

        let indent = if first {
            options.first_indent
        } else {
            options.rest_indent
        };
        let candidates = enumerate_lines(words, wi, off, indent, options.width);
        let mut best: Option<Solution> = None;

        for line in candidates {
            let final_line = line.end_word >= words.len();
            let gap = line.gap as u64;
            let mut cost = if final_line {
                gap * gap / 4
            } else {
                gap * gap
            };
            if line.split == SplitKind::Marker {
                cost += options.hyphen_cost as u64;
            } else if line.split == SplitKind::Raw {
                cost += (2 * options.width + 1) as u64;
            }
            if !final_line && line.pieces == 1 {
                cost += 3;
            }

            let next = rec(
                words,
                line.end_word,
                line.end_offset,
                false,
                options,
                memo,
            )?;
            let mut candidate = Solution {
                cost: cost + next.cost,
                marker_lines: next.marker_lines
                    + usize::from(line.split == SplitKind::Marker),
                lines: Vec::with_capacity(next.lines.len() + 1),
                consumed: Vec::with_capacity(next.consumed.len() + 1),
            };
            candidate.lines.push(line.text);
            candidate.lines.extend(next.lines);
            candidate.consumed.push(line.consumed);
            candidate.consumed.extend(next.consumed);

            if best
                .as_ref()
                .map_or(true, |current| better(&candidate, current))
            {
                best = Some(candidate);
            }
        }

        if off == 0 && !first {
            if let Some(value) = &best {
                memo[wi][0] = Some(value.clone());
            }
        }
        best
    }

    let mut solution =
        rec(words, 0, 0, true, options, &mut memo).expect("raw split guarantees a legal continuation");

    if options.justify {
        for line_index in 0..solution.lines.len().saturating_sub(1) {
            let original = solution.lines[line_index].clone();
            let indent = if line_index == 0 {
                options.first_indent
            } else {
                options.rest_indent
            };
            let pieces: Vec<&str> = original[indent..].split(' ').collect();
            if pieces.len() < 2 {
                continue;
            }
            let base_width = original.chars().count();
            let extra = options.width.saturating_sub(base_width);
            let gaps = pieces.len() - 1;
            let each = extra / gaps;
            let remainder = extra % gaps;
            let start = line_index % gaps;

            let mut rendered = " ".repeat(indent);
            for (i, piece) in pieces.iter().enumerate() {
                if i > 0 {
                    let gap_index = i - 1;
                    let bonus = usize::from((gap_index + gaps - start) % gaps < remainder);
                    rendered.push_str(&" ".repeat(1 + each + bonus));
                }
                rendered.push_str(piece);
            }
            solution.lines[line_index] = rendered;
        }
    }

    solution.lines
}

pub fn wrap(input: &str, options: &WrapOptions) -> Result<Vec<String>, WrapError> {
    if options.width == 0 {
        return Err(WrapError::WidthZero);
    }
    if options.width > 256 {
        return Err(WrapError::WidthTooLarge {
            width: options.width,
        });
    }
    if options.first_indent >= options.width {
        return Err(WrapError::FirstIndentTooWide {
            indent: options.first_indent,
        });
    }
    if options.rest_indent >= options.width {
        return Err(WrapError::RestIndentTooWide {
            indent: options.rest_indent,
        });
    }
    if options.hyphen_cost > 1_000_000 {
        return Err(WrapError::HyphenCostTooLarge {
            cost: options.hyphen_cost,
        });
    }

    let chars: Vec<(usize, char)> = input.char_indices().collect();
    if chars.len() > 512 {
        return Err(WrapError::InputTooLong { chars: chars.len() });
    }

    for &(index, c) in &chars {
        if c.is_control() && !matches!(c, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index });
        }
    }

    for i in 0..chars.len() {
        if chars[i].1 != '~' {
            continue;
        }
        let valid_neighbor = |j: isize| -> bool {
            if j < 0 || j as usize >= chars.len() {
                return false;
            }
            let c = chars[j as usize].1;
            c != '~' && !is_separator(c)
        };
        if !valid_neighbor(i as isize - 1) || !valid_neighbor(i as isize + 1) {
            return Err(WrapError::InvalidMarker { index: chars[i].0 });
        }
    }

    let mut normalized = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i].1;
        if c == '\r' && i + 1 < chars.len() && chars[i + 1].1 == '\n' {
            normalized.push('\n');
            i += 2;
        } else {
            normalized.push(if c == '\r' { '\n' } else { c });
            i += 1;
        }
    }

    let mut output = Vec::new();
    let mut segment_start = 0usize;
    let mut pos = 0usize;
    while pos < normalized.len() {
        if normalized[pos] != '\n' {
            pos += 1;
            continue;
        }
        let run_start = pos;
        while pos < normalized.len() && normalized[pos] == '\n' {
            pos += 1;
        }
        let run_len = pos - run_start;
        if run_len >= 2 {
            let words = parse_segment(&normalized[segment_start..run_start]);
            output.extend(solve_paragraph(&words, options));
            output.extend(std::iter::repeat(String::new()).take(run_len - 1));
            segment_start = pos;
        }
    }

    let words = parse_segment(&normalized[segment_start..]);
    output.extend(solve_paragraph(&words, options));
    Ok(output)
}

fn main() {}