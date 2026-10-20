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
    exact_day_stars: [bool; 2],
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

fn bounds(field: Field) -> (u32, u32) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
    }
}

fn error(field: Field, offset: usize, kind: ErrorKind) -> ParseError {
    match kind {
        ErrorKind::InvalidCharacter => ParseError::InvalidCharacter { field, offset },
        ErrorKind::EmptyItem => ParseError::EmptyItem { field, offset },
        ErrorKind::InvalidSyntax => ParseError::InvalidSyntax { field, offset },
        ErrorKind::LeadingZero => ParseError::LeadingZero { field, offset },
        ErrorKind::NumberOverflow => ParseError::NumberOverflow { field, offset },
        ErrorKind::OutOfRange => ParseError::OutOfRange { field, offset },
        ErrorKind::ReversedRange => ParseError::ReversedRange { field, offset },
        ErrorKind::BadStep => ParseError::BadStep { field, offset },
    }
}

#[derive(Clone, Copy)]
enum ErrorKind {
    InvalidCharacter,
    EmptyItem,
    InvalidSyntax,
    LeadingZero,
    NumberOverflow,
    OutOfRange,
    ReversedRange,
    BadStep,
}

fn parse_number(
    bytes: &[u8],
    field: Field,
    offset: usize,
    number_kind: ErrorKind,
) -> Result<u32, ParseError> {
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(error(field, offset, ErrorKind::LeadingZero));
    }

    let mut value = 0u32;
    for &byte in bytes {
        let digit = (byte - b'0') as u32;
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(digit))
            .ok_or_else(|| error(field, offset, ErrorKind::NumberOverflow))?;
    }

    match number_kind {
        ErrorKind::OutOfRange => {
            let (min, max) = bounds(field);
            if value < min || value > max {
                return Err(error(field, offset, ErrorKind::OutOfRange));
            }
        }
        ErrorKind::BadStep => {
            let (_, max) = bounds(field);
            if value == 0 || value > max + 1 {
                return Err(error(field, offset, ErrorKind::BadStep));
            }
        }
        _ => {}
    }

    Ok(value)
}

fn parse_field(input: &str, field: Field) -> Result<Vec<u8>, ParseError> {
    let bytes = input.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !(byte.is_ascii_digit() || matches!(byte, b'*' | b'-' | b'/' | b',')) {
            return Err(error(field, offset, ErrorKind::InvalidCharacter));
        }
    }

    let (domain_min, domain_max) = bounds(field);
    let width = domain_max - domain_min + 1;
    let mut selected = vec![false; (domain_max + 1) as usize];

    let mut item_start = 0usize;
    for item in input.split(',') {
        let item_bytes = item.as_bytes();
        if item_bytes.is_empty() {
            return Err(error(field, item_start, ErrorKind::EmptyItem));
        }

        let slash_positions: Vec<usize> = item_bytes
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'/').then_some(i))
            .collect();

        let (base, step_bytes, step_offset) = match slash_positions.as_slice() {
            [] => (item_bytes, None, 0),
            [slash] => (
                &item_bytes[..*slash],
                Some(&item_bytes[*slash + 1..]),
                item_start + slash + 1,
            ),
            _ => return Err(error(field, item_start, ErrorKind::InvalidSyntax)),
        };

        let is_star = base == b"*";
        let hyphen_positions: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();

        let syntax_valid = if is_star {
            true
        } else if hyphen_positions.is_empty() {
            !base.is_empty() && base.iter().all(u8::is_ascii_digit)
        } else if hyphen_positions.len() == 1 {
            let hyphen = hyphen_positions[0];
            hyphen > 0
                && hyphen + 1 < base.len()
                && base[..hyphen].iter().all(u8::is_ascii_digit)
                && base[hyphen + 1..].iter().all(u8::is_ascii_digit)
        } else {
            false
        };

        if !syntax_valid || step_bytes.is_some_and(|s| s.is_empty() || !s.iter().all(u8::is_ascii_digit)) {
            return Err(error(field, item_start, ErrorKind::InvalidSyntax));
        }

        let (start, end, end_offset) = if is_star {
            (domain_min, domain_max, item_start)
        } else if let Some(&hyphen) = hyphen_positions.first() {
            let start_value = parse_number(
                &base[..hyphen],
                field,
                item_start,
                ErrorKind::OutOfRange,
            )?;
            let end_value = parse_number(
                &base[hyphen + 1..],
                field,
                item_start + hyphen + 1,
                ErrorKind::OutOfRange,
            )?;
            if start_value > end_value {
                return Err(error(
                    field,
                    item_start + hyphen + 1,
                    ErrorKind::ReversedRange,
                ));
            }
            (start_value, end_value, item_start + hyphen + 1)
        } else {
            let value = parse_number(base, field, item_start, ErrorKind::OutOfRange)?;
            (value, value, item_start)
        };

        let step = if let Some(step_bytes) = step_bytes {
            parse_number(step_bytes, field, step_offset, ErrorKind::BadStep)?
        } else {
            1
        };

        let effective_end = if !is_star && hyphen_positions.is_empty() && step_bytes.is_some() {
            domain_max
        } else {
            end
        };

        let mut value = start;
        while value <= effective_end {
            selected[value as usize] = true;
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
            }
        }

        let _ = end_offset;
        item_start += item_bytes.len() + 1;
    }

    Ok(selected
        .into_iter()
        .enumerate()
        .filter_map(|(value, is_selected)| is_selected.then_some(value as u8))
        .collect())
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

    for (index, (&field, part)) in fields.iter().zip(parts.iter()).enumerate() {
        selected[index] = parse_field(part, field)?;
    }

    Ok(Cron {
        selected,
        exact_day_stars: [parts[2] == "*", parts[4] == "*"],
    })
}