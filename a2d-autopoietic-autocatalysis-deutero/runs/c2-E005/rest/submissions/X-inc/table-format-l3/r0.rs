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

    for (row, cells) in rows.iter().enumerate() {
        if cells.is_empty() {
            return Err(TableError::EmptyRow { row });
        }
    }

    let n = rows[0].len();
    for (row, cells) in rows.iter().enumerate() {
        if cells.len() != n {
            return Err(TableError::Ragged {
                row,
                expected: n,
                actual: cells.len(),
            });
        }
    }

    if !options.alignments.is_empty() && options.alignments.len() != n {
        return Err(TableError::AlignmentCount {
            expected: n,
            actual: options.alignments.len(),
        });
    }

    let minimum = 3 * n + 1 + n;
    if options.max_width != 0 && options.max_width < minimum {
        return Err(TableError::TooNarrow {
            minimum,
            provided: options.max_width,
        });
    }

    for (row, cells) in rows.iter().enumerate() {
        for (column, cell) in cells.iter().enumerate() {
            for (byte, ch) in cell.char_indices() {
                if ch.is_control() || ch == '\u{2028}' || ch == '\u{2029}' {
                    return Err(TableError::InvalidChar { row, column, byte });
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

    let mut widths = vec![1usize; n];
    for row in &trimmed {
        for (column, cell) in row.iter().enumerate() {
            widths[column] = widths[column].max(cell.chars().count());
        }
    }

    if options.max_width != 0 {
        let mut total = 3 * n + 1 + widths.iter().sum::<usize>();
        while total > options.max_width {
            let widest = (0..n)
                .filter(|&column| widths[column] > 1)
                .max_by_key(|&column| (widths[column], column));
            match widest {
                Some(column) => {
                    widths[column] -= 1;
                    total -= 1;
                }
                None => break,
            }
        }
    }

    let mut output = String::new();
    append_border(&mut output, &widths);

    for (row_index, row) in trimmed.iter().enumerate() {
        output.push('|');
        for (column, cell) in row.iter().enumerate() {
            output.push(' ');
            let width = widths[column];
            let content = truncate(cell, width);
            let content_width = content.chars().count();
            let extra = width - content_width;
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

            output.extend(std::iter::repeat_n(' ', left));
            output.push_str(&content);
            output.extend(std::iter::repeat_n(' ', right));
            output.push(' ');
            output.push('|');
        }
        output.push('\n');

        if options.header && row_index == 0 && trimmed.len() > 1 {
            append_border(&mut output, &widths);
        }
    }

    append_border(&mut output, &widths);
    Ok(output)
}

fn append_border(output: &mut String, widths: &[usize]) {
    output.push('+');
    for &width in widths {
        output.extend(std::iter::repeat_n('-', width + 2));
        output.push('+');
    }
    output.push('\n');
}

fn truncate(cell: &str, width: usize) -> String {
    if cell.chars().count() <= width {
        return cell.to_owned();
    }

    let mut result: String = cell.chars().take(width - 1).collect();
    result.push('…');
    result
}