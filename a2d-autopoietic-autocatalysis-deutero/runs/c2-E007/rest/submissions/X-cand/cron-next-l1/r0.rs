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
    day_of_month_is_star: bool,
    day_of_week_is_star: bool,
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
    start: usize,
    end: usize,
    field: Field,
    step: bool,
) -> Result<u32, ParseError> {
    if end <= start {
        return Err(error(field, start, ErrorKind::InvalidSyntax));
    }

    if end - start > 1 && bytes[start] == b'0' {
        return Err(error(field, start, ErrorKind::LeadingZero));
    }

    let mut value = 0u32;
    for &byte in &bytes[start..end] {
        let digit = (byte - b'0') as u32;
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add(digit))
            .ok_or_else(|| error(field, start, ErrorKind::NumberOverflow))?;
    }

    let (min, max) = field_info(field);
    if step {
        let width = max - min + 1;
        if value == 0 || value > width {
            return Err(error(field, start, ErrorKind::BadStep));
        }
    } else if value < min || value > max {
        return Err(error(field, start, ErrorKind::OutOfRange));
    }

    Ok(value)
}

fn parse_field(field: Field, text: &str) -> Result<Vec<u8>, ParseError> {
    let bytes = text.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(error(field, offset, ErrorKind::InvalidCharacter));
        }
    }

    let (min, max) = field_info(field);
    let mut selected = vec![false; (max + 1) as usize];

    let mut item_start = 0;
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
            .filter_map(|(i, &b)| (b == b'/').then_some(i))
            .collect();

        if slash_positions.len() > 1 {
            return Err(error(field, item_start, ErrorKind::InvalidSyntax));
        }

        let (base_end, step_range) = if let Some(&slash) = slash_positions.first() {
            (slash, Some((slash + 1, item.len())))
        } else {
            (item.len(), None)
        };

        let hyphens: Vec<usize> = item[..base_end]
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();

        let is_star = base_end == 1 && item[0] == b'*';
        let (range_start, range_end) = if is_star {
            if !hyphens.is_empty() {
                return Err(error(field, item_start, ErrorKind::InvalidSyntax));
            }
            (None, None)
        } else if hyphens.is_empty() {
            if item[..base_end].is_empty()
                || !item[..base_end].iter().all(u8::is_ascii_digit)
            {
                return Err(error(field, item_start, ErrorKind::InvalidSyntax));
            }
            (Some((0, base_end)), None)
        } else if hyphens.len() == 1 {
            let hyphen = hyphens[0];
            if hyphen == 0
                || hyphen + 1 == base_end
                || !item[..hyphen].iter().all(u8::is_ascii_digit)
                || !item[hyphen + 1..base_end].iter().all(u8::is_ascii_digit)
            {
                return Err(error(field, item_start, ErrorKind::InvalidSyntax));
            }
            (Some((0, hyphen)), Some((hyphen + 1, base_end)))
        } else {
            return Err(error(field, item_start, ErrorKind::InvalidSyntax));
        };

        if let Some((step_start, step_end)) = step_range {
            if step_start == step_end
                || !item[step_start..step_end].iter().all(u8::is_ascii_digit)
            {
                return Err(error(field, item_start, ErrorKind::InvalidSyntax));
            }
        }

        let mut start_value = min;
        let mut end_value = max;

        if let Some((start, end)) = range_start {
            start_value = parse_number(bytes, item_start + start, item_start + end, field, false)?;
            if let Some((end_start, end_end)) = range_end {
                end_value =
                    parse_number(bytes, item_start + end_start, item_start + end_end, field, false)?;
                if start_value > end_value {
                    return Err(error(
                        field,
                        item_start + end_start,
                        ErrorKind::ReversedRange,
                    ));
                }
            } else if step_range.is_none() {
                end_value = start_value;
            }
        }

        let step = if let Some((step_start, step_end)) = step_range {
            parse_number(
                bytes,
                item_start + step_start,
                item_start + step_end,
                field,
                true,
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

    Ok(selected
        .iter()
        .enumerate()
        .filter_map(|(value, &present)| present.then_some(value as u8))
        .collect())
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
    let mut parsed = Vec::with_capacity(5);

    for (field, text) in field_kinds.into_iter().zip(fields.iter().copied()) {
        parsed.push(parse_field(field, text)?);
    }

    Ok(Cron {
        selected: [
            parsed.remove(0),
            parsed.remove(0),
            parsed.remove(0),
            parsed.remove(0),
            parsed.remove(0),
        ],
        day_of_month_is_star: fields[2] == "*",
        day_of_week_is_star: fields[4] == "*",
    })
}