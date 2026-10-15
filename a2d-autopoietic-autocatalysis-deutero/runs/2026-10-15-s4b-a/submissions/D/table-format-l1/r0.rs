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

    let mut trimmed = Vec::with_capacity(rows.len());
    for (row, cells) in rows.iter().enumerate() {
        let mut trimmed_row = Vec::with_capacity(cells.len());
        for (column, cell) in cells.iter().enumerate() {
            for (byte, ch) in cell.char_indices() {
                if !('\u{20}'..='\u{7e}').contains(&ch) {
                    return Err(TableError::InvalidChar { row, column, byte });
                }
            }
            trimmed_row.push(cell.trim_end_matches(' ').to_owned());
        }
        trimmed.push(trimmed_row);
    }

    let mut widths = vec![1usize; expected];
    for row in &trimmed {
        for (column, cell) in row.iter().enumerate() {
            widths[column] = widths[column].max(cell.len());
        }
    }

    for (column, &width) in widths.iter().enumerate() {
        if width > 32 {
            return Err(TableError::TooWide { column, width });
        }
    }

    let mut output = String::new();

    let write_border = |output: &mut String| {
        output.push('+');
        for &width in &widths {
            for _ in 0..(width + 2) {
                output.push('-');
            }
            output.push('+');
        }
        output.push('\n');
    };

    write_border(&mut output);
    for row in &trimmed {
        output.push('|');
        for (column, cell) in row.iter().enumerate() {
            output.push(' ');
            output.push_str(cell);
            for _ in 0..(widths[column] - cell.len()) {
                output.push(' ');
            }
            output.push(' ');
            output.push('|');
        }
        output.push('\n');
    }
    write_border(&mut output);

    Ok(output)
}