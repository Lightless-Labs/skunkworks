// Deliberate fault: Trims source trailing spaces in Compact output.
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
pub enum Alignment { Left, Right, Center }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overflow { Clip, Wrap }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode { Grid, Compact, Markdown }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub alignments: Vec<Alignment>,
    pub header: bool,
    pub max_width: usize,
    pub overflow: Vec<Overflow>,
    pub mode: Mode,
}
fn fitted(text: &str, width: usize, alignment: &Alignment) -> String {
    let pad = width - text.chars().count();
    let left = match alignment { Alignment::Left => 0, Alignment::Right => pad, Alignment::Center => pad / 2 };
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(pad - left))
}
fn tokens(line: &str, markdown: bool) -> Vec<String> {
    line.chars().map(|ch| {
        if markdown && (ch == '|' || ch == '\\') { format!("\\{}", ch) }
        else { ch.to_string() }
    }).collect()
}
fn fragments(parts: &[String], width: usize, overflow: &Overflow) -> Vec<String> {
    if parts.is_empty() { return vec![String::new()]; }
    let total: usize = parts.iter().map(|x| x.chars().count()).sum();
    if *overflow == Overflow::Clip {
        if total <= width { return vec![parts.concat()]; }
        let mut out = String::new();
        let mut used = 0;
        for part in parts {
            let len = part.chars().count();
            if used + len > width - 1 { break; }
            out.push_str(part);
            used += len;
        }
        out.push('…');
        return vec![out];
    }
    let mut out = Vec::new();
    let mut current = String::new();
    let mut used = 0;
    for part in parts {
        let len = part.chars().count();
        if used + len > width {
            out.push(current);
            current = String::new();
            used = 0;
        }
        current.push_str(part);
        used += len;
    }
    out.push(current);
    out
}
pub fn render(rows: &[Vec<String>], options: &Options) -> Result<String, TableError> {
    if rows.is_empty() { return Ok(String::new()); }
    for (r, row) in rows.iter().enumerate() {
        if row.is_empty() { return Err(TableError::EmptyRow { row: r }); }
    }
    let n = rows[0].len();
    for (r, row) in rows.iter().enumerate() {
        if row.len() != n {
            return Err(TableError::Ragged { row: r, expected: n, actual: row.len() });
        }
    }
    if !options.alignments.is_empty() && options.alignments.len() != n {
        return Err(TableError::AlignmentCount { expected: n, actual: options.alignments.len() });
    }
    if !options.overflow.is_empty() && options.overflow.len() != n {
        return Err(TableError::OverflowCount { expected: n, actual: options.overflow.len() });
    }
    let markdown = options.mode == Mode::Markdown;
    if markdown && !options.header { return Err(TableError::MarkdownNeedsHeader); }
    let overhead = if options.mode == Mode::Compact { 2 * (n - 1) } else { 3 * n + 1 };
    let minimum_cell = if markdown { 3 } else { 1 };
    let minimum = overhead + n * minimum_cell;
    if options.max_width != 0 && options.max_width < minimum {
        return Err(TableError::TooNarrow { minimum, provided: options.max_width });
    }
    for (r, row) in rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            let mut chars = cell.char_indices().peekable();
            while let Some((b, ch)) = chars.next() {
                if ch == '\r' && chars.peek().map(|(_, x)| *x) == Some('\n') {
                    chars.next();
                } else if ch != '\n' && (ch.is_control() || ch == '\u{2028}' || ch == '\u{2029}') {
                    return Err(TableError::InvalidChar { row: r, column: c, byte: b });
                }
            }
        }
    }
    let cells: Vec<Vec<Vec<Vec<String>>>> = rows.iter().map(|row| row.iter().map(|cell| {
        cell.replace("\r\n", "\n").split('\n')
            .map(|line| tokens(line.trim_end_matches(' '), markdown)).collect()
    }).collect()).collect();
    let mut widths = vec![minimum_cell; n];
    for row in &cells {
        for (c, lines) in row.iter().enumerate() {
            for line in lines {
                let len = line.iter().map(|token| token.chars().count()).sum();
                widths[c] = widths[c].max(len);
            }
        }
    }
    if options.max_width != 0 {
        while widths.iter().sum::<usize>() + overhead > options.max_width {
            let c = (0..n).filter(|&c| widths[c] > minimum_cell)
                .max_by_key(|&c| (widths[c], c)).unwrap();
            widths[c] -= 1;
        }
    }
    let border = format!("+{}\n", widths.iter().map(|w| format!("{}+", "-".repeat(w + 2))).collect::<String>());
    let mut out = String::new();
    if options.mode == Mode::Grid { out.push_str(&border); }
    for (r, row) in cells.iter().enumerate() {
        let rendered: Vec<Vec<String>> = row.iter().enumerate().map(|(c, lines)| {
            let overflow = options.overflow.get(c).unwrap_or(&Overflow::Wrap);
            lines.iter().flat_map(|line| fragments(line, widths[c], overflow)).collect()
        }).collect();
        let height = rendered.iter().map(Vec::len).max().unwrap();
        for k in 0..height {
            if options.mode != Mode::Compact { out.push('|'); }
            for c in 0..n {
                let text = rendered[c].get(k).map(String::as_str).unwrap_or("");
                let alignment = options.alignments.get(c).unwrap_or(&Alignment::Left);
                let padded = fitted(text, widths[c], alignment);
                if options.mode == Mode::Compact {
                    if c > 0 { out.push_str("  "); }
                    if c == n - 1 {
                        // Drop only padding to the right, not source spaces in a wrapped fragment.
                        let right = widths[c] - text.chars().count();
                        let left = match alignment { Alignment::Left => 0, Alignment::Right => right, Alignment::Center => right / 2 };
                        out.push_str(padded[..padded.len() - (right - left)].trim_end_matches(' '));
                    } else { out.push_str(&padded); }
                } else { out.push_str(&format!(" {} |", padded)); }
            }
            out.push('\n');
        }
        if r == 0 && options.header {
            match options.mode {
                Mode::Grid if cells.len() > 1 => out.push_str(&border),
                Mode::Markdown => {
                    out.push('|');
                    for c in 0..n {
                        let w = widths[c];
                        let alignment = options.alignments.get(c).unwrap_or(&Alignment::Left);
                        let marker = match alignment {
                            Alignment::Left => format!(":{}", "-".repeat(w - 1)),
                            Alignment::Right => format!("{}:", "-".repeat(w - 1)),
                            Alignment::Center => format!(":{}:", "-".repeat(w - 2)),
                        };
                        out.push_str(&format!(" {} |", marker));
                    }
                    out.push('\n');
                }
                _ => {}
            }
        }
    }
    if options.mode == Mode::Grid { out.push_str(&border); }
    Ok(out)
}
