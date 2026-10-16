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

fn min_column_width(mode: &Mode) -> usize {
    match mode {
        Mode::Markdown => 3,
        Mode::Grid | Mode::Compact => 1,
    }
}

fn first_invalid_char(s: &str) -> Option<usize> {
    let mut chars = s.char_indices().peekable();

    while let Some((byte, ch)) = chars.next() {
        if ch == '\r' {
            if matches!(chars.peek(), Some((_, '\n'))) {
                chars.next();
            } else {
                return Some(byte);
            }
        } else if ch == '\n' {
            continue;
        } else if ch.is_control() || ch == '\u{2028}' || ch == '\u{2029}' {
            return Some(byte);
        }
    }

    None
}

fn normalize_lines(s: &str, markdown: bool) -> Vec<Vec<Token>> {
    let normalized = s.replace("\r\n", "\n");
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

            let prefix_limit = width.saturating_sub(1);
            let mut used = 0;
            let mut out = String::new();

            for token in tokens {
                if used + token.width > prefix_limit {
                    break;
                }
                used += token.width;
                out.push_str(&token.text);
            }

            out.push('…');
            vec![out]
        }
        Overflow::Wrap => {
            let mut fragments = Vec::new();
            let mut index = 0;

            while index < tokens.len() {
                let mut used = 0;
                let start = index;

                while index < tokens.len() && used + tokens[index].width <= width {
                    used += tokens[index].width;
                    index += 1;
                }

                let fragment = tokens[start..index]
                    .iter()
                    .map(|token| token.text.as_str())
                    .collect();
                fragments.push(fragment);
            }

            fragments
        }
    }
}

fn align_fragment(text: &str, width: usize, alignment: &Alignment) -> String {
    let actual = text.chars().count();
    let extra = width.saturating_sub(actual);

    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };

    format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
}

fn append_data_line(
    output: &mut String,
    cells: &[String],
    widths: &[usize],
    alignments: &[Alignment],
    mode: &Mode,
) {
    match mode {
        Mode::Grid | Mode::Markdown => {
            output.push('|');
            for column in 0..widths.len() {
                let aligned = align_fragment(&cells[column], widths[column], &alignments[column]);
                output.push(' ');
                output.push_str(&aligned);
                output.push(' ');
                output.push('|');
            }
            output.push('\n');
        }
        Mode::Compact => {
            for column in 0..widths.len() {
                if column > 0 {
                    output.push_str("  ");
                }

                let aligned = align_fragment(&cells[column], widths[column], &alignments[column]);
                if column + 1 == widths.len() {
                    let trimmed_right = aligned.trim_end_matches(' ');
                    output.push_str(trimmed_right);
                } else {
                    output.push_str(&aligned);
                }
            }
            output.push('\n');
        }
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

fn append_markdown_separator(
    output: &mut String,
    widths: &[usize],
    alignments: &[Alignment],
) {
    output.push('|');
    for (width, alignment) in widths.iter().zip(alignments) {
        let marker = match alignment {
            Alignment::Left => format!(":{}", "-".repeat(width - 1)),
            Alignment::Right => format!("{}:", "-".repeat(width - 1)),
            Alignment::Center => format!(":{}:", "-".repeat(width - 2)),
        };
        output.push(' ');
        output.push_str(&marker);
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

    let min_width = min_column_width(&options.mode);
    let overhead = fixed_overhead(n, &options.mode);
    let minimum = overhead + n * min_width;

    if options.max_width != 0 && options.max_width < minimum {
        return Err(TableError::TooNarrow {
            minimum,
            provided: options.max_width,
        });
    }

    for (row, cells) in rows.iter().enumerate() {
        for (column, cell) in cells.iter().enumerate() {
            if let Some(byte) = first_invalid_char(cell) {
                return Err(TableError::InvalidChar { row, column, byte });
            }
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
    let mut contents = Vec::with_capacity(rows.len());
    let mut widths = vec![min_width; n];

    for row in rows {
        let mut cells = Vec::with_capacity(n);

        for (column, cell) in row.iter().enumerate() {
            let lines = normalize_lines(cell, markdown);
            for line in &lines {
                widths[column] = widths[column].max(token_width(line));
            }
            cells.push(lines);
        }

        contents.push(cells);
    }

    if options.max_width != 0 {
        let mut total = overhead + widths.iter().sum::<usize>();
        while total > options.max_width {
            let candidate = (0..n)
                .rev()
                .find(|&column| widths[column] > min_width)
                .expect("minimum budget was validated");
            widths[candidate] -= 1;
            total -= 1;
        }
    }

    let mut output = String::new();

    if matches!(options.mode, Mode::Grid) {
        append_border(&mut output, &widths);
    }

    for (row_index, row) in contents.iter().enumerate() {
        let mut rendered_cells = Vec::with_capacity(n);

        for column in 0..n {
            let mut fragments = Vec::new();
            for line in &row[column] {
                fragments.extend(split_fragments(
                    line,
                    widths[column],
                    &overflows[column],
                ));
            }
            rendered_cells.push(fragments);
        }

        let height = rendered_cells.iter().map(Vec::len).max().unwrap_or(1);
        for fragment_index in 0..height {
            let physical: Vec<String> = rendered_cells
                .iter()
                .map(|fragments| fragments.get(fragment_index).cloned().unwrap_or_default())
                .collect();

            append_data_line(
                &mut output,
                &physical,
                &widths,
                &alignments,
                &options.mode,
            );
        }

        if matches!(options.mode, Mode::Grid) && options.header && row_index == 0 {
            if rows.len() > 1 {
                append_border(&mut output, &widths);
            }
        }

        if matches!(options.mode, Mode::Markdown) && row_index == 0 {
            append_markdown_separator(&mut output, &widths, &alignments);
        }
    }

    if matches!(options.mode, Mode::Grid) {
        append_border(&mut output, &widths);
    }

    Ok(output)
}