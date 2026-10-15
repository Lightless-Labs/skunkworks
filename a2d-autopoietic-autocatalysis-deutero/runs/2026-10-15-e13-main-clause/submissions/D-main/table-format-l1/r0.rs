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

    let trimmed: Vec<Vec<&str>> = rows
        .iter()
        .map(|cells| {
            cells
                .iter()
                .map(|cell| cell.trim_end_matches(' '))
                .collect()
        })
        .collect();

    let mut widths = vec![1usize; expected];
    for cells in &trimmed {
        for (column, cell) in cells.iter().enumerate() {
            widths[column] = widths[column].max(cell.len());
        }
    }

    for (column, &width) in widths.iter().enumerate() {
        if width > 32 {
            return Err(TableError::TooWide { column, width });
        }
    }

    let mut output = String::new();

    fn append_border(output: &mut String, widths: &[usize]) {
        output.push('+');
        for &width in widths {
            output.extend(std::iter::repeat_n('-', width + 2));
            output.push('+');
        }
        output.push('\n');
    }

    append_border(&mut output, &widths);

    for cells in &trimmed {
        output.push('|');
        for (column, cell) in cells.iter().enumerate() {
            output.push(' ');
            output.push_str(cell);
            output.extend(std::iter::repeat_n(' ', widths[column] - cell.len()));
            output.push(' ');
            output.push('|');
        }
        output.push('\n');
    }

    append_border(&mut output, &widths);
    Ok(output)
}

fn main() {}