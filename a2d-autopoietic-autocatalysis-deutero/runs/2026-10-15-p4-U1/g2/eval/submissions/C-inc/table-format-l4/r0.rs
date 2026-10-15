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

    let markdown = matches!(options.mode, Mode::Markdown);
    let compact = matches!(options.mode, Mode::Compact);

    if markdown && !options.header {
        return Err(TableError::MarkdownNeedsHeader);
    }

    let min_col = if markdown { 3 } else { 1 };
    let overhead = if compact {
        2 * (n - 1)
    } else {
        3 * n + 1
    };
    let minimum = overhead + n * min_col;

    if options.max_width != 0 && options.max_width < minimum {
        return Err(TableError::TooNarrow {
            minimum,
            provided: options.max_width,
        });
    }

    for (r, cells) in rows.iter().enumerate() {
        for (c, cell) in cells.iter().enumerate() {
            let chars: Vec<(usize, char)> = cell.char_indices().collect();
            let mut i = 0;
            while i < chars.len() {
                let (byte, ch) = chars[i];
                if ch == '\r' {
                    if i + 1 < chars.len() && chars[i + 1].1 == '\n' {
                        i += 2;
                        continue;
                    }
                    return Err(TableError::InvalidChar {
                        row: r,
                        column: c,
                        byte,
                    });
                }
                if (ch.is_control() && ch != '\n') || ch == '\u{2028}' || ch == '\u{2029}' {
                    return Err(TableError::InvalidChar {
                        row: r,
                        column: c,
                        byte,
                    });
                }
                i += 1;
            }
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

    let mut content: Vec<Vec<Vec<Vec<Token>>>> = Vec::with_capacity(rows.len());
    let mut widths = vec![min_col; n];

    for cells in rows {
        let mut row_content = Vec::with_capacity(n);
        for (c, cell) in cells.iter().enumerate() {
            let normalized = cell.replace("\r\n", "\n");
            let logical_lines: Vec<&str> = normalized.split('\n').collect();
            let mut cell_lines = Vec::with_capacity(logical_lines.len());

            for line in logical_lines {
                let trimmed = line.trim_end_matches(' ');
                let mut tokens = Vec::new();
                let mut width = 0;

                for ch in trimmed.chars() {
                    if markdown && ch == '|' {
                        tokens.push(Token {
                            text: "\\|".to_string(),
                            width: 2,
                        });
                        width += 2;
                    } else if markdown && ch == '\\' {
                        tokens.push(Token {
                            text: "\\\\".to_string(),
                            width: 2,
                        });
                        width += 2;
                    } else {
                        tokens.push(Token {
                            text: ch.to_string(),
                            width: 1,
                        });
                        width += 1;
                    }
                }

                widths[c] = widths[c].max(width);
                cell_lines.push(tokens);
            }
            row_content.push(cell_lines);
        }
        content.push(row_content);
    }

    if options.max_width != 0 {
        let mut total = widths.iter().sum::<usize>() + overhead;
        while total > options.max_width {
            let mut candidate: Option<usize> = None;
            for c in 0..n {
                if widths[c] > min_col
                    && candidate.map_or(true, |best| widths[c] >= widths[best])
                {
                    candidate = Some(c);
                }
            }
            if let Some(c) = candidate {
                widths[c] -= 1;
                total -= 1;
            } else {
                break;
            }
        }
    }

    let mut output = String::new();

    if !compact && !markdown {
        append_border(&mut output, &widths);
    }

    for (r, row) in content.iter().enumerate() {
        let mut rendered_cells: Vec<Vec<Vec<Token>>> = Vec::with_capacity(n);
        let mut height = 1;

        for c in 0..n {
            let mut fragments = Vec::new();
            for line in &row[c] {
                let parts = match overflows[c] {
                    Overflow::Clip => clip_tokens(line, widths[c]),
                    Overflow::Wrap => wrap_tokens(line, widths[c]),
                };
                fragments.extend(parts);
            }
            height = height.max(fragments.len());
            rendered_cells.push(fragments);
        }

        for fragment_index in 0..height {
            if compact {
                for c in 0..n {
                    if c > 0 {
                        output.push_str("  ");
                    }
                    let tokens = rendered_cells[c]
                        .get(fragment_index)
                        .map(Vec::as_slice)
                        .unwrap_or(&[]);
                    let text = tokens_text(tokens);
                    let used = tokens_width(tokens);
                    let (left, right) = padding(widths[c], used, &alignments[c]);
                    output.push_str(&" ".repeat(left));
                    output.push_str(&text);
                    if c + 1 < n {
                        output.push_str(&" ".repeat(right));
                    }
                }
                output.push('\n');
            } else {
                output.push('|');
                for c in 0..n {
                    let tokens = rendered_cells[c]
                        .get(fragment_index)
                        .map(Vec::as_slice)
                        .unwrap_or(&[]);
                    let text = tokens_text(tokens);
                    let used = tokens_width(tokens);
                    let (left, right) = padding(widths[c], used, &alignments[c]);
                    output.push(' ');
                    output.push_str(&" ".repeat(left));
                    output.push_str(&text);
                    output.push_str(&" ".repeat(right));
                    output.push_str(" |");
                }
                output.push('\n');
            }
        }

        if markdown && r == 0 {
            append_markdown_marker(&mut output, &widths, &alignments);
        } else if !compact && !markdown && options.header && r == 0 && rows.len() > 1 {
            append_border(&mut output, &widths);
        }
    }

    if !compact && !markdown {
        append_border(&mut output, &widths);
    }

    Ok(output)
}

fn tokens_width(tokens: &[Token]) -> usize {
    tokens.iter().map(|token| token.width).sum()
}

fn tokens_text(tokens: &[Token]) -> String {
    let mut text = String::new();
    for token in tokens {
        text.push_str(&token.text);
    }
    text
}

fn clip_tokens(tokens: &[Token], width: usize) -> Vec<Vec<Token>> {
    if tokens.is_empty() {
        return vec![Vec::new()];
    }
    if tokens_width(tokens) <= width {
        return vec![tokens.to_vec()];
    }

    let target = width.saturating_sub(1);
    let mut used = 0;
    let mut result = Vec::new();

    for token in tokens {
        if used + token.width > target {
            break;
        }
        result.push(token.clone());
        used += token.width;
    }

    result.push(Token {
        text: "…".to_string(),
        width: 1,
    });
    vec![result]
}

fn wrap_tokens(tokens: &[Token], width: usize) -> Vec<Vec<Token>> {
    if tokens.is_empty() {
        return vec![Vec::new()];
    }

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

fn padding(width: usize, used: usize, alignment: &Alignment) -> (usize, usize) {
    let extra = width.saturating_sub(used);
    match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    }
}

fn append_border(output: &mut String, widths: &[usize]) {
    output.push('+');
    for width in widths {
        output.push_str(&"-".repeat(width + 2));
        output.push('+');
    }
    output.push('\n');
}

fn append_markdown_marker(output: &mut String, widths: &[usize], alignments: &[Alignment]) {
    output.push('|');
    for (width, alignment) in widths.iter().zip(alignments) {
        let marker = match alignment {
            Alignment::Left => format!(":{}", "-".repeat(width - 1)),
            Alignment::Right => format!("{}:", "-".repeat(width - 1)),
            Alignment::Center => format!(":{}:", "-".repeat(width - 2)),
        };
        output.push(' ');
        output.push_str(&marker);
        output.push_str(" |");
    }
    output.push('\n');
}

fn main() {}