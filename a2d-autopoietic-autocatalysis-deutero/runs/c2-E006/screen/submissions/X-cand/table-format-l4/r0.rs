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

fn saturating_mul_add(a: usize, b: usize, c: usize) -> usize {
    a.saturating_mul(b).saturating_add(c)
}

fn validate_cell(cell: &str, row: usize, column: usize) -> Result<(), TableError> {
    let mut chars = cell.char_indices().peekable();
    while let Some((byte, ch)) = chars.next() {
        if ch == '\r' {
            if matches!(chars.peek(), Some((_, '\n'))) {
                chars.next();
                continue;
            }
            return Err(TableError::InvalidChar { row, column, byte });
        }
        if ch == '\n' {
            continue;
        }
        if ch == '\u{2028}' || ch == '\u{2029}' || ch.is_control() {
            return Err(TableError::InvalidChar { row, column, byte });
        }
    }
    Ok(())
}

fn tokenize_line(line: &str, markdown: bool) -> Vec<Token> {
    let mut tokens = Vec::new();
    for ch in line.chars() {
        match (markdown, ch) {
            (true, '|') => tokens.push(Token {
                text: "\\|".to_owned(),
                width: 2,
            }),
            (true, '\\') => tokens.push(Token {
                text: "\\\\".to_owned(),
                width: 2,
            }),
            _ => tokens.push(Token {
                text: ch.to_string(),
                width: 1,
            }),
        }
    }
    tokens
}

fn split_and_tokenize(cell: &str, markdown: bool) -> Vec<Vec<Token>> {
    let normalized = cell.replace("\r\n", "\n");
    normalized
        .split('\n')
        .map(|part| {
            let trimmed = part.trim_end_matches(' ');
            tokenize_line(trimmed, markdown)
        })
        .collect()
}

fn token_width(tokens: &[Token]) -> usize {
    tokens.iter().fold(0usize, |sum, token| sum.saturating_add(token.width))
}

fn fragments(tokens: &[Token], width: usize, overflow: &Overflow) -> Vec<Vec<Token>> {
    if tokens.is_empty() {
        return vec![Vec::new()];
    }

    match overflow {
        Overflow::Clip => {
            if token_width(tokens) <= width {
                return vec![tokens.to_vec()];
            }

            let available = width.saturating_sub(1);
            let mut kept = Vec::new();
            let mut used = 0usize;
            for token in tokens {
                if used.saturating_add(token.width) > available {
                    break;
                }
                used += token.width;
                kept.push(token.clone());
            }
            kept.push(Token {
                text: "…".to_owned(),
                width: 1,
            });
            vec![kept]
        }
        Overflow::Wrap => {
            let mut result = Vec::new();
            let mut start = 0usize;
            while start < tokens.len() {
                let mut end = start;
                let mut used = 0usize;
                while end < tokens.len()
                    && used.saturating_add(tokens[end].width) <= width
                {
                    used += tokens[end].width;
                    end += 1;
                }
                // All tokens have width at most two and valid mode minima are at least one;
                // Markdown's escaped tokens fit because its minimum width is three.
                if end == start {
                    end += 1;
                }
                result.push(tokens[start..end].to_vec());
                start = end;
            }
            result
        }
    }
}

fn render_fragment(
    tokens: &[Token],
    width: usize,
    alignment: &Alignment,
    omit_right_padding: bool,
) -> String {
    let content_width = token_width(tokens);
    let extra = width.saturating_sub(content_width);
    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };

    let mut result = String::new();
    result.extend(std::iter::repeat_n(' ', left));
    for token in tokens {
        result.push_str(&token.text);
    }
    if !omit_right_padding {
        result.extend(std::iter::repeat_n(' ', right));
    }
    result
}

fn append_grid_border(out: &mut String, widths: &[usize]) {
    out.push('+');
    for &width in widths {
        out.extend(std::iter::repeat_n('-', width.saturating_add(2)));
        out.push('+');
    }
    out.push('\n');
}

