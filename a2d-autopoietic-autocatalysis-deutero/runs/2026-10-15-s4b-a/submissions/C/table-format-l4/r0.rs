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

fn invalid_byte(s: &str) -> Option<usize> {
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
        if ch.is_control() && ch != '\n' || ch == '\u{2028}' || ch == '\u{2029}' {
            return Some(byte);
        }
        i += 1;
    }
    None
}

fn logical_lines(s: &str) -> Vec<String> {
    let normalized = s.replace("\r\n", "\n");
    normalized
        .split('\n')
        .map(|line| line.trim_end_matches(' ').to_owned())
        .collect()
}

fn tokens(line: &str, markdown: bool) -> Vec<Token> {
    let mut out = Vec::new();
    for ch in line.chars() {
        let text = if markdown && ch == '|' {
            "\\|".to_owned()
        } else if markdown && ch == '\\' {
            "\\\\".to_owned()
        } else {
            ch.to_string()
        };
        let width = text.chars().count();
        out.push(Token { text, width });
    }
    out
}

fn token_width(ts: &[Token]) -> usize {
    ts.iter().map(|t| t.width).sum()
}

fn fragments(ts: &[Token], width: usize, overflow: &Overflow) -> Vec<String> {
    if ts.is_empty() {
        return vec![String::new()];
    }
    match overflow {
        Overflow::Clip => {
            if token_width(ts) <= width {
                return vec![ts.iter().map(|t| t.text.as_str()).collect()];
            }
            let room = width.saturating_sub(1);
            let mut used = 0;
            let mut s = String::new();
            for t in ts {
                if used + t.width > room {
                    break;
                }
                used += t.width;
                s.push_str(&t.text);
            }
            s.push('…');
            vec![s]
        }
        Overflow::Wrap => {
            let mut result = Vec::new();
            let mut start = 0;
            while start < ts.len() {
                let mut end = start;
                let mut used = 0;
                while end < ts.len() && used + ts[end].width <= width {
                    used += ts[end].width;
                    end += 1;
                }
                // All tokens fit the minimum width; this fallback avoids stalling.
                if end == start {
                    end += 1;
                }
                result.push(ts[start..end].iter().map(|t| t.text.as_str()).collect());
                start = end;
            }
            result
        }
    }
}

fn align(content: &str, width: usize, alignment: &Alignment) -> String {
    let used = content.chars().count();
    let extra = width.saturating_sub(used);
    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };
    format!("{}{}{}", " ".repeat(left), content, " ".repeat(right))
}

fn append_data_line(
    out: &mut String,
    cells: &[String],
    widths: &[usize],
    aligns: &[Alignment],
    mode: &Mode,
) {
    match mode {
        Mode::Compact => {
            for i in 0..cells.len() {
                if i > 0 {
                    out.push_str("  ");
                }
                let mut field = align(&cells[i], widths[i], &aligns[i]);
                if i + 1 == cells.len() {
                    let right_padding = match aligns[i] {
                        Alignment::Left => widths[i].saturating_sub(cells[i].chars().count()),
                        Alignment::Center => {
                            let extra = widths[i].saturating_sub(cells[i].chars().count());
                            extra - extra / 2
                        }
                        Alignment::Right => 0,
                    };
                    for _ in 0..right_padding {
                        field.pop();
                    }
                }
                out.push_str(&field);
            }
            out.push('\n');
        }
        Mode::Grid | Mode::Markdown => {
            out.push('|');
            for i in 0..cells.len() {
                out.push(' ');
                out.push_str(&align(&cells[i], widths[i], &aligns[i]));
                out.push_str(" |");
            }
            out.push('\n');
        }
    }
}

fn border(widths: &[usize], out: &mut String) {
    out.push('+');
    for &w in widths {
        out.push_str(&"-".repeat(w + 2));
        out.push('+');
    }
    out.push('\n');
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

    let minimum_width = if options.mode == Mode::Markdown { 3 } else { 1 };
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

    for (r, row) in rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            if let Some(byte) = invalid_byte(cell) {
                return Err(TableError::InvalidChar {
                    row: r,
                    column: c,
                    byte,
                });
            }
        }
    }

    let aligns: Vec<Alignment> = if options.alignments.is_empty() {
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
    let lines: Vec<Vec<Vec<Vec<Token>>>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| {
                    logical_lines(cell)
                        .iter()
                        .map(|line| tokens(line, markdown))
                        .collect()
                })
                .collect()
        })
        .collect();

    let mut widths = vec![minimum_width; n];
    for row in &lines {
        for (c, cell_lines) in row.iter().enumerate() {
            for line in cell_lines {
                widths[c] = widths[c].max(token_width(line));
            }
        }
    }

    if options.max_width != 0 {
        while widths.iter().sum::<usize>() + overhead > options.max_width {
            let candidate = (0..n)
                .filter(|&c| widths[c] > minimum_width)
                .max_by_key(|&c| (widths[c], c));
            if let Some(c) = candidate {
                widths[c] -= 1;
            } else {
                break;
            }
        }
    }

    let mut out = String::new();
    if options.mode == Mode::Grid {
        border(&widths, &mut out);
    }

    for (r, row) in lines.iter().enumerate() {
        let mut cells_fragments: Vec<Vec<String>> = Vec::with_capacity(n);
        let mut height = 1;
        for c in 0..n {
            let mut cell_frags = Vec::new();
            for line in &row[c] {
                cell_frags.extend(fragments(line, widths[c], &overflows[c]));
            }
            height = height.max(cell_frags.len());
            cells_fragments.push(cell_frags);
        }

        for idx in 0..height {
            let cells: Vec<String> = (0..n)
                .map(|c| {
                    cells_fragments[c]
                        .get(idx)
                        .cloned()
                        .unwrap_or_default()
                })
                .collect();
            append_data_line(&mut out, &cells, &widths, &aligns, &options.mode);
        }

        if options.mode == Mode::Grid && options.header && r == 0 && rows.len() > 1 {
            border(&widths, &mut out);
        }
        if options.mode == Mode::Markdown && options.header && r == 0 {
            out.push('|');
            for c in 0..n {
                out.push(' ');
                let marker = match aligns[c] {
                    Alignment::Left => format!(":{}", "-".repeat(widths[c] - 1)),
                    Alignment::Right => format!("{}:", "-".repeat(widths[c] - 1)),
                    Alignment::Center => format!(":{}:", "-".repeat(widths[c] - 2)),
                };
                out.push_str(&marker);
                out.push_str(" |");
            }
            out.push('\n');
        }
    }

    if options.mode == Mode::Grid {
        border(&widths, &mut out);
    }
    Ok(out)
}

fn main() {}