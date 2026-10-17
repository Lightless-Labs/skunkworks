#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Minute,
    Hour,
    DayOfMonth,
    Month,
    DayOfWeek,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    FieldCount { found: usize },
    InvalidCharacter { field: Field, offset: usize },
    EmptyItem { field: Field, offset: usize },
    InvalidSyntax { field: Field, offset: usize },
    LeadingZero { field: Field, offset: usize },
    NumberOverflow { field: Field, offset: usize },
    OutOfRange { field: Field, offset: usize },
    ReversedRange { field: Field, offset: usize },
    BadStep { field: Field, offset: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    selected: [Vec<bool>; 5],
    day_stars: [bool; 2],
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let index = field_index(field);
        self.selected[index]
            .iter()
            .enumerate()
            .filter_map(|(value, &is_selected)| {
                if is_selected {
                    Some((value as u8) + field_min(field))
                } else {
                    None
                }
            })
            .collect()
    }
}

fn field_index(field: Field) -> usize {
    match field {
        Field::Minute => 0,
        Field::Hour => 1,
        Field::DayOfMonth => 2,
        Field::Month => 3,
        Field::DayOfWeek => 4,
    }
}

fn field_min(field: Field) -> u8 {
    match field {
        Field::Minute | Field::Hour | Field::DayOfWeek => 0,
        Field::DayOfMonth | Field::Month => 1,
    }
}

fn field_max(field: Field) -> u8 {
    match field {
        Field::Minute => 59,
        Field::Hour => 23,
        Field::DayOfMonth => 31,
        Field::Month => 12,
        Field::DayOfWeek => 6,
    }
}

fn field_width(field: Field) -> u8 {
    field_max(field) - field_min(field) + 1
}

fn err(field: Field, offset: usize, kind: ErrorKind) -> ParseError {
    match kind {
        ErrorKind::EmptyItem => ParseError::EmptyItem { field, offset },
        ErrorKind::InvalidSyntax => ParseError::InvalidSyntax { field, offset },
        ErrorKind::LeadingZero => ParseError::LeadingZero { field, offset },
        ErrorKind::NumberOverflow => ParseError::NumberOverflow { field, offset },
        ErrorKind::OutOfRange => ParseError::OutOfRange { field, offset },
        ErrorKind::ReversedRange => ParseError::ReversedRange { field, offset },
        ErrorKind::BadStep => ParseError::BadStep { field, offset },
    }
}

#[derive(Clone, Copy)]
enum ErrorKind {
    EmptyItem,
    InvalidSyntax,
    LeadingZero,
    NumberOverflow,
    OutOfRange,
    ReversedRange,
    BadStep,
}

fn parse_number(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: Field,
    step: bool,
) -> Result<u32, ParseError> {
    if start == end {
        return Err(err(
            field,
            start,
            if step {
                ErrorKind::BadStep
            } else {
                ErrorKind::InvalidSyntax
            },
        ));
    }

    if end - start > 1 && bytes[start] == b'0' {
        return Err(err(field, start, ErrorKind::LeadingZero));
    }

    let mut value = 0u32;
    for &byte in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or_else(|| err(field, start, ErrorKind::NumberOverflow))?;
    }

    if step {
        if value == 0 || value > field_width(field) as u32 {
            return Err(err(field, start, ErrorKind::BadStep));
        }
    } else if value < field_min(field) as u32 || value > field_max(field) as u32 {
        return Err(err(field, start, ErrorKind::OutOfRange));
    }

    Ok(value)
}

