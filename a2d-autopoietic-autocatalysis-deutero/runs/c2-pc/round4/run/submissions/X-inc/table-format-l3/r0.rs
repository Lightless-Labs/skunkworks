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
        let mut total = overhead + widths.iter().sum::<usize>();
        while total > options.max_width {
            let widest = widths
                .iter()
                .enumerate()
                .filter(|(_, width)| **width > 1)
                .max_by_key(|(column, width)| (**width, *column))
                .map(|(column, _)| column);

            if let Some(column) = widest {
                widths[column] -= 1;
                total -= 1;
            } else {
                break;
            }
        }
    }

    let alignments = if options.alignments.is_empty() {
        vec![Alignment::Left; n]
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
        cells: &[String],
        widths: &[usize],
        alignments: &[Alignment],
    ) {
        output.push('|');

        for ((cell, &width), alignment) in cells.iter().zip(widths).zip(alignments) {
            let scalar_count = cell.chars().count();
            let content = if scalar_count > width {
                let mut clipped: String = cell.chars().take(width - 1).collect();
                clipped.push('…');
                clipped
            } else {
                cell.clone()
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
    for (row_index, cells) in trimmed.iter().enumerate() {
        append_data_line(&mut output, cells, &widths, &alignments);
        if options.header && row_index == 0 && rows.len() > 1 {
            append_border(&mut output, &widths);
        }
    }
    append_border(&mut output, &widths);

    Ok(output)
}