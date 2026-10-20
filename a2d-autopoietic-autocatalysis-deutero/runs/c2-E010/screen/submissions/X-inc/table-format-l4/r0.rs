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

fn overhead(n: usize, mode: &Mode) -> usize {
    match mode {
        Mode::Grid | Mode::Markdown => 3 * n + 1,
        Mode::Compact => 2 * n.saturating_sub(1),
    }
}

fn minimum_column_width(mode: &Mode) -> usize {
    match mode {
        Mode::Markdown => 3,
        Mode::Grid | Mode::Compact => 1,
    }
}

fn invalid_scalar(ch: char) -> bool {
    ch == '\r'
        || (ch.is_control() && ch != '\n' && ch != '\r')
        || ch == '\u{2028}'
        || ch == '\u{2029}'
}

fn validate_cell(cell: &str, row: usize, column: usize) -> Result<(), TableError> {
    let mut iter = cell.char_indices().peekable();
    while let Some((byte, ch)) = iter.next() {
        if ch == '\r' {
            if matches!(iter.peek(), Some((_, '\n'))) {
                iter.next();
            } else {
                return Err(TableError::InvalidChar { row, column, byte });
            }
        } else if invalid_scalar(ch) {
            return Err(TableError::InvalidChar { row, column, byte });
        }
    }
    Ok(())
}

fn logical_lines(cell: &str, mode: &Mode) -> Vec<Vec<Token>> {
    let normalized = cell.replace("\r\n", "\n");
    normalized
        .split('\n')
        .map(|line| {
            let line = line.trim_end_matches(' ');
            line.chars()
                .map(|ch| {
                    let text = match mode {
                        Mode::Markdown if ch == '|' => "\\|".to_owned(),
                        Mode::Markdown if ch == '\\' => "\\\\".to_owned(),
                        _ => ch.to_string(),
                    };
                    let width = text.chars().count();
                    Token { text, width }
                })
                .collect()
        })
        .collect()
}

fn token_width(tokens: &[Token]) -> usize {
    tokens.iter().map(|t| t.width).sum()
}

fn distribute(
    tokens: &[Token],
    width: usize,
    overflow: &Overflow,
) -> Vec<Vec<Token>> {
    if tokens.is_empty() {
        return vec![Vec::new()];
    }
    if token_width(tokens) <= width {
        return vec![tokens.to_vec()];
    }

    match overflow {
        Overflow::Clip => {
            let mut out = Vec::new();
            let mut used = 0;
            for token in tokens {
                if used + token.width > width.saturating_sub(1) {
                    break;
                }
                used += token.width;
                out.push(token.clone());
            }
            out.push(Token {
                text: "…".to_owned(),
                width: 1,
            });
            vec![out]
        }
        Overflow::Wrap => {
            let mut result = Vec::new();
            let mut start = 0;
            while start < tokens.len() {
                let mut end = start;
                let mut used = 0;
                while end < tokens.len() && used + tokens[end].width <= width {
                    used += tokens[end].width;
                    end += 1;
                }
                result.push(tokens[start..end].to_vec());
                start = end;
            }
            result
        }
    }
}

fn aligned(content: &[Token], width: usize, alignment: &Alignment) -> String {
    let text: String = content.iter().map(|t| t.text.as_str()).collect();
    let used = token_width(content);
    let extra = width.saturating_sub(used);
    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
}

