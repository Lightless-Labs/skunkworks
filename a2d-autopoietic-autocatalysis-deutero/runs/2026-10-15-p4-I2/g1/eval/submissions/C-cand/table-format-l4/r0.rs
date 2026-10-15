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

fn invalid_scalar(ch: char) -> bool {
    ch == '\r'
        && false
        || (ch.is_control() && ch != '\n' && ch != '\r')
        || ch == '\u{2028}'
        || ch == '\u{2029}'
}

fn validate_characters(rows: &[Vec<String>]) -> Result<(), TableError> {
    for (row_index, row) in rows.iter().enumerate() {
        for (column_index, cell) in row.iter().enumerate() {
            let mut iter = cell.char_indices().peekable();
            while let Some((byte, ch)) = iter.next() {
                if ch == '\r' {
                    if matches!(iter.peek(), Some((_, '\n'))) {
                        iter.next();
                    } else {
                        return Err(TableError::InvalidChar {
                            row: row_index,
                            column: column_index,
                            byte,
                        });
                    }
                } else if invalid_scalar(ch) {
                    return Err(TableError::InvalidChar {
                        row: row_index,
                        column: column_index,
                        byte,
                    });
                }
            }
        }
    }
    Ok(())
}

fn logical_lines(cell: &str, markdown: bool) -> Vec<Vec<String>> {
    let normalized = cell.replace("\r\n", "\n");
    normalized
        .split('\n')
        .map(|line| {
            let trimmed = line.trim_end_matches(' ');
            let mut tokens = Vec::new();
            for ch in trimmed.chars() {
                if markdown && ch == '|' {
                    tokens.push("\\|".to_string());
                } else if markdown && ch == '\\' {
                    tokens.push("\\\\".to_string());
                } else {
                    tokens.push(ch.to_string());
                }
            }
            tokens
        })
        .collect()
}

fn token_width(tokens: &[String]) -> usize {
    tokens.iter().map(|token| token.chars().count()).sum()
}

fn fragments(tokens: &[String], width: usize, overflow: &Overflow) -> Vec<String> {
    if tokens.is_empty() {
        return vec![String::new()];
    }
    match overflow {
        Overflow::Clip => {
            let total = token_width(tokens);
            if total <= width {
                return vec![tokens.concat()];
            }
            let mut result = String::new();
            let mut used = 0usize;
            let limit = width.saturating_sub(1);
            for token in tokens {
                let w = token.chars().count();
                if used + w > limit {
                    break;
                }
                used += w;
                result.push_str(token);
            }
            result.push('…');
            vec![result]
        }
        Overflow::Wrap => {
            let mut result = Vec::new();
            let mut current = String::new();
            let mut used = 0usize;
            for token in tokens {
                let w = token.chars().count();
                if used + w > width {
                    result.push(std::mem::take(&mut current));
                    used = 0;
                }
                current.push_str(token);
                used += w;
            }
            if !current.is_empty() {
                result.push(current);
            }
            if result.is_empty() {
                result.push(String::new());
            }
            result
        }
    }
}

fn pad(content: &str, width: usize, alignment: &Alignment, omit_right: bool) -> String {
    let content_width = content.chars().count();
    let extra = width.saturating_sub(content_width);
    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };
    let right = if omit_right { 0 } else { right };
    format!("{}{}{}", " ".repeat(left), content, " ".repeat(right))
}

fn data_line(
    contents: &[String],
    widths: &[usize],
    alignments: &[Alignment],
    mode: &Mode,
) -> String {
    match mode {
        Mode::Compact => {
            let mut parts = Vec::with_capacity(contents.len());
            for i in 0..contents.len() {
                parts.push(pad(
                    &contents[i],
                    widths[i],
                    &alignments[i],
                    i + 1 == contents.len(),
                ));
            }
            parts.join("  ")
        }
        Mode::Grid | Mode::Markdown => {
            let mut line = String::from("|");
            for i in 0..contents.len() {
                line.push(' ');
                line.push_str(&pad(&contents[i], widths[i], &alignments[i], false));
                line.push(' ');
                line.push('|');
            }
            line
        }
    }
}

