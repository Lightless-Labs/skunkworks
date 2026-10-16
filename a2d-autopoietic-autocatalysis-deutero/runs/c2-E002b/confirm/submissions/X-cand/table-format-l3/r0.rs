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

    let columns = rows[0].len();
    for (row_index, row) in rows.iter().enumerate() {
        if row.len() != columns {
            return Err(TableError::Ragged {
                row: row_index,
                expected: columns,
                actual: row.len(),
            });
        }
    }

    if !options.alignments.is_empty() && options.alignments.len() != columns {
        return Err(TableError::AlignmentCount {
            expected: columns,
            actual: options.alignments.len(),
        });
    }

    let fixed_overhead = 3 * columns + 1;
    let minimum = fixed_overhead + columns;
    if options.max_width != 0 && options.max_width < minimum {
        return Err(TableError::TooNarrow {
            minimum,
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

    let trimmed: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| cell.trim_end_matches(' ').to_owned())
                .collect()
        })
        .collect();

    let mut widths = vec![1usize; columns];
    for row in &trimmed {
        for (column, cell) in row.iter().enumerate() {
            widths[column] = widths[column].max(cell.chars().count());
        }
    }

    if options.max_width != 0 {
        let mut total = fixed_overhead + widths.iter().sum::<usize>();
        while total > options.max_width {
            let candidate = widths
                .iter()
                .enumerate()
                .filter(|(_, width)| **width > 1)
                .max_by_key(|(index, width)| (**width, *index))
                .map(|(index, _)| index);

            if let Some(index) = candidate {
                widths[index] -= 1;
                total -= 1;
            } else {
                break;
            }
        }
    }

    let mut output = String::new();
    append_border(&mut output, &widths);

    for (row_index, row) in trimmed.iter().enumerate() {
        output.push('|');
        for (column_index, cell) in row.iter().enumerate() {
            let width = widths[column_index];
            let content = truncate(cell, width);
            let content_width = content.chars().count();
            let extra = width - content_width;
            let alignment = options
                .alignments
                .get(column_index)
                .unwrap_or(&Alignment::Left);

            let (left, right) = match alignment {
                Alignment::Left => (0, extra),
                Alignment::Right => (extra, 0),
                Alignment::Center => (extra / 2, extra - extra / 2),
            };

            output.push(' ');
            for _ in 0..left {
                output.push(' ');
            }
            output.push_str(&content);
            for _ in 0..right {
                output.push(' ');
            }
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
        for _ in 0..(width + 2) {
            output.push('-');
        }
        output.push('+');
    }
    output.push('\n');
}

fn truncate(cell: &str, width: usize) -> String {
    let count = cell.chars().count();
    if count <= width {
        return cell.to_owned();
    }

    let mut result: String = cell.chars().take(width - 1).collect();
    result.push('…');
    result
}