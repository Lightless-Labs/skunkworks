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

    let mut trimmed = Vec::with_capacity(rows.len());
    let mut widths = vec![1usize; column_count];

    for (row_index, row) in rows.iter().enumerate() {
        let mut trimmed_row = Vec::with_capacity(column_count);
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

            let content = cell.trim_end_matches(' ').to_owned();
            widths[column_index] = widths[column_index].max(content.len());
            trimmed_row.push(content);
        }
        trimmed.push(trimmed_row);
    }

    for (column, &width) in widths.iter().enumerate() {
        if width > 32 {
            return Err(TableError::TooWide { column, width });
        }
    }

    let alignment_for = |column: usize| {
        if options.alignments.is_empty() {
            Alignment::Left
        } else {
            options.alignments[column].clone()
        }
    };

    let mut output = String::new();

    let write_border = |out: &mut String| {
        out.push('+');
        for &width in &widths {
            out.extend(std::iter::repeat_n('-', width + 2));
            out.push('+');
        }
        out.push('\n');
    };

    write_border(&mut output);

    for (row_index, row) in trimmed.iter().enumerate() {
        output.push('|');
        for (column_index, content) in row.iter().enumerate() {
            let padding = widths[column_index] - content.len();
            let (left, right) = match alignment_for(column_index) {
                Alignment::Left => (0, padding),
                Alignment::Right => (padding, 0),
                Alignment::Center => (padding / 2, padding - padding / 2),
            };

            output.push(' ');
            output.extend(std::iter::repeat_n(' ', left));
            output.push_str(content);
            output.extend(std::iter::repeat_n(' ', right));
            output.push(' ');
            output.push('|');
        }
        output.push('\n');

        if options.header && row_index == 0 && trimmed.len() > 1 {
            write_border(&mut output);
        }
    }

    write_border(&mut output);
    Ok(output)
}