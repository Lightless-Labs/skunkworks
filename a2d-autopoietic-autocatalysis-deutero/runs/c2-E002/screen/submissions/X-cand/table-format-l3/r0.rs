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

    let fixed_overhead = 3 * column_count + 1;
    let minimum = fixed_overhead + column_count;
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
            widths[column_index] = widths[column_index].max(cell.chars().count());
        }
    }

    if options.max_width != 0 {
        let mut total: usize = widths.iter().sum::<usize>() + fixed_overhead;
        while total > options.max_width {
            let widest = widths
                .iter()
                .enumerate()
                .filter(|(_, width)| **width > 1)
                .max_by_key(|(index, width)| (**width, *index))
                .map(|(index, _)| index)
                .expect("minimum budget check guarantees a shrinkable column");

            widths[widest] -= 1;
            total -= 1;
        }
    }

    let alignments: Vec<&Alignment> = if options.alignments.is_empty() {
        (0..column_count).map(|_| &Alignment::Left).collect()
    } else {
        options.alignments.iter().collect()
    };

    let mut output = String::new();

    let write_border = |out: &mut String, widths: &[usize]| {
        out.push('+');
        for &width in widths {
            for _ in 0..width + 2 {
                out.push('-');
            }
            out.push('+');
        }
        out.push('\n');
    };

    write_border(&mut output, &widths);

    for (row_index, row) in trimmed.iter().enumerate() {
        output.push('|');

        for (column_index, cell) in row.iter().enumerate() {
            let width = widths[column_index];
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
            let (left, right) = match alignments[column_index] {
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

        if options.header && row_index == 0 && trimmed.len() > 1 {
            write_border(&mut output, &widths);
        }
    }

    write_border(&mut output, &widths);
    Ok(output)
}