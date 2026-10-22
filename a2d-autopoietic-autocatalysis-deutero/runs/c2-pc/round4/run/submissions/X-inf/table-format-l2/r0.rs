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

    for (row_index, row) in rows.iter().enumerate() {
        if row.is_empty() {
            return Err(TableError::EmptyRow { row: row_index });
        }
    }

    let column_count = rows[0].len();
    for (row_index, row) in rows.iter().enumerate() {
        if row.len() != column_count {
            return Err(TableError::Ragged {
                row: row_index,
                expected: column_count,
                actual: row.len(),
            });
        }
    }

    if !options.alignments.is_empty() && options.alignments.len() != column_count {
        return Err(TableError::AlignmentCount {
            expected: column_count,
            actual: options.alignments.len(),
        });
    }

    for (row_index, row) in rows.iter().enumerate() {
        for (column_index, cell) in row.iter().enumerate() {
            for (byte_index, ch) in cell.char_indices() {
                if !(' '..='~').contains(&ch) {
                    return Err(TableError::InvalidChar {
                        row: row_index,
                        column: column_index,
                        byte: byte_index,
                    });
                }
            }
        }
    }

    let trimmed: Vec<Vec<&str>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| cell.trim_end_matches(' '))
                .collect()
        })
        .collect();

    let mut widths = vec![1usize; column_count];
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

    let alignment_for = |column: usize| -> &Alignment {
        if options.alignments.is_empty() {
            &Alignment::Left
        } else {
            &options.alignments[column]
        }
    };

    let mut output = String::new();

    let append_border = |out: &mut String| {
        out.push('+');
        for &width in &widths {
            out.extend(std::iter::repeat_n('-', width + 2));
            out.push('+');
        }
        out.push('\n');
    };

    let append_row = |out: &mut String, row: &[&str]| {
        out.push('|');
        for (column, cell) in row.iter().enumerate() {
            let width = widths[column];
            let extra = width - cell.len();
            let (left, right) = match alignment_for(column) {
                Alignment::Left => (0, extra),
                Alignment::Right => (extra, 0),
                Alignment::Center => (extra / 2, extra - extra / 2),
            };

            out.push(' ');
            out.extend(std::iter::repeat_n(' ', left));
            out.push_str(cell);
            out.extend(std::iter::repeat_n(' ', right));
            out.push(' ');
            out.push('|');
        }
        out.push('\n');
    };

    append_border(&mut output);
    for (row_index, row) in trimmed.iter().enumerate() {
        append_row(&mut output, row);
        if options.header && row_index == 0 && rows.len() > 1 {
            append_border(&mut output);
        }
    }
    append_border(&mut output);

    Ok(output)
}