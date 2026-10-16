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

fn invalid_scalar(c: char) -> bool {
    c == '\r' || c == '\u{2028}' || c == '\u{2029}' || (c.is_control() && c != '\n')
}

fn logical_lines(cell: &str, markdown: bool) -> Vec<Vec<Token>> {
    let normalized = cell.replace("\r\n", "\n");
    normalized
        .split('\n')
        .map(|line| {
            let trimmed = line.trim_end_matches(' ');
            let mut tokens = Vec::new();
            for c in trimmed.chars() {
                if markdown && c == '|' {
                    tokens.push(Token { text: "\\|".to_string(), width: 2 });
                } else if markdown && c == '\\' {
                    tokens.push(Token { text: "\\\\".to_string(), width: 2 });
                } else {
                    tokens.push(Token { text: c.to_string(), width: 1 });
                }
            }
            tokens
        })
        .collect()
}

fn token_width(tokens: &[Token]) -> usize {
    tokens.iter().map(|t| t.width).sum()
}

fn fragments(tokens: &[Token], width: usize, overflow: &Overflow) -> Vec<Vec<Token>> {
    if tokens.is_empty() {
        return vec![Vec::new()];
    }
    match overflow {
        Overflow::Clip => {
            let total = token_width(tokens);
            if total <= width {
                return vec![tokens.to_vec()];
            }
            let mut out = Vec::new();
            let mut used = 0;
            for t in tokens {
                if used + t.width > width.saturating_sub(1) {
                    break;
                }
                used += t.width;
                out.push(t.clone());
            }
            out.push(Token { text: "…".to_string(), width: 1 });
            vec![out]
        }
        Overflow::Wrap => {
            let mut result = Vec::new();
            let mut start = 0;
            while start < tokens.len() {
                let mut used = 0;
                let mut end = start;
                while end < tokens.len() && used + tokens[end].width <= width {
                    used += tokens[end].width;
                    end += 1;
                }
                if end == start {
                    // All valid widths are sufficient for every token.
                    end += 1;
                }
                result.push(tokens[start..end].to_vec());
                start = end;
            }
            result
        }
    }
}

fn padded(fragment: &[Token], width: usize, alignment: &Alignment, omit_right: bool) -> String {
    let content: String = fragment.iter().map(|t| t.text.as_str()).collect();
    let extra = width.saturating_sub(token_width(fragment));
    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };
    format!(
        "{}{}{}",
        " ".repeat(left),
        content,
        if omit_right { String::new() } else { " ".repeat(right) }
    )
}

fn border(widths: &[usize]) -> String {
    let mut out = String::from("+");
    for &w in widths {
        out.push_str(&"-".repeat(w + 2));
        out.push('+');
    }
    out.push('\n');
    out
}

fn markdown_marker(width: usize, alignment: &Alignment) -> String {
    match alignment {
        Alignment::Left => format!(":{} ", "-".repeat(width - 1)).trim_end().to_string(),
        Alignment::Right => format!("{}:", "-".repeat(width - 1)),
        Alignment::Center => format!(":{}:", "-".repeat(width - 2)),
    }
}

