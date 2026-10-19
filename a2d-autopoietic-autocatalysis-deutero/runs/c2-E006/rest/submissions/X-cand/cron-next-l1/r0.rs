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
    selected: [Vec<bool>; 5],
    day_of_month_is_star: bool,
    day_of_week_is_star: bool,
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let index = field_index(field);
        self.selected[index]
            .iter()
            .enumerate()
            .filter_map(|(value, selected)| selected.then_some(value as u8))
            .collect()
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

fn domain(field: Field) -> (u32, u32) {
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
    step: bool,
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
            .and_then(|n| n.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow {
                field,
                offset: start,
            })?;
    }

    let (min, max) = domain(field);
    if step {
        let width = max - min + 1;
        if value == 0 || value > width {
            return Err(ParseError::BadStep {
                field,
                offset: start,
            });
        }
    } else if value < min || value > max {
        return Err(ParseError::OutOfRange {
            field,
            offset: start,
        });
    }

    Ok(value)
}

fn parse_item(
    bytes: &[u8],
    item_start: usize,
    item_end: usize,
    field: Field,
    selected: &mut [bool],
) -> Result<(), ParseError> {
    if item_start == item_end {
        return Err(ParseError::EmptyItem {
            field,
            offset: item_start,
        });
    }

    let slash = bytes[item_start..item_end]
        .iter()
        .position(|&b| b == b'/')
        .map(|p| item_start + p);

    let base_end = slash.unwrap_or(item_end);
    let step_span = slash.map(|p| (p + 1, item_end));

    let is_star = base_end == item_start + 1 && bytes[item_start] == b'*';
    let hyphen = bytes[item_start..base_end]
        .iter()
        .position(|&b| b == b'-')
        .map(|p| item_start + p);

    let base_valid = if is_star {
        true
    } else if let Some(h) = hyphen {
        h > item_start
            && h + 1 < base_end
            && bytes[item_start..h].iter().all(u8::is_ascii_digit)
            && bytes[h + 1..base_end].iter().all(u8::is_ascii_digit)
    } else {
        item_start < base_end
            && bytes[item_start..base_end].iter().all(u8::is_ascii_digit)
    };

    let step_valid = match step_span {
        None => true,
        Some((start, end)) => {
            start < end && bytes[start..end].iter().all(u8::is_ascii_digit)
        }
    };

    if !base_valid || !step_valid {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_start,
        });
    }

    let (min, max) = domain(field);
    let (start, end) = if is_star {
        (min, max)
    } else if let Some(h) = hyphen {
        let range_start = parse_number(bytes, item_start, h, field, false)?;
        let range_end = parse_number(bytes, h + 1, base_end, field, false)?;
        if range_start > range_end {
            return Err(ParseError::ReversedRange {
                field,
                offset: h + 1,
            });
        }
        (range_start, range_end)
    } else {
        let value = parse_number(bytes, item_start, base_end, field, false)?;
        (value, if step_span.is_some() { max } else { value })
    };

    let step = if let Some((step_start, step_end)) = step_span {
        parse_number(bytes, step_start, step_end, field, true)?
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

    Ok(())
}

fn parse_field(field_text: &str, field: Field) -> Result<Vec<bool>, ParseError> {
    let bytes = field_text.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (_, max) = domain(field);
    let mut selected = vec![false; (max + 1) as usize];

    let mut item_start = 0;
    for item_end in (0..=bytes.len()).filter(|&i| i == bytes.len() || bytes[i] == b',') {
        parse_item(bytes, item_start, item_end, field, &mut selected)?;
        item_start = item_end + 1;
    }

    Ok(selected)
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
    for (part, field) in parts.iter().zip(fields) {
        selected.push(parse_field(part, field)?);
    }

    Ok(Cron {
        selected: selected.try_into().ok().unwrap(),
        day_of_month_is_star: parts[2].as_bytes() == b"*",
        day_of_week_is_star: parts[4].as_bytes() == b"*",
    })
}