#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableError {
    EmptyRow { row: usize },
    Ragged {
        row: usize,
        expected: usize,
        actual: usize,
    },
    InvalidChar {
        row: usize,
        column: usize,
        byte: usize,
    },
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
    if matches!(options.mode, Mode::Markdown) && !options.header {
        return Err(TableError::MarkdownNeedsHeader);
    }

    let compact = matches!(options.mode, Mode::Compact);
    let markdown = matches!(options.mode, Mode::Markdown);
    let overhead = if compact { 2 * (n - 1) } else { 3 * n + 1 };
    let min_col = if markdown { 3 } else { 1 };
    let minimum = overhead + n * min_col;

    if options.max_width != 0 && options.max_width < minimum {
        return Err(TableError::TooNarrow {
            minimum,
            provided: options.max_width,
        });
    }

    for (row, cells) in rows.iter().enumerate() {
        for (column, cell) in cells.iter().enumerate() {
            let chars: Vec<(usize, char)> = cell.char_indices().collect();
            let mut i = 0;
            while i < chars.len() {
                let (byte, ch) = chars[i];
                if ch == '\r' {
                    if i + 1 < chars.len() && chars[i + 1].1 == '\n' {
                        i += 2;
                        continue;
                    }
                    return Err(TableError::InvalidChar { row, column, byte });
                }
                if (ch.is_control() && ch != '\n') || ch == '\u{2028}' || ch == '\u{2029}' {
                    return Err(TableError::InvalidChar { row, column, byte });
                }
                i += 1;
            }
        }
    }

    let mut lines: Vec<Vec<Vec<Vec<Token>>>> = Vec::with_capacity(rows.len());
    let mut widths = vec![min_col; n];

    for row in rows {
        let mut cooked_row = Vec::with_capacity(n);
        for (column, cell) in row.iter().enumerate() {
            let normalized = cell.replace("\r\n", "\n");
            let mut cooked_lines = Vec::new();

            for line in normalized.split('\n') {
                let trimmed = line.trim_end_matches(' ');
                let mut tokens = Vec::new();

                for ch in trimmed.chars() {
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

                let line_width: usize = tokens.iter().map(|token| token.width).sum();
                widths[column] = widths[column].max(line_width);
                cooked_lines.push(tokens);
            }

            cooked_row.push(cooked_lines);
        }
        lines.push(cooked_row);
    }

    if options.max_width != 0 {
        let mut total: usize = widths.iter().sum::<usize>() + overhead;
        while total > options.max_width {
            let candidate = (0..n)
                .filter(|&column| widths[column] > min_col)
                .max_by_key(|&column| (widths[column], column));

            if let Some(column) = candidate {
                widths[column] -= 1;
                total -= 1;
            } else {
                break;
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

    let mut output = String::new();

    let emit_border = |out: &mut String| {
        out.push('+');
        for &width in &widths {
            out.extend(std::iter::repeat_n('-', width + 2));
            out.push('+');
        }
        out.push('\n');
    };

    if matches!(options.mode, Mode::Grid) {
        emit_border(&mut output);
    }

    for (row_index, row) in lines.iter().enumerate() {
        let mut fragments: Vec<Vec<String>> = Vec::with_capacity(n);
        let mut height = 1;

        for column in 0..n {
            let mut col_fragments = Vec::new();
            for tokens in &row[column] {
                col_fragments.extend(split_tokens(
                    tokens,
                    widths[column],
                    &overflows[column],
                ));
            }
            height = height.max(col_fragments.len());
            fragments.push(col_fragments);
        }

        for line_index in 0..height {
            if compact {
                for column in 0..n {
                    if column > 0 {
                        output.push_str("  ");
                    }
                    let content = fragments[column]
                        .get(line_index)
                        .map(String::as_str)
                        .unwrap_or("");
                    let (left, right) = padding(
                        widths[column],
                        content.chars().count(),
                        &alignments[column],
                    );
                    output.extend(std::iter::repeat_n(' ', left));
                    output.push_str(content);
                    if column + 1 != n {
                        output.extend(std::iter::repeat_n(' ', right));
                    }
                }
                output.push('\n');
            } else {
                output.push('|');
                for column in 0..n {
                    let content = fragments[column]
                        .get(line_index)
                        .map(String::as_str)
                        .unwrap_or("");
                    let (left, right) = padding(
                        widths[column],
                        content.chars().count(),
                        &alignments[column],
                    );
                    output.push(' ');
                    output.extend(std::iter::repeat_n(' ', left));
                    output.push_str(content);
                    output.extend(std::iter::repeat_n(' ', right));
                    output.push(' ');
                    output.push('|');
                }
                output.push('\n');
            }
        }

        if markdown && row_index == 0 {
            output.push('|');
            for column in 0..n {
                let width = widths[column];
                output.push(' ');
                match alignments[column] {
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
        } else if matches!(options.mode, Mode::Grid)
            && options.header
            && row_index == 0
            && rows.len() > 1
        {
            emit_border(&mut output);
        }
    }

    if matches!(options.mode, Mode::Grid) {
        emit_border(&mut output);
    }

    Ok(output)
}

fn split_tokens(tokens: &[Token], width: usize, overflow: &Overflow) -> Vec<String> {
    if tokens.is_empty() {
        return vec![String::new()];
    }

    let total: usize = tokens.iter().map(|token| token.width).sum();
    if matches!(overflow, Overflow::Clip) {
        if total <= width {
            return vec![tokens.iter().map(|token| token.text.as_str()).collect()];
        }

        let mut result = String::new();
        let mut used = 0;
        for token in tokens {
            if used + token.width > width.saturating_sub(1) {
                break;
            }
            result.push_str(&token.text);
            used += token.width;
        }
        result.push('…');
        return vec![result];
    }

    let mut result = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let mut fragment = String::new();
        let mut used = 0;
        while index < tokens.len() && used + tokens[index].width <= width {
            fragment.push_str(&tokens[index].text);
            used += tokens[index].width;
            index += 1;
        }
        result.push(fragment);
    }
    result
}

fn padding(width: usize, content_width: usize, alignment: &Alignment) -> (usize, usize) {
    let extra = width.saturating_sub(content_width);
    match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    }
}

fn main() {}