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
    day_star: [bool; 2],
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let index = field_index(field);
        self.selected[index]
            .iter()
            .enumerate()
            .filter_map(|(value, &yes)| yes.then_some(value as u8 + field_min(field)))
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
        Field::DayOfMonth | Field::Month => 1,
        _ => 0,
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

fn parse_error(field: Field, offset: usize, kind: fn(Field, usize) -> ParseError) -> ParseError {
    kind(field, offset)
}

fn parse_number(
    bytes: &[u8],
    field: Field,
    offset: usize,
    is_step: bool,
) -> Result<u32, ParseError> {
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }
    let mut value = 0u32;
    for &byte in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }

    if is_step {
        if value == 0 || value > field_width(field) as u32 {
            return Err(ParseError::BadStep { field, offset });
        }
    } else if value < field_min(field) as u32 || value > field_max(field) as u32 {
        return Err(ParseError::OutOfRange { field, offset });
    }
    Ok(value)
}

fn parse_item(
    item: &[u8],
    field: Field,
    item_offset: usize,
    selected: &mut [bool],
) -> Result<(), ParseError> {
    if item.is_empty() {
        return Err(ParseError::EmptyItem {
            field,
            offset: item_offset,
        });
    }

    let slash_positions: Vec<usize> = item
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'/').then_some(i))
        .collect();
    if slash_positions.len() > 1 {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    }

    let (base, step_bytes, step_offset) = if let Some(&slash) = slash_positions.first() {
        (&item[..slash], Some(&item[slash + 1..]), item_offset + slash + 1)
    } else {
        (item, None, 0)
    };

    let is_star = base == b"*";
    let hyphens: Vec<usize> = base
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'-').then_some(i))
        .collect();

    let (start_bytes, end_bytes, end_rel) = if is_star {
        (None, None, 0)
    } else if hyphens.is_empty() {
        if base.is_empty() || !base.iter().all(u8::is_ascii_digit) {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
        (Some(base), None, 0)
    } else if hyphens.len() == 1 {
        let hyphen = hyphens[0];
        let start = &base[..hyphen];
        let end = &base[hyphen + 1..];
        if start.is_empty()
            || end.is_empty()
            || !start.iter().all(u8::is_ascii_digit)
            || !end.iter().all(u8::is_ascii_digit)
        {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
        (Some(start), Some(end), hyphen + 1)
    } else {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    };

    if step_bytes.is_some_and(|s| s.is_empty() || !s.iter().all(u8::is_ascii_digit)) {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    }

    let min = field_min(field) as u32;
    let max = field_max(field) as u32;

    let (start, end) = if is_star {
        (min, max)
    } else {
        let start_slice = start_bytes.unwrap();
        let start_value = parse_number(start_slice, field, item_offset, false)?;
        let end_value = if let Some(end_slice) = end_bytes {
            parse_number(end_slice, field, item_offset + end_rel, false)?
        } else if step_bytes.is_some() {
            max
        } else {
            start_value
        };

        if end_bytes.is_some() && start_value > end_value {
            return Err(ParseError::ReversedRange {
                field,
                offset: item_offset + end_rel,
            });
        }
        (start_value, end_value)
    };

    let step = if let Some(step_slice) = step_bytes {
        parse_number(step_slice, field, step_offset, true)?
    } else {
        1
    };

    let mut value = start;
    while value <= end {
        selected[(value - min) as usize] = true;
        match value.checked_add(step) {
            Some(next) => value = next,
            None => break,
        }
    }
    Ok(())
}

fn parse_field(input: &str, field: Field) -> Result<(Vec<bool>, bool), ParseError> {
    let bytes = input.as_bytes();
    for (offset, &byte) in bytes.iter().enumerate() {
        if !(byte.is_ascii_digit() || matches!(byte, b'*' | b'-' | b'/' | b',')) {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let min = field_min(field);
    let mut selected = vec![false; field_width(field) as usize];
    let day_star = input == "*";

    let mut start = 0;
    for end in 0..=bytes.len() {
        if end == bytes.len() || bytes[end] == b',' {
            parse_item(&bytes[start..end], field, start, &mut selected)?;
            start = end + 1;
        }
    }

    let _ = min;
    Ok((selected, day_star))
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let fields: Vec<&str> = input.split(' ').collect();
    if fields.len() != 5 {
        return Err(ParseError::FieldCount {
            found: fields.len(),
        });
    }

    let field_kinds = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];
    let mut selected: [Vec<bool>; 5] = std::array::from_fn(|i| {
        vec![false; field_width(field_kinds[i]) as usize]
    });
    let mut day_star = [false; 2];

    for i in 0..5 {
        let (values, exact_star) = parse_field(fields[i], field_kinds[i])?;
        selected[i] = values;
        if i == 2 {
            day_star[0] = exact_star;
        } else if i == 4 {
            day_star[1] = exact_star;
        }
    }

    Ok(Cron { selected, day_star })
}