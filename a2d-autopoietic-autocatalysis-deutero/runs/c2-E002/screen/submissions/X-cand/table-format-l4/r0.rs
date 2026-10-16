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

fn invalid_char(ch: char, next: Option<char>) -> bool {
    match ch {
        '\r' => next != Some('\n'),
        '\n' => false,
        '\u{2028}' | '\u{2029}' => true,
        _ => ch.is_control(),
    }
}

fn validate(rows: &[Vec<String>]) -> Result<(), TableError> {
    for (row_idx, row) in rows.iter().enumerate() {
        for (column_idx, cell) in row.iter().enumerate() {
            let mut iter = cell.char_indices().peekable();
            while let Some((byte, ch)) = iter.next() {
                let next = iter.peek().map(|(_, next)| *next);
                if invalid_char(ch, next) {
                    return Err(TableError::InvalidChar {
                        row: row_idx,
                        column: column_idx,
                        byte,
                    });
                }
                if ch == '\r' && next == Some('\n') {
                    iter.next();
                }
            }
        }
    }
    Ok(())
}

fn normalized_lines(cell: &str, markdown: bool) -> Vec<Vec<Token>> {
    let normalized = cell.replace("\r\n", "\n");
    normalized
        .split('\n')
        .map(|line| {
            let trimmed = line.trim_end_matches(' ');
            let mut tokens = Vec::new();
            for ch in trimmed.chars() {
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
        })
        .collect()
}

fn token_width(tokens: &[Token]) -> usize {
    tokens.iter().map(|token| token.width).sum()
}

fn clip(tokens: &[Token], width: usize) -> Vec<String> {
    if token_width(tokens) <= width {
        return vec![tokens.iter().map(|t| t.text.as_str()).collect()];
    }
    let mut result = String::new();
    let mut used = 0;
    if width > 0 {
        for token in tokens {
            if used + token.width > width - 1 {
                break;
            }
            result.push_str(&token.text);
            used += token.width;
        }
        result.push('…');
    }
    vec![result]
}

fn wrap(tokens: &[Token], width: usize) -> Vec<String> {
    if tokens.is_empty() {
        return vec![String::new()];
    }
    let mut fragments = Vec::new();
    let mut start = 0;
    while start < tokens.len() {
        let mut end = start;
        let mut used = 0;
        while end < tokens.len() && used + tokens[end].width <= width {
            used += tokens[end].width;
            end += 1;
        }
        fragments.push(tokens[start..end].iter().map(|t| t.text.as_str()).collect());
        start = end;
    }
    fragments
}

fn align_fragment(content: &str, width: usize, alignment: &Alignment) -> String {
    let content_width = content.chars().count();
    let extra = width.saturating_sub(content_width);
    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };
    format!("{}{}{}", " ".repeat(left), content, " ".repeat(right))
}

fn append_data_line(
    output: &mut String,
    fragments: &[String],
    widths: &[usize],
    alignments: &[Alignment],
    mode: &Mode,
) {
    if matches!(mode, Mode::Grid | Mode::Markdown) {
        output.push('|');
        for (index, width) in widths.iter().enumerate() {
            output.push(' ');
            output.push_str(&align_fragment(&fragments[index], *width, &alignments[index]));
            output.push(' ');
            output.push('|');
        }
    } else {
        for index in 0..widths.len() {
            if index != 0 {
                output.push_str("  ");
            }
            let aligned = align_fragment(&fragments[index], widths[index], &alignments[index]);
            if index + 1 == widths.len() {
                let right_padding = match alignments[index] {
                    Alignment::Left => widths[index].saturating_sub(fragments[index].chars().count()),
                    Alignment::Center => {
                        let extra = widths[index].saturating_sub(fragments[index].chars().count());
                        extra - extra / 2
                    }
                    Alignment::Right => 0,
                };
                let keep = aligned.chars().count().saturating_sub(right_padding);
                output.extend(aligned.chars().take(keep));
            } else {
                output.push_str(&aligned);
            }
        }
    }
    output.push('\n');
}

fn append_border(output: &mut String, widths: &[usize]) {
    output.push('+');
    for width in widths {
        output.push_str(&"-".repeat(width + 2));
        output.push('+');
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

    let minimum_width = if matches!(options.mode, Mode::Markdown) { 3 } else { 1 };
    let overhead = if matches!(options.mode, Mode::Compact) {
        2 * (n - 1)
    } else {
        3 * n + 1
    };
    let minimum_total = overhead + n * minimum_width;
    if options.max_width != 0 && options.max_width < minimum_total {
        return Err(TableError::TooNarrow {
            minimum: minimum_total,
            provided: options.max_width,
        });
    }

    validate(rows)?;

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
    let logical: Vec<Vec<Vec<Vec<Token>>>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| normalized_lines(cell, markdown))
                .collect()
        })
        .collect();

    let mut widths = vec![minimum_width; n];
    for row in &logical {
        for (column, lines) in row.iter().enumerate() {
            for line in lines {
                widths[column] = widths[column].max(token_width(line));
            }
        }
    }

    if options.max_width != 0 {
        while widths.iter().sum::<usize>() + overhead > options.max_width {
            let candidate = (0..n)
                .filter(|&column| widths[column] > minimum_width)
                .max_by_key(|&column| (widths[column], column));
            match candidate {
                Some(column) => widths[column] -= 1,
                None => break,
            }
        }
    }

    let mut output = String::new();
    if matches!(options.mode, Mode::Grid) {
        append_border(&mut output, &widths);
    }

    for (row_idx, row) in logical.iter().enumerate() {
        let mut cell_fragments = Vec::with_capacity(n);
        let mut height = 1;
        for column in 0..n {
            let mut fragments = Vec::new();
            for line in &row[column] {
                let mut line_fragments = match overflows[column] {
                    Overflow::Clip => clip(line, widths[column]),
                    Overflow::Wrap => wrap(line, widths[column]),
                };
                fragments.append(&mut line_fragments);
            }
            height = height.max(fragments.len());
            cell_fragments.push(fragments);
        }

        for fragment_idx in 0..height {
            let fragments: Vec<String> = (0..n)
                .map(|column| {
                    cell_fragments[column]
                        .get(fragment_idx)
                        .cloned()
                        .unwrap_or_default()
                })
                .collect();
            append_data_line(
                &mut output,
                &fragments,
                &widths,
                &alignments,
                &options.mode,
            );
        }

        if markdown && row_idx == 0 {
            output.push('|');
            for column in 0..n {
                output.push(' ');
                let marker = match alignments[column] {
                    Alignment::Left => format!(":{}", "-".repeat(widths[column] - 1)),
                    Alignment::Right => format!("{}:", "-".repeat(widths[column] - 1)),
                    Alignment::Center => {
                        format!(":{}:", "-".repeat(widths[column] - 2))
                    }
                };
                output.push_str(&marker);
                output.push(' ');
                output.push('|');
            }
            output.push('\n');
        }

        if matches!(options.mode, Mode::Grid) {
            if row_idx + 1 == rows.len() || (options.header && row_idx == 0) {
                append_border(&mut output, &widths);
            }
        }
    }

    Ok(output)
}