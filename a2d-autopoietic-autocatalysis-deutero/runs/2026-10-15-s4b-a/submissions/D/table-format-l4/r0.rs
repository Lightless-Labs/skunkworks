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

fn token_width(tokens: &[Token]) -> usize {
    tokens.iter().map(|token| token.width).sum()
}

fn tokenize_line(line: &str, markdown: bool) -> Vec<Token> {
    let mut tokens = Vec::new();

    for ch in line.chars() {
        let text = if markdown && ch == '|' {
            "\\|".to_owned()
        } else if markdown && ch == '\\' {
            "\\\\".to_owned()
        } else {
            ch.to_string()
        };
        let width = text.chars().count();
        tokens.push(Token { text, width });
    }

    tokens
}

fn split_fragments(tokens: &[Token], width: usize, overflow: &Overflow) -> Vec<String> {
    if tokens.is_empty() {
        return vec![String::new()];
    }

    match overflow {
        Overflow::Clip => {
            let total = token_width(tokens);
            if total <= width {
                return vec![tokens.iter().map(|token| token.text.as_str()).collect()];
            }

            let mut used = 0usize;
            let mut text = String::new();
            for token in tokens {
                if used + token.width > width.saturating_sub(1) {
                    break;
                }
                used += token.width;
                text.push_str(&token.text);
            }
            text.push('…');
            vec![text]
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

                result.push(
                    tokens[start..end]
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

fn align_fragment(fragment: &str, width: usize, alignment: &Alignment) -> String {
    let content_width = fragment.chars().count();
    let extra = width.saturating_sub(content_width);

    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };

    let mut result = String::new();
    result.extend(std::iter::repeat_n(' ', left));
    result.push_str(fragment);
    result.extend(std::iter::repeat_n(' ', right));
    result
}

fn append_border(output: &mut String, widths: &[usize]) {
    output.push('+');
    for width in widths {
        output.extend(std::iter::repeat_n('-', width + 2));
        output.push('+');
    }
    output.push('\n');
}

fn append_grid_line(
    output: &mut String,
    fragments: &[String],
    widths: &[usize],
    alignments: &[Alignment],
) {
    output.push('|');
    for ((fragment, width), alignment) in fragments.iter().zip(widths).zip(alignments) {
        output.push(' ');
        output.push_str(&align_fragment(fragment, *width, alignment));
        output.push(' ');
        output.push('|');
    }
    output.push('\n');
}

fn append_compact_line(
    output: &mut String,
    fragments: &[String],
    widths: &[usize],
    alignments: &[Alignment],
) {
    for column in 0..fragments.len() {
        if column > 0 {
            output.push_str("  ");
        }

        let aligned = align_fragment(&fragments[column], widths[column], &alignments[column]);
        if column + 1 == fragments.len() {
            let right_padding = widths[column].saturating_sub(fragments[column].chars().count());
            let keep = aligned.chars().count().saturating_sub(right_padding);
            output.extend(aligned.chars().take(keep));
        } else {
            output.push_str(&aligned);
        }
    }
    output.push('\n');
}

fn append_markdown_marker(
    output: &mut String,
    widths: &[usize],
    alignments: &[Alignment],
) {
    output.push('|');
    for (width, alignment) in widths.iter().zip(alignments) {
        output.push(' ');
        match alignment {
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
        output.push(' ');
        output.push('|');
    }
    output.push('\n');
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

    let (overhead, minimum_width) = match options.mode {
        Mode::Grid | Mode::Markdown => (
            3 * n + 1,
            if matches!(options.mode, Mode::Markdown) { 3 } else { 1 },
        ),
        Mode::Compact => (2 * (n - 1), 1),
    };

    let minimum = overhead + n * minimum_width;
    if options.max_width != 0 && options.max_width < minimum {
        return Err(TableError::TooNarrow {
            minimum,
            provided: options.max_width,
        });
    }

    for (row_index, row) in rows.iter().enumerate() {
        for (column_index, cell) in row.iter().enumerate() {
            let mut byte = 0usize;
            let mut chars = cell.chars().peekable();
            while let Some(ch) = chars.next() {
                if ch == '\r' {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                        byte += 2;
                        continue;
                    }
                    return Err(TableError::InvalidChar {
                        row: row_index,
                        column: column_index,
                        byte,
                    });
                }

                if (ch.is_control() && ch != '\n') || ch == '\u{2028}' || ch == '\u{2029}' {
                    return Err(TableError::InvalidChar {
                        row: row_index,
                        column: column_index,
                        byte,
                    });
                }
                byte += ch.len_utf8();
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

    let markdown = matches!(options.mode, Mode::Markdown);
    let mut logical_lines: Vec<Vec<Vec<Vec<Token>>>> = Vec::with_capacity(rows.len());
    let mut widths = vec![minimum_width; n];

    for row in rows {
        let mut token_row = Vec::with_capacity(n);
        for (column, cell) in row.iter().enumerate() {
            let normalized = cell.replace("\r\n", "\n");
            let mut cell_lines = Vec::new();
            for line in normalized.split('\n') {
                let tokens = tokenize_line(line.trim_end_matches(' '), markdown);
                widths[column] = widths[column].max(token_width(&tokens));
                cell_lines.push(tokens);
            }
            token_row.push(cell_lines);
        }
        logical_lines.push(token_row);
    }

    if options.max_width != 0 {
        while widths.iter().sum::<usize>() + overhead > options.max_width {
            let candidate = (0..n)
                .filter(|&column| widths[column] > minimum_width)
                .max_by_key(|&column| (widths[column], column));
            if let Some(column) = candidate {
                widths[column] -= 1;
            } else {
                break;
            }
        }
    }

    let mut output = String::new();
    if matches!(options.mode, Mode::Grid) {
        append_border(&mut output, &widths);
    }

    for (row_index, row) in logical_lines.iter().enumerate() {
        let mut cells: Vec<Vec<String>> = Vec::with_capacity(n);
        let mut height = 1usize;

        for column in 0..n {
            let mut fragments = Vec::new();
            for line in &row[column] {
                fragments.extend(split_fragments(
                    line,
                    widths[column],
                    &overflows[column],
                ));
            }
            height = height.max(fragments.len());
            cells.push(fragments);
        }

        for fragment_index in 0..height {
            let fragments: Vec<String> = (0..n)
                .map(|column| {
                    cells[column]
                        .get(fragment_index)
                        .cloned()
                        .unwrap_or_default()
                })
                .collect();

            match options.mode {
                Mode::Grid => append_grid_line(&mut output, &fragments, &widths, &alignments),
                Mode::Compact => {
                    append_compact_line(&mut output, &fragments, &widths, &alignments)
                }
                Mode::Markdown => append_grid_line(&mut output, &fragments, &widths, &alignments),
            }
        }

        if matches!(options.mode, Mode::Grid) && options.header && row_index == 0 {
            if rows.len() > 1 {
                append_border(&mut output, &widths);
            }
        }

        if matches!(options.mode, Mode::Markdown) && row_index == 0 {
            append_markdown_marker(&mut output, &widths, &alignments);
        }
    }

    if matches!(options.mode, Mode::Grid) {
        append_border(&mut output, &widths);
    }

    Ok(output)
}