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

fn escaped_line(source: &str, markdown: bool) -> Vec<(String, usize)> {
    source
        .chars()
        .map(|c| {
            if markdown && (c == '|' || c == '\\') {
                (format!("\\{c}"), 2)
            } else {
                (c.to_string(), 1)
            }
        })
        .collect()
}

fn fragments(
    tokens: &[(String, usize)],
    width: usize,
    rule: &Overflow,
) -> Vec<String> {
    if tokens.is_empty() {
        return vec![String::new()];
    }

    let mut result = Vec::new();
    let mut start = 0;
    while start < tokens.len() {
        let mut end = start;
        let mut used = 0;
        while end < tokens.len() && used + tokens[end].1 <= width {
            used += tokens[end].1;
            end += 1;
        }

        match rule {
            Overflow::Wrap => {
                result.push(tokens[start..end].iter().map(|x| x.0.as_str()).collect());
                start = end;
            }
            Overflow::Clip => {
                let remaining_width: usize = tokens[start..].iter().map(|x| x.1).sum();
                if remaining_width <= width {
                    result.push(tokens[start..].iter().map(|x| x.0.as_str()).collect());
                } else {
                    let cap = width.saturating_sub(1);
                    let mut prefix_end = start;
                    let mut prefix_width = 0;
                    while prefix_end < tokens.len()
                        && prefix_width + tokens[prefix_end].1 <= cap
                    {
                        prefix_width += tokens[prefix_end].1;
                        prefix_end += 1;
                    }
                    let mut clipped: String =
                        tokens[start..prefix_end].iter().map(|x| x.0.as_str()).collect();
                    clipped.push('…');
                    result.push(clipped);
                }
                break;
            }
        }
    }
    result
}

fn pad(text: &str, alignment: &Alignment, width: usize, last_compact: bool) -> String {
    let extra = width.saturating_sub(text.chars().count());
    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };
    let mut out = String::new();
    for _ in 0..left {
        out.push(' ');
    }
    out.push_str(text);
    if !last_compact {
        for _ in 0..right {
            out.push(' ');
        }
    }
    out
}

fn border(out: &mut String, widths: &[usize]) {
    out.push('+');
    for width in widths {
        for _ in 0..width + 2 {
            out.push('-');
        }
        out.push('+');
    }
    out.push('\n');
}

fn data_line(
    out: &mut String,
    texts: &[String],
    widths: &[usize],
    alignments: &[Alignment],
    mode: &Mode,
) {
    match mode {
        Mode::Grid | Mode::Markdown => {
            out.push('|');
            for i in 0..texts.len() {
                out.push(' ');
                out.push_str(&pad(&texts[i], &alignments[i], widths[i], false));
                out.push(' ');
                out.push('|');
            }
            out.push('\n');
        }
        Mode::Compact => {
            for i in 0..texts.len() {
                if i > 0 {
                    out.push_str("  ");
                }
                out.push_str(&pad(
                    &texts[i],
                    &alignments[i],
                    widths[i],
                    i + 1 == texts.len(),
                ));
            }
            out.push('\n');
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
            return Err(TableError::Ragged {
                row: i,
                expected: n,
                actual: row.len(),
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

    let compact = options.mode == Mode::Compact;
    let markdown = options.mode == Mode::Markdown;
    let floor = if markdown { 3 } else { 1 };
    let overhead = if compact { 2 * (n - 1) } else { 3 * n + 1 };
    let min_total = overhead + floor * n;
    if options.max_width != 0 && options.max_width < min_total {
        return Err(TableError::TooNarrow {
            minimum: min_total,
            provided: options.max_width,
        });
    }

    for (row_i, row) in rows.iter().enumerate() {
        for (col_i, cell) in row.iter().enumerate() {
            let mut iter = cell.char_indices().peekable();
            while let Some((byte, ch)) = iter.next() {
                if ch == '\r' {
                    if matches!(iter.peek(), Some((_, '\n'))) {
                        iter.next();
                    } else {
                        return Err(TableError::InvalidChar {
                            row: row_i,
                            column: col_i,
                            byte,
                        });
                    }
                } else if (ch.is_control() && ch != '\n') || ch == '\u{2028}' || ch == '\u{2029}' {
                    return Err(TableError::InvalidChar {
                        row: row_i,
                        column: col_i,
                        byte,
                    });
                }
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

    let mut prepared: Vec<Vec<Vec<Vec<(String, usize)>>>> = Vec::new();
    let mut widths = vec![floor; n];
    for row in rows {
        let mut prepared_row = Vec::new();
        for (col, cell) in row.iter().enumerate() {
            let normalized = cell.replace("\r\n", "\n");
            let mut lines = Vec::new();
            for line in normalized.split('\n') {
                let trimmed = line.trim_end_matches(' ');
                let tokens = escaped_line(trimmed, markdown);
                let total: usize = tokens.iter().map(|x| x.1).sum();
                widths[col] = widths[col].max(total);
                lines.push(tokens);
            }
            prepared_row.push(lines);
        }
        prepared.push(prepared_row);
    }

    if options.max_width != 0 {
        let mut used = overhead + widths.iter().sum::<usize>();
        while used > options.max_width {
            let mut chosen = None;
            for i in 0..n {
                if widths[i] > floor
                    && chosen.map_or(true, |j: usize| widths[i] >= widths[j])
                {
                    chosen = Some(i);
                }
            }
            if let Some(i) = chosen {
                widths[i] -= 1;
                used -= 1;
            } else {
                break;
            }
        }
    }

    let mut out = String::new();
    if options.mode == Mode::Grid {
        border(&mut out, &widths);
    }

    for (row_i, row) in prepared.iter().enumerate() {
        let mut per_cell = Vec::new();
        let mut height = 1;
        for (col, lines) in row.iter().enumerate() {
            let mut all = Vec::new();
            for line in lines {
                let parts = fragments(line, widths[col], &overflows[col]);
                height = height.max(all.len() + parts.len());
                all.extend(parts);
            }
            per_cell.push(all);
        }

        for index in 0..height {
            let texts: Vec<String> = per_cell
                .iter()
                .map(|column| column.get(index).cloned().unwrap_or_default())
                .collect();
            data_line(&mut out, &texts, &widths, &alignments, &options.mode);
        }

        if options.mode == Mode::Grid && options.header && row_i == 0 && rows.len() > 1 {
            border(&mut out, &widths);
        }
        if markdown && row_i == 0 {
            out.push('|');
            for col in 0..n {
                out.push(' ');
                match alignments[col] {
                    Alignment::Left => {
                        out.push(':');
                        for _ in 1..widths[col] {
                            out.push('-');
                        }
                    }
                    Alignment::Right => {
                        for _ in 1..widths[col] {
                            out.push('-');
                        }
                        out.push(':');
                    }
                    Alignment::Center => {
                        out.push(':');
                        for _ in 2..widths[col] {
                            out.push('-');
                        }
                        out.push(':');
                    }
                }
                out.push(' ');
                out.push('|');
            }
            out.push('\n');
        }
    }

    if options.mode == Mode::Grid {
        border(&mut out, &widths);
    }
    Ok(out)
}
