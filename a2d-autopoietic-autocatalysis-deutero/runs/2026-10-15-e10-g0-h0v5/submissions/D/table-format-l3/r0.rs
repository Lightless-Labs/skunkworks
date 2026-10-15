#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableError {
    EmptyRow { row: usize },
    Ragged { row: usize, expected: usize, actual: usize },
    InvalidChar { row: usize, column: usize, byte: usize },
    AlignmentCount { expected: usize, actual: usize },
    TooNarrow { minimum: usize, provided: usize },
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
    pub max_width: usize,
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

    let minimum_width = 3 * column_count + 1 + column_count;
    if options.max_width != 0 && options.max_width < minimum_width {
        return Err(TableError::TooNarrow {
            minimum: minimum_width,
            provided: options.max_width,
        });
    }

    for (row_index, row) in rows.iter().enumerate() {
        for (column_index, cell) in row.iter().enumerate() {
            for (byte, ch) in cell.char_indices() {
                if ch.is_control() || ch == '\u{2028}' || ch == '\u{2029}' {
                    return Err(TableError::InvalidChar {
                        row: row_index,
                        column: column_index,
                        byte,
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
            widths[column] = widths[column].max(cell.chars().count());
        }
    }

    if options.max_width != 0 {
        let overhead = 3 * column_count + 1;
        while widths.iter().sum::<usize>() + overhead > options.max_width {
            let widest = widths
                .iter()
                .enumerate()
                .filter(|(_, width)| **width > 1)
                .max_by_key(|(column, width)| (**width, *column))
                .map(|(column, _)| column);

            if let Some(column) = widest {
                widths[column] -= 1;
            } else {
                break;
            }
        }
    }

    let alignments: Vec<Alignment> = if options.alignments.is_empty() {
        vec![Alignment::Left; column_count]
    } else {
        options.alignments.clone()
    };

    let mut output = String::new();

    fn append_border(output: &mut String, widths: &[usize]) {
        output.push('+');
        for &width in widths {
            output.extend(std::iter::repeat_n('-', width + 2));
            output.push('+');
        }
        output.push('\n');
    }

    fn append_data_line(
        output: &mut String,
        row: &[&str],
        widths: &[usize],
        alignments: &[Alignment],
    ) {
        output.push('|');
        for ((cell, &width), alignment) in row.iter().zip(widths).zip(alignments) {
            let scalar_count = cell.chars().count();
            let content = if scalar_count > width {
                let mut clipped: String = cell.chars().take(width - 1).collect();
                clipped.push('…');
                clipped
            } else {
                (*cell).to_owned()
            };

            let content_width = content.chars().count();
            let extra = width - content_width;
            let (left, right) = match alignment {
                Alignment::Left => (0, extra),
                Alignment::Right => (extra, 0),
                Alignment::Center => (extra / 2, extra - extra / 2),
            };

            output.push(' ');
            output.extend(std::iter::repeat_n(' ', left));
            output.push_str(&content);
            output.extend(std::iter::repeat_n(' ', right));
            output.push(' ');
            output.push('|');
        }
        output.push('\n');
    }

    append_border(&mut output, &widths);
    for (row_index, row) in trimmed.iter().enumerate() {
        append_data_line(&mut output, row, &widths, &alignments);
        if options.header && row_index == 0 && rows.len() > 1 {
            append_border(&mut output, &widths);
        }
    }
    append_border(&mut output, &widths);

    Ok(output)
}