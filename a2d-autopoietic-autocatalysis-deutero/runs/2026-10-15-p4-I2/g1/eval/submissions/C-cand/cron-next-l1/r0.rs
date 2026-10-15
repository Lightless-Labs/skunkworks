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

fn error(field: Field, offset: usize, make: fn(Field, usize) -> ParseError) -> ParseError {
    make(field, offset)
}

fn parse_number(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: Field,
    step: bool,
    width: u8,
) -> Result<u32, ParseError> {
    if end - start > 1 && bytes[start] == b'0' {
        return Err(error(field, start, |field, offset| {
            ParseError::LeadingZero { field, offset }
        }));
    }

    let mut value = 0u32;
    for &byte in &bytes[start..end] {
        let digit = (byte - b'0') as u32;
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(digit))
            .ok_or_else(|| {
                error(field, start, |field, offset| {
                    ParseError::NumberOverflow { field, offset }
                })
            })?;
    }

    if step {
        if value < 1 || value > width as u32 {
            return Err(error(field, start, |field, offset| {
                ParseError::BadStep { field, offset }
            }));
        }
    } else {
        let (min, max, _) = field_info(field);
        if value < min as u32 || value > max as u32 {
            return Err(error(field, start, |field, offset| {
                ParseError::OutOfRange { field, offset }
            }));
        }
    }

    Ok(value)
}

fn parse_field(field: Field, text: &str) -> Result<(Vec<u8>, bool), ParseError> {
    let bytes = text.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (domain_min, domain_max, width) = field_info(field);
    let mut selected = vec![false; domain_max as usize + 1];
    let mut exact_star = false;

    let mut item_start = 0;
    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|relative| item_start + relative)
            .unwrap_or(bytes.len());

        if item_start == item_end {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let item = &bytes[item_start..item_end];
        let slash_positions: Vec<usize> = item
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'/').then_some(i))
            .collect();

        let (base_end, step_start) = match slash_positions.as_slice() {
            [] => (item.len(), None),
            [slash] => (*slash, Some(slash + 1)),
            _ => {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
        };

        let base = &item[..base_end];
        let hyphens: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();

        let base_kind = if base == b"*" {
            0
        } else if hyphens.is_empty() && !base.is_empty() && base.iter().all(u8::is_ascii_digit) {
            1
        } else if let [hyphen] = hyphens.as_slice() {
            let (left, right_with_hyphen) = base.split_at(*hyphen);
            let right = &right_with_hyphen[1..];
            if !left.is_empty()
                && !right.is_empty()
                && left.iter().all(u8::is_ascii_digit)
                && right.iter().all(u8::is_ascii_digit)
            {
                2
            } else {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
        } else {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        };

        if let Some(step_start) = step_start {
            if step_start == item.len()
                || !item[step_start..].iter().all(u8::is_ascii_digit)
            {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
        }

        let (range_start, range_end, numeric_start, numeric_end) = match base_kind {
            0 => (domain_min, domain_max, None, None),
            1 => (0, domain_max, Some((0, base.len())), None),
            _ => {
                let hyphen = hyphens[0];
                (
                    0,
                    0,
                    Some((0, hyphen)),
                    Some((hyphen + 1, base.len())),
                )
            }
        };

        let (start_value, end_value) = match base_kind {
            0 => (domain_min as u32, domain_max as u32),
            1 => {
                let (start, end) = numeric_start.unwrap();
                let value = parse_number(bytes, item_start + start, item_start + end, field, false, width)?;
                if step_start.is_none() {
                    (value, value)
                } else {
                    (value, domain_max as u32)
                }
            }
            _ => {
                let (start, end) = numeric_start.unwrap();
                let start_value =
                    parse_number(bytes, item_start + start, item_start + end, field, false, width)?;
                let (end_start, end_end) = numeric_end.unwrap();
                let end_value = parse_number(
                    bytes,
                    item_start + end_start,
                    item_start + end_end,
                    field,
                    false,
                    width,
                )?;
                if start_value > end_value {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: item_start + end_start,
                    });
                }
                (start_value, end_value)
            }
        };

        if let Some(step_start) = step_start {
            let step = parse_number(
                bytes,
                item_start + step_start,
                item.len(),
                field,
                true,
                width,
            )?;
            let mut value = start_value;
            while value <= end_value {
                selected[value as usize] = true;
                match value.checked_add(step) {
                    Some(next) if next > value => value = next,
                    _ => break,
                }
            }
        } else {
            let mut value = start_value;
            while value <= end_value {
                selected[value as usize] = true;
                value += 1;
            }
        }

        if field == Field::DayOfMonth && text == "*" {
            exact_star = true;
        }
        if field == Field::DayOfWeek && text == "*" {
            exact_star = true;
        }

        if item_end == bytes.len() {
            break;
        }
        item_start = item_end + 1;
    }

    let values = selected
        .into_iter()
        .enumerate()
        .filter_map(|(value, is_selected)| is_selected.then_some(value as u8))
        .collect();

    Ok((values, exact_star))
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let fields: Vec<&str> = input.split(' ').collect();
    if fields.len() != 5 {
        return Err(ParseError::FieldCount {
            found: fields.len(),
        });
    }

    let field_order = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];
    let mut selected: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());
    let mut day_star = [false; 2];

    for (index, (&field, &text)) in field_order.iter().zip(fields.iter()).enumerate() {
        let (values, exact_star) = parse_field(field, text)?;
        selected[index] = values;
        if field == Field::DayOfMonth {
            day_star[0] = exact_star;
        } else if field == Field::DayOfWeek {
            day_star[1] = exact_star;
        }
    }

    Ok(Cron {
        selected,
        day_star,
    })
}

fn main() {}