// Deliberate fault: Accepts printable Unicode in the ASCII-only level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableError {
    EmptyRow { row: usize },
    Ragged { row: usize, expected: usize, actual: usize },
    InvalidChar { row: usize, column: usize, byte: usize },
    TooWide { column: usize, width: usize },
}
pub fn render(rows: &[Vec<String>]) -> Result<String, TableError> {
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
    for (r, row) in rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            for (b, ch) in cell.char_indices() {
                if ch.is_control() {
                    return Err(TableError::InvalidChar { row: r, column: c, byte: b });
                }
            }
        }
    }
    let cells: Vec<Vec<String>> = rows.iter().map(|row| row.iter()
        .map(|cell| cell.trim_end_matches(' ').to_owned()).collect()).collect();
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
            let padded = format!("{}{}", text, " ".repeat(widths[c] - text.len()));
            out.push_str(&format!(" {} |", padded));
        }
        out.push('\n');
    }
    out.push_str(&border);
    Ok(out)
}
