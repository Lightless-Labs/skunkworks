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
    minute: u64,
    hour: u32,
    day_of_month: u32,
    month: u16,
    day_of_week: u8,
    day_of_month_star: bool,
    day_of_week_star: bool,
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        match field {
            Field::Minute => bits_to_values(self.minute, 60),
            Field::Hour => bits_to_values(self.hour as u64, 24),
            Field::DayOfMonth => bits_to_values(self.day_of_month as u64, 32),
            Field::Month => bits_to_values(self.month as u64, 13),
            Field::DayOfWeek => bits_to_values(self.day_of_week as u64, 8),
        }
    }
}

fn bits_to_values(bits: u64, count: usize) -> Vec<u8> {
    (0..count)
        .filter_map(|n| ((bits >> n) & 1 == 1).then_some(n as u8))
        .collect()
}

fn field_data(field: Field) -> (u32, u32, u32) {
    match field {
        Field::Minute => (0, 59, 60),
        Field::Hour => (0, 23, 24),
        Field::DayOfMonth => (1, 31, 31),
        Field::Month => (1, 12, 12),
        Field::DayOfWeek => (0, 6, 7),
    }
}

fn make_error(field: Field, offset: usize, kind: u8) -> ParseError {
    match kind {
        0 => ParseError::InvalidCharacter { field, offset },
        1 => ParseError::EmptyItem { field, offset },
        2 => ParseError::InvalidSyntax { field, offset },
        3 => ParseError::LeadingZero { field, offset },
        4 => ParseError::NumberOverflow { field, offset },
        5 => ParseError::OutOfRange { field, offset },
        6 => ParseError::ReversedRange { field, offset },
        _ => ParseError::BadStep { field, offset },
    }
}

fn number(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: Field,
    item_offset: usize,
) -> Result<u32, ParseError> {
    let field_start = item_offset + start;
    if end - start > 1 && bytes[start] == b'0' {
        return Err(make_error(field, field_start, 3));
    }
    let mut result = 0u32;
    for &byte in &bytes[start..end] {
        result = result
            .checked_mul(10)
            .and_then(|n| n.checked_add((byte - b'0') as u32))
            .ok_or_else(|| make_error(field, field_start, 4))?;
    }
    Ok(result)
}

fn parse_one(text: &str, field: Field, bits: &mut u64) -> Result<bool, ParseError> {
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if !(b.is_ascii_digit() || b == b'*' || b == b'-' || b == b'/' || b == b',') {
            return Err(make_error(field, i, 0));
        }
    }

    let (min, max, width) = field_data(field);
    let mut item_offset = 0usize;
    let mut any_exact_star = false;

    for item in text.split(',') {
        let item_bytes = item.as_bytes();
        if item_bytes.is_empty() {
            return Err(make_error(field, item_offset, 1));
        }

        let mut slash = None;
        for (i, &b) in item_bytes.iter().enumerate() {
            if b == b'/' {
                if slash.is_some() {
                    return Err(make_error(field, item_offset, 2));
                }
                slash = Some(i);
            }
        }
        let base_end = slash.unwrap_or(item_bytes.len());
        let base = &item_bytes[..base_end];

        let (shape, hyphen) = if base == b"*" {
            (0, None)
        } else {
            let mut found = None;
            for (i, &b) in base.iter().enumerate() {
                if b == b'-' {
                    if found.is_some() {
                        return Err(make_error(field, item_offset, 2));
                    }
                    found = Some(i);
                }
            }
            match found {
                None if !base.is_empty() && base.iter().all(u8::is_ascii_digit) => (1, None),
                Some(i)
                    if i > 0
                        && i + 1 < base.len()
                        && base[..i].iter().all(u8::is_ascii_digit)
                        && base[i + 1..].iter().all(u8::is_ascii_digit) =>
                {
                    (2, Some(i))
                }
                _ => return Err(make_error(field, item_offset, 2)),
            }
        };

        let step_range = if let Some(slash_at) = slash {
            let from = slash_at + 1;
            if from == item_bytes.len()
                || !item_bytes[from..].iter().all(u8::is_ascii_digit)
            {
                return Err(make_error(field, item_offset, 2));
            }
            Some((from, item_bytes.len()))
        } else {
            None
        };

        let (start_value, end_value) = match shape {
            0 => (min, max),
            1 => {
                let n = number(item_bytes, 0, base_end, field, item_offset)?;
                if n < min || n > max {
                    return Err(make_error(field, item_offset, 5));
                }
                (n, if step_range.is_some() { max } else { n })
            }
            _ => {
                let h = hyphen.unwrap();
                let a = number(item_bytes, 0, h, field, item_offset)?;
                if a < min || a > max {
                    return Err(make_error(field, item_offset, 5));
                }
                let b = number(item_bytes, h + 1, base_end, field, item_offset)?;
                if b < min || b > max {
                    return Err(make_error(field, item_offset + h + 1, 5));
                }
                if a > b {
                    return Err(make_error(field, item_offset + h + 1, 6));
                }
                (a, b)
            }
        };

        let step = if let Some((from, to)) = step_range {
            let n = number(item_bytes, from, to, field, item_offset)?;
            if n == 0 || n > width {
                return Err(make_error(field, item_offset + from, 7));
            }
            n
        } else {
            1
        };

        let mut n = start_value;
        while n <= end_value {
            *bits |= 1u64 << n;
            match n.checked_add(step) {
                Some(next) => n = next,
                None => break,
            }
        }

        if item_offset == 0 && item_bytes == b"*" {
            any_exact_star = true;
        }
        item_offset += item_bytes.len() + 1;
    }

    Ok(any_exact_star && text == "*")
}

fn parse_field(text: &str, field: Field) -> Result<(u64, bool), ParseError> {
    let mut bits = 0u64;
    let star = parse_one(text, field, &mut bits)?;
    Ok((bits, star))
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let fields: Vec<&str> = input.split(' ').collect();
    if fields.len() != 5 {
        return Err(ParseError::FieldCount { found: fields.len() });
    }

    let order = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];

    let mut parsed = Vec::new();
    for (field, text) in order.into_iter().zip(fields.iter()) {
        parsed.push(parse_field(text, field)?);
    }

    Ok(Cron {
        minute: parsed[0].0,
        hour: parsed[1].0 as u32,
        day_of_month: parsed[2].0 as u32,
        month: parsed[3].0 as u16,
        day_of_week: parsed[4].0 as u8,
        day_of_month_star: parsed[2].1,
        day_of_week_star: parsed[4].1,
    })
}