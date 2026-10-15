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

fn render_tokens(tokens: &[Token]) -> String {
    let mut output = String::new();
    for token in tokens {
        output.push_str(&token.text);
    }
    output
}

fn invalid_char(c: char) -> bool {
    (c.is_control() && c != '\n' && c != '\r') || c == '\u{2028}' || c == '\u{2029}'
}

fn validate_cell(cell: &str, row: usize, column: usize) -> Result<(), TableError> {
    let mut iter = cell.char_indices().peekable();

    while let Some((byte, c)) = iter.next() {
        if c == '\r' {
            if matches!(iter.peek(), Some((_, '\n'))) {
                iter.next();
            } else {
                return Err(TableError::InvalidChar { row, column, byte });
            }
        } else if invalid_char(c) {
            return Err(TableError::InvalidChar { row, column, byte });
        }
    }

    Ok(())
}

fn tokenize_line(line: &str, markdown: bool) -> Vec<Token> {
    let mut tokens = Vec::new();

    for c in line.chars() {
        if markdown && c == '|' {
            tokens.push(Token {
                text: "\\|".to_owned(),
                width: 2,
            });
        } else if markdown && c == '\\' {
            tokens.push(Token {
                text: "\\\\".to_owned(),
                width: 2,
            });
        } else {
            tokens.push(Token {
                text: c.to_string(),
                width: 1,
            });
        }
    }

    tokens
}

fn split_cell(cell: &str, markdown: bool) -> Vec<Vec<Token>> {
    cell.replace("\r\n", "\n")
        .split('\n')
        .map(|line| tokenize_line(line.trim_end_matches(' '), markdown))
        .collect()
}

fn total_width(widths: &[usize], overhead: usize) -> usize {
    widths.iter().sum::<usize>().saturating_add(overhead)
}

fn fragments(tokens: &[Token], width: usize, overflow: &Overflow) -> Vec<String> {
    if tokens.is_empty() {
        return vec![String::new()];
    }

    let total = tokens.iter().map(|token| token.width).sum::<usize>();

    match overflow {
        Overflow::Clip if total <= width => vec![render_tokens(tokens)],
        Overflow::Clip => {
            let limit = width.saturating_sub(1);
            let mut used = 0;
            let mut end = 0;

            for token in tokens {
                if used + token.width > limit {
                    break;
                }
                used += token.width;
                end += 1;
            }

            let mut output = render_tokens(&tokens[..end]);
            output.push('\u{2026}');
            vec![output]
        }
        Overflow::Wrap => {
            let mut output = Vec::new();
            let mut start = 0;

            while start < tokens.len() {
                let mut end = start;
                let mut used = 0;

                while end < tokens.len() && used + tokens[end].width <= width {
                    used += tokens[end].width;
                    end += 1;
                }

                output.push(render_tokens(&tokens[start..end]));
                start = end;
            }

            output
        }
    }
}

fn align_fragment(
    content: &str,
    content_width: usize,
    width: usize,
    alignment: &Alignment,
) -> String {
    let extra = width.saturating_sub(content_width);
    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };

    let mut output = String::new();
    output.extend(std::iter::repeat_n(' ', left));
    output.push_str(content);
    output.extend(std::iter::repeat_n(' ', right));
    output
}

fn emit_line(
    row: &[String],
    widths: &[usize],
    alignments: &[Alignment],
    compact: bool,
) -> String {
    let mut output = String::new();

    if !compact {
        output.push('|');
    }

    for column in 0..widths.len() {
        if compact {
            if column > 0 {
                output.push_str("  ");
            }
        } else {
            output.push(' ');
        }

        let content = row.get(column).map(String::as_str).unwrap_or("");
        let content_width = content.chars().count();
        let mut aligned = align_fragment(content, content_width, widths[column], &alignments[column]);

        if compact && column + 1 == widths.len() {
            let right_padding = match alignments[column] {
                Alignment::Left => widths[column].saturating_sub(content_width),
                Alignment::Right => 0,
                Alignment::Center => (widths[column].saturating_sub(content_width) + 1) / 2,
            };
            for _ in 0..right_padding {
                aligned.pop();
            }
        }

        output.push_str(&aligned);

        if !compact {
            output.push_str(" |");
        }
    }

    output.push('\n');
    output
}

