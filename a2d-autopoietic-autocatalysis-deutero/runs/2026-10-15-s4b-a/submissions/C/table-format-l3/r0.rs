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

    let fixed = 3 * columns + 1;
    let minimum = fixed + columns;
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
        while widths.iter().sum::<usize>() + fixed > options.max_width {
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

    fn border(widths: &[usize]) -> String {
        let mut line = String::from("+");
        for width in widths {
            line.push_str(&"-".repeat(width + 2));
            line.push('+');
        }
        line.push('\n');
        line
    }

    fn align(content: &str, width: usize, alignment: &Alignment) -> String {
        let count = content.chars().count();
        let (left, right) = match alignment {
            Alignment::Left => (0, width - count),
            Alignment::Right => (width - count, 0),
            Alignment::Center => {
                let extra = width - count;
                (extra / 2, extra - extra / 2)
            }
        };
        let mut result = String::with_capacity(content.len() + left + right);
        result.push_str(&" ".repeat(left));
        result.push_str(content);
        result.push_str(&" ".repeat(right));
        result
    }

    let mut output = border(&widths);
    for (row_index, row) in trimmed.iter().enumerate() {
        output.push('|');
        for (column, cell) in row.iter().enumerate() {
            output.push(' ');
            let width = widths[column];
            let count = cell.chars().count();
            let content = if count > width {
                let mut clipped: String = cell.chars().take(width - 1).collect();
                clipped.push('…');
                clipped
            } else {
                cell.clone()
            };
            let alignment = options
                .alignments
                .get(column)
                .unwrap_or(&Alignment::Left);
            output.push_str(&align(&content, width, alignment));
            output.push(' ');
            output.push('|');
        }
        output.push('\n');

        if options.header && row_index == 0 && trimmed.len() > 1 {
            output.push_str(&border(&widths));
        }
    }
    output.push_str(&border(&widths));
    Ok(output)
}

fn main() {}