pub fn render(rows: &[Vec<String>], options: &Options) -> Result<String, TableError> {
    if rows.is_empty() {
        return Ok(String::new());
    }

    for (i, row) in rows.iter().enumerate() {
        if row.is_empty() {
            return Err(TableError::EmptyRow { row: i });
        }
    }

    let n = rows[0].len();
    for (i, row) in rows.iter().enumerate() {
        if row.len() != n {
            return Err(TableError::Ragged { row: i, expected: n, actual: row.len() });
        }
    }

    if !options.alignments.is_empty() && options.alignments.len() != n {
        return Err(TableError::AlignmentCount { expected: n, actual: options.alignments.len() });
    }
    if !options.overflow.is_empty() && options.overflow.len() != n {
        return Err(TableError::OverflowCount { expected: n, actual: options.overflow.len() });
    }
    if options.mode == Mode::Markdown && !options.header {
        return Err(TableError::MarkdownNeedsHeader);
    }

    let minimum_width = if options.mode == Mode::Markdown { 3 } else { 1 };
    let fixed_overhead = if options.mode == Mode::Compact { 2 * (n - 1) } else { 3 * n + 1 };
    let minimum = fixed_overhead + n * minimum_width;
    if options.max_width != 0 && options.max_width < minimum {
        return Err(TableError::TooNarrow { minimum, provided: options.max_width });
    }

    for (ri, row) in rows.iter().enumerate() {
        for (ci, cell) in row.iter().enumerate() {
            let mut byte = 0;
            let chars: Vec<char> = cell.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                let c = chars[i];
                if c == '\r' {
                    if i + 1 < chars.len() && chars[i + 1] == '\n' {
                        byte += 1;
                        i += 1;
                    } else {
                        return Err(TableError::InvalidChar { row: ri, column: ci, byte });
                    }
                } else if invalid_scalar(c) {
                    return Err(TableError::InvalidChar { row: ri, column: ci, byte });
                }
                byte += c.len_utf8();
                i += 1;
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

    let transformed: Vec<Vec<Vec<Vec<Token>>>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| logical_lines(cell, options.mode == Mode::Markdown))
                .collect()
        })
        .collect();

    let mut widths = vec![minimum_width; n];
    for row in &transformed {
        for (ci, lines) in row.iter().enumerate() {
            for line in lines {
                widths[ci] = widths[ci].max(token_width(line));
            }
        }
    }

    if options.max_width != 0 {
        let mut total: usize = widths.iter().sum::<usize>() + fixed_overhead;
        while total > options.max_width {
            let candidate = (0..n)
                .filter(|&i| widths[i] > minimum_width)
                .max_by_key(|&i| (widths[i], i));
            match candidate {
                Some(i) => {
                    widths[i] -= 1;
                    total -= 1;
                }
                None => break,
            }
        }
    }

    let mut output = String::new();
    if options.mode == Mode::Grid {
        output.push_str(&border(&widths));
    }

    for (ri, row) in transformed.iter().enumerate() {
        let cell_fragments: Vec<Vec<Vec<Token>>> = row
            .iter()
            .enumerate()
            .map(|(ci, lines)| {
                let mut all = Vec::new();
                for line in lines {
                    all.extend(fragments(line, widths[ci], &overflows[ci]));
                }
                all
            })
            .collect();
        let height = cell_fragments.iter().map(Vec::len).max().unwrap_or(1);

        for fi in 0..height {
            match options.mode {
                Mode::Grid | Mode::Markdown => {
                    output.push('|');
                    for ci in 0..n {
                        let empty = Vec::new();
                        let frag = cell_fragments[ci].get(fi).unwrap_or(&empty);
                        output.push(' ');
                        output.push_str(&padded(frag, widths[ci], &alignments[ci], false));
                        output.push(' ');
                        output.push('|');
                    }
                    output.push('\n');
                }
                Mode::Compact => {
                    for ci in 0..n {
                        if ci > 0 {
                            output.push_str("  ");
                        }
                        let empty = Vec::new();
                        let frag = cell_fragments[ci].get(fi).unwrap_or(&empty);
                        output.push_str(&padded(frag, widths[ci], &alignments[ci], ci + 1 == n));
                    }
                    output.push('\n');
                }
            }
        }

        if options.mode == Mode::Grid && options.header && ri == 0 && rows.len() > 1 {
            output.push_str(&border(&widths));
        }
        if options.mode == Mode::Markdown && ri == 0 {
            output.push('|');
            for ci in 0..n {
                output.push(' ');
                let marker = markdown_marker(widths[ci], &alignments[ci]);
                output.push_str(&marker);
                output.push(' ');
                output.push('|');
            }
            output.push('\n');
        }
    }

    if options.mode == Mode::Grid {
        output.push_str(&border(&widths));
    }
    Ok(output)
}