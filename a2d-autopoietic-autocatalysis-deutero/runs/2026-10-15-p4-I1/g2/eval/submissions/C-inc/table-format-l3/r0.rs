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

    if let Some(row) = rows.iter().position(Vec::is_empty) {
        return Err(TableError::EmptyRow { row });
    }

    let columns = rows[0].len();
    if let Some((row, cells)) = rows
        .iter()
        .enumerate()
        .find(|(_, cells)| cells.len() != columns)
    {
        return Err(TableError::Ragged {
            row,
            expected: columns,
            actual: cells.len(),
        });
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
        let mut total: usize = widths.iter().sum::<usize>() + fixed_overhead;
        while total > options.max_width {
            let widest = (0..columns)
                .filter(|&column| widths[column] > 1)
                .max_by_key(|&column| (widths[column], column));
            if let Some(column) = widest {
                widths[column] -= 1;
                total -= 1;
            } else {
                break;
            }
        }
    }

    let alignments: Vec<&Alignment> = if options.alignments.is_empty() {
        vec![&Alignment::Left; columns]
    } else {
        options.alignments.iter().collect()
    };

    let mut output = String::new();

    let emit_border = |out: &mut String| {
        out.push('+');
        for &width in &widths {
            out.extend(std::iter::repeat_n('-', width + 2));
            out.push('+');
        }
        out.push('\n');
    };

    let emit_row = |out: &mut String, row: &[String]| {
        out.push('|');
        for (column, cell) in row.iter().enumerate() {
            out.push(' ');
            let width = widths[column];
            let char_count = cell.chars().count();
            let content = if char_count > width {
                let mut clipped: String = cell.chars().take(width - 1).collect();
                clipped.push('…');
                clipped
            } else {
                cell.clone()
            };
            let content_width = content.chars().count();
            let extra = width - content_width;

            let (left, right) = match alignments[column] {
                Alignment::Left => (0, extra),
                Alignment::Right => (extra, 0),
                Alignment::Center => (extra / 2, extra - extra / 2),
            };

            out.extend(std::iter::repeat_n(' ', left));
            out.push_str(&content);
            out.extend(std::iter::repeat_n(' ', right));
            out.push(' ');
            out.push('|');
        }
        out.push('\n');
    };

    emit_border(&mut output);
    for (index, row) in trimmed.iter().enumerate() {
        emit_row(&mut output, row);
        if options.header && index == 0 && trimmed.len() > 1 {
            emit_border(&mut output);
        }
    }
    emit_border(&mut output);

    Ok(output)
}

fn main() {}