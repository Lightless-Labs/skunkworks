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
    selected: [Vec<u8>; 5],
    day_stars: [bool; 2],
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.selected[field_index(field)].clone()
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

fn domain(field: Field) -> (u8, u8) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
    }
}

fn parse_number(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: Field,
) -> Result<u32, ParseError> {
    if end - start > 1 && bytes[start] == b'0' {
        return Err(ParseError::LeadingZero {
            field,
            offset: start,
        });
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
    Ok(value)
}

fn parse_item(
    bytes: &[u8],
    item_start: usize,
    item_end: usize,
    field: Field,
    min: u8,
    max: u8,
    selected: &mut [bool],
) -> Result<(), ParseError> {
    if item_start == item_end {
        return Err(ParseError::EmptyItem {
            field,
            offset: item_start,
        });
    }

    let item = &bytes[item_start..item_end];
    let slash_count = item.iter().filter(|&&b| b == b'/').count();
    if slash_count > 1 {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_start,
        });
    }

    let (base_end, step_range) = if let Some(slash) = item.iter().position(|&b| b == b'/') {
        if slash == 0 || slash + 1 == item.len() {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }
        (slash, Some((item_start + slash + 1, item_end)))
    } else {
        (item.len(), None)
    };

    let base = &item[..base_end];
    let star = base == b"*";
    let hyphen_count = base.iter().filter(|&&b| b == b'-').count();

    let (start_range, end_range) = if star {
        (None, None)
    } else if hyphen_count == 0 {
        if base.is_empty() {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }
        (Some((item_start, item_start + base.len())), None)
    } else if hyphen_count == 1 {
        let hyphen = base.iter().position(|&b| b == b'-').unwrap();
        if hyphen == 0 || hyphen + 1 == base.len() {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }
        (
            Some((item_start, item_start + hyphen)),
            Some((item_start + hyphen + 1, item_start + base.len())),
        )
    } else {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_start,
        });
    };

    let (start, end) = if star {
        (min as u32, max as u32)
    } else {
        let (start_at, start_end) = start_range.unwrap();
        let first = parse_number(bytes, start_at, start_end, field)?;
        if first < min as u32 || first > max as u32 {
            return Err(ParseError::OutOfRange {
                field,
                offset: start_at,
            });
        }

        if let Some((end_at, end_end)) = end_range {
            let last = parse_number(bytes, end_at, end_end, field)?;
            if last < min as u32 || last > max as u32 {
                return Err(ParseError::OutOfRange {
                    field,
                    offset: end_at,
                });
            }
            if first > last {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: end_at,
                });
            }
            (first, last)
        } else if step_range.is_some() {
            (first, max as u32)
        } else {
            (first, first)
        }
    };

    let step = if let Some((step_start, step_end)) = step_range {
        let value = parse_number(bytes, step_start, step_end, field)?;
        let width = (max - min + 1) as u32;
        if value == 0 || value > width {
            return Err(ParseError::BadStep {
                field,
                offset: step_start,
            });
        }
        value
    } else {
        1
    };

    let mut value = start;
    while value <= end {
        selected[(value as u8 - min) as usize] = true;
        match value.checked_add(step) {
            Some(next) if next > value => value = next,
            _ => break,
        }
    }
    Ok(())
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
    let mut selected: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());
    let mut day_stars = [false; 2];

    for (index, (&text, &field)) in fields.iter().zip(field_kinds.iter()).enumerate() {
        let bytes = text.as_bytes();

        for (offset, &byte) in bytes.iter().enumerate() {
            if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
                return Err(ParseError::InvalidCharacter { field, offset });
            }
        }

        if index == 2 {
            day_stars[0] = text == "*";
        } else if index == 4 {
            day_stars[1] = text == "*";
        }

        let (min, max) = domain(field);
        let mut bitmap = vec![false; (max - min + 1) as usize];
        let mut offset = 0;
        for item in text.split(',') {
            let end = offset + item.len();
            parse_item(bytes, offset, end, field, min, max, &mut bitmap)?;
            offset = end + 1;
        }

        selected[index] = bitmap
            .iter()
            .enumerate()
            .filter_map(|(i, &yes)| yes.then_some(min + i as u8))
            .collect();
    }

    Ok(Cron {
        selected,
        day_stars,
    })
}

fn main() {}