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
    fields: [Vec<u8>; 5],
    day_stars: [bool; 2],
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.fields[field_index(field)].clone()
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

fn bounds(field: Field) -> (u32, u32) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
    }
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let field_kinds = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];
    let mut parsed: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());
    let mut day_stars = [false; 2];

    for i in 0..5 {
        let field = field_kinds[i];
        if i == 2 || i == 4 {
            day_stars[if i == 2 { 0 } else { 1 }] = parts[i] == "*";
        }
        parsed[i] = parse_field(parts[i], field)?;
    }

    Ok(Cron {
        fields: parsed,
        day_stars,
    })
}

fn parse_field(input: &str, field: Field) -> Result<Vec<u8>, ParseError> {
    for (offset, byte) in input.bytes().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (min, max) = bounds(field);
    let mut selected = vec![false; (max + 1) as usize];

    let mut item_start = 0usize;
    for item in input.split(',') {
        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        parse_item(item, item_start, field, min, max, &mut selected)?;
        item_start += item.len() + 1;
    }

    Ok(selected
        .into_iter()
        .enumerate()
        .filter_map(|(value, yes)| yes.then_some(value as u8))
        .collect())
}

fn parse_item(
    item: &str,
    offset: usize,
    field: Field,
    min: u32,
    max: u32,
    selected: &mut [bool],
) -> Result<(), ParseError> {
    let slash_count = item.bytes().filter(|&b| b == b'/').count();
    if slash_count > 1 {
        return Err(ParseError::InvalidSyntax { field, offset });
    }

    let (base, step_text, step_offset) = if let Some(slash) = item.find('/') {
        (&item[..slash], Some(&item[slash + 1..]), offset + slash + 1)
    } else {
        (item, None, 0)
    };

    let (base_kind, start_text, end_text, start_offset, end_offset) = if base == "*" {
        (0u8, None, None, 0usize, 0usize)
    } else {
        let hyphen_count = base.bytes().filter(|&b| b == b'-').count();
        if hyphen_count > 1 {
            return Err(ParseError::InvalidSyntax { field, offset });
        }

        if hyphen_count == 1 {
            let hyphen = base.find('-').unwrap();
            let left = &base[..hyphen];
            let right = &base[hyphen + 1..];
            if left.is_empty() || right.is_empty() {
                return Err(ParseError::InvalidSyntax { field, offset });
            }
            (
                2u8,
                Some(left),
                Some(right),
                offset,
                offset + hyphen + 1,
            )
        } else {
            if base.is_empty() || !base.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ParseError::InvalidSyntax { field, offset });
            }
            (1u8, Some(base), None, offset, 0usize)
        }
    };

    if let Some(step_str) = step_text {
        if step_str.is_empty() || !step_str.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ParseError::InvalidSyntax { field, offset });
        }
    }

    let mut start = min;
    let mut end = max;

    if let Some(text) = start_text {
        start = parse_number(text, start_offset, field)?;
        if start < min || start > max {
            return Err(ParseError::OutOfRange {
                field,
                offset: start_offset,
            });
        }
    }

    if let Some(text) = end_text {
        end = parse_number(text, end_offset, field)?;
        if end < min || end > max {
            return Err(ParseError::OutOfRange {
                field,
                offset: end_offset,
            });
        }
        if start > end {
            return Err(ParseError::ReversedRange {
                field,
                offset: end_offset,
            });
        }
    }

    let step = if let Some(text) = step_text {
        let value = parse_number(text, step_offset, field)?;
        if value == 0 || value > max - min + 1 {
            return Err(ParseError::BadStep {
                field,
                offset: step_offset,
            });
        }
        value
    } else {
        0
    };

    if step == 0 && base_kind == 1 {
        selected[start as usize] = true;
    } else {
        let increment = if step == 0 { 1 } else { step };
        let mut value = start;
        while value <= end {
            selected[value as usize] = true;
            match value.checked_add(increment) {
                Some(next) => value = next,
                None => break,
            }
        }
    }

    Ok(())
}

fn parse_number(text: &str, offset: usize, field: Field) -> Result<u32, ParseError> {
    let bytes = text.as_bytes();
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }

    let mut value = 0u32;
    for &byte in bytes {
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    Ok(value)
}

fn main() {}