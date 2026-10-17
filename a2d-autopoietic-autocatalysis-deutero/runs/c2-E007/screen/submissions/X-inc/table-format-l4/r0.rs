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

type CellLines = Vec<Vec<Token>>;

fn validate_cell(cell: &str, row: usize, column: usize) -> Result<(), TableError> {
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
    Ok(())
}

fn transform_cell(cell: &str, markdown: bool) -> CellLines {
    let normalized = cell.replace("\r\n", "\n");
    normalized
        .split('\n')
        .map(|line| {
            let trimmed = line.trim_end_matches(' ');
            let mut tokens = Vec::new();
            for ch in trimmed.chars() {
                if markdown && ch == '|' {
                    tokens.push(Token { text: "\\|".to_owned(), width: 2 });
                } else if markdown && ch == '\\' {
                    tokens.push(Token { text: "\\\\".to_owned(), width: 2 });
                } else {
                    tokens.push(Token { text: ch.to_string(), width: 1 });
                }
            }
            tokens
        })
        .collect()
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
            let mut used = 0;
            let mut out = String::new();
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
            let mut start = 0;
            while start < tokens.len() {
                let mut end = start;
                let mut used = 0;
                while end < tokens.len() && used + tokens[end].width <= width {
                    used += tokens[end].width;
                    end += 1;
                }
                let mut out = String::new();
                for token in &tokens[start..end] {
                    out.push_str(&token.text);
                }
                result.push(out);
                start = end;
            }
            result
        }
    }
}

fn scalar_len(s: &str) -> usize {
    s.chars().count()
}

fn align_text(text: &str, width: usize, alignment: &Alignment, omit_right: bool) -> String {
    let len = scalar_len(text);
    let extra = width.saturating_sub(len);
    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };
    let mut result = String::new();
    result.push_str(&" ".repeat(left));
    result.push_str(text);
    if !omit_right {
        result.push_str(&" ".repeat(right));
    }
    result
}

fn data_line(
    pieces: &[String],
    widths: &[usize],
    alignments: &[Alignment],
    mode: &Mode,
) -> String {
    match mode {
        Mode::Compact => {
            let mut out = String::new();
            for col in 0..pieces.len() {
                if col > 0 {
                    out.push_str("  ");
                }
                out.push_str(&align_text(
                    &pieces[col],
                    widths[col],
                    &alignments[col],
                    col + 1 == pieces.len(),
                ));
            }
            out.push('\n');
            out
        }
        Mode::Grid | Mode::Markdown => {
            let mut out = String::from("|");
            for col in 0..pieces.len() {
                out.push(' ');
                out.push_str(&align_text(&pieces[col], widths[col], &alignments[col], false));
                out.push_str(" |");
            }
            out.push('\n');
            out
        }
    }
}

fn border(widths: &[usize]) -> String {
    let mut out = String::from("+");
    for width in widths {
        out.push_str(&"-".repeat(width + 2));
        out.push('+');
    }
    out.push('\n');
    out
}

fn markdown_marker(widths: &[usize], alignments: &[Alignment]) -> String {
    let mut out = String::from("|");
    for (width, alignment) in widths.iter().zip(alignments) {
        out.push(' ');
        match alignment {
            Alignment::Left => {
                out.push(':');
                out.push_str(&"-".repeat(width - 1));
            }
            Alignment::Right => {
                out.push_str(&"-".repeat(width - 1));
                out.push(':');
            }
            Alignment::Center => {
                out.push(':');
                out.push_str(&"-".repeat(width - 2));
                out.push(':');
            }
        }
        out.push_str(" |");
    }
    out.push('\n');
    out
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
    let overhead = match options.mode {
        Mode::Compact => 2 * (n - 1),
        Mode::Grid | Mode::Markdown => 3 * n + 1,
    };
    let minimum = overhead + n * minimum_width;
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

    let transformed: Vec<Vec<CellLines>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| transform_cell(cell, matches!(options.mode, Mode::Markdown)))
                .collect()
        })
        .collect();

    let mut widths = vec![minimum_width; n];
    for row in &transformed {
        for (column, lines) in row.iter().enumerate() {
            for line in lines {
                widths[column] = widths[column].max(token_width(line));
            }
        }
    }

    if options.max_width != 0 {
        let mut total: usize = widths.iter().sum::<usize>() + overhead;
        while total > options.max_width {
            let candidate = (0..n)
                .filter(|&col| widths[col] > minimum_width)
                .max_by_key(|&col| (widths[col], col));
            let Some(col) = candidate else { break };
            widths[col] -= 1;
            total -= 1;
        }
    }

    let mut output = String::new();
    if matches!(options.mode, Mode::Grid) {
        output.push_str(&border(&widths));
    }

    for (row_index, row) in transformed.iter().enumerate() {
        let cell_fragments: Vec<Vec<String>> = row
            .iter()
            .enumerate()
            .map(|(column, lines)| {
                let mut all = Vec::new();
                for line in lines {
                    all.extend(fragments(line, widths[column], &overflows[column]));
                }
                all
            })
            .collect();
        let height = cell_fragments.iter().map(Vec::len).max().unwrap_or(1);
        for fragment_index in 0..height {
            let pieces: Vec<String> = cell_fragments
                .iter()
                .map(|frags| frags.get(fragment_index).cloned().unwrap_or_default())
                .collect();
            output.push_str(&data_line(&pieces, &widths, &alignments, &options.mode));
        }

        if matches!(options.mode, Mode::Grid) {
            if options.header && row_index == 0 && row_index + 1 < rows.len() {
                output.push_str(&border(&widths));
            }
        } else if matches!(options.mode, Mode::Markdown) && row_index == 0 {
            output.push_str(&markdown_marker(&widths, &alignments));
        }
    }

    if matches!(options.mode, Mode::Grid) {
        output.push_str(&border(&widths));
    }
    Ok(output)
}