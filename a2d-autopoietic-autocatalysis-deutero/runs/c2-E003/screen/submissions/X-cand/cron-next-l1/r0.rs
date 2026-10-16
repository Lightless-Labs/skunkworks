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
    star_day_fields: [bool; 2],
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let index = field_index(field);
        self.selected[index]
            .iter()
            .enumerate()
            .filter_map(|(value, &allowed)| allowed.then_some(value as u8))
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

fn field_info(field: Field) -> (u32, u32) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
    }
}

fn parse_error(field: Field, offset: usize, kind: fn(Field, usize) -> ParseError) -> ParseError {
    kind(field, offset)
}

fn parse_number(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: Field,
    is_step: bool,
    maximum: u32,
) -> Result<u32, ParseError> {
    if start == end {
        return Err(ParseError::InvalidSyntax { field, offset: start });
    }

    if end - start > 1 && bytes[start] == b'0' {
        return Err(ParseError::LeadingZero { field, offset: start });
    }

    let mut value = 0u32;
    for &byte in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow {
                field,
                offset: start,
            })?;
    }

    if is_step {
        if value == 0 || value > maximum {
            return Err(ParseError::BadStep { field, offset: start });
        }
    } else if value < field_info(field).0 || value > field_info(field).1 {
        return Err(ParseError::OutOfRange { field, offset: start });
    }

    Ok(value)
}

fn parse_item(
    item: &[u8],
    item_offset: usize,
    field: Field,
    selected: &mut [bool],
) -> Result<(), ParseError> {
    if item.is_empty() {
        return Err(ParseError::EmptyItem {
            field,
            offset: item_offset,
        });
    }

    let (minimum, maximum) = field_info(field);
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

    let (base_end, step_start) = if let Some(&slash) = slash_positions.first() {
        (slash, Some(slash + 1))
    } else {
        (item.len(), None)
    };

    let base = &item[..base_end];
    let is_star = base == b"*";

    let hyphens: Vec<usize> = base
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'-').then_some(i))
        .collect();

    if !is_star && hyphens.len() > 1 {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    }

    if is_star && !hyphens.is_empty() {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    }

    let range = hyphens.first().copied();
    if base.is_empty()
        || (!is_star && range.is_none() && !base.iter().all(u8::is_ascii_digit))
        || (is_star && base != b"*")
    {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    }

    if let Some(hyphen) = range {
        if hyphen == 0
            || hyphen + 1 == base.len()
            || !base[..hyphen].iter().all(u8::is_ascii_digit)
            || !base[hyphen + 1..].iter().all(u8::is_ascii_digit)
        {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
    }

    if let Some(step_start) = step_start {
        if step_start == item.len()
            || !item[step_start..].iter().all(u8::is_ascii_digit)
        {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
    }

    let (start, end, numeric_end_offset) = if is_star {
        (minimum, maximum, None)
    } else if let Some(hyphen) = range {
        let start = parse_number(
            item,
            0,
            hyphen,
            field,
            false,
            maximum,
        )?;
        let end = parse_number(
            item,
            hyphen + 1,
            base.len(),
            field,
            false,
            maximum,
        )?;
        (start, end, Some(hyphen + 1))
    } else {
        let value = parse_number(item, 0, base.len(), field, false, maximum)?;
        let end = if step_start.is_some() { maximum } else { value };
        (value, end, None)
    };

    if let Some(end_offset) = numeric_end_offset {
        if start > end {
            return Err(ParseError::ReversedRange {
                field,
                offset: end_offset,
            });
        }
    }

    let step = if let Some(step_start) = step_start {
        let step = parse_number(item, step_start, item.len(), field, true, maximum - minimum + 1)?;
        step
    } else {
        1
    };

    let mut value = start;
    while value <= end {
        selected[value as usize] = true;
        match value.checked_add(step) {
            Some(next) => value = next,
            None => break,
        }
    }

    Ok(())
}

fn parse_field(text: &str, field: Field) -> Result<Vec<bool>, ParseError> {
    let bytes = text.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (_, maximum) = field_info(field);
    let mut selected = vec![false; (maximum + 1) as usize];

    let mut start = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        if byte == b',' {
            parse_item(&bytes[start..i], start, field, &mut selected)?;
            start = i + 1;
        }
    }
    parse_item(&bytes[start..], start, field, &mut selected)?;

    Ok(selected)
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
    let mut parsed: [Vec<bool>; 5] = std::array::from_fn(|_| Vec::new());

    for (index, (&text, &field)) in fields.iter().zip(field_kinds.iter()).enumerate() {
        parsed[index] = parse_field(text, field)?;
    }

    Ok(Cron {
        selected: parsed,
        star_day_fields: [fields[2] == "*", fields[4] == "*"],
    })
}