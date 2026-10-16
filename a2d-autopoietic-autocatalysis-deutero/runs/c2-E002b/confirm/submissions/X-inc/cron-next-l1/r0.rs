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
    day_of_month_is_exact_star: bool,
    day_of_week_is_exact_star: bool,
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

fn field_spec(field: Field) -> (u32, u32, u32) {
    match field {
        Field::Minute => (0, 59, 60),
        Field::Hour => (0, 23, 24),
        Field::DayOfMonth => (1, 31, 31),
        Field::Month => (1, 12, 12),
        Field::DayOfWeek => (0, 6, 7),
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
    offset: usize,
    field: Field,
    max: u32,
    step: bool,
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

    if step {
        if value == 0 || value > max {
            return Err(error(field, offset, ErrorKind::BadStep));
        }
    } else if value > max {
        return Err(error(field, offset, ErrorKind::OutOfRange));
    }

    Ok(value)
}

fn parse_field(
    text: &str,
    field: Field,
) -> Result<(Vec<u8>, bool), ParseError> {
    let bytes = text.as_bytes();
    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(error(field, offset, ErrorKind::InvalidCharacter));
        }
    }

    let (minimum, maximum, width) = field_spec(field);
    let mut selected = vec![false; (maximum + 1) as usize];

    let mut item_start = 0usize;
    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|relative| item_start + relative)
            .unwrap_or(bytes.len());

        if item_start == item_end {
            return Err(error(field, item_start, ErrorKind::EmptyItem));
        }

        let item = &bytes[item_start..item_end];
        let slash_positions: Vec<usize> = item
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| if b == b'/' { Some(i) } else { None })
            .collect();

        if slash_positions.len() > 1 {
            return Err(error(field, item_start, ErrorKind::InvalidSyntax));
        }

        let (base, step_part) = if let Some(&slash) = slash_positions.first() {
            (&item[..slash], Some((slash + 1, &item[slash + 1..])))
        } else {
            (item, None)
        };

        let star = base == b"*";
        let hyphen_positions: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| if b == b'-' { Some(i) } else { None })
            .collect();

        let base_kind = if star && hyphen_positions.is_empty() {
            BaseKind::Star
        } else if hyphen_positions.is_empty() && !base.is_empty() && base.iter().all(u8::is_ascii_digit) {
            BaseKind::Number
        } else if hyphen_positions.len() == 1 {
            let hyphen = hyphen_positions[0];
            if hyphen == 0
                || hyphen + 1 == base.len()
                || !base[..hyphen].iter().all(u8::is_ascii_digit)
                || !base[hyphen + 1..].iter().all(u8::is_ascii_digit)
            {
                return Err(error(field, item_start, ErrorKind::InvalidSyntax));
            }
            BaseKind::Range(hyphen)
        } else {
            return Err(error(field, item_start, ErrorKind::InvalidSyntax));
        };

        if let Some((_, step_bytes)) = step_part {
            if step_bytes.is_empty() || !step_bytes.iter().all(u8::is_ascii_digit) {
                return Err(error(field, item_start, ErrorKind::InvalidSyntax));
            }
        }

        let (start, end, end_offset) = match base_kind {
            BaseKind::Star => (minimum, maximum, item_start),
            BaseKind::Number => {
                let start = parse_number(base, item_start, field, maximum, false)?;
                (start, if step_part.is_some() { maximum } else { start }, item_start)
            }
            BaseKind::Range(hyphen) => {
                let start = parse_number(&base[..hyphen], item_start, field, maximum, false)?;
                let end_offset = item_start + hyphen + 1;
                let end = parse_number(&base[hyphen + 1..], end_offset, field, maximum, false)?;
                if start > end {
                    return Err(error(field, end_offset, ErrorKind::ReversedRange));
                }
                (start, end, end_offset)
            }
        };

        let step = if let Some((step_offset, step_bytes)) = step_part {
            parse_number(step_bytes, item_start + step_offset, field, width, true)?
        } else {
            1
        };

        let mut value = start;
        while value <= end {
            selected[value as usize] = true;
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
            }
        }

        let _ = end_offset;

        if item_end == bytes.len() {
            break;
        }
        item_start = item_end + 1;
    }

    let values = selected
        .iter()
        .enumerate()
        .filter_map(|(value, &is_selected)| {
            if is_selected {
                Some(value as u8)
            } else {
                None
            }
        })
        .collect();

    Ok((values, text == "*"))
}

enum BaseKind {
    Star,
    Number,
    Range(usize),
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
    let mut exact_stars = [false; 5];

    for (index, (&field, &part)) in fields.iter().zip(parts.iter()).enumerate() {
        let (values, exact_star) = parse_field(part, field)?;
        selected[index] = values;
        exact_stars[index] = exact_star;
    }

    Ok(Cron {
        selected,
        day_of_month_is_exact_star: exact_stars[2],
        day_of_week_is_exact_star: exact_stars[4],
    })
}