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

fn fixed_overhead(mode: &Mode, n: usize) -> usize {
    match mode {
        Mode::Grid | Mode::Markdown => 3 * n + 1,
        Mode::Compact => 2 * (n - 1),
    }
}

fn minimum_column_width(mode: &Mode) -> usize {
    match mode {
        Mode::Markdown => 3,
        Mode::Grid | Mode::Compact => 1,
    }
}

fn invalid_scalar(c: char) -> bool {
    (c.is_control() && c != '\n' && c != '\r') || c == '\u{2028}' || c == '\u{2029}'
}

fn normalize_cell(
    text: &str,
    row: usize,
    column: usize,
    mode: &Mode,
) -> Result<Vec<Vec<Token>>, TableError> {
    let mut normalized = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();

    while let Some((byte, ch)) = chars.next() {
        if ch == '\r' {
            if matches!(chars.peek(), Some((_, '\n'))) {
                chars.next();
                normalized.push('\n');
            } else {
                return Err(TableError::InvalidChar { row, column, byte });
            }
        } else if invalid_scalar(ch) {
            return Err(TableError::InvalidChar { row, column, byte });
        } else {
            normalized.push(ch);
        }
    }

    let mut lines = Vec::new();
    for line in normalized.split('\n') {
        let trimmed = line.trim_end_matches(' ');
        let mut tokens = Vec::new();
        for ch in trimmed.chars() {
            match mode {
                Mode::Markdown if ch == '|' => tokens.push(Token {
                    text: "\\|".to_owned(),
                    width: 2,
                }),
                Mode::Markdown if ch == '\\' => tokens.push(Token {
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

fn allocate_widths(
    natural: &[usize],
    mode: &Mode,
    budget: usize,
) -> Vec<usize> {
    let minimum = minimum_column_width(mode);
    let overhead = fixed_overhead(mode, natural.len());
    let mut widths = natural.to_vec();

    if budget == 0 {
        return widths;
    }

    while widths.iter().sum::<usize>() + overhead > budget {
        let mut widest: Option<usize> = None;
        for (i, width) in widths.iter().enumerate() {
            if *width > minimum
                && widest.map_or(true, |current| *width >= widths[current])
            {
                widest = Some(i);
            }
        }
        if let Some(i) = widest {
            widths[i] -= 1;
        } else {
            break;
        }
    }
    widths
}

fn split_line(tokens: &[Token], width: usize, overflow: &Overflow) -> Vec<Vec<Token>> {
    if tokens.is_empty() {
        return vec![Vec::new()];
    }

    match overflow {
        Overflow::Clip => {
            if token_width(tokens) <= width {
                return vec![tokens.to_vec()];
            }
            let mut fragment = Vec::new();
            let mut used = 0;
            for token in tokens {
                if used + token.width > width.saturating_sub(1) {
                    break;
                }
                used += token.width;
                fragment.push(token.clone());
            }
            fragment.push(Token {
                text: "…".to_owned(),
                width: 1,
            });
            vec![fragment]
        }
        Overflow::Wrap => {
            let mut fragments = Vec::new();
            let mut start = 0;
            while start < tokens.len() {
                let mut end = start;
                let mut used = 0;
                while end < tokens.len() && used + tokens[end].width <= width {
                    used += tokens[end].width;
                    end += 1;
                }
                // Every token fits the minimum width in all supported modes.
                if end == start {
                    end += 1;
                }
                fragments.push(tokens[start..end].to_vec());
                start = end;
            }
            fragments
        }
    }
}

fn render_aligned(
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

    let mut out = String::new();
    out.extend(std::iter::repeat_n(' ', left));
    for token in tokens {
        out.push_str(&token.text);
    }
    if !omit_right_padding {
        out.extend(std::iter::repeat_n(' ', right));
    }
    out
}

fn emit_data_line(
    output: &mut String,
    fragments: &[Vec<Token>],
    widths: &[usize],
    alignments: &[Alignment],
    mode: &Mode,
) {
    match mode {
        Mode::Grid | Mode::Markdown => {
            output.push('|');
            for (i, fragment) in fragments.iter().enumerate() {
                output.push(' ');
                output.push_str(&render_aligned(
                    fragment,
                    widths[i],
                    &alignments[i],
                    false,
                ));
                output.push(' ');
                output.push('|');
            }
            output.push('\n');
        }
        Mode::Compact => {
            for i in 0..fragments.len() {
                if i > 0 {
                    output.push_str("  ");
                }
                output.push_str(&render_aligned(
                    &fragments[i],
                    widths[i],
                    &alignments[i],
                    i + 1 == fragments.len(),
                ));
            }
            output.push('\n');
        }
    }
}

fn emit_grid_border(output: &mut String, widths: &[usize]) {
    output.push('+');
    for width in widths {
        output.extend(std::iter::repeat_n('-', width + 2));
        output.push('+');
    }
    output.push('\n');
}

fn emit_markdown_marker(
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

    let min_width = minimum_column_width(&options.mode);
    let minimum = fixed_overhead(&options.mode, n) + n * min_width;
    if options.max_width != 0 && options.max_width < minimum {
        return Err(TableError::TooNarrow {
            minimum,
            provided: options.max_width,
        });
    }

    let default_alignment = Alignment::Left;
    let default_overflow = Overflow::Wrap;
    let alignments: Vec<&Alignment> = (0..n)
        .map(|i| options.alignments.get(i).unwrap_or(&default_alignment))
        .collect();
    let overflows: Vec<&Overflow> = (0..n)
        .map(|i| options.overflow.get(i).unwrap_or(&default_overflow))
        .collect();

    let mut content = Vec::with_capacity(rows.len());
    let mut natural = vec![min_width; n];

    for (row_index, row) in rows.iter().enumerate() {
        let mut normalized_row = Vec::with_capacity(n);
        for (column, cell) in row.iter().enumerate() {
            let lines = normalize_cell(cell, row_index, column, &options.mode)?;
            for line in &lines {
                natural[column] = natural[column].max(token_width(line));
            }
            normalized_row.push(lines);
        }
        content.push(normalized_row);
    }

    let widths = allocate_widths(&natural, &options.mode, options.max_width);
    let owned_alignments: Vec<Alignment> = alignments.iter().map(|a| (*a).clone()).collect();

    let mut output = String::new();
    if matches!(options.mode, Mode::Grid) {
        emit_grid_border(&mut output, &widths);
    }

    for (row_index, row) in content.iter().enumerate() {
        let mut cell_fragments: Vec<Vec<Vec<Token>>> = Vec::with_capacity(n);
        let mut height = 1;
        for column in 0..n {
            let mut fragments = Vec::new();
            for line in &row[column] {
                fragments.extend(split_line(line, widths[column], overflows[column]));
            }
            height = height.max(fragments.len());
            cell_fragments.push(fragments);
        }

        for fragment_index in 0..height {
            let fragments: Vec<Vec<Token>> = (0..n)
                .map(|column| {
                    cell_fragments[column]
                        .get(fragment_index)
                        .cloned()
                        .unwrap_or_default()
                })
                .collect();
            emit_data_line(
                &mut output,
                &fragments,
                &widths,
                &owned_alignments,
                &options.mode,
            );
        }

        if matches!(options.mode, Mode::Grid)
            && options.header
            && row_index == 0
            && rows.len() > 1
        {
            emit_grid_border(&mut output, &widths);
        }

        if matches!(options.mode, Mode::Markdown) && row_index == 0 {
            emit_markdown_marker(&mut output, &widths, &owned_alignments);
        }
    }

    if matches!(options.mode, Mode::Grid) {
        emit_grid_border(&mut output, &widths);
    }

    Ok(output)
}