fn grid_border(widths: &[usize]) -> String {
    let mut output = String::from("+");

    for &width in widths {
        output.extend(std::iter::repeat_n('-', width + 2));
        output.push('+');
    }

    output.push('\n');
    output
}

fn markdown_marker(widths: &[usize], alignments: &[Alignment]) -> String {
    let mut output = String::from("|");

    for (column, &width) in widths.iter().enumerate() {
        output.push(' ');

        match alignments[column] {
            Alignment::Left => {
                output.push(':');
                output.extend(std::iter::repeat_n('-', width - 1));
            }
            Alignment::Right => {
                output.extend(std::iter::repeat_n('-', width - 1));
                output.push(':');
            }
            Alignment::Center => {
                output.push(':');
                output.extend(std::iter::repeat_n('-', width - 2));
                output.push(':');
            }
        }

        output.push_str(" |");
    }

    output.push('\n');
    output
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

    if matches!(options.mode, Mode::Markdown) && !options.header {
        return Err(TableError::MarkdownNeedsHeader);
    }

    let (overhead, minimum_width) = match options.mode {
        Mode::Grid => (3 * column_count + 1, 1),
        Mode::Markdown => (3 * column_count + 1, 3),
        Mode::Compact => (2 * (column_count - 1), 1),
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
            validate_cell(cell, row, column)?;
        }
    }

    let alignments = if options.alignments.is_empty() {
        vec![Alignment::Left; column_count]
    } else {
        options.alignments.clone()
    };

    let overflows = if options.overflow.is_empty() {
        vec![Overflow::Wrap; column_count]
    } else {
        options.overflow.clone()
    };

    let markdown = matches!(options.mode, Mode::Markdown);
    let tokenized: Vec<Vec<Vec<Vec<Token>>>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| split_cell(cell, markdown))
                .collect()
        })
        .collect();

    let mut widths = vec![minimum_width; column_count];

    for row in &tokenized {
        for (column, lines) in row.iter().enumerate() {
            for line in lines {
                let line_width = line.iter().map(|token| token.width).sum::<usize>();
                widths[column] = widths[column].max(line_width);
            }
        }
    }

    if options.max_width != 0 {
        while total_width(&widths, overhead) > options.max_width {
            let widest = (0..column_count)
                .filter(|&column| widths[column] > minimum_width)
                .map(|column| widths[column])
                .max()
                .unwrap();

            let candidate = (0..column_count)
                .rev()
                .find(|&column| widths[column] == widest && widths[column] > minimum_width)
                .unwrap();

            widths[candidate] -= 1;
        }
    }

    let mut output = String::new();

    if matches!(options.mode, Mode::Grid) {
        output.push_str(&grid_border(&widths));
    }

    for row_index in 0..rows.len() {
        let mut cell_fragments: Vec<Vec<String>> = Vec::with_capacity(column_count);

        for column in 0..column_count {
            let mut combined = Vec::new();

            for line in &tokenized[row_index][column] {
                combined.extend(fragments(line, widths[column], &overflows[column]));
            }

            cell_fragments.push(combined);
        }

        let height = cell_fragments.iter().map(Vec::len).max().unwrap_or(1);

        for fragment_index in 0..height {
            let physical: Vec<String> = (0..column_count)
                .map(|column| {
                    cell_fragments[column]
                        .get(fragment_index)
                        .cloned()
                        .unwrap_or_default()
                })
                .collect();

            output.push_str(&emit_line(
                &physical,
                &widths,
                &alignments,
                matches!(options.mode, Mode::Compact),
            ));
        }

        match options.mode {
            Mode::Grid if options.header && row_index == 0 && row_index + 1 < rows.len() => {
                output.push_str(&grid_border(&widths));
            }
            Mode::Markdown if row_index == 0 => {
                output.push_str(&markdown_marker(&widths, &alignments));
            }
            Mode::Grid if row_index + 1 == rows.len() => {
                output.push_str(&grid_border(&widths));
            }
            _ => {}
        }
    }

    Ok(output)
}

fn main() {}