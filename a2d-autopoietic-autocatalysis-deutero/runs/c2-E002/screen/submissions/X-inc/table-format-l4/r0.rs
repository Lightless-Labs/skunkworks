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
type TableContent = Vec<Vec<CellLines>>;

pub fn render(rows: &[Vec<String>], options: &Options) -> Result<String, TableError> {
    if rows.is_empty() {
        return Ok(String::new());
    }

    // Check every row for emptiness before checking any row's width.
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

    let min_width = if options.mode == Mode::Markdown { 3 } else { 1 };
    let overhead = if options.mode == Mode::Compact {
        2 * (n - 1)
    } else {
        3 * n + 1
    };
    let minimum = overhead + n * min_width;

    if options.max_width != 0 && options.max_width < minimum {
        return Err(TableError::TooNarrow {
            minimum,
            provided: options.max_width,
        });
    }

    // Validate original input and report byte offsets before normalizing it.
    for (row_index, row) in rows.iter().enumerate() {
        for (column_index, cell) in row.iter().enumerate() {
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

    let mut content: TableContent = Vec::with_capacity(rows.len());
    let mut widths = vec![min_width; n];

    for row in rows {
        let mut output_row = Vec::with_capacity(n);
        for (column, cell) in row.iter().enumerate() {
            let normalized = cell.replace("\r\n", "\n");
            let mut lines = Vec::new();

            for raw_line in normalized.split('\n') {
                let trimmed = raw_line.trim_end_matches(' ');
                let mut tokens = Vec::new();
                let mut line_width = 0usize;

                for ch in trimmed.chars() {
                    let token_text = if options.mode == Mode::Markdown {
                        match ch {
                            '|' => "\\|".to_owned(),
                            '\\' => "\\\\".to_owned(),
                            _ => ch.to_string(),
                        }
                    } else {
                        ch.to_string()
                    };
                    let token_width = token_text.chars().count();
                    line_width += token_width;
                    tokens.push(Token {
                        text: token_text,
                        width: token_width,
                    });
                }

                widths[column] = widths[column].max(line_width);
                lines.push(tokens);
            }
            output_row.push(lines);
        }
        content.push(output_row);
    }

    if options.max_width != 0 {
        while widths.iter().sum::<usize>() + overhead > options.max_width {
            let mut candidate: Option<usize> = None;
            for column in 0..n {
                if widths[column] > min_width
                    && candidate.map_or(true, |current| widths[column] >= widths[current])
                {
                    candidate = Some(column);
                }
            }
            if let Some(column) = candidate {
                widths[column] -= 1;
            } else {
                break;
            }
        }
    }

    let mut out = String::new();

    if options.mode == Mode::Grid {
        push_border(&mut out, &widths);
    }

    for (row_index, row) in content.iter().enumerate() {
        // Each cell's logical lines are laid out independently. The fragments
        // from a line follow one another, so later logical lines cannot be
        // dropped when an earlier line has only one fragment.
        let mut cell_fragments: Vec<Vec<String>> = Vec::with_capacity(n);
        let mut row_height = 1usize;

        for column in 0..n {
            let mut fragments = Vec::new();
            for line in &row[column] {
                let line_fragments = split_line(line, widths[column], &overflows[column]);
                fragments.extend(line_fragments);
            }
            row_height = row_height.max(fragments.len());
            cell_fragments.push(fragments);
        }

        for fragment_index in 0..row_height {
            match options.mode {
                Mode::Compact => {
                    for column in 0..n {
                        if column != 0 {
                            out.push_str("  ");
                        }
                        let fragment = cell_fragments[column]
                            .get(fragment_index)
                            .map(String::as_str)
                            .unwrap_or("");
                        let (left, right) =
                            padding(fragment.chars().count(), widths[column], &alignments[column]);
                        out.push_str(&" ".repeat(left));
                        out.push_str(fragment);
                        if column + 1 != n {
                            out.push_str(&" ".repeat(right));
                        }
                    }
                    out.push('\n');
                }
                Mode::Grid | Mode::Markdown => {
                    out.push('|');
                    for column in 0..n {
                        let fragment = cell_fragments[column]
                            .get(fragment_index)
                            .map(String::as_str)
                            .unwrap_or("");
                        let (left, right) =
                            padding(fragment.chars().count(), widths[column], &alignments[column]);
                        out.push(' ');
                        out.push_str(&" ".repeat(left));
                        out.push_str(fragment);
                        out.push_str(&" ".repeat(right));
                        out.push_str(" |");
                    }
                    out.push('\n');
                }
            }
        }

        if options.mode == Mode::Grid
            && options.header
            && row_index == 0
            && row_index + 1 < rows.len()
        {
            push_border(&mut out, &widths);
        }
        if options.mode == Mode::Markdown && row_index == 0 {
            push_markers(&mut out, &widths, &alignments);
        }
    }

    if options.mode == Mode::Grid {
        push_border(&mut out, &widths);
    }

    Ok(out)
}

fn split_line(tokens: &[Token], width: usize, overflow: &Overflow) -> Vec<String> {
    if tokens.is_empty() {
        return vec![String::new()];
    }

    let total: usize = tokens.iter().map(|token| token.width).sum();
    if total <= width {
        return vec![join_tokens(tokens)];
    }

    match overflow {
        Overflow::Clip => {
            let mut fragment = String::new();
            let mut used = 0usize;
            let limit = width.saturating_sub(1);

            for token in tokens {
                if used + token.width > limit {
                    break;
                }
                fragment.push_str(&token.text);
                used += token.width;
            }

            fragment.push('…');
            vec![fragment]
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
                result.push(join_tokens(&tokens[start..end]));
                start = end;
            }
            result
        }
    }
}

fn join_tokens(tokens: &[Token]) -> String {
    let capacity: usize = tokens.iter().map(|token| token.text.len()).sum();
    let mut result = String::with_capacity(capacity);
    for token in tokens {
        result.push_str(&token.text);
    }
    result
}

fn padding(content_width: usize, width: usize, alignment: &Alignment) -> (usize, usize) {
    let extra = width.saturating_sub(content_width);
    match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    }
}

fn push_border(out: &mut String, widths: &[usize]) {
    out.push('+');
    for width in widths {
        out.push_str(&"-".repeat(width + 2));
        out.push('+');
    }
    out.push('\n');
}

fn push_markers(out: &mut String, widths: &[usize], alignments: &[Alignment]) {
    out.push('|');
    for (width, alignment) in widths.iter().zip(alignments) {
        let marker = match alignment {
            Alignment::Left => format!(":{}", "-".repeat(width - 1)),
            Alignment::Right => format!("{}:", "-".repeat(width - 1)),
            Alignment::Center => format!(":{}:", "-".repeat(width - 2)),
        };
        out.push(' ');
        out.push_str(&marker);
        out.push_str(" |");
    }
    out.push('\n');
}