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

fn field_spec(index: usize) -> (Field, u8, u8) {
    match index {
        0 => (Field::Minute, 0, 59),
        1 => (Field::Hour, 0, 23),
        2 => (Field::DayOfMonth, 1, 31),
        3 => (Field::Month, 1, 12),
        _ => (Field::DayOfWeek, 0, 6),
    }
}

fn parse_number(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: Field,
    min: u8,
    max: u8,
    is_step: bool,
) -> Result<u32, ParseError> {
    if end - start > 1 && bytes[start] == b'0' {
        return Err(ParseError::LeadingZero { field, offset: start });
    }

    let mut value: u32 = 0;
    for &byte in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset: start })?;
    }

    if is_step {
        let width = (max - min + 1) as u32;
        if value == 0 || value > width {
            return Err(ParseError::BadStep { field, offset: start });
        }
    } else if value < min as u32 || value > max as u32 {
        return Err(ParseError::OutOfRange { field, offset: start });
    }

    Ok(value)
}

fn parse_field(input: &str, index: usize) -> Result<(Vec<u8>, bool), ParseError> {
    let (field, min, max) = field_spec(index);
    let bytes = input.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !(byte.is_ascii_digit() || matches!(byte, b'*' | b'-' | b'/' | b',')) {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let mut selected = vec![false; (max - min + 1) as usize];
    let mut item_start = 0usize;
    let mut exact_star_day = false;

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

        let item = &bytes[item_start..item_end];
        let slash_count = item.iter().filter(|&&b| b == b'/').count();

        let (base_start, base_end, step_range) = if slash_count == 0 {
            (item_start, item_end, None)
        } else if slash_count == 1 {
            let slash = item_start + item.iter().position(|&b| b == b'/').unwrap();
            if slash == item_start || slash + 1 == item_end {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            (item_start, slash, Some((slash + 1, item_end)))
        } else {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        };

        let base = &bytes[base_start..base_end];
        let is_star = base == b"*";
        let hyphens: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(base_start + i))
            .collect();

        let (start_range, end_range) = if is_star && hyphens.is_empty() {
            (None, None)
        } else if hyphens.is_empty() {
            if base.iter().all(u8::is_ascii_digit) {
                (Some((base_start, base_end)), None)
            } else {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
        } else if hyphens.len() == 1 {
            let hyphen = hyphens[0];
            let left = (base_start, hyphen);
            let right = (hyphen + 1, base_end);
            if left.0 == left.1
                || right.0 == right.1
                || !bytes[left.0..left.1].iter().all(u8::is_ascii_digit)
                || !bytes[right.0..right.1].iter().all(u8::is_ascii_digit)
            {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            (Some(left), Some(right))
        } else {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        };

        let step = if let Some((start, end)) = step_range {
            if start == end || !bytes[start..end].iter().all(u8::is_ascii_digit) {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            Some(parse_number(bytes, start, end, field, min, max, true)? as u8)
        } else {
            None
        };

        let (start, end) = if let Some(start_range) = start_range {
            let start = parse_number(
                bytes,
                start_range.0,
                start_range.1,
                field,
                min,
                max,
                false,
            )? as u8;
            let end = if let Some(end_range) = end_range {
                parse_number(
                    bytes,
                    end_range.0,
                    end_range.1,
                    field,
                    min,
                    max,
                    false,
                )? as u8
            } else if step.is_some() {
                max
            } else {
                start
            };
            if end_range.is_some() && start > end {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: end_range.unwrap().0,
                });
            }
            (start, end)
        } else {
            (min, max)
        };

        let stride = step.unwrap_or(1);
        let mut value = start;
        while value <= end {
            selected[(value - min) as usize] = true;
            match value.checked_add(stride) {
                Some(next) if next > value => value = next,
                _ => break,
            }
        }

        if index == 2 || index == 4 {
            if item_start == 0 && item_end == bytes.len() && item == b"*" {
                exact_star_day = true;
            }
        }

        if item_end == bytes.len() {
            break;
        }
        item_start = item_end + 1;
    }

    let values = selected
        .iter()
        .enumerate()
        .filter_map(|(i, &yes)| yes.then_some(min + i as u8))
        .collect();

    Ok((values, exact_star_day))
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut selected: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());
    let mut day_stars = [false; 2];

    for index in 0..5 {
        let (values, exact_star) = parse_field(parts[index], index)?;
        selected[index] = values;
        if index == 2 {
            day_stars[0] = exact_star;
        } else if index == 4 {
            day_stars[1] = exact_star;
        }
    }

    Ok(Cron {
        selected,
        day_stars,
    })
}

fn main() {}