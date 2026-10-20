#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableError {
    EmptyRow { row: usize },
    Ragged { row: usize, expected: usize, actual: usize },
    InvalidChar { row: usize, column: usize, byte: usize },
    TooWide { column: usize, width: usize },
}

pub fn render(rows: &[Vec<String>]) -> Result<String, TableError> {
    if rows.is_empty() {
        return Ok(String::new());
    }

    for (row, cells) in rows.iter().enumerate() {
        if cells.is_empty() {
            return Err(TableError::EmptyRow { row });
        }
    }

    let expected = rows[0].len();
    for (row, cells) in rows.iter().enumerate() {
        if cells.len() != expected {
            return Err(TableError::Ragged {
                row,
                expected,
                actual: cells.len(),
            });
        }
    }

    for (row, cells) in rows.iter().enumerate() {
        for (column, cell) in cells.iter().enumerate() {
            for (byte, ch) in cell.char_indices() {
                if !(' '..='~').contains(&ch) {
                    return Err(TableError::InvalidChar { row, column, byte });
                }
            }
        }
    }

    let mut widths = vec![1usize; expected];
    for cells in rows {
        for (column, cell) in cells.iter().enumerate() {
            let trimmed_len = cell.trim_end_matches(' ').len();
            widths[column] = widths[column].max(trimmed_len);
        }
    }

    for (column, &width) in widths.iter().enumerate() {
        if width > 32 {
            return Err(TableError::TooWide { column, width });
        }
    }

    let mut out = String::new();

    for &width in &widths {
        out.push('+');
        let border_width = width + 2;
        for _ in 0..border_width {
            out.push('-');
        }
    }
    out.push_str("+\n");

    for cells in rows {
        out.push('|');
        for (column, cell) in cells.iter().enumerate() {
            let content = cell.trim_end_matches(' ');
            out.push(' ');
            out.push_str(content);
            for _ in content.len()..widths[column] {
                out.push(' ');
            }
            out.push(' ');
            out.push('|');
        }
        out.push('\n');
    }

    for &width in &widths {
        out.push('+');
        let border_width = width + 2;
        for _ in 0..border_width {
            out.push('-');
        }
    }
    out.push_str("+\n");

    Ok(out)
}
