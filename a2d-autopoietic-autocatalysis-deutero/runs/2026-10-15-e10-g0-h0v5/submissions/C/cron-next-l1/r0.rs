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
    day_wildcard: [bool; 2],
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

fn field_info(field: Field) -> (u32, u32) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
    }
}

fn error(field: Field, offset: usize, kind: u8) -> ParseError {
    match kind {
        0 => ParseError::InvalidSyntax { field, offset },
        1 => ParseError::LeadingZero { field, offset },
        2 => ParseError::NumberOverflow { field, offset },
        3 => ParseError::OutOfRange { field, offset },
        4 => ParseError::ReversedRange { field, offset },
        5 => ParseError::BadStep { field, offset },
        _ => ParseError::EmptyItem { field, offset },
    }
}

fn parse_number(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: Field,
    is_step: bool,
) -> Result<u32, ParseError> {
    if start == end {
        return Err(error(field, start, 0));
    }

    if end - start > 1 && bytes[start] == b'0' {
        return Err(error(field, start, 1));
    }

    let mut value = 0u32;
    for &byte in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or_else(|| error(field, start, 2))?;
    }

    let (min, max) = field_info(field);
    if is_step {
        let width = max - min + 1;
        if value == 0 || value > width {
            return Err(error(field, start, 5));
        }
    } else if value < min || value > max {
        return Err(error(field, start, 3));
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

    let (min, max) = field_info(field);
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

        let item = &bytes[item_start..item_end];
        let slash = item.iter().position(|&b| b == b'/');

        let (base_end, step_start) = match slash {
            Some(pos) => {
                if item[pos + 1..].contains(&b'/') {
                    return Err(error(field, item_start, 0));
                }
                (pos, Some(item_start + pos + 1))
            }
            None => (item.len(), None),
        };

        let base = &item[..base_end];
        let (is_star, range_dash) = if base == b"*" {
            (true, None)
        } else {
            if base.contains(&b'*') {
                return Err(error(field, item_start, 0));
            }
            let dashes: Vec<usize> = base
                .iter()
                .enumerate()
                .filter_map(|(i, &b)| if b == b'-' { Some(i) } else { None })
                .collect();
            if dashes.len() > 1 {
                return Err(error(field, item_start, 0));
            }
            (false, dashes.first().copied())
        };

        let (start_value, end_value, start_offset, end_offset) = if is_star {
            (min, max, item_start, item_start)
        } else if let Some(dash) = range_dash {
            if dash == 0 || dash + 1 == base.len() {
                return Err(error(field, item_start, 0));
            }
            let left = parse_number(bytes, item_start, item_start + dash, field, false)?;
            let right_start = item_start + dash + 1;
            let right = parse_number(bytes, right_start, item_end - (item.len() - base_end), field, false)?;
            if left > right {
                return Err(error(field, right_start, 4));
            }
            (left, right, item_start, right_start)
        } else {
            let value = parse_number(bytes, item_start, item_start + base.len(), field, false)?;
            let end = if step_start.is_some() { max } else { value };
            (value, end, item_start, item_start)
        };

        let step = if let Some(offset) = step_start {
            let value = parse_number(bytes, offset, item_end, field, true)?;
            value
        } else {
            1
        };

        let _ = (start_offset, end_offset);
        let mut value = start_value;
        while value <= end_value {
            selected[value as usize] = true;
            match value.checked_add(step) {
                Some(next) if next > value => value = next,
                _ => break,
            }
        }

        if item_end == bytes.len() {
            break;
        }
        item_start = item_end + 1;
    }

    Ok(selected
        .iter()
        .enumerate()
        .filter_map(|(value, &yes)| if yes { Some(value as u8) } else { None })
        .collect())
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let pieces: Vec<&str> = input.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount {
            found: pieces.len(),
        });
    }

    let fields = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];
    let mut selected: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());
    let mut day_wildcard = [false; 2];

    for i in 0..5 {
        if i == 2 {
            day_wildcard[0] = pieces[i] == "*";
        } else if i == 4 {
            day_wildcard[1] = pieces[i] == "*";
        }
        selected[i] = parse_field(pieces[i], fields[i])?;
    }

    Ok(Cron {
        selected,
        day_wildcard,
    })
}

fn main() {}