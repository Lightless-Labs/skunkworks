#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableError {
    EmptyRow { row: usize },
    Ragged { row: usize, expected: usize, actual: usize },
    InvalidChar { row: usize, column: usize, byte: usize },
    TooWide { column: usize, width: usize },
    AlignmentCount { expected: usize, actual: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Right,
    Center,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub alignments: Vec<Alignment>,
    pub header: bool,
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

    let column_count = rows[0].len();
    for (row, cells) in rows.iter().enumerate() {
        if cells.len() != column_count {
            return Err(TableError::Ragged {
                row,
                expected: column_count,
                actual: cells.len(),
            });
        }
    }

    if !options.alignments.is_empty() && options.alignments.len() != column_count {
        return Err(TableError::AlignmentCount {
            expected: column_count,
            actual: options.alignments.len(),
        });
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

    let mut widths = vec![1usize; column_count];
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

    let write_border = |out: &mut String| {
        out.push('+');
        for &width in &widths {
            for _ in 0..(width + 2) {
                out.push('-');
            }
            out.push('+');
        }
        out.push('\n');
    };

    write_border(&mut output);

    for (row_index, cells) in trimmed.iter().enumerate() {
        output.push('|');
        for (column, cell) in cells.iter().enumerate() {
            let extra = widths[column] - cell.len();
            let alignment = if options.alignments.is_empty() {
                &Alignment::Left
            } else {
                &options.alignments[column]
            };
            let (left, right) = match alignment {
                Alignment::Left => (0, extra),
                Alignment::Right => (extra, 0),
                Alignment::Center => (extra / 2, extra - extra / 2),
            };

            output.push(' ');
            for _ in 0..left {
                output.push(' ');
            }
            output.push_str(cell);
            for _ in 0..right {
                output.push(' ');
            }
            output.push_str(" |");
        }
        output.push('\n');

        if options.header && row_index == 0 && trimmed.len() > 1 {
            write_border(&mut output);
        }
    }

    write_border(&mut output);
    Ok(output)
}