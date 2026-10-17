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

    let n = rows[0].len();
    if let Some((row, cells)) = rows
        .iter()
        .enumerate()
        .find(|(_, cells)| cells.len() != n)
    {
        return Err(TableError::Ragged {
            row,
            expected: n,
            actual: cells.len(),
        });
    }

    if !options.alignments.is_empty() && options.alignments.len() != n {
        return Err(TableError::AlignmentCount {
            expected: n,
            actual: options.alignments.len(),
        });
    }

    let fixed = n.checked_mul(3).and_then(|v| v.checked_add(1));
    let minimum = fixed.and_then(|v| v.checked_add(n));
    if options.max_width != 0
        && minimum.map_or(true, |min| options.max_width < min)
    {
        return Err(TableError::TooNarrow {
            minimum: minimum.unwrap_or(usize::MAX),
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

    let mut widths = vec![1usize; n];
    for row in &trimmed {
        for (column, cell) in row.iter().enumerate() {
            widths[column] = widths[column].max(cell.chars().count());
        }
    }

    if options.max_width != 0 {
        let overhead = fixed.unwrap_or(usize::MAX);
        let mut total = widths
            .iter()
            .try_fold(overhead, |sum, width| sum.checked_add(*width))
            .unwrap_or(usize::MAX);

        while total > options.max_width {
            let eligible = widths
                .iter()
                .enumerate()
                .filter(|(_, width)| **width > 1)
                .max_by_key(|(column, width)| (**width, *column))
                .map(|(column, _)| column);

            let Some(column) = eligible else {
                break;
            };
            widths[column] -= 1;
            total -= 1;
        }
    }

    let alignments: Vec<&Alignment> = if options.alignments.is_empty() {
        vec![&Alignment::Left; n]
    } else {
        options.alignments.iter().collect()
    };

    let mut output = String::new();
    append_border(&mut output, &widths);

    for (row_index, row) in trimmed.iter().enumerate() {
        output.push('|');
        for (column, cell) in row.iter().enumerate() {
            output.push(' ');
            let content = truncate(cell, widths[column]);
            append_aligned(
                &mut output,
                &content,
                widths[column],
                alignments[column],
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

fn truncate(cell: &str, width: usize) -> String {
    if cell.chars().count() <= width {
        return cell.to_owned();
    }

    let mut result: String = cell.chars().take(width - 1).collect();
    result.push('…');
    result
}

fn append_aligned(output: &mut String, content: &str, width: usize, alignment: &Alignment) {
    let content_width = content.chars().count();
    let extra = width - content_width;

    let (left, right) = match alignment {
        Alignment::Left => (0, extra),
        Alignment::Right => (extra, 0),
        Alignment::Center => (extra / 2, extra - extra / 2),
    };

    output.extend(std::iter::repeat_n(' ', left));
    output.push_str(content);
    output.extend(std::iter::repeat_n(' ', right));
}

fn append_border(output: &mut String, widths: &[usize]) {
    output.push('+');
    for width in widths {
        output.extend(std::iter::repeat_n('-', width + 2));
        output.push('+');
    }
    output.push('\n');
}