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

    let fixed = 3 * n + 1;
    let minimum = fixed + n;
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

    let mut widths = vec![1usize; n];
    for cells in &trimmed {
        for (column, cell) in cells.iter().enumerate() {
            widths[column] = widths[column].max(cell.chars().count());
        }
    }

    if options.max_width != 0 {
        let mut total: usize = widths.iter().sum::<usize>() + fixed;
        while total > options.max_width {
            let widest = widths
                .iter()
                .enumerate()
                .filter(|(_, width)| **width > 1)
                .max_by_key(|(column, width)| (**width, *column))
                .map(|(column, _)| column);
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

    for (row_index, cells) in trimmed.iter().enumerate() {
        for (column, cell) in cells.iter().enumerate() {
            if column == 0 {
                output.push('|');
            }
            let content = truncate(cell, widths[column]);
            let alignment = if options.alignments.is_empty() {
                &Alignment::Left
            } else {
                &options.alignments[column]
            };
            let (left, right) = padding(widths[column] - content.chars().count(), alignment);
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

fn append_border(output: &mut String, widths: &[usize]) {
    output.push('+');
    for &width in widths {
        output.extend(std::iter::repeat_n('-', width + 2));
        output.push('+');
    }
    output.push('\n');
}

fn truncate(cell: &str, width: usize) -> String {
    let count = cell.chars().count();
    if count <= width {
        return cell.to_owned();
    }
    let keep = width - 1;
    let mut result: String = cell.chars().take(keep).collect();
    result.push('…');
    result
}

fn padding(extra: usize, alignment: &Alignment) -> (usize, usize) {
    match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    }
}