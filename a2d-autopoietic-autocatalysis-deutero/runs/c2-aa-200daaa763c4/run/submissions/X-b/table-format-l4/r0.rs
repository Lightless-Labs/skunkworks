#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableError {
    EmptyRow { row: usize },
    Ragged { row: usize, expected: usize, actual: usize },
    InvalidChar { row: usize, column: usize, byte: usize },
    AlignmentCount { expected: usize, actual: usize },
    TooNarrow { minimum: usize, provided: usize },
    OverflowCount { expected: usize, actual: usize },
    MarkdownNeedsHeader,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Right,
    Center,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overflow {
    Clip,
    Wrap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Grid,
    Compact,
    Markdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub alignments: Vec<Alignment>,
    pub header: bool,
    pub max_width: usize,
    pub overflow: Vec<Overflow>,
    pub mode: Mode,
}

#[derive(Clone)]
struct Token {
    text: String,
    width: usize,
}

fn validate_cell(s: &str, row: usize, column: usize) -> Result<(), TableError> {
    let mut iter = s.char_indices().peekable();

    while let Some((byte, ch)) = iter.next() {
        if ch == '\r' {
            if matches!(iter.peek(), Some((_, '\n'))) {
                iter.next();
                continue;
            }
            return Err(TableError::InvalidChar { row, column, byte });
        }

        if ch != '\n' && (ch.is_control() || ch == '\u{2028}' || ch == '\u{2029}') {
            return Err(TableError::InvalidChar { row, column, byte });
        }
    }

    Ok(())
}

fn logical_lines(s: &str) -> Vec<String> {
    s.replace("\r\n", "\n")
        .split('\n')
        .map(|line| line.trim_end_matches(' ').to_owned())
        .collect()
}

fn tokens(line: &str, markdown: bool) -> Vec<Token> {
    let mut out = Vec::new();

    for ch in line.chars() {
        if markdown && ch == '|' {
            out.push(Token {
                text: "\\|".to_owned(),
                width: 2,
            });
        } else if markdown && ch == '\\' {
            out.push(Token {
                text: "\\\\".to_owned(),
                width: 2,
            });
        } else {
            out.push(Token {
                text: ch.to_string(),
                width: 1,
            });
        }
    }

    out
}

fn token_width(ts: &[Token]) -> usize {
    ts.iter().map(|token| token.width).sum()
}

fn fragments(ts: &[Token], width: usize, rule: &Overflow) -> Vec<String> {
    if ts.is_empty() {
        return vec![String::new()];
    }

    match rule {
        Overflow::Clip => {
            if token_width(ts) <= width {
                return vec![ts.iter().map(|token| token.text.as_str()).collect()];
            }

            let mut result = String::new();
            let mut used = 0;
            let limit = width.saturating_sub(1);

            for token in ts {
                if used + token.width > limit {
                    break;
                }
                result.push_str(&token.text);
                used += token.width;
            }

            result.push('…');
            vec![result]
        }
        Overflow::Wrap => {
            let mut result = Vec::new();
            let mut start = 0;

            while start < ts.len() {
                let mut end = start;
                let mut used = 0;

                while end < ts.len() && used + ts[end].width <= width {
                    used += ts[end].width;
                    end += 1;
                }

                // The mandated minimum widths ensure every token fits.
                if end == start {
                    end += 1;
                }

                result.push(
                    ts[start..end]
                        .iter()
                        .map(|token| token.text.as_str())
                        .collect(),
                );
                start = end;
            }

            result
        }
    }
}

fn aligned(content: &str, width: usize, alignment: &Alignment) -> String {
    let extra = width.saturating_sub(content.chars().count());
    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };

    format!("{}{}{}", " ".repeat(left), content, " ".repeat(right))
}

fn border(widths: &[usize]) -> String {
    let mut out = String::from("+");

    for &width in widths {
        out.push_str(&"-".repeat(width + 2));
        out.push('+');
    }

    out.push('\n');
    out
}

fn data_line(
    contents: &[String],
    widths: &[usize],
    alignments: &[Alignment],
    mode: &Mode,
) -> String {
    if matches!(mode, Mode::Compact) {
        let mut out = String::new();

        for i in 0..contents.len() {
            if i > 0 {
                out.push_str("  ");
            }

            let mut cell = aligned(&contents[i], widths[i], &alignments[i]);

            if i + 1 == contents.len() {
                let padding = widths[i].saturating_sub(contents[i].chars().count());
                let right_padding = match alignments[i] {
                    Alignment::Left => padding,
                    Alignment::Right => 0,
                    Alignment::Center => padding - padding / 2,
                };

                for _ in 0..right_padding {
                    cell.pop();
                }
            }

            out.push_str(&cell);
        }

        out.push('\n');
        return out;
    }

    let mut out = String::from("|");

    for i in 0..contents.len() {
        out.push(' ');
        out.push_str(&aligned(&contents[i], widths[i], &alignments[i]));
        out.push_str(" |");
    }

    out.push('\n');
    out
}

