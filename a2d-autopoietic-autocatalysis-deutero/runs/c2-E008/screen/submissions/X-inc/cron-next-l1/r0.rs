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

fn field_domain(field: Field) -> (u32, u32) {
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
) -> Result<u32, ParseError> {
    let number_start = start;
    if end - start > 1 && bytes[start] == b'0' {
        return Err(ParseError::LeadingZero {
            field,
            offset: number_start,
        });
    }

    let mut value = 0u32;
    for &byte in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow {
                field,
                offset: number_start,
            })?;
    }
    Ok(value)
}

fn parse_field(input: &str, field: Field) -> Result<(Vec<u8>, bool), ParseError> {
    let bytes = input.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (domain_min, domain_max) = field_domain(field);
    let width = domain_max - domain_min + 1;
    let mut selected = vec![false; (domain_max + 1) as usize];
    let mut item_start = 0usize;
    let mut day_field_exact_star = false;

    for item_end in (0..=bytes.len())
        .filter(|&i| i == bytes.len() || bytes[i] == b',')
    {
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

        let (base_end, step_start) = if let Some(&slash) = slash_positions.first() {
            (slash, Some(slash + 1))
        } else {
            (item.len(), None)
        };

        let base = &item[..base_end];
        let is_star = base == b"*";
        let mut range_start = None;
        let mut range_end = None;

        if is_star {
            // The only valid starred base is exactly '*'.
        } else {
            let hyphens: Vec<usize> = base
                .iter()
                .enumerate()
                .filter_map(|(i, &b)| (b == b'-').then_some(i))
                .collect();

            if hyphens.is_empty() {
                if base.is_empty() || !base.iter().all(u8::is_ascii_digit) {
                    return Err(ParseError::InvalidSyntax {
                        field,
                        offset: item_start,
                    });
                }
                range_start = Some((0usize, base.len()));
            } else if hyphens.len() == 1 {
                let hyphen = hyphens[0];
                let left = &base[..hyphen];
                let right = &base[hyphen + 1..];
                if left.is_empty()
                    || right.is_empty()
                    || !left.iter().all(u8::is_ascii_digit)
                    || !right.iter().all(u8::is_ascii_digit)
                {
                    return Err(ParseError::InvalidSyntax {
                        field,
                        offset: item_start,
                    });
                }
                range_start = Some((0usize, hyphen));
                range_end = Some((hyphen + 1, base.len()));
            } else {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
        }

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

        let mut start_value = domain_min;
        let mut end_value = domain_max;

        if let Some((start, end)) = range_start {
            let value = parse_number(bytes, item_start + start, item_start + end, field)?;
            if value < domain_min || value > domain_max {
                return Err(ParseError::OutOfRange {
                    field,
                    offset: item_start + start,
                });
            }
            start_value = value;

            if let Some((end_start, end_end)) = range_end {
                let value = parse_number(
                    bytes,
                    item_start + end_start,
                    item_start + end_end,
                    field,
                )?;
                if value < domain_min || value > domain_max {
                    return Err(ParseError::OutOfRange {
                        field,
                        offset: item_start + end_start,
                    });
                }
                if start_value > value {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: item_start + end_start,
                    });
                }
                end_value = value;
            } else if step_start.is_none() {
                end_value = start_value;
            }
        }

        let step = if let Some(step_offset) = step_start {
            let value = parse_number(
                bytes,
                item_start + step_offset,
                item_end,
                field,
            )?;
            if value == 0 || value > width {
                return Err(ParseError::BadStep {
                    field,
                    offset: item_start + step_offset,
                });
            }
            value
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

        if field == Field::DayOfMonth && item_start == 0 && item_end == 1 && item == b"*" {
            day_field_exact_star = true;
        }
        if field == Field::DayOfWeek && item_start == 0 && item_end == 1 && item == b"*" {
            day_field_exact_star = true;
        }

        item_start = item_end + 1;
    }

    let values = (domain_min..=domain_max)
        .filter(|&value| selected[value as usize])
        .map(|value| value as u8)
        .collect();

    Ok((values, day_field_exact_star))
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let fields: Vec<&str> = input.split(' ').collect();
    if fields.len() != 5 {
        return Err(ParseError::FieldCount {
            found: fields.len(),
        });
    }

    let order = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];
    let mut selected: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());
    let mut day_of_month_is_star = false;
    let mut day_of_week_is_star = false;

    for (index, (&field, &text)) in order.iter().zip(fields.iter()).enumerate() {
        let (values, exact_star) = parse_field(text, field)?;
        selected[index] = values;
        match field {
            Field::DayOfMonth => day_of_month_is_star = exact_star,
            Field::DayOfWeek => day_of_week_is_star = exact_star,
            _ => {}
        }
    }

    Ok(Cron {
        selected,
        day_of_month_is_star,
        day_of_week_is_star,
    })
}