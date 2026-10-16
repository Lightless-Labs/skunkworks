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
    values: [Vec<u8>; 5],
    day_of_month_star: bool,
    day_of_week_star: bool,
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.values[field_index(field)].clone()
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

fn field_spec(field: Field) -> (u32, u32) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
    }
}

fn parse_number(bytes: &[u8], field: Field, offset: usize) -> Result<u32, ParseError> {
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

    let (min, max) = field_spec(field);
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
        (&item[..slash], Some(&item[slash + 1..]), Some(item_offset + slash + 1))
    } else {
        (item, None, None)
    };

    let is_star = base == b"*";
    let hyphens: Vec<usize> = base
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'-').then_some(i))
        .collect();

    let base_kind = if is_star && hyphens.is_empty() {
        0
    } else if !is_star && hyphens.is_empty() && !base.is_empty() {
        1
    } else if !is_star && hyphens.len() == 1 {
        let h = hyphens[0];
        if h == 0 || h + 1 == base.len() {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
        2
    } else {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    };

    if let Some(step) = step_bytes {
        if step.is_empty() {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
    }

    let (start, end, end_offset) = match base_kind {
        0 => (min, max, item_offset),
        1 => {
            let n = parse_number(base, field, item_offset)?;
            if n < min || n > max {
                return Err(ParseError::OutOfRange {
                    field,
                    offset: item_offset,
                });
            }
            let end = if step_bytes.is_some() { max } else { n };
            (n, end, item_offset)
        }
        _ => {
            let h = hyphens[0];
            let start = parse_number(&base[..h], field, item_offset)?;
            if start < min || start > max {
                return Err(ParseError::OutOfRange {
                    field,
                    offset: item_offset,
                });
            }
            let end_start = item_offset + h + 1;
            let end = parse_number(&base[h + 1..], field, end_start)?;
            if end < min || end > max {
                return Err(ParseError::OutOfRange {
                    field,
                    offset: end_start,
                });
            }
            if start > end {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: end_start,
                });
            }
            (start, end, end_start)
        }
    };

    let step = if let Some(step_bytes) = step_bytes {
        let offset = step_offset.unwrap();
        let value = parse_number(step_bytes, field, offset)?;
        let width = max - min + 1;
        if value == 0 || value > width {
            return Err(ParseError::BadStep {
                field,
                offset,
            });
        }
        value
    } else {
        1
    };

    let _ = end_offset;
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

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let fields = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];
    let mut values: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());
    let mut day_of_month_star = false;
    let mut day_of_week_star = false;

    for (index, (&field, part)) in fields.iter().zip(parts.iter()).enumerate() {
        let bytes = part.as_bytes();
        for (offset, &byte) in bytes.iter().enumerate() {
            if !matches!(byte, b'0'..=b'9' | b'*' | b'-' | b'/' | b',') {
                return Err(ParseError::InvalidCharacter { field, offset });
            }
        }

        if field == Field::DayOfMonth {
            day_of_month_star = bytes == b"*";
        } else if field == Field::DayOfWeek {
            day_of_week_star = bytes == b"*";
        }

        let (min, max) = field_spec(field);
        let mut selected = vec![false; (max - min + 1) as usize];
        let mut offset = 0usize;

        for item in bytes.split(|&b| b == b',') {
            parse_item(item, field, offset, &mut selected)?;
            offset += item.len() + 1;
        }

        values[index] = selected
            .iter()
            .enumerate()
            .filter_map(|(i, &is_selected)| {
                is_selected.then_some((min as usize + i) as u8)
            })
            .collect();
    }

    Ok(Cron {
        values,
        day_of_month_star,
        day_of_week_star,
    })
}