fn markdown_marker(widths: &[usize], alignments: &[Alignment]) -> String {
    let mut out = String::from("|");

    for (i, &width) in widths.iter().enumerate() {
        let marker = match alignments[i] {
            Alignment::Left => format!(":{}", "-".repeat(width - 1)),
            Alignment::Right => format!("{}:", "-".repeat(width - 1)),
            Alignment::Center => format!(":{}:", "-".repeat(width - 2)),
        };

        out.push(' ');
        out.push_str(&marker);
        out.push_str(" |");
    }

    out.push('\n');
    out
}

pub fn render(rows: &[Vec<String>], options: &Options) -> Result<String, TableError> {
    if rows.is_empty() {
        return Ok(String::new());
    }

    for (row, cells) in rows.iter().enumerate() {
        if cells.is_empty() {
            return Err(TableError::EmptyRow { row });
        }
    }

    let n = rows[0].len();

    for (row, cells) in rows.iter().enumerate() {
        if cells.len() != n {
            return Err(TableError::Ragged {
                row,
                expected: n,
                actual: cells.len(),
            });
        }
    }

    if !options.alignments.is_empty() && options.alignments.len() != n {
        return Err(TableError::AlignmentCount {
            expected: n,
            actual: options.alignments.len(),
        });
    }

    if !options.overflow.is_empty() && options.overflow.len() != n {
        return Err(TableError::OverflowCount {
            expected: n,
            actual: options.overflow.len(),
        });
    }

    if matches!(options.mode, Mode::Markdown) && !options.header {
        return Err(TableError::MarkdownNeedsHeader);
    }

    let overhead = match options.mode {
        Mode::Compact => 2 * n.saturating_sub(1),
        Mode::Grid | Mode::Markdown => 3 * n + 1,
    };
    let min_width = if matches!(options.mode, Mode::Markdown) {
        3
    } else {
        1
    };
    let minimum = overhead + n * min_width;

    if options.max_width != 0 && options.max_width < minimum {
        return Err(TableError::TooNarrow {
            minimum,
            provided: options.max_width,
        });
    }

    for (row, cells) in rows.iter().enumerate() {
        for (column, cell) in cells.iter().enumerate() {
            validate_cell(cell, row, column)?;
        }
    }

    let alignments = if options.alignments.is_empty() {
        vec![Alignment::Left; n]
    } else {
        options.alignments.clone()
    };

    let overflows = if options.overflow.is_empty() {
        vec![Overflow::Wrap; n]
    } else {
        options.overflow.clone()
    };

    let markdown = matches!(options.mode, Mode::Markdown);

    // Store each cell as its logical lines, with each line represented by tokens.
    let all_lines: Vec<Vec<Vec<Vec<Token>>>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| {
                    logical_lines(cell)
                        .iter()
                        .map(|line| tokens(line, markdown))
                        .collect()
                })
                .collect()
        })
        .collect();

    let mut widths = vec![min_width; n];

    for row in &all_lines {
        for (column, cell_lines) in row.iter().enumerate() {
            for line in cell_lines {
                widths[column] = widths[column].max(token_width(line));
            }
        }
    }

    if options.max_width != 0 {
        while widths.iter().sum::<usize>() + overhead > options.max_width {
            let candidate = (0..n)
                .filter(|&i| widths[i] > min_width)
                .max_by_key(|&i| (widths[i], i));

            if let Some(i) = candidate {
                widths[i] -= 1;
            } else {
                break;
            }
        }
    }

    // Each cell becomes a flat sequence of fragments in logical-line order.
    let prepared: Vec<Vec<Vec<String>>> = all_lines
        .iter()
        .map(|row| {
            row.iter()
                .enumerate()
                .map(|(column, cell_lines)| {
                    let mut cell_fragments = Vec::new();
                    for line in cell_lines {
                        cell_fragments.extend(fragments(
                            line,
                            widths[column],
                            &overflows[column],
                        ));
                    }
                    cell_fragments
                })
                .collect()
        })
        .collect();

    let mut out = String::new();

    if matches!(options.mode, Mode::Grid) {
        out.push_str(&border(&widths));
    }

    for (row_index, row) in prepared.iter().enumerate() {
        let height = row.iter().map(Vec::len).max().unwrap_or(1);

        for physical in 0..height {
            let contents: Vec<String> = row
                .iter()
                .map(|cell| cell.get(physical).cloned().unwrap_or_default())
                .collect();

            out.push_str(&data_line(
                &contents,
                &widths,
                &alignments,
                &options.mode,
            ));
        }

        if matches!(options.mode, Mode::Markdown) && row_index == 0 {
            out.push_str(&markdown_marker(&widths, &alignments));
        }

        if matches!(options.mode, Mode::Grid)
            && options.header
            && row_index == 0
            && rows.len() > 1
        {
            out.push_str(&border(&widths));
        }
    }

    if matches!(options.mode, Mode::Grid) {
        out.push_str(&border(&widths));
    }

    Ok(out)
}