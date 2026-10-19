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
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
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

    for &(index, c) in &chars {
        if c.is_control() && !matches!(c, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index });
        }
    }

    for i in 0..chars.len() {
        if chars[i].1 == '~' {
            let valid_neighbor = |j: Option<usize>| {
                j.map(|j| chars[j].1 != '~' && !is_separator(chars[j].1))
                    .unwrap_or(false)
            };
            if !valid_neighbor(i.checked_sub(1)) || !valid_neighbor(Some(i + 1).filter(|&j| j < chars.len())) {
                return Err(WrapError::InvalidMarker { index: chars[i].0 });
            }
        }
    }

    if input.is_empty() {
        return Ok(Vec::new());
    }

    let mut normalized = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i].1;
        if c == '\r' {
            if i + 1 < chars.len() && chars[i + 1].1 == '\n' {
                normalized.push('\n');
                i += 2;
            } else {
                normalized.push('\n');
                i += 1;
            }
        } else {
            normalized.push(c);
            i += 1;
        }
    }

    let mut output = Vec::new();
    let mut segment_start = 0;
    let mut pos = 0;

    while pos < normalized.len() {
        if normalized[pos] == '\n' {
            let run_start = pos;
            while pos < normalized.len() && normalized[pos] == '\n' {
                pos += 1;
            }
            if pos - run_start >= 2 {
                layout_segment(&normalized[segment_start..run_start], options, &mut output);
                for _ in 1..(pos - run_start) {
                    output.push(String::new());
                }
                segment_start = pos;
            }
        } else {
            pos += 1;
        }
    }
    layout_segment(&normalized[segment_start..], options, &mut output);

    Ok(output)
}

#[derive(Clone)]
struct Line {
    indent: usize,
    pieces: Vec<Vec<char>>,
}

impl Line {
    fn content_len(&self) -> usize {
        self.pieces.iter().map(Vec::len).sum::<usize>() + self.pieces.len().saturating_sub(1)
    }

    fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }

    fn render(&self, width: usize, justify: bool, final_line: bool, line_index: usize) -> String {
        let gaps = self.pieces.len().saturating_sub(1);
        let extra = if justify && !final_line && gaps > 0 {
            width.saturating_sub(self.indent + self.content_len())
        } else {
            0
        };
        let each = if gaps > 0 { extra / gaps } else { 0 };
        let rem = if gaps > 0 { extra % gaps } else { 0 };
        let start = if gaps > 0 { line_index % gaps } else { 0 };

        let mut s = " ".repeat(self.indent);
        for (i, piece) in self.pieces.iter().enumerate() {
            if i > 0 {
                let gap = i - 1;
                let bonus = if rem > 0 && (gap + gaps - start) % gaps < rem {
                    1
                } else {
                    0
                };
                s.push_str(&" ".repeat(1 + each + bonus));
            }
            s.extend(piece.iter());
        }
        s
    }
}

fn layout_segment(segment: &[char], options: &WrapOptions, output: &mut Vec<String>) {
    let mut words: Vec<Vec<char>> = Vec::new();
    let mut current = Vec::new();

    for &c in segment {
        if is_separator(c) {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        } else if c == '~' {
            current.push(c);
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    if words.is_empty() {
        return;
    }

    let mut lines = Vec::<Line>::new();
    let mut line_number = 0usize;
    let mut line = Line {
        indent: options.first_indent,
        pieces: Vec::new(),
    };

    for word in words {
        let markers: Vec<usize> = word
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| (c == '~').then_some(i))
            .collect();
        let displayed: Vec<char> = word.iter().copied().filter(|&c| c != '~').collect();
        let mut offset = 0usize;

        while offset < displayed.len() {
            let cap = options.width - line.indent;
            let room = cap.saturating_sub(line.content_len());
            let separating = usize::from(!line.is_empty());

            if displayed.len() - offset <= room.saturating_sub(separating) {
                let mut piece = displayed[offset..].to_vec();
                line.pieces.push(std::mem::take(&mut piece));
                offset = displayed.len();
                continue;
            }

            let usable = markers
                .iter()
                .copied()
                .filter(|&m| m > offset && m < word.len())
                .filter_map(|m| {
                    let prefix_len = word[..m]
                        .iter()
                        .filter(|&&c| c != '~')
                        .count()
                        .saturating_sub(displayed[..offset].len());
                    let needed = prefix_len + 1 + separating;
                    (needed <= room).then_some((m, prefix_len))
                })
                .last();

            if let Some((marker, prefix_len)) = usable {
                let unused = room - separating - prefix_len - 1;
                let accept = line.is_empty()
                    || unused * unused + options.hyphen_cost
                        < (cap - line.content_len()) * (cap - line.content_len());
                if accept {
                    let piece_end = offset + prefix_len;
                    let mut piece = displayed[offset..piece_end].to_vec();
                    piece.push('-');
                    line.pieces.push(piece);
                    lines.push(line);
                    line_number += 1;
                    line = Line {
                        indent: options.rest_indent,
                        pieces: Vec::new(),
                    };
                    offset = word[..marker]
                        .iter()
                        .filter(|&&c| c != '~')
                        .count();
                    continue;
                }
            }

            if !line.is_empty() {
                lines.push(line);
                line_number += 1;
                line = Line {
                    indent: options.rest_indent,
                    pieces: Vec::new(),
                };
                continue;
            }

            let cap = options.width - line.indent;
            let remaining = displayed.len() - offset;
            if remaining > cap {
                let end = offset + cap;
                line.pieces.push(displayed[offset..end].to_vec());
                lines.push(line);
                line_number += 1;
                line = Line {
                    indent: options.rest_indent,
                    pieces: Vec::new(),
                };
                offset = end;
            } else {
                line.pieces.push(displayed[offset..].to_vec());
                offset = displayed.len();
            }
        }
    }

    if !line.is_empty() {
        lines.push(line);
    }

    let total = lines.len();
    for (idx, line) in lines.iter().enumerate() {
        output.push(line.render(
            options.width,
            options.justify,
            idx + 1 == total,
            idx,
        ));
    }

    let _ = line_number;
}