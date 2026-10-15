// Deliberate fault: Strips source leading spaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableError {
    EmptyRow { row: usize },
    Ragged { row: usize, expected: usize, actual: usize },
    InvalidChar { row: usize, column: usize, byte: usize },
    TooWide { column: usize, width: usize },
    AlignmentCount { expected: usize, actual: usize },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Alignment { Left, Right, Center }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub alignments: Vec<Alignment>,
    pub header: bool,
}
fn fitted(text: &str, width: usize, alignment: &Alignment) -> String {
    let pad = width - text.chars().count();
    let left = match alignment { Alignment::Left => 0, Alignment::Right => pad, Alignment::Center => pad / 2 };
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(pad - left))
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
    for (r, row) in rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            for (b, ch) in cell.char_indices() {
                if !(' '..='~').contains(&ch) {
                    return Err(TableError::InvalidChar { row: r, column: c, byte: b });
                }
            }
        }
    }
    let cells: Vec<Vec<String>> = rows.iter().map(|row| row.iter()
        .map(|cell| cell.trim_matches(' ').to_owned()).collect()).collect();
    let mut widths = vec![1; n];
    for row in &cells {
        for (c, cell) in row.iter().enumerate() {
            widths[c] = widths[c].max(cell.chars().count());
        }
    }
    for (c, &w) in widths.iter().enumerate() {
        if w > 32 { return Err(TableError::TooWide { column: c, width: w }); }
    }
    let border = format!("+{}\n", widths.iter().map(|w| format!("{}+", "-".repeat(w + 2))).collect::<String>());
    let mut out = border.clone();
    for (r, row) in cells.iter().enumerate() {
        out.push('|');
        for (c, cell) in row.iter().enumerate() {
            let text = cell.clone();
            let alignment = options.alignments.get(c).unwrap_or(&Alignment::Left);
            let padded = fitted(&text, widths[c], alignment);
            out.push_str(&format!(" {} |", padded));
        }
        out.push('\n');
        if options.header && r == 0 && cells.len() > 1 { out.push_str(&border); }
    }
    out.push_str(&border);
    Ok(out)
}