fn border(widths: &[usize]) -> String {
    let mut line = String::from("+");
    for width in widths {
        line.push_str(&"-".repeat(width + 2));
        line.push('+');
    }
    line
}

fn marker_line(widths: &[usize], alignments: &[Alignment]) -> String {
    let mut line = String::from("|");
    for (width, alignment) in widths.iter().zip(alignments) {
        let marker = match alignment {
            Alignment::Left => format!(":{}", "-".repeat(width - 1)),
            Alignment::Right => format!("{}:", "-".repeat(width - 1)),
            Alignment::Center => format!(":{}:", "-".repeat(width - 2)),
        };
        line.push(' ');
        line.push_str(&marker);
        line.push(' ');
        line.push('|');
    }
    line
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

    if options.mode == Mode::Markdown && !options.header {
        return Err(TableError::MarkdownNeedsHeader);
    }

    let minimum_column = if options.mode == Mode::Markdown { 3 } else { 1 };
    let overhead = match options.mode {
        Mode::Compact => 2 * n.saturating_sub(1),
        Mode::Grid | Mode::Markdown => 3 * n + 1,
    };
    let minimum = overhead + n * minimum_column;
    if options.max_width != 0 && options.max_width < minimum {
        return Err(TableError::TooNarrow {
            minimum,
            provided: options.max_width,
        });
    }

    validate_characters(rows)?;

    let alignments: Vec<Alignment> = if options.alignments.is_empty() {
        vec![Alignment::Left; n]
    } else {
        options.alignments.clone()
    };
    let overflows: Vec<Overflow> = if options.overflow.is_empty() {
        vec![Overflow::Wrap; n]
    } else {
        options.overflow.clone()
    };

    let markdown = options.mode == Mode::Markdown;
    let logical: Vec<Vec<Vec<Vec<String>>>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| logical_lines(cell, markdown))
                .collect()
        })
        .collect();

    let mut widths = vec![minimum_column; n];
    for row in &logical {
        for (column, lines) in row.iter().enumerate() {
            for line in lines {
                widths[column] = widths[column].max(token_width(line));
            }
        }
    }

    if options.max_width != 0 {
        let mut total = overhead + widths.iter().sum::<usize>();
        while total > options.max_width {
            let mut widest: Option<usize> = None;
            for column in 0..n {
                if widths[column] > minimum_column
                    && widest.map_or(true, |current| widths[column] >= widths[current])
                {
                    widest = Some(column);
                }
            }
            if let Some(column) = widest {
                widths[column] -= 1;
                total -= 1;
            } else {
                break;
            }
        }
    }

    let mut output = String::new();
    if options.mode == Mode::Grid {
        output.push_str(&border(&widths));
        output.push('\n');
    }

    for (row_index, row) in logical.iter().enumerate() {
        let mut cell_fragments: Vec<Vec<String>> = Vec::with_capacity(n);
        let mut height = 1usize;
        for column in 0..n {
            let mut combined = Vec::new();
            for line in &row[column] {
                combined.extend(fragments(line, widths[column], &overflows[column]));
            }
            height = height.max(combined.len());
            cell_fragments.push(combined);
        }

        for fragment_index in 0..height {
            let contents: Vec<String> = (0..n)
                .map(|column| {
                    cell_fragments[column]
                        .get(fragment_index)
                        .cloned()
                        .unwrap_or_default()
                })
                .collect();
            output.push_str(&data_line(
                &contents,
                &widths,
                &alignments,
                &options.mode,
            ));
            output.push('\n');
        }

        if options.mode == Mode::Markdown && row_index == 0 {
            output.push_str(&marker_line(&widths, &alignments));
            output.push('\n');
        } else if options.mode == Mode::Grid
            && options.header
            && row_index == 0
            && rows.len() > 1
        {
            output.push_str(&border(&widths));
            output.push('\n');
        }
    }

    if options.mode == Mode::Grid {
        output.push_str(&border(&widths));
        output.push('\n');
    }

    Ok(output)
}

fn main() {}