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

#[derive(Debug, Clone)]
struct Token {
    text: String,
    width: usize,
}

#[derive(Debug, Clone)]
struct Cell {
    lines: Vec<Vec<Token>>,
}

fn valid_scalar(c: char) -> bool {
    !c.is_control() && c != '\u{2028}' && c != '\u{2029}' || c == '\n' || c == '\r'
}

fn validate_cell(cell: &str, row: usize, column: usize) -> Result<(), TableError> {
    let mut chars = cell.char_indices().peekable();
    while let Some((byte, c)) = chars.next() {
        if c == '\r' {
            if matches!(chars.peek(), Some((_, '\n'))) {
                chars.next();
            } else {
                return Err(TableError::InvalidChar { row, column, byte });
            }
        } else if !valid_scalar(c) {
            return Err(TableError::InvalidChar { row, column, byte });
        }
    }
    Ok(())
}

fn tokenize_line(line: &str, markdown: bool) -> Vec<Token> {
    let trimmed = line.trim_end_matches(' ');
    let mut tokens = Vec::new();
    for c in trimmed.chars() {
        if markdown && c == '|' {
            tokens.push(Token { text: "\\|".to_owned(), width: 2 });
        } else if markdown && c == '\\' {
            tokens.push(Token { text: "\\\\".to_owned(), width: 2 });
        } else {
            tokens.push(Token { text: c.to_string(), width: 1 });
        }
    }
    tokens
}

fn split_cell(cell: &str, markdown: bool) -> Cell {
    let normalized = cell.replace("\r\n", "\n");
    let lines = normalized
        .split('\n')
        .map(|line| tokenize_line(line, markdown))
        .collect();
    Cell { lines }
}

fn token_width(tokens: &[Token]) -> usize {
    tokens.iter().map(|t| t.width).sum()
}

fn fragment_text(tokens: &[Token]) -> String {
    tokens.iter().map(|t| t.text.as_str()).collect()
}

fn fragments(tokens: &[Token], width: usize, overflow: &Overflow) -> Vec<String> {
    if tokens.is_empty() {
        return vec![String::new()];
    }
    match overflow {
        Overflow::Clip => {
            let total = token_width(tokens);
            if total <= width {
                return vec![fragment_text(tokens)];
            }
            let mut used = 0;
            let mut selected = Vec::new();
            for token in tokens {
                if used + token.width > width.saturating_sub(1) {
                    break;
                }
                used += token.width;
                selected.push(token.clone());
            }
            let mut text = fragment_text(&selected);
            text.push('…');
            vec![text]
        }
        Overflow::Wrap => {
            let mut out = Vec::new();
            let mut start = 0;
            while start < tokens.len() {
                let mut end = start;
                let mut used = 0;
                while end < tokens.len() && used + tokens[end].width <= width {
                    used += tokens[end].width;
                    end += 1;
                }
                // All tokens are at most two scalars and Markdown's minimum
                // width is three, so each iteration consumes at least one.
                out.push(fragment_text(&tokens[start..end]));
                start = end;
            }
            out
        }
    }
}

fn pad(text: &str, width: usize, alignment: &Alignment) -> String {
    let count = text.chars().count();
    let extra = width.saturating_sub(count);
    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };
    let mut out = String::new();
    out.extend(std::iter::repeat_n(' ', left));
    out.push_str(text);
    out.extend(std::iter::repeat_n(' ', right));
    out
}

fn border(widths: &[usize]) -> String {
    let mut out = String::from("+");
    for &width in widths {
        out.extend(std::iter::repeat_n('-', width + 2));
        out.push('+');
    }
    out.push('\n');
    out
}

fn data_line(parts: &[String]) -> String {
    let mut out = String::from("|");
    for part in parts {
        out.push(' ');
        out.push_str(part);
        out.push(' ');
        out.push('|');
    }
    out.push('\n');
    out
}

