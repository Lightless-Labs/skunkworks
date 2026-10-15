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

pub fn render(rows: &[Vec<String>], options: &Options) -> Result<String, TableError> {
    if rows.is_empty() {
        return Ok(String::new());
    }

    for (row, cells) in rows.iter().enumerate() {
        if cells.is_empty() {
            return Err(TableError::EmptyRow { row });
        }
    }

    let column_count = rows[0].len();
    for (row, cells) in rows.iter().enumerate() {
        if cells.len() != column_count {
            return Err(TableError::Ragged {
                row,
                expected: column_count,
                actual: cells.len(),
            });
        }
    }

    if !options.alignments.is_empty() && options.alignments.len() != column_count {
        return Err(TableError::AlignmentCount {
            expected: column_count,
            actual: options.alignments.len(),
        });
    }
    if !options.overflow.is_empty() && options.overflow.len() != column_count {
        return Err(TableError::OverflowCount {
            expected: column_count,
            actual: options.overflow.len(),
        });
    }

    let is_markdown = matches!(options.mode, Mode::Markdown);
    if is_markdown && !options.header {
        return Err(TableError::MarkdownNeedsHeader);
    }

    let minimum_width = if is_markdown { 3 } else { 1 };
    let overhead = match options.mode {
        Mode::Compact => 2 * (column_count - 1),
        Mode::Grid | Mode::Markdown => 3 * column_count + 1,
    };
    let minimum = overhead + column_count * minimum_width;
    if options.max_width != 0 && options.max_width < minimum {
        return Err(TableError::TooNarrow {
            minimum,
            provided: options.max_width,
        });
    }

    for (row, cells) in rows.iter().enumerate() {
        for (column, cell) in cells.iter().enumerate() {
            let mut chars = cell.char_indices().peekable();
            while let Some((byte, ch)) = chars.next() {
                let invalid = if ch == '\r' {
                    if matches!(chars.peek(), Some((_, '\n'))) {
                        chars.next();
                        false
                    } else {
                        true
                    }
                } else {
                    (ch.is_control() && ch != '\n') || ch == '\u{2028}' || ch == '\u{2029}'
                };

                if invalid {
                    return Err(TableError::InvalidChar { row, column, byte });
                }
            }
        }
    }

    let mut widths = vec![minimum_width; column_count];
    let mut processed: Vec<Vec<Vec<Vec<Token>>>> = Vec::with_capacity(rows.len());

    for cells in rows {
        let mut processed_cells = Vec::with_capacity(column_count);
        for (column, cell) in cells.iter().enumerate() {
            let normalized = cell.replace("\r\n", "\n");
            let mut logical_lines = Vec::new();

            for line in normalized.split('\n') {
                let trimmed = line.trim_end_matches(' ');
                let mut tokens = Vec::new();

                for ch in trimmed.chars() {
                    if is_markdown && ch == '|' {
                        tokens.push(Token {
                            text: "\\|".to_owned(),
                            width: 2,
                        });
                    } else if is_markdown && ch == '\\' {
                        tokens.push(Token {
                            text: "\\\\".to_owned(),
                            width: 2,
                        });
                    } else {
                        tokens.push(Token {
                            text: ch.to_string(),
                            width: 1,
                        });
                    }
                }

                let line_width: usize = tokens.iter().map(|token| token.width).sum();
                widths[column] = widths[column].max(line_width);
                logical_lines.push(tokens);
            }

            processed_cells.push(logical_lines);
        }
        processed.push(processed_cells);
    }

    if options.max_width != 0 {
        let mut total: usize = widths.iter().sum::<usize>() + overhead;
        while total > options.max_width {
            let column = (0..column_count)
                .rev()
                .find(|&column| widths[column] > minimum_width)
                .expect("minimum budget was validated");
            widths[column] -= 1;
            total -= 1;
        }
    }

    let alignments: Vec<Alignment> = if options.alignments.is_empty() {
        vec![Alignment::Left; column_count]
    } else {
        options.alignments.clone()
    };
    let overflows: Vec<Overflow> = if options.overflow.is_empty() {
        vec![Overflow::Wrap; column_count]
    } else {
        options.overflow.clone()
    };

    let mut output = String::new();

    if matches!(options.mode, Mode::Grid) {
        append_border(&mut output, &widths);
    }

    for (row_index, cells) in processed.iter().enumerate() {
        let fragments_by_cell: Vec<Vec<Vec<Token>>> = (0..column_count)
            .map(|column| {
                let mut fragments = Vec::new();
                for logical_line in &cells[column] {
                    fragments.extend(split_line(
                        logical_line,
                        widths[column],
                        &overflows[column],
                    ));
                }
                fragments
            })
            .collect();

        let row_height = fragments_by_cell
            .iter()
            .map(Vec::len)
            .max()
            .unwrap_or(1);

        for fragment_index in 0..row_height {
            if matches!(options.mode, Mode::Compact) {
                for column in 0..column_count {
                    if column > 0 {
                        output.push_str("  ");
                    }

                    let fragment = fragments_by_cell[column].get(fragment_index);
                    let text = fragment.map(|tokens| tokens_text(tokens)).unwrap_or_default();
                    let content_width = fragment
                        .map(|tokens| tokens.iter().map(|token| token.width).sum())
                        .unwrap_or(0);

                    output.push_str(&align_fragment(
                        &text,
                        content_width,
                        widths[column],
                        &alignments[column],
                        column + 1 == column_count,
                    ));
                }
                output.push('\n');
            } else {
                output.push('|');
                for column in 0..column_count {
                    output.push(' ');
                    let fragment = fragments_by_cell[column].get(fragment_index);
                    let text = fragment.map(|tokens| tokens_text(tokens)).unwrap_or_default();
                    let content_width = fragment
                        .map(|tokens| tokens.iter().map(|token| token.width).sum())
                        .unwrap_or(0);

                    output.push_str(&align_fragment(
                        &text,
                        content_width,
                        widths[column],
                        &alignments[column],
                        false,
                    ));
                    output.push(' ');
                    output.push('|');
                }
                output.push('\n');
            }
        }

        if is_markdown && row_index == 0 {
            append_markers(&mut output, &widths, &alignments);
        } else if matches!(options.mode, Mode::Grid)
            && options.header
            && row_index == 0
            && rows.len() > 1
        {
            append_border(&mut output, &widths);
        }
    }

    if matches!(options.mode, Mode::Grid) {
        append_border(&mut output, &widths);
    }

    Ok(output)
}

