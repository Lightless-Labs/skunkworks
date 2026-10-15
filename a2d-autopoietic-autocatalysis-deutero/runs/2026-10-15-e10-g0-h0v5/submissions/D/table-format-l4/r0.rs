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

fn fixed_overhead(n: usize, mode: &Mode) -> usize {
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

fn validate_cell(
    text: &str,
    row: usize,
    column: usize,
) -> Result<Vec<String>, TableError> {
    let mut normalized = String::with_capacity(text.len());
    let mut iter = text.char_indices().peekable();

    while let Some((byte, ch)) = iter.next() {
        match ch {
            '\r' => {
                if matches!(iter.peek(), Some((_, '\n'))) {
                    iter.next();
                    normalized.push('\n');
                } else {
                    return Err(TableError::InvalidChar { row, column, byte });
                }
            }
            '\n' => normalized.push('\n'),
            c if c.is_control() || c == '\u{2028}' || c == '\u{2029}' => {
                return Err(TableError::InvalidChar { row, column, byte });
            }
            c => normalized.push(c),
        }
    }

    let mut lines = Vec::new();
    for line in normalized.split('\n') {
        lines.push(line.trim_end_matches(' ').to_owned());
    }
    Ok(lines)
}

fn tokenize_line(line: &str, markdown: bool) -> Vec<Token> {
    let mut tokens = Vec::new();
    for ch in line.chars() {
        if markdown && ch == '|' {
            tokens.push(Token {
                text: "\\|".to_owned(),
                width: 2,
            });
        } else if markdown && ch == '\\' {
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
    tokens
}

fn token_width(tokens: &[Token]) -> usize {
    tokens.iter().map(|t| t.width).sum()
}

fn fragments(tokens: &[Token], width: usize, overflow: &Overflow) -> Vec<String> {
    if tokens.is_empty() {
        return vec![String::new()];
    }

    match overflow {
        Overflow::Clip => {
            let total = token_width(tokens);
            if total <= width {
                return vec![tokens.iter().map(|t| t.text.as_str()).collect()];
            }

            let mut out = String::new();
            let mut used = 0usize;
            let limit = width.saturating_sub(1);
            for token in tokens {
                if used + token.width > limit {
                    break;
                }
                used += token.width;
                out.push_str(&token.text);
            }
            out.push('…');
            vec![out]
        }
        Overflow::Wrap => {
            let mut result = Vec::new();
            let mut start = 0usize;
            while start < tokens.len() {
                let mut end = start;
                let mut used = 0usize;
                while end < tokens.len() && used + tokens[end].width <= width {
                    used += tokens[end].width;
                    end += 1;
                }

                // Widths are always at least three in Markdown and one otherwise,
                // so every token fits in a fresh fragment.
                if end == start {
                    end += 1;
                }

                result.push(tokens[start..end].iter().map(|t| t.text.as_str()).collect());
                start = end;
            }
            result
        }
    }
}

fn align(text: &str, width: usize, alignment: &Alignment) -> String {
    let count = text.chars().count();
    let extra = width.saturating_sub(count);
    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };

    let mut result = String::with_capacity(text.len() + extra);
    result.extend(std::iter::repeat_n(' ', left));
    result.push_str(text);
    result.extend(std::iter::repeat_n(' ', right));
    result
}

fn grid_border(widths: &[usize]) -> String {
    let mut line = String::from("+");
    for &width in widths {
        line.extend(std::iter::repeat_n('-', width + 2));
        line.push('+');
    }
    line.push('\n');
    line
}

fn data_line(
    values: &[String],
    widths: &[usize],
    alignments: &[Alignment],
    mode: &Mode,
) -> String {
    match mode {
        Mode::Grid | Mode::Markdown => {
            let mut line = String::from("|");
            for i in 0..widths.len() {
                line.push(' ');
                line.push_str(&align(&values[i], widths[i], &alignments[i]));
                line.push(' ');
                line.push('|');
            }
            line.push('\n');
            line
        }
        Mode::Compact => {
            let mut line = String::new();
            for i in 0..widths.len() {
                if i > 0 {
                    line.push_str("  ");
                }
                let padded = align(&values[i], widths[i], &alignments[i]);
                if i + 1 == widths.len() && matches!(alignments[i], Alignment::Left | Alignment::Center) {
                    let desired = match alignments[i] {
                        Alignment::Left => values[i].chars().count(),
                        Alignment::Center => {
                            let extra = widths[i].saturating_sub(values[i].chars().count());
                            values[i].chars().count() + extra / 2
                        }
                        Alignment::Right => unreachable!(),
                    };
                    line.extend(padded.chars().take(desired));
                } else {
                    line.push_str(&padded);
                }
            }
            line.push('\n');
            line
        }
    }
}

fn markdown_marker(widths: &[usize], alignments: &[Alignment]) -> String {
    let mut line = String::from("|");
    for (i, &width) in widths.iter().enumerate() {
        line.push(' ');
        match alignments[i] {
            Alignment::Left => {
                line.push(':');
                line.extend(std::iter::repeat_n('-', width - 1));
            }
            Alignment::Right => {
                line.extend(std::iter::repeat_n('-', width - 1));
                line.push(':');
            }
            Alignment::Center => {
                line.push(':');
                line.extend(std::iter::repeat_n('-', width - 2));
                line.push(':');
            }
        }
        line.push(' ');
        line.push('|');
    }
    line.push('\n');
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

    if matches!(options.mode, Mode::Markdown) && !options.header {
        return Err(TableError::MarkdownNeedsHeader);
    }

    let min_col = minimum_column_width(&options.mode);
    let overhead = fixed_overhead(n, &options.mode);
    let minimum = overhead + n * min_col;
    if options.max_width != 0 && options.max_width < minimum {
        return Err(TableError::TooNarrow {
            minimum,
            provided: options.max_width,
        });
    }

    let mut logical: Vec<Vec<Vec<String>>> = Vec::with_capacity(rows.len());
    for (ri, row) in rows.iter().enumerate() {
        let mut logical_row = Vec::with_capacity(n);
        for (ci, cell) in row.iter().enumerate() {
            logical_row.push(validate_cell(cell, ri, ci)?);
        }
        logical.push(logical_row);
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

    let markdown = matches!(options.mode, Mode::Markdown);
    let mut widths = vec![min_col; n];
    for row in &logical {
        for (ci, cell_lines) in row.iter().enumerate() {
            for line in cell_lines {
                let width = token_width(&tokenize_line(line, markdown));
                widths[ci] = widths[ci].max(width);
            }
        }
    }

    if options.max_width != 0 {
        while widths.iter().sum::<usize>() + overhead > options.max_width {
            let candidate = (0..n)
                .rev()
                .find(|&i| widths[i] > min_col)
                .expect("minimum width was validated");
            widths[candidate] -= 1;
        }
    }

    let mut output = String::new();
    if matches!(options.mode, Mode::Grid) {
        output.push_str(&grid_border(&widths));
    }

    for (ri, row) in logical.iter().enumerate() {
        let mut cell_fragments: Vec<Vec<String>> = Vec::with_capacity(n);
        let mut height = 1usize;

        for ci in 0..n {
            let mut combined = Vec::new();
            for line in &row[ci] {
                let tokens = tokenize_line(line, markdown);
                combined.extend(fragments(&tokens, widths[ci], &overflows[ci]));
            }
            height = height.max(combined.len());
            cell_fragments.push(combined);
        }

        for fi in 0..height {
            let mut values = Vec::with_capacity(n);
            for ci in 0..n {
                values.push(
                    cell_fragments[ci]
                        .get(fi)
                        .cloned()
                        .unwrap_or_default(),
                );
            }
            output.push_str(&data_line(&values, &widths, &alignments, &options.mode));
        }

        if markdown && ri == 0 {
            output.push_str(&markdown_marker(&widths, &alignments));
        }

        if matches!(options.mode, Mode::Grid) {
            if options.header && ri == 0 && rows.len() > 1 {
                output.push_str(&grid_border(&widths));
            } else if ri + 1 == rows.len() {
                output.push_str(&grid_border(&widths));
            }
        }
    }

    Ok(output)
}