fn append_data_line(
    out: &mut String,
    fragments: &[Vec<Token>],
    widths: &[usize],
    alignments: &[Alignment],
    mode: &Mode,
) {
    match mode {
        Mode::Grid | Mode::Markdown => {
            out.push('|');
            for i in 0..widths.len() {
                out.push(' ');
                out.push_str(&render_fragment(&fragments[i], widths[i], &alignments[i], false));
                out.push(' ');
                out.push('|');
            }
            out.push('\n');
        }
        Mode::Compact => {
            for i in 0..widths.len() {
                if i > 0 {
                    out.push_str("  ");
                }
                out.push_str(&render_fragment(
                    &fragments[i],
                    widths[i],
                    &alignments[i],
                    i + 1 == widths.len(),
                ));
            }
            out.push('\n');
        }
    }
}

fn append_markdown_marker(out: &mut String, widths: &[usize], alignments: &[Alignment]) {
    out.push('|');
    for (i, &width) in widths.iter().enumerate() {
        out.push(' ');
        match alignments[i] {
            Alignment::Left => {
                out.push(':');
                out.extend(std::iter::repeat_n('-', width.saturating_sub(1)));
            }
            Alignment::Right => {
                out.extend(std::iter::repeat_n('-', width.saturating_sub(1)));
                out.push(':');
            }
            Alignment::Center => {
                out.push(':');
                out.extend(std::iter::repeat_n('-', width.saturating_sub(2)));
                out.push(':');
            }
        }
        out.push(' ');
        out.push('|');
    }
    out.push('\n');
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

    let minimum_column_width = if matches!(options.mode, Mode::Markdown) {
        3
    } else {
        1
    };
    let overhead = match options.mode {
        Mode::Grid | Mode::Markdown => saturating_mul_add(3, n, 1),
        Mode::Compact => n.saturating_sub(1).saturating_mul(2),
    };
    let minimum_total = overhead.saturating_add(n.saturating_mul(minimum_column_width));
    if options.max_width != 0 && options.max_width < minimum_total {
        return Err(TableError::TooNarrow {
            minimum: minimum_total,
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

    let markdown = matches!(options.mode, Mode::Markdown);
    let transformed: Vec<Vec<Vec<Vec<Token>>>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| split_and_tokenize(cell, markdown))
                .collect()
        })
        .collect();

    let mut widths = vec![minimum_column_width; n];
    for row in &transformed {
        for (column, lines) in row.iter().enumerate() {
            for line in lines {
                widths[column] = widths[column].max(token_width(line));
            }
        }
    }

    if options.max_width != 0 {
        let available = options.max_width.saturating_sub(overhead);
        let mut total = widths.iter().fold(0usize, |sum, width| sum.saturating_add(*width));
        while total > available {
            let mut candidate: Option<usize> = None;
            for i in 0..n {
                if widths[i] > minimum_column_width
                    && candidate.map_or(true, |best| widths[i] >= widths[best])
                {
                    candidate = Some(i);
                }
            }
            if let Some(column) = candidate {
                widths[column] -= 1;
                total -= 1;
            } else {
                break;
            }
        }
    }

    let mut out = String::new();
    if matches!(options.mode, Mode::Grid) {
        append_grid_border(&mut out, &widths);
    }

    for (row_index, row) in transformed.iter().enumerate() {
        let mut cell_fragments: Vec<Vec<Vec<Token>>> = Vec::with_capacity(n);
        let mut row_height = 1usize;
        for column in 0..n {
            let mut combined = Vec::new();
            for line in &row[column] {
                combined.extend(fragments(line, widths[column], &overflows[column]));
            }
            row_height = row_height.max(combined.len());
            cell_fragments.push(combined);
        }

        for fragment_index in 0..row_height {
            let line: Vec<Vec<Token>> = (0..n)
                .map(|column| {
                    cell_fragments[column]
                        .get(fragment_index)
                        .cloned()
                        .unwrap_or_default()
                })
                .collect();
            append_data_line(&mut out, &line, &widths, &alignments, &options.mode);
        }

        if matches!(options.mode, Mode::Markdown) && row_index == 0 {
            append_markdown_marker(&mut out, &widths, &alignments);
        } else if matches!(options.mode, Mode::Grid)
            && options.header
            && row_index == 0
            && rows.len() > 1
        {
            append_grid_border(&mut out, &widths);
        }
    }

    if matches!(options.mode, Mode::Grid) {
        append_grid_border(&mut out, &widths);
    }

    Ok(out)
}