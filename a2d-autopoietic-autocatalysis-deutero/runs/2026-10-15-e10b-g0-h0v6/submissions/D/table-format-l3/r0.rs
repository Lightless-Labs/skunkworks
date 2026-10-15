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

    if let Some(row) = rows.iter().position(|r| r.is_empty()) {
        return Err(TableError::EmptyRow { row });
    }

    let n = rows[0].len();
    if let Some((row, cells)) = rows.iter().enumerate().find(|(_, r)| r.len() != n) {
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

    let alignment = |column: usize| {
        if options.alignments.is_empty() {
            &Alignment::Left
        } else {
            &options.alignments[column]
        }
    };

    let mut output = String::new();

    let append_border = |out: &mut String, widths: &[usize]| {
        out.push('+');
        for width in widths {
            for _ in 0..width + 2 {
                out.push('-');
            }
            out.push('+');
        }
        out.push('\n');
    };

    let append_row = |out: &mut String, row: &[String], widths: &[usize]| {
        out.push('|');
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
            let (left, right) = match alignment(column) {
                Alignment::Left => (0, extra),
                Alignment::Right => (extra, 0),
                Alignment::Center => (extra / 2, extra - extra / 2),
            };

            out.push(' ');
            for _ in 0..left {
                out.push(' ');
            }
            out.push_str(&content);
            for _ in 0..right {
                out.push(' ');
            }
            out.push(' ');
            out.push('|');
        }
        out.push('\n');
    };

    append_border(&mut output, &widths);
    for (row_index, row) in trimmed.iter().enumerate() {
        append_row(&mut output, row, &widths);
        if options.header && row_index == 0 && trimmed.len() > 1 {
            append_border(&mut output, &widths);
        }
    }
    append_border(&mut output, &widths);

    Ok(output)
}