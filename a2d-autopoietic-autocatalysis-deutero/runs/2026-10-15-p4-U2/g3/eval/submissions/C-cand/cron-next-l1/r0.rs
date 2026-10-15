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
    day_star: [bool; 2],
}

const FIELDS: [Field; 5] = [
    Field::Minute,
    Field::Hour,
    Field::DayOfMonth,
    Field::Month,
    Field::DayOfWeek,
];

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

fn error(field: Field, offset: usize, make: fn(Field, usize) -> ParseError) -> ParseError {
    make(field, offset)
}

fn invalid_syntax(field: Field, offset: usize) -> ParseError {
    error(field, offset, |field, offset| ParseError::InvalidSyntax { field, offset })
}

fn parse_number(
    bytes: &[u8],
    field: Field,
    offset: usize,
    step: bool,
) -> Result<u32, ParseError> {
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }

    let mut value = 0u32;
    for &byte in bytes {
        let digit = (byte - b'0') as u32;
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(digit))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }

    if step {
        let (_, max) = bounds(field);
        let width = max - bounds(field).0 + 1;
        if value == 0 || value > width {
            return Err(ParseError::BadStep { field, offset });
        }
    } else {
        let (min, max) = bounds(field);
        if value < min || value > max {
            return Err(ParseError::OutOfRange { field, offset });
        }
    }

    Ok(value)
}

fn parse_field(input: &str, field: Field) -> Result<(Vec<u8>, bool), ParseError> {
    let bytes = input.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !(byte.is_ascii_digit() || matches!(byte, b'*' | b'-' | b'/' | b',')) {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (domain_min, domain_max) = bounds(field);
    let mut selected = vec![false; (domain_max - domain_min + 1) as usize];
    let mut day_star = false;

    let mut item_start = 0;
    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|relative| item_start + relative)
            .unwrap_or(bytes.len());

        let item = &bytes[item_start..item_end];
        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let slash_positions: Vec<usize> = item
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'/').then_some(i))
            .collect();

        if slash_positions.len() > 1 {
            return Err(invalid_syntax(field, item_start));
        }

        let (base, step_bytes, step_offset) = if let Some(&slash) = slash_positions.first() {
            if slash == 0 || slash + 1 == item.len() {
                return Err(invalid_syntax(field, item_start));
            }
            (
                &item[..slash],
                Some(&item[slash + 1..]),
                Some(item_start + slash + 1),
            )
        } else {
            (item, None, None)
        };

        let is_star = base == b"*";
        let hyphens: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();

        if is_star {
            // valid
        } else if hyphens.len() <= 1 {
            if hyphens.is_empty() {
                if base.is_empty() || !base.iter().all(u8::is_ascii_digit) {
                    return Err(invalid_syntax(field, item_start));
                }
            } else {
                let hyphen = hyphens[0];
                if hyphen == 0
                    || hyphen + 1 == base.len()
                    || !base[..hyphen].iter().all(u8::is_ascii_digit)
                    || !base[hyphen + 1..].iter().all(u8::is_ascii_digit)
                {
                    return Err(invalid_syntax(field, item_start));
                }
            }
        } else {
            return Err(invalid_syntax(field, item_start));
        }

        // Syntax has been validated before numeric values are inspected.
        let (start, end, end_offset) = if is_star {
            (domain_min, domain_max, item_start)
        } else if let Some(&hyphen) = hyphens.first() {
            let start = parse_number(&base[..hyphen], field, item_start, false)?;
            let end_offset = item_start + hyphen + 1;
            let end = parse_number(&base[hyphen + 1..], field, end_offset, false)?;
            if start > end {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: end_offset,
                });
            }
            (start, end, end_offset)
        } else {
            let start = parse_number(base, field, item_start, false)?;
            let end = if step_bytes.is_some() {
                domain_max
            } else {
                start
            };
            (start, end, item_start)
        };

        let step = match (step_bytes, step_offset) {
            (Some(bytes), Some(offset)) => parse_number(bytes, field, offset, true)?,
            _ => 1,
        };

        let mut value = start;
        while value <= end {
            selected[(value - domain_min) as usize] = true;
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
            }
        }

        if field == Field::DayOfMonth && item == b"*" {
            day_star = true;
        }
        if field == Field::DayOfWeek && item == b"*" {
            day_star = true;
        }

        if item_end == bytes.len() {
            break;
        }
        item_start = item_end + 1;
    }

    let values = selected
        .iter()
        .enumerate()
        .filter_map(|(index, &is_selected)| {
            is_selected.then_some(domain_min as usize + index)
        })
        .map(|value| value as u8)
        .collect();

    Ok((values, day_star))
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let pieces: Vec<&str> = input.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount {
            found: pieces.len(),
        });
    }

    let mut selected: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());
    let mut day_star = [false; 2];

    for (index, (&field, piece)) in FIELDS.iter().zip(pieces.iter()).enumerate() {
        let (values, is_star) = parse_field(piece, field)?;
        selected[index] = values;
        if field == Field::DayOfMonth {
            day_star[0] = is_star;
        } else if field == Field::DayOfWeek {
            day_star[1] = is_star;
        }
    }

    Ok(Cron { selected, day_star })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.selected[field_index(field)].clone()
    }
}

fn main() {}