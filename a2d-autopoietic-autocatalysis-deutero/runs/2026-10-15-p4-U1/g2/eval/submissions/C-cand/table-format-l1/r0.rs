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

    if let Some(row) = rows.iter().position(Vec::is_empty) {
        return Err(TableError::EmptyRow { row });
    }

    let expected = rows[0].len();
    if let Some((row, cells)) = rows
        .iter()
        .enumerate()
        .find(|(_, cells)| cells.len() != expected)
    {
        return Err(TableError::Ragged {
            row,
            expected,
            actual: cells.len(),
        });
    }

    for (row, cells) in rows.iter().enumerate() {
        for (column, cell) in cells.iter().enumerate() {
            if let Some((byte, _)) = cell
                .bytes()
                .enumerate()
                .find(|(_, byte)| !(b' '..=b'~').contains(byte))
            {
                return Err(TableError::InvalidChar { row, column, byte });
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

    if let Some((column, &width)) = widths
        .iter()
        .enumerate()
        .find(|(_, width)| **width > 32)
    {
        return Err(TableError::TooWide { column, width });
    }

    let mut output = String::new();

    let append_border = |output: &mut String| {
        output.push('+');
        for &width in &widths {
            for _ in 0..width + 2 {
                output.push('-');
            }
            output.push('+');
        }
        output.push('\n');
    };

    append_border(&mut output);

    for cells in &trimmed {
        output.push('|');
        for (column, cell) in cells.iter().enumerate() {
            output.push(' ');
            output.push_str(cell);
            for _ in cell.len()..widths[column] {
                output.push(' ');
            }
            output.push_str(" |");
        }
        output.push('\n');
    }

    append_border(&mut output);
    Ok(output)
}

fn main() {}