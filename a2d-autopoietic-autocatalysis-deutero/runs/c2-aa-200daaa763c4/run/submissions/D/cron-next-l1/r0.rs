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
    allowed: [Vec<u8>; 5],
    day_star: [bool; 2],
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.allowed[field_index(field)].clone()
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

fn parse_number(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: Field,
    is_step: bool,
) -> Result<u32, ParseError> {
    let fail = |kind| match kind {
        0 => ParseError::LeadingZero { field, offset: start },
        1 => ParseError::NumberOverflow { field, offset: start },
        2 => ParseError::BadStep { field, offset: start },
        _ => ParseError::OutOfRange { field, offset: start },
    };

    if start == end {
        return Err(ParseError::InvalidSyntax { field, offset: start });
    }
    if end - start > 1 && bytes[start] == b'0' {
        return Err(fail(0));
    }

    let mut value = 0u32;
    for &byte in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or_else(|| fail(1))?;
    }

    if is_step {
        let (_, max) = bounds(field);
        let width = max - bounds(field).0 + 1;
        if value == 0 || value > width {
            return Err(fail(2));
        }
    } else {
        let (min, max) = bounds(field);
        if value < min || value > max {
            return Err(fail(3));
        }
    }
    Ok(value)
}

fn parse_field(text: &str, field: Field) -> Result<(Vec<u8>, bool), ParseError> {
    let bytes = text.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (domain_min, domain_max) = bounds(field);
    let mut selected = vec![false; (domain_max - domain_min + 1) as usize];
    let is_exact_star = text == "*";

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
        let slash_positions: Vec<usize> = item
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'/').then_some(i))
            .collect();

        if slash_positions.len() > 1 {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let (base_end, step_range) = if let Some(&slash) = slash_positions.first() {
            (slash, Some((item_start + slash + 1, item_end)))
        } else {
            (item.len(), None)
        };

        let base = &item[..base_end];
        let hyphens: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();

        let syntax_valid = if base == b"*" {
            true
        } else if hyphens.is_empty() {
            !base.is_empty() && base.iter().all(u8::is_ascii_digit)
        } else if hyphens.len() == 1 {
            let h = hyphens[0];
            h > 0
                && h + 1 < base.len()
                && base[..h].iter().all(u8::is_ascii_digit)
                && base[h + 1..].iter().all(u8::is_ascii_digit)
        } else {
            false
        };

        if !syntax_valid {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let parsed_step = if let Some((step_start, step_end)) = step_range {
            Some(parse_number(
                bytes,
                step_start,
                step_end,
                field,
                true,
            )?)
        } else {
            None
        };

        let (start, end, stepped) = if base == b"*" {
            (domain_min, domain_max, parsed_step.is_some())
        } else if hyphens.is_empty() {
            let number = parse_number(
                bytes,
                item_start,
                item_start + base.len(),
                field,
                false,
            )?;
            (
                number,
                if parsed_step.is_some() {
                    domain_max
                } else {
                    number
                },
                parsed_step.is_some(),
            )
        } else {
            let h = hyphens[0];
            let start = parse_number(
                bytes,
                item_start,
                item_start + h,
                field,
                false,
            )?;
            let end_start = item_start + h + 1;
            let end = parse_number(bytes, end_start, item_end - usize::from(step_range.is_some()), field, false)?;

            if start > end {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: end_start,
                });
            }
            (start, end, parsed_step.is_some())
        };

        let step = parsed_step.unwrap_or(1);
        let mut value = start;
        while value <= end {
            selected[(value - domain_min) as usize] = true;
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
            .filter_map(|(i, &yes)| yes.then_some(domain_min as u8 + i as u8))
            .collect(),
        is_exact_star,
    ))
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let fields: Vec<&str> = input.split(' ').collect();
    if fields.len() != 5 {
        return Err(ParseError::FieldCount {
            found: fields.len(),
        });
    }

    let kinds = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];

    let mut allowed: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());
    let mut day_star = [false; 2];

    for (index, (&text, &field)) in fields.iter().zip(kinds.iter()).enumerate() {
        let (values, exact_star) = parse_field(text, field)?;
        allowed[index] = values;
        if index == 2 {
            day_star[0] = exact_star;
        } else if index == 4 {
            day_star[1] = exact_star;
        }
    }

    Ok(Cron { allowed, day_star })
}