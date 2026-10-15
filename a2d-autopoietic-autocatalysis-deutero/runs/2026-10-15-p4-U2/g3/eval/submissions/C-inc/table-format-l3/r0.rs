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

    let overhead = 3 * n + 1;
    let minimum = overhead + n;
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
        let mut total: usize = widths.iter().sum::<usize>() + overhead;
        while total > options.max_width {
            let widest = (0..n)
                .rev()
                .find(|&column| widths[column] > 1)
                .expect("minimum width was checked");
            widths[widest] -= 1;
            total -= 1;
        }
    }

    let mut output = String::new();
    output.push('+');
    for &width in &widths {
        output.extend(std::iter::repeat_n('-', width + 2));
        output.push('+');
    }
    output.push('\n');

    for (row_index, row) in trimmed.iter().enumerate() {
        output.push('|');
        for (column, cell) in row.iter().enumerate() {
            let width = widths[column];
            let chars: Vec<char> = cell.chars().collect();
            let content: String = if chars.len() > width {
                let mut clipped: String = chars[..width - 1].iter().collect();
                clipped.push('…');
                clipped
            } else {
                cell.clone()
            };
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
            output.push('+');
            for &width in &widths {
                output.extend(std::iter::repeat_n('-', width + 2));
                output.push('+');
            }
            output.push('\n');
        }
    }

    output.push('+');
    for &width in &widths {
        output.extend(std::iter::repeat_n('-', width + 2));
        output.push('+');
    }
    output.push('\n');

    Ok(output)
}

fn main() {}