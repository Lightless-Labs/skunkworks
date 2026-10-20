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

    let overhead = n.saturating_mul(3).saturating_add(1);
    let minimum = overhead.saturating_add(n);
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

    let trimmed: Vec<Vec<&str>> = rows
        .iter()
        .map(|row| row.iter().map(|cell| cell.trim_end_matches(' ')).collect())
        .collect();

    let mut widths = vec![1usize; n];
    for row in &trimmed {
        for (column, cell) in row.iter().enumerate() {
            widths[column] = widths[column].max(cell.chars().count());
        }
    }

    if options.max_width != 0 {
        let mut total = widths
            .iter()
            .fold(overhead, |sum, width| sum.saturating_add(*width));
        while total > options.max_width {
            let mut selected = None;
            for column in 0..n {
                if widths[column] > 1
                    && selected.map_or(true, |old| widths[column] >= widths[old])
                {
                    selected = Some(column);
                }
            }
            match selected {
                Some(column) => {
                    widths[column] -= 1;
                    total -= 1;
                }
                None => break,
            }
        }
    }

    fn border(widths: &[usize]) -> String {
        let mut line = String::from("+");
        for &width in widths {
            for _ in 0..width.saturating_add(2) {
                line.push('-');
            }
            line.push('+');
        }
        line.push('\n');
        line
    }

    let mut output = border(&widths);
    for (row_index, row) in trimmed.iter().enumerate() {
        output.push('|');
        for (column, cell) in row.iter().enumerate() {
            let width = widths[column];
            let count = cell.chars().count();
            let content = if count > width {
                let mut clipped: String = cell.chars().take(width - 1).collect();
                clipped.push('…');
                clipped
            } else {
                (*cell).to_owned()
            };
            let content_width = content.chars().count();
            let extra = width - content_width;
            let alignment = options
                .alignments
                .get(column)
                .unwrap_or(&Alignment::Left);
            let (left, right) = match alignment {
                Alignment::Left => (0, extra),
                Alignment::Right => (extra, 0),
                Alignment::Center => (extra / 2, extra - extra / 2),
            };
            output.push(' ');
            for _ in 0..left {
                output.push(' ');
            }
            output.push_str(&content);
            for _ in 0..right {
                output.push(' ');
            }
            output.push(' ');
            output.push('|');
        }
        output.push('\n');

        if options.header && row_index == 0 && rows.len() > 1 {
            output.push_str(&border(&widths));
        }
    }
    output.push_str(&border(&widths));
    Ok(output)
}
