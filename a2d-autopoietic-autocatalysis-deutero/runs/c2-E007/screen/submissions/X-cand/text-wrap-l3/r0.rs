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

    for &(index, ch) in &chars {
        if ch.is_control() && !matches!(ch, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index });
        }
    }

    for i in 0..chars.len() {
        if chars[i].1 != '~' {
            continue;
        }
        let valid_side = |j: usize| {
            j < chars.len() && chars[j].1 != '~' && !is_separator(chars[j].1)
        };
        if i == 0 || i + 1 >= chars.len() || !valid_side(i - 1) || !valid_side(i + 1) {
            return Err(WrapError::InvalidMarker { index: chars[i].0 });
        }
    }

    let mut normalized = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i].1;
        if ch == '\r' {
            normalized.push('\n');
            if i + 1 < chars.len() && chars[i + 1].1 == '\n' {
                i += 1;
            }
        } else {
            normalized.push(ch);
        }
        i += 1;
    }

    let mut output = Vec::new();
    let mut segment_start = 0;
    let mut pos = 0;
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
            layout_segment(&normalized[segment_start..run_start], options, &mut output);
            output.extend(std::iter::repeat_n(String::new(), run_len - 1));
            segment_start = pos;
        }
    }
    layout_segment(&normalized[segment_start..], options, &mut output);

    Ok(output)
}

fn is_separator(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{00A0}')
}

#[derive(Debug)]
struct Line {
    indent: usize,
    pieces: Vec<String>,
}

fn layout_segment(segment: &[char], options: &WrapOptions, output: &mut Vec<String>) {
    let mut words: Vec<Vec<char>> = Vec::new();
    let mut current = Vec::new();

    for &ch in segment {
        if is_separator(ch) {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    if words.is_empty() {
        return;
    }

    let mut lines = Vec::<Line>::new();
    let mut line = Line {
        indent: options.first_indent,
        pieces: Vec::new(),
    };
    let mut first_line = true;

    for word in words {
        let mut offset = 0;
        while offset < word.len() {
            let indent = if first_line {
                options.first_indent
            } else {
                options.rest_indent
            };
            let capacity = options.width - indent;
            let content_len = line_content_len(&line);
            let room = capacity - content_len;
            let nonempty = !line.pieces.is_empty();
            let gap = usize::from(nonempty);

            let suffix_len = visible_len(&word[offset..]);
            if suffix_len + gap <= room {
                if nonempty {
                    line.pieces.push(" ".to_string());
                }
                line.pieces.push(render(&word[offset..]));
                offset = word.len();
                continue;
            }

            // At most `room - gap` visible prefix scalars can fit before the
            // hyphen. Scan only that bounded prefix and remember the rightmost
            // usable opportunity.
            let prefix_limit = room.saturating_sub(gap + 1);
            let mut visible_prefix = 0usize;
            let mut opportunity = None;
            for marker in offset..word.len() {
                if word[marker] == '~' {
                    if visible_prefix > 0 && visible_prefix <= prefix_limit {
                        opportunity = Some((marker, visible_prefix));
                    }
                    continue;
                }
                visible_prefix += 1;
                if visible_prefix > prefix_limit {
                    break;
                }
            }

            if let Some((marker, prefix_len)) = opportunity {
                let used = prefix_len + 1;
                let unused = room - gap - used;
                let accept = if nonempty {
                    let baseline = capacity - content_len;
                    unused * unused + options.hyphen_cost < baseline * baseline
                } else {
                    true
                };

                if accept {
                    if nonempty {
                        line.pieces.push(" ".to_string());
                    }
                    let mut fragment = render(&word[offset..marker]);
                    fragment.push('-');
                    line.pieces.push(fragment);
                    lines.push(line);
                    first_line = false;
                    line = Line {
                        indent: options.rest_indent,
                        pieces: Vec::new(),
                    };
                    offset = marker + 1;
                    continue;
                }
            }

            if nonempty {
                lines.push(line);
                first_line = false;
                line = Line {
                    indent: options.rest_indent,
                    pieces: Vec::new(),
                };
                continue;
            }

            if suffix_len > capacity {
                let mut end = offset;
                let mut count = 0;
                while end < word.len() && count < capacity {
                    if word[end] != '~' {
                        count += 1;
                    }
                    end += 1;
                }
                line.pieces.push(render(&word[offset..end]));
                lines.push(line);
                first_line = false;
                line = Line {
                    indent: options.rest_indent,
                    pieces: Vec::new(),
                };
                offset = end;
            } else {
                line.pieces.push(render(&word[offset..]));
                offset = word.len();
            }
        }
    }

    if !line.pieces.is_empty() {
        lines.push(line);
    }

    let final_index = lines.len().saturating_sub(1);
    for (line_index, line) in lines.into_iter().enumerate() {
        let pre_width = line.indent + line_content_len(&line);
        let gaps = line.pieces.iter().filter(|piece| piece.as_str() == " ").count();
        let justify = options.justify && line_index != final_index && gaps > 0;
        let mut rendered = " ".repeat(line.indent);

        if justify {
            let extra = options.width - pre_width;
            let base = extra / gaps;
            let remainder = extra % gaps;
            let start = line_index % gaps;
            let mut gap_index = 0;
            for piece in &line.pieces {
                if piece == " " {
                    rendered.push(' ');
                    rendered.push_str(&" ".repeat(base));
                    let distance = (gap_index + gaps - start) % gaps;
                    if distance < remainder {
                        rendered.push(' ');
                    }
                    gap_index += 1;
                } else {
                    rendered.push_str(piece);
                }
            }
        } else {
            for piece in &line.pieces {
                rendered.push_str(piece);
            }
        }
        output.push(rendered);
    }
}

fn visible_len(chars: &[char]) -> usize {
    chars.iter().filter(|&&ch| ch != '~').count()
}

fn line_content_len(line: &Line) -> usize {
    line.pieces.iter().map(|piece| piece.chars().count()).sum()
}

fn render(chars: &[char]) -> String {
    chars.iter().filter(|&&ch| ch != '~').collect()
}