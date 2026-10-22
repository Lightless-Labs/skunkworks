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

    let mut widths = vec![1usize; expected];
    for (row, cells) in rows.iter().enumerate() {
        for (column, cell) in cells.iter().enumerate() {
            for (byte, ch) in cell.char_indices() {
                if !(' '..='~').contains(&ch) {
                    return Err(TableError::InvalidChar { row, column, byte });
                }
            }

            let trimmed_len = cell.trim_end_matches(' ').len();
            widths[column] = widths[column].max(trimmed_len);
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
            output.push_str(&"-".repeat(width + 2));
            output.push('+');
        }
        output.push('\n');
    };

    write_border(&mut output);

    for cells in rows {
        output.push('|');
        for (column, cell) in cells.iter().enumerate() {
            let content = cell.trim_end_matches(' ');
            output.push(' ');
            output.push_str(content);
            output.push_str(&" ".repeat(widths[column] - content.len()));
            output.push(' ');
            output.push('|');
        }
        output.push('\n');
    }

    write_border(&mut output);
    Ok(output)
}