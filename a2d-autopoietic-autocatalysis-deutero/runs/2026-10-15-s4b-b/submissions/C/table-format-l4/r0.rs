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

fn invalid_char(s: &str) -> Option<usize> {
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (byte, ch) = chars[i];
        if ch == '\r' {
            if i + 1 < chars.len() && chars[i + 1].1 == '\n' {
                i += 2;
                continue;
            }
            return Some(byte);
        }
        if (ch.is_control() && ch != '\n') || ch == '\u{2028}' || ch == '\u{2029}' {
            return Some(byte);
        }
        i += 1;
    }
    None
}

fn normalized_lines(s: &str, markdown: bool) -> Vec<Vec<Token>> {
    let normalized = s.replace("\r\n", "\n");
    normalized
        .split('\n')
        .map(|line| {
            let trimmed = line.trim_end_matches(' ');
            let mut tokens = Vec::new();
            let chars: Vec<char> = trimmed.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                let c = chars[i];
                if markdown && c == '|' {
                    tokens.push(Token { text: "\\|".to_string(), width: 2 });
                } else if markdown && c == '\\' {
                    tokens.push(Token { text: "\\\\".to_string(), width: 2 });
                } else {
                    tokens.push(Token { text: c.to_string(), width: 1 });
                }
                i += 1;
            }
            tokens
        })
        .collect()
}

fn token_width(tokens: &[Token]) -> usize {
    tokens.iter().map(|t| t.width).sum()
}

fn aligned(content: &str, content_width: usize, width: usize, alignment: &Alignment, omit_right: bool) -> String {
    let extra = width.saturating_sub(content_width);
    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };
    let right = if omit_right { 0 } else { right };
    format!("{}{}{}", " ".repeat(left), content, " ".repeat(right))
}

fn fragments(tokens: &[Token], width: usize, rule: &Overflow) -> Vec<(String, usize)> {
    if tokens.is_empty() {
        return vec![(String::new(), 0)];
    }
    let total = token_width(tokens);
    if total <= width {
        return vec![(tokens.iter().map(|t| t.text.as_str()).collect(), total)];
    }
    match rule {
        Overflow::Clip => {
            let available = width.saturating_sub(1);
            let mut used = 0;
            let mut text = String::new();
            for token in tokens {
                if used + token.width > available {
                    break;
                }
                used += token.width;
                text.push_str(&token.text);
            }
            text.push('…');
            vec![(text, used + 1)]
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
                let text = tokens[start..end].iter().map(|t| t.text.as_str()).collect();
                result.push((text, used));
                start = end;
            }
            result
        }
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

    let (overhead, minimum_width) = if options.mode == Mode::Compact {
        (2usize.saturating_mul(n.saturating_sub(1)), 1usize)
    } else {
        (3usize.saturating_mul(n).saturating_add(1), if options.mode == Mode::Markdown { 3 } else { 1 })
    };
    let minimum = overhead.saturating_add(n.saturating_mul(minimum_width));
    if options.max_width != 0 && options.max_width < minimum {
        return Err(TableError::TooNarrow { minimum, provided: options.max_width });
    }

    for (ri, row) in rows.iter().enumerate() {
        for (ci, cell) in row.iter().enumerate() {
            if let Some(byte) = invalid_char(cell) {
                return Err(TableError::InvalidChar { row: ri, column: ci, byte });
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

    let markdown = options.mode == Mode::Markdown;
    let mut cell_lines: Vec<Vec<Vec<Vec<Token>>>> = Vec::with_capacity(rows.len());
    let mut widths = vec![minimum_width; n];
    for row in rows {
        let mut out_row = Vec::with_capacity(n);
        for (ci, cell) in row.iter().enumerate() {
            let lines = normalized_lines(cell, markdown);
            for line in &lines {
                widths[ci] = widths[ci].max(token_width(line));
            }
            out_row.push(lines);
        }
        cell_lines.push(out_row);
    }

    if options.max_width != 0 {
        let mut total = overhead.saturating_add(widths.iter().sum::<usize>());
        while total > options.max_width {
            let candidate = (0..n)
                .filter(|&i| widths[i] > minimum_width)
                .max_by_key(|&i| (widths[i], i));
            if let Some(i) = candidate {
                widths[i] -= 1;
                total -= 1;
            } else {
                break;
            }
        }
    }

    let mut out = String::new();

    let emit_border = |out: &mut String| {
        out.push('+');
        for &w in &widths {
            out.push_str(&"-".repeat(w + 2));
            out.push('+');
        }
        out.push('\n');
    };

    if options.mode == Mode::Grid {
        emit_border(&mut out);
    }

    for (ri, row) in cell_lines.iter().enumerate() {
        let mut rendered_cells: Vec<Vec<(String, usize)>> = Vec::with_capacity(n);
        let mut height = 1usize;
        for ci in 0..n {
            let mut cell_fragments = Vec::new();
            for line in &row[ci] {
                cell_fragments.extend(fragments(line, widths[ci], &overflows[ci]));
            }
            height = height.max(cell_fragments.len());
            rendered_cells.push(cell_fragments);
        }

        for fi in 0..height {
            if options.mode != Mode::Compact {
                out.push('|');
            }
            for ci in 0..n {
                let (content, content_width) = rendered_cells[ci]
                    .get(fi)
                    .cloned()
                    .unwrap_or_else(|| (String::new(), 0));
                if options.mode != Mode::Compact {
                    out.push(' ');
                }
                let omit_right = options.mode == Mode::Compact && ci + 1 == n;
                out.push_str(&aligned(
                    &content,
                    content_width,
                    widths[ci],
                    &alignments[ci],
                    omit_right,
                ));
                if options.mode != Mode::Compact {
                    out.push(' ');
                    out.push('|');
                } else if ci + 1 < n {
                    out.push_str("  ");
                }
            }
            out.push('\n');
        }

        if options.mode == Mode::Markdown && ri == 0 {
            out.push('|');
            for ci in 0..n {
                out.push(' ');
                match alignments[ci] {
                    Alignment::Left => {
                        out.push(':');
                        out.push_str(&"-".repeat(widths[ci] - 1));
                    }
                    Alignment::Right => {
                        out.push_str(&"-".repeat(widths[ci] - 1));
                        out.push(':');
                    }
                    Alignment::Center => {
                        out.push(':');
                        out.push_str(&"-".repeat(widths[ci] - 2));
                        out.push(':');
                    }
                }
                out.push(' ');
                out.push('|');
            }
            out.push('\n');
        }

        if options.mode == Mode::Grid && options.header && ri == 0 && rows.len() > 1 {
            emit_border(&mut out);
        }
    }

    if options.mode == Mode::Grid {
        emit_border(&mut out);
    }

    Ok(out)
}

fn main() {}