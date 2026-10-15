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

    let fixed_overhead = 3 * column_count + 1;
    let minimum = fixed_overhead + column_count;
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
        .map(|cells| {
            cells
                .iter()
                .map(|cell| cell.trim_end_matches(' ').to_owned())
                .collect()
        })
        .collect();

    let mut widths = vec![1usize; column_count];
    for cells in &trimmed {
        for (column, cell) in cells.iter().enumerate() {
            widths[column] = widths[column].max(cell.chars().count());
        }
    }

    if options.max_width != 0 {
        while fixed_overhead + widths.iter().sum::<usize>() > options.max_width {
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

    let mut output = String::new();
    append_border(&mut output, &widths);

    for (row_index, cells) in trimmed.iter().enumerate() {
        output.push('|');

        for (column, cell) in cells.iter().enumerate() {
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

            output.push(' ');
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

fn truncate(cell: &str, width: usize) -> String {
    let count = cell.chars().count();
    if count <= width {
        return cell.to_owned();
    }

    let mut result: String = cell.chars().take(width - 1).collect();
    result.push('\u{2026}');
    result
}

fn append_border(output: &mut String, widths: &[usize]) {
    output.push('+');
    for &width in widths {
        output.extend(std::iter::repeat_n('-', width + 2));
        output.push('+');
    }
    output.push('\n');
}

fn main() {}