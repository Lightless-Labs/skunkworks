use std::collections::BTreeSet;

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

fn field_info(field: Field) -> (u8, u8, u8) {
    match field {
        Field::Minute => (0, 59, 60),
        Field::Hour => (0, 23, 24),
        Field::DayOfMonth => (1, 31, 31),
        Field::Month => (1, 12, 12),
        Field::DayOfWeek => (0, 6, 7),
    }
}

fn error(field: Field, offset: usize, kind: fn(Field, usize) -> ParseError) -> ParseError {
    kind(field, offset)
}

fn parse_number(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: Field,
) -> Result<u32, ParseError> {
    let number = &bytes[start..end];
    if number.len() > 1 && number[0] == b'0' {
        return Err(error(field, start, |field, offset| {
            ParseError::LeadingZero { field, offset }
        }));
    }
    let mut value = 0u32;
    for &byte in number {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or_else(|| {
                error(field, start, |field, offset| ParseError::NumberOverflow {
                    field,
                    offset,
                })
            })?;
    }
    Ok(value)
}

fn parse_field(text: &str, field: Field) -> Result<Vec<u8>, ParseError> {
    let bytes = text.as_bytes();
    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (minimum, maximum, width) = field_info(field);
    let mut selected = BTreeSet::new();
    let mut item_start = 0usize;

    for item in text.split(',') {
        let item_bytes = item.as_bytes();
        if item_bytes.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let slash_positions: Vec<usize> = item_bytes
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'/').then_some(i))
            .collect();

        let base_end = slash_positions.first().copied().unwrap_or(item_bytes.len());
        let step_span = slash_positions
            .first()
            .map(|&slash| (slash + 1, item_bytes.len()));

        let base = &item_bytes[..base_end];
        let (kind, start_span, end_span) = if base == b"*" {
            (0u8, None, None)
        } else {
            let hyphens: Vec<usize> = base
                .iter()
                .enumerate()
                .filter_map(|(i, &b)| (b == b'-').then_some(i))
                .collect();

            match hyphens.as_slice() {
                [] if !base.is_empty() && base.iter().all(u8::is_ascii_digit) => {
                    (1, Some((0, base.len())), None)
                }
                [hyphen]
                    if *hyphen > 0
                        && *hyphen + 1 < base.len()
                        && base[..*hyphen].iter().all(u8::is_ascii_digit)
                        && base[*hyphen + 1..].iter().all(u8::is_ascii_digit) =>
                {
                    (2, Some((0, *hyphen)), Some((*hyphen + 1, base.len())))
                }
                _ => {
                    return Err(ParseError::InvalidSyntax {
                        field,
                        offset: item_start,
                    });
                }
            }
        };

        if slash_positions.len() > 1
            || step_span.is_some_and(|(start, end)| {
                start == end || !item_bytes[start..end].iter().all(u8::is_ascii_digit)
            })
        {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let mut start_value = minimum as u32;
        let mut end_value = maximum as u32;

        if let Some((start, end)) = start_span {
            start_value = parse_number(bytes, item_start + start, item_start + end, field)?;
            if start_value < minimum as u32 || start_value > maximum as u32 {
                return Err(ParseError::OutOfRange {
                    field,
                    offset: item_start + start,
                });
            }
        }

        if let Some((start, end)) = end_span {
            end_value = parse_number(bytes, item_start + start, item_start + end, field)?;
            if end_value < minimum as u32 || end_value > maximum as u32 {
                return Err(ParseError::OutOfRange {
                    field,
                    offset: item_start + start,
                });
            }
            if start_value > end_value {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: item_start + start,
                });
            }
        } else if kind == 1 && step_span.is_none() {
            end_value = start_value;
        }

        let step = if let Some((start, end)) = step_span {
            let step = parse_number(bytes, item_start + start, item_start + end, field)?;
            if step == 0 || step > width as u32 {
                return Err(ParseError::BadStep {
                    field,
                    offset: item_start + start,
                });
            }
            step
        } else {
            1
        };

        let mut value = start_value;
        while value <= end_value {
            selected.insert(value as u8);
            match value.checked_add(step) {
                Some(next) if next > value => value = next,
                _ => break,
            }
        }

        item_start += item_bytes.len() + 1;
    }

    Ok(selected.into_iter().collect())
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
    let mut selected = Vec::with_capacity(5);

    for (text, field) in parts.iter().zip(fields) {
        selected.push(parse_field(text, field)?);
    }

    Ok(Cron {
        selected: selected.try_into().ok().unwrap(),
        day_stars: [parts[2] == "*", parts[4] == "*"],
    })
}
