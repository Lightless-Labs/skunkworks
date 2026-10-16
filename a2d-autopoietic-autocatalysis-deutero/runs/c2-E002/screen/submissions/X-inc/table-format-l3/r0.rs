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

    let minimum = 3 * column_count + 1 + column_count;
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

    let mut widths = vec![1usize; column_count];
    for row in &trimmed {
        for (column, cell) in row.iter().enumerate() {
            widths[column] = widths[column].max(cell.chars().count());
        }
    }

    if options.max_width != 0 {
        let overhead = 3 * column_count + 1;
        while overhead + widths.iter().sum::<usize>() > options.max_width {
            let mut widest_column = None;
            let mut widest_width = 1usize;
            for (column, &width) in widths.iter().enumerate() {
                if width > 1 && width >= widest_width {
                    widest_width = width;
                    widest_column = Some(column);
                }
            }
            if let Some(column) = widest_column {
                widths[column] -= 1;
            } else {
                break;
            }
        }
    }

    let alignments: Vec<Alignment> = if options.alignments.is_empty() {
        vec![Alignment::Left; column_count]
    } else {
        options.alignments.clone()
    };

    fn border(widths: &[usize], out: &mut String) {
        out.push('+');
        for &width in widths {
            out.extend(std::iter::repeat_n('-', width + 2));
            out.push('+');
        }
        out.push('\n');
    }

    fn append_row(
        row: &[String],
        widths: &[usize],
        alignments: &[Alignment],
        out: &mut String,
    ) {
        out.push('|');
        for ((cell, &width), alignment) in row.iter().zip(widths).zip(alignments) {
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
            let (left, right) = match alignment {
                Alignment::Left => (0, extra),
                Alignment::Right => (extra, 0),
                Alignment::Center => (extra / 2, extra - extra / 2),
            };

            out.push(' ');
            out.extend(std::iter::repeat_n(' ', left));
            out.push_str(&content);
            out.extend(std::iter::repeat_n(' ', right));
            out.push(' ');
            out.push('|');
        }
        out.push('\n');
    }

    let mut output = String::new();
    border(&widths, &mut output);

    for (row_index, row) in trimmed.iter().enumerate() {
        append_row(row, &widths, &alignments, &mut output);
        if options.header && row_index == 0 && trimmed.len() > 1 {
            border(&widths, &mut output);
        }
    }

    border(&widths, &mut output);
    Ok(output)
}