fn render_data_line(
    fragments: &[Vec<Token>],
    widths: &[usize],
    alignments: &[Alignment],
    mode: &Mode,
) -> String {
    match mode {
        Mode::Grid | Mode::Markdown => {
            let mut out = String::from("|");
            for i in 0..widths.len() {
                out.push(' ');
                out.push_str(&aligned(&fragments[i], widths[i], &alignments[i]));
                out.push(' ');
                out.push('|');
            }
            out.push('\n');
            out
        }
        Mode::Compact => {
            let mut out = String::new();
            for i in 0..widths.len() {
                if i > 0 {
                    out.push_str("  ");
                }
                let mut cell = aligned(&fragments[i], widths[i], &alignments[i]);
                if i + 1 == widths.len() {
                    let right_padding = match alignments[i] {
                        Alignment::Left => widths[i].saturating_sub(token_width(&fragments[i])),
                        Alignment::Center => {
                            let extra = widths[i].saturating_sub(token_width(&fragments[i]));
                            extra - extra / 2
                        }
                        Alignment::Right => 0,
                    };
                    for _ in 0..right_padding {
                        cell.pop();
                    }
                }
                out.push_str(&cell);
            }
            out.push('\n');
            out
        }
    }
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

fn markdown_marker(widths: &[usize], alignments: &[Alignment]) -> String {
    let mut out = String::from("|");
    for (i, &width) in widths.iter().enumerate() {
        out.push(' ');
        match alignments[i] {
            Alignment::Left => {
                out.push(':');
                out.push_str(&"-".repeat(width - 1));
            }
            Alignment::Right => {
                out.push_str(&"-".repeat(width - 1));
                out.push(':');
            }
            Alignment::Center => {
                out.push(':');
                out.push_str(&"-".repeat(width - 2));
                out.push(':');
            }
        }
        out.push(' ');
        out.push('|');
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

    let min_col = minimum_column_width(&options.mode);
    let min_total = overhead(n, &options.mode) + n * min_col;
    if options.max_width != 0 && options.max_width < min_total {
        return Err(TableError::TooNarrow {
            minimum: min_total,
            provided: options.max_width,
        });
    }

    for (row, cells) in rows.iter().enumerate() {
        for (column, cell) in cells.iter().enumerate() {
            validate_cell(cell, row, column)?;
        }
    }

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

    let transformed: Vec<Vec<Vec<Vec<Token>>>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| logical_lines(cell, &options.mode))
                .collect()
        })
        .collect();

    let mut widths = vec![min_col; n];
    for row in &transformed {
        for (column, lines) in row.iter().enumerate() {
            for line in lines {
                widths[column] = widths[column].max(token_width(line));
            }
        }
    }

    if options.max_width != 0 {
        let mut total = widths.iter().sum::<usize>() + overhead(n, &options.mode);
        while total > options.max_width {
            let candidate = (0..n)
                .filter(|&i| widths[i] > min_col)
                .max_by_key(|&i| (widths[i], i));
            match candidate {
                Some(i) => {
                    widths[i] -= 1;
                    total -= 1;
                }
                None => break,
            }
        }
    }

    let mut output = String::new();
    if matches!(options.mode, Mode::Grid) {
        output.push_str(&border(&widths));
    }

    for (row_index, row) in transformed.iter().enumerate() {
        let mut cells_fragments: Vec<Vec<Vec<Token>>> = Vec::with_capacity(n);
        let mut height = 1;
        for column in 0..n {
            let mut fragments = Vec::new();
            for line in &row[column] {
                fragments.extend(distribute(line, widths[column], &overflows[column]));
            }
            height = height.max(fragments.len());
            cells_fragments.push(fragments);
        }

        for fragment_index in 0..height {
            let fragments: Vec<Vec<Token>> = (0..n)
                .map(|column| {
                    cells_fragments[column]
                        .get(fragment_index)
                        .cloned()
                        .unwrap_or_default()
                })
                .collect();
            output.push_str(&render_data_line(
                &fragments,
                &widths,
                &alignments,
                &options.mode,
            ));
        }

        if matches!(options.mode, Mode::Markdown) && row_index == 0 {
            output.push_str(&markdown_marker(&widths, &alignments));
        }

        if matches!(options.mode, Mode::Grid)
            && options.header
            && row_index == 0
            && rows.len() > 1
        {
            output.push_str(&border(&widths));
        }
    }

    if matches!(options.mode, Mode::Grid) {
        output.push_str(&border(&widths));
    }

    Ok(output)
}