fn render_data_row(
    row: &[Cell],
    widths: &[usize],
    alignments: &[Alignment],
    overflows: &[Overflow],
    mode: &Mode,
) -> Vec<String> {
    let mut cell_fragments: Vec<Vec<String>> = Vec::with_capacity(row.len());
    for (column, cell) in row.iter().enumerate() {
        let mut all = Vec::new();
        for line in &cell.lines {
            all.extend(fragments(line, widths[column], &overflows[column]));
        }
        cell_fragments.push(all);
    }
    let height = cell_fragments.iter().map(Vec::len).max().unwrap_or(1);
    let mut output = Vec::with_capacity(height);
    for index in 0..height {
        let mut padded = Vec::with_capacity(row.len());
        for column in 0..row.len() {
            let fragment = cell_fragments[column]
                .get(index)
                .map(String::as_str)
                .unwrap_or("");
            let mut value = pad(fragment, widths[column], &alignments[column]);
            if matches!(mode, Mode::Compact) && column + 1 == row.len() {
                let fragment_width = fragment.chars().count();
                let extra = widths[column].saturating_sub(fragment_width);
                let right_padding = match alignments[column] {
                    Alignment::Left => extra,
                    Alignment::Right => 0,
                    Alignment::Center => extra - extra / 2,
                };
                for _ in 0..right_padding {
                    value.pop();
                }
            }
            padded.push(value);
        }
        if matches!(mode, Mode::Compact) {
            output.push(padded.join("  ") + "\n");
        } else {
            output.push(data_line(&padded));
        }
    }
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
        Mode::Grid | Mode::Markdown => 3usize.saturating_mul(n).saturating_add(1),
        Mode::Compact => 2usize.saturating_mul(n.saturating_sub(1)),
    };
    let minimum_width = if matches!(options.mode, Mode::Markdown) { 3 } else { 1 };
    let minimum = overhead.saturating_add(n.saturating_mul(minimum_width));
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
    let cells: Vec<Vec<Cell>> = rows
        .iter()
        .map(|row| row.iter().map(|cell| split_cell(cell, markdown)).collect())
        .collect();

    let mut widths = vec![minimum_width; n];
    for row in &cells {
        for (column, cell) in row.iter().enumerate() {
            for line in &cell.lines {
                widths[column] = widths[column].max(token_width(line));
            }
        }
    }

    if options.max_width != 0 {
        let mut total = overhead.saturating_add(widths.iter().sum::<usize>());
        while total > options.max_width {
            let candidate = (0..n)
                .filter(|&column| widths[column] > minimum_width)
                .max_by_key(|&column| (widths[column], column));
            match candidate {
                Some(column) => {
                    widths[column] -= 1;
                    total -= 1;
                }
                None => break,
            }
        }
    }

    let mut out = String::new();
    if matches!(options.mode, Mode::Grid) {
        out.push_str(&border(&widths));
    }

    for (row_index, row) in cells.iter().enumerate() {
        for line in render_data_row(row, &widths, &alignments, &overflows, &options.mode) {
            out.push_str(&line);
        }

        if markdown && row_index == 0 {
            let mut marker_parts = Vec::with_capacity(n);
            for column in 0..n {
                let marker = match alignments[column] {
                    Alignment::Left => format!(":{}", "-".repeat(widths[column] - 1)),
                    Alignment::Right => format!("{}:", "-".repeat(widths[column] - 1)),
                    Alignment::Center => {
                        format!(":{}:", "-".repeat(widths[column] - 2))
                    }
                };
                marker_parts.push(marker);
            }
            out.push_str(&data_line(&marker_parts));
        }

        if matches!(options.mode, Mode::Grid) {
            if options.header && row_index == 0 && row_index + 1 < cells.len() {
                out.push_str(&border(&widths));
            } else if row_index + 1 == cells.len() {
                out.push_str(&border(&widths));
            }
        }
    }

    Ok(out)
}

fn main() {}