fn split_line(line: &[Token], width: usize, overflow: &Overflow) -> Vec<Vec<Token>> {
    if line.is_empty() {
        return vec![Vec::new()];
    }

    let total_width: usize = line.iter().map(|token| token.width).sum();
    if total_width <= width {
        return vec![line.to_vec()];
    }

    match overflow {
        Overflow::Clip => {
            let mut used = 0;
            let mut kept = Vec::new();
            for token in line {
                if used + token.width > width.saturating_sub(1) {
                    break;
                }
                used += token.width;
                kept.push(token.clone());
            }
            kept.push(Token {
                text: "\u{2026}".to_owned(),
                width: 1,
            });
            vec![kept]
        }
        Overflow::Wrap => {
            let mut fragments = Vec::new();
            let mut start = 0;
            while start < line.len() {
                let mut used = 0;
                let mut end = start;
                while end < line.len() && used + line[end].width <= width {
                    used += line[end].width;
                    end += 1;
                }
                fragments.push(line[start..end].to_vec());
                start = end;
            }
            fragments
        }
    }
}

fn tokens_text(tokens: &[Token]) -> String {
    tokens.iter().map(|token| token.text.as_str()).collect()
}

fn align_fragment(
    text: &str,
    content_width: usize,
    width: usize,
    alignment: &Alignment,
    omit_right_padding: bool,
) -> String {
    let extra = width.saturating_sub(content_width);
    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };

    let mut result = String::new();
    result.push_str(&" ".repeat(left));
    result.push_str(text);
    if !omit_right_padding {
        result.push_str(&" ".repeat(right));
    }
    result
}

fn append_border(output: &mut String, widths: &[usize]) {
    output.push('+');
    for width in widths {
        output.push_str(&"-".repeat(width + 2));
        output.push('+');
    }
    output.push('\n');
}

fn append_markers(output: &mut String, widths: &[usize], alignments: &[Alignment]) {
    output.push('|');
    for (width, alignment) in widths.iter().zip(alignments) {
        output.push(' ');
        match alignment {
            Alignment::Left => {
                output.push(':');
                output.push_str(&"-".repeat(width - 1));
            }
            Alignment::Right => {
                output.push_str(&"-".repeat(width - 1));
                output.push(':');
            }
            Alignment::Center => {
                output.push(':');
                output.push_str(&"-".repeat(width - 2));
                output.push(':');
            }
        }
        output.push(' ');
        output.push('|');
    }
    output.push('\n');
}