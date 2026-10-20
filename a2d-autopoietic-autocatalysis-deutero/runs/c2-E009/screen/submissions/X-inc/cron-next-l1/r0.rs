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
    day_wildcards: [bool; 2],
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

fn field_bounds(index: usize) -> (u32, u32) {
    match index {
        0 => (0, 59),
        1 => (0, 23),
        2 => (1, 31),
        3 => (1, 12),
        _ => (0, 6),
    }
}

fn parse_number(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: Field,
    is_step: bool,
    width: u32,
    min: u32,
    max: u32,
) -> Result<u32, ParseError> {
    let offset = start;
    if end - start > 1 && bytes[start] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }

    let mut value = 0u32;
    for &byte in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }

    if is_step {
        if value == 0 || value > width {
            return Err(ParseError::BadStep { field, offset });
        }
    } else if value < min || value > max {
        return Err(ParseError::OutOfRange { field, offset });
    }

    Ok(value)
}

fn parse_field(
    input: &str,
    field: Field,
    index: usize,
) -> Result<(Vec<u8>, bool), ParseError> {
    let bytes = input.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (min, max) = field_bounds(index);
    let width = max - min + 1;
    let day_wildcard = matches!(field, Field::DayOfMonth | Field::DayOfWeek)
        && bytes == b"*";
    let mut selected = vec![false; (max + 1) as usize];

    let mut item_start = 0;
    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|p| item_start + p)
            .unwrap_or(bytes.len());

        if item_start == item_end {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let mut slash = None;
        let mut hyphen = None;
        for pos in item_start..item_end {
            match bytes[pos] {
                b'/' => {
                    if slash.is_some() {
                        return Err(ParseError::InvalidSyntax {
                            field,
                            offset: item_start,
                        });
                    }
                    slash = Some(pos);
                }
                b'-' => {
                    if hyphen.is_some() {
                        return Err(ParseError::InvalidSyntax {
                            field,
                            offset: item_start,
                        });
                    }
                    hyphen = Some(pos);
                }
                _ => {}
            }
        }

        let base_end = slash.unwrap_or(item_end);
        let base_start = item_start;
        let base_is_star = base_end - base_start == 1 && bytes[base_start] == b'*';

        let (range_start, range_end, start_offset) = if base_is_star {
            (None, None, base_start)
        } else if let Some(hyphen_pos) = hyphen {
            if hyphen_pos >= base_end
                || hyphen_pos == base_start
                || hyphen_pos + 1 == base_end
                || bytes[base_start..base_end].contains(&b'*')
            {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            (Some(base_start), Some(hyphen_pos + 1), base_start)
        } else {
            if base_start >= base_end || bytes[base_start..base_end].contains(&b'*') {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            (Some(base_start), None, base_start)
        };

        if slash.is_some() && slash.unwrap() + 1 == item_end {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let (start_value, end_value) = if let Some(start_pos) = range_start {
            let start_end = if let Some(hyphen_pos) = hyphen {
                hyphen_pos
            } else {
                base_end
            };
            let start_value = parse_number(
                bytes, start_pos, start_end, field, false, width, min, max,
            )?;
            let end_value = if let Some(end_pos) = range_end {
                Some(parse_number(
                    bytes, end_pos, base_end, field, false, width, min, max,
                )?)
            } else {
                None
            };

            if let Some(end) = end_value {
                if start_value > end {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: range_end.unwrap(),
                    });
                }
            }
            (start_value, end_value.unwrap_or(if slash.is_some() { max } else { start_value }))
        } else {
            (min, max)
        };

        let step = if let Some(slash_pos) = slash {
            parse_number(
                bytes,
                slash_pos + 1,
                item_end,
                field,
                true,
                width,
                min,
                max,
            )?
        } else {
            1
        };

        let mut value = start_value;
        while value <= end_value {
            selected[value as usize] = true;
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
            }
        }

        if item_end == bytes.len() {
            break;
        }
        item_start = item_end + 1;
    }

    Ok((
        selected
            .iter()
            .enumerate()
            .filter_map(|(value, &on)| on.then_some(value as u8))
            .collect(),
        day_wildcard,
    ))
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

    let mut selected: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());
    let mut day_wildcards = [false; 2];

    for index in 0..5 {
        let (values, wildcard) = parse_field(parts[index], fields[index], index)?;
        selected[index] = values;
        if index == 2 {
            day_wildcards[0] = wildcard;
        } else if index == 4 {
            day_wildcards[1] = wildcard;
        }
    }

    Ok(Cron {
        selected,
        day_wildcards,
    })
}