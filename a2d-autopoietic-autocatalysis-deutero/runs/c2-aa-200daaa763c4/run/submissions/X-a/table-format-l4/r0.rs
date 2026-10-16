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

fn invalid_scalar(ch: char) -> bool {
    (ch.is_control() && ch != '\n' && ch != '\r') || ch == '\u{2028}' || ch == '\u{2029}'
}

fn normalize_cell(
    cell: &str,
    row: usize,
    column: usize,
    markdown: bool,
) -> Result<Vec<Vec<Token>>, TableError> {
    let mut normalized = String::with_capacity(cell.len());
    let mut iter = cell.char_indices().peekable();

    while let Some((byte, ch)) = iter.next() {
        if ch == '\r' {
            match iter.peek() {
                Some((_, '\n')) => {
                    iter.next();
                    normalized.push('\n');
                }
                _ => return Err(TableError::InvalidChar { row, column, byte }),
            }
        } else if invalid_scalar(ch) {
            return Err(TableError::InvalidChar { row, column, byte });
        } else {
            normalized.push(ch);
        }
    }

    let mut lines = Vec::new();
    for raw_line in normalized.split('\n') {
        let trimmed = raw_line.trim_end_matches(' ');
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

        lines.push(tokens);
    }
    Ok(lines)
}

fn token_width(tokens: &[Token]) -> usize {
    tokens.iter().map(|token| token.width).sum()
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

            let mut used = 0;
            let mut result = String::new();
            for token in tokens {
                if used + token.width > width.saturating_sub(1) {
                    break;
                }
                used += token.width;
                result.push_str(&token.text);
            }
            result.push('…');
            vec![result]
        }
        Overflow::Wrap => {
            let mut result = Vec::new();
            let mut current = String::new();
            let mut used = 0;

            for token in tokens {
                if used + token.width > width {
                    result.push(current);
                    current = String::new();
                    used = 0;
                }
                current.push_str(&token.text);
                used += token.width;
            }

            if !current.is_empty() {
                result.push(current);
            }
            result
        }
    }
}

fn align_fragment(fragment: &str, width: usize, alignment: &Alignment) -> String {
    let used = fragment.chars().count();
    let extra = width.saturating_sub(used);
    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };

    let mut result = String::with_capacity(fragment.len() + extra);
    result.extend(std::iter::repeat_n(' ', left));
    result.push_str(fragment);
    result.extend(std::iter::repeat_n(' ', right));
    result
}

fn append_data_line(
    output: &mut String,
    contents: &[String],
    widths: &[usize],
    alignments: &[Alignment],
    mode: &Mode,
) {
    match mode {
        Mode::Compact => {
            for column in 0..contents.len() {
                if column > 0 {
                    output.push_str("  ");
                }
                let mut aligned = align_fragment(&contents[column], widths[column], &alignments[column]);
                if column + 1 == contents.len() {
                    while aligned.ends_with(' ') && aligned.chars().count() > contents[column].chars().count() {
                        aligned.pop();
                    }
                }
                output.push_str(&aligned);
            }
            output.push('\n');
        }
        Mode::Grid | Mode::Markdown => {
            output.push('|');
            for column in 0..contents.len() {
                output.push(' ');
                output.push_str(&align_fragment(
                    &contents[column],
                    widths[column],
                    &alignments[column],
                ));
                output.push(' ');
                output.push('|');
            }
            output.push('\n');
        }
    }
}

fn append_border(output: &mut String, widths: &[usize]) {
    output.push('+');
    for width in widths {
        output.extend(std::iter::repeat_n('-', width + 2));
        output.push('+');
    }
    output.push('\n');
}

fn append_markdown_separator(
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

    if options.mode == Mode::Markdown && !options.header {
        return Err(TableError::MarkdownNeedsHeader);
    }

    let minimum_column_width = if options.mode == Mode::Markdown { 3 } else { 1 };
    let overhead = match options.mode {
        Mode::Compact => 2 * (n - 1),
        Mode::Grid | Mode::Markdown => 3 * n + 1,
    };
    let minimum = overhead + n * minimum_column_width;

    if options.max_width != 0 && options.max_width < minimum {
        return Err(TableError::TooNarrow {
            minimum,
            provided: options.max_width,
        });
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

    let markdown = options.mode == Mode::Markdown;
    let mut cells = Vec::with_capacity(rows.len());
    let mut widths = vec![minimum_column_width; n];

    for (row_index, row) in rows.iter().enumerate() {
        let mut normalized_row = Vec::with_capacity(n);
        for (column, cell) in row.iter().enumerate() {
            let lines = normalize_cell(cell, row_index, column, markdown)?;
            for line in &lines {
                widths[column] = widths[column].max(token_width(line));
            }
            normalized_row.push(lines);
        }
        cells.push(normalized_row);
    }

    if options.max_width != 0 {
        while overhead + widths.iter().sum::<usize>() > options.max_width {
            let candidate = (0..n)
                .filter(|&column| widths[column] > minimum_column_width)
                .max_by_key(|&column| (widths[column], column));
            match candidate {
                Some(column) => widths[column] -= 1,
                None => break,
            }
        }
    }

    let mut output = String::new();
    if options.mode == Mode::Grid {
        append_border(&mut output, &widths);
    }

    for (row_index, row) in cells.iter().enumerate() {
        let mut row_fragments: Vec<Vec<Vec<String>>> = Vec::with_capacity(n);
        let mut row_height = 1;

        for column in 0..n {
            let column_fragments: Vec<Vec<String>> = row[column]
                .iter()
                .map(|line| fragments(line, widths[column], &overflows[column]))
                .collect();
            let height = column_fragments.iter().map(Vec::len).sum::<usize>();
            row_height = row_height.max(height);
            row_fragments.push(column_fragments);
        }

        for fragment_index in 0..row_height {
            let mut line_contents = Vec::with_capacity(n);
            for column in 0..n {
                let mut remaining = fragment_index;
                let mut selected: Option<&String> = None;
                for logical_line in &row_fragments[column] {
                    if remaining < logical_line.len() {
                        selected = Some(&logical_line[remaining]);
                        break;
                    }
                    remaining -= logical_line.len();
                }
                line_contents.push(selected.cloned().unwrap_or_default());
            }
            append_data_line(
                &mut output,
                &line_contents,
                &widths,
                &alignments,
                &options.mode,
            );
        }

        if options.mode == Mode::Markdown && row_index == 0 {
            append_markdown_separator(&mut output, &widths, &alignments);
        } else if options.mode == Mode::Grid && options.header && row_index == 0 && rows.len() > 1 {
            append_border(&mut output, &widths);
        }
    }

    if options.mode == Mode::Grid {
        append_border(&mut output, &widths);
    }

    Ok(output)
}