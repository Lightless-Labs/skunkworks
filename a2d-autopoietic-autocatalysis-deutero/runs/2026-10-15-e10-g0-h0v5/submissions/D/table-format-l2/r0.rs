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
            for (byte_index, byte) in cell.bytes().enumerate() {
                if !(0x20..=0x7e).contains(&byte) {
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
        for (column_index, cell) in row.iter().enumerate() {
            widths[column_index] = widths[column_index].max(cell.len());
        }
    }

    for (column, &width) in widths.iter().enumerate() {
        if width > 32 {
            return Err(TableError::TooWide { column, width });
        }
    }

    let alignment_for = |column: usize| {
        options
            .alignments
            .get(column)
            .unwrap_or(&Alignment::Left)
    };

    let mut output = String::new();
    append_border(&mut output, &widths);

    for (row_index, row) in trimmed.iter().enumerate() {
        output.push('|');
        for (column_index, cell) in row.iter().enumerate() {
            output.push(' ');
            append_aligned(
                &mut output,
                cell,
                widths[column_index],
                alignment_for(column_index),
            );
            output.push(' ');
            output.push('|');
        }
        output.push('\n');

        if options.header && row_index == 0 && rows.len() > 1 {
            append_border(&mut output, &widths);
        }
    }

    append_border(&mut output, &widths);
    Ok(output)
}

fn append_border(output: &mut String, widths: &[usize]) {
    output.push('+');
    for &width in widths {
        for _ in 0..width + 2 {
            output.push('-');
        }
        output.push('+');
    }
    output.push('\n');
}

fn append_aligned(output: &mut String, content: &str, width: usize, alignment: &Alignment) {
    let extra = width - content.len();
    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };

    for _ in 0..left {
        output.push(' ');
    }
    output.push_str(content);
    for _ in 0..right {
        output.push(' ');
    }
}