fn parse_field(field: Field, text: &str) -> Result<Vec<bool>, ParseError> {
    let bytes = text.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let min = field_min(field) as u32;
    let max = field_max(field) as u32;
    let mut selected = vec![false; field_width(field) as usize];

    let mut item_start = 0usize;
    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|relative| item_start + relative)
            .unwrap_or(bytes.len());

        if item_start == item_end {
            return Err(err(field, item_start, ErrorKind::EmptyItem));
        }

        let item = &bytes[item_start..item_end];
        let slash_positions: Vec<usize> = item
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| if b == b'/' { Some(i) } else { None })
            .collect();

        if slash_positions.len() > 1 {
            return Err(err(field, item_start, ErrorKind::InvalidSyntax));
        }

        let base_end = slash_positions.first().copied().unwrap_or(item.len());
        let step_bounds = slash_positions
            .first()
            .map(|&slash| (item_start + slash + 1, item_end));

        let base = &item[..base_end];
        let (is_star, range_dash) = if base == b"*" {
            (true, None)
        } else {
            let dashes: Vec<usize> = base
                .iter()
                .enumerate()
                .filter_map(|(i, &b)| if b == b'-' { Some(i) } else { None })
                .collect();
            if dashes.len() > 1 {
                return Err(err(field, item_start, ErrorKind::InvalidSyntax));
            }
            if dashes.len() == 1 {
                let dash = dashes[0];
                if dash == 0
                    || dash + 1 == base.len()
                    || base[..dash].contains(&b'*')
                    || base[dash + 1..].contains(&b'*')
                {
                    return Err(err(field, item_start, ErrorKind::InvalidSyntax));
                }
                (false, Some(dash))
            } else {
                if base.is_empty() || base.contains(&b'*') {
                    return Err(err(field, item_start, ErrorKind::InvalidSyntax));
                }
                (false, None)
            }
        };

        if let Some((step_start, step_end)) = step_bounds {
            if step_start == step_end || !bytes[step_start..step_end].iter().all(u8::is_ascii_digit) {
                return Err(err(field, item_start, ErrorKind::InvalidSyntax));
            }
        }

        if !is_star {
            let (start_start, start_end) = if let Some(dash) = range_dash {
                (item_start, item_start + dash)
            } else {
                (item_start, item_start + base.len())
            };
            let start = parse_number(bytes, start_start, start_end, field, false)?;

            let (end, end_start) = if let Some(dash) = range_dash {
                let end_start = item_start + dash + 1;
                (parse_number(bytes, end_start, item_end.min(item_start + base_end), field, false)?, end_start)
            } else {
                (start, start_start)
            };

            if start > end {
                return Err(err(field, end_start, ErrorKind::ReversedRange));
            }

            let step = if let Some((step_start, step_end)) = step_bounds {
                parse_number(bytes, step_start, step_end, field, true)?
            } else {
                1
            };

            let effective_end = if range_dash.is_none() && step_bounds.is_some() {
                max
            } else {
                end
            };

            let mut value = start;
            while value <= effective_end {
                selected[(value - min) as usize] = true;
                match value.checked_add(step) {
                    Some(next) => value = next,
                    None => break,
                }
            }
        } else {
            let step = if let Some((step_start, step_end)) = step_bounds {
                parse_number(bytes, step_start, step_end, field, true)?
            } else {
                1
            };

            let mut value = min;
            while value <= max {
                selected[(value - min) as usize] = true;
                match value.checked_add(step) {
                    Some(next) => value = next,
                    None => break,
                }
            }
        }

        if item_end == bytes.len() {
            break;
        }
        item_start = item_end + 1;
    }

    Ok(selected)
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let fields: Vec<&str> = input.split(' ').collect();
    if fields.len() != 5 {
        return Err(ParseError::FieldCount { found: fields.len() });
    }

    let kinds = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];
    let mut selected: [Vec<bool>; 5] = std::array::from_fn(|_| Vec::new());
    let mut day_stars = [false; 2];

    for i in 0..5 {
        if i == 2 || i == 4 {
            day_stars[if i == 2 { 0 } else { 1 }] = fields[i] == "*";
        }
        selected[i] = parse_field(kinds[i], fields[i])?;
    }

    Ok(Cron {
        selected,
        day_stars,
    })
}