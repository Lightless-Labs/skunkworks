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
    day_stars: [bool; 2],
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

fn field_domain(field: Field) -> (u32, u32) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
    }
}

fn number(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: Field,
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
    Ok(value)
}

fn parse_field(text: &str, field: Field) -> Result<(Vec<u8>, bool), ParseError> {
    let bytes = text.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (domain_min, domain_max) = field_domain(field);
    let width = domain_max - domain_min + 1;
    let mut selected = vec![false; (domain_max + 1) as usize];
    let day_star = matches!(field, Field::DayOfMonth | Field::DayOfWeek) && bytes == b"*";

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

        let mut slash = None;
        let mut hyphen = None;
        for (relative, &byte) in bytes[item_start..item_end].iter().enumerate() {
            let pos = item_start + relative;
            match byte {
                b'/' => {
                    if slash.replace(pos).is_some() {
                        return Err(ParseError::InvalidSyntax {
                            field,
                            offset: item_start,
                        });
                    }
                }
                b'-' => {
                    if hyphen.replace(pos).is_some() {
                        return Err(ParseError::InvalidSyntax {
                            field,
                            offset: item_start,
                        });
                    }
                }
                _ => {}
            }
        }

        let base_end = slash.unwrap_or(item_end);
        if let Some(h) = hyphen {
            if h >= base_end {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
        }

        let is_star = base_end - item_start == 1 && bytes[item_start] == b'*';
        let (start_value, end_value, end_number_start) = if is_star {
            (domain_min, domain_max, None)
        } else if base_end - item_start == 0 {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        } else if let Some(h) = hyphen {
            if bytes[item_start..h].contains(&b'*') || bytes[h + 1..base_end].contains(&b'*') {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            let start = number(bytes, item_start, h, field)?;
            if start < domain_min || start > domain_max {
                return Err(ParseError::OutOfRange {
                    field,
                    offset: item_start,
                });
            }
            let end = number(bytes, h + 1, base_end, field)?;
            if end < domain_min || end > domain_max {
                return Err(ParseError::OutOfRange {
                    field,
                    offset: h + 1,
                });
            }
            if start > end {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: h + 1,
                });
            }
            (start, end, Some(h + 1))
        } else {
            if bytes[item_start..base_end].contains(&b'*') {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            let value = number(bytes, item_start, base_end, field)?;
            if value < domain_min || value > domain_max {
                return Err(ParseError::OutOfRange {
                    field,
                    offset: item_start,
                });
            }
            let end = if slash.is_some() { domain_max } else { value };
            (value, end, None)
        };

        let step = if let Some(slash_pos) = slash {
            if slash_pos + 1 == item_end {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            if bytes[slash_pos + 1..item_end].contains(&b'*')
                || bytes[slash_pos + 1..item_end].contains(&b'-')
                || bytes[slash_pos + 1..item_end].contains(&b'/')
            {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            let value = number(bytes, slash_pos + 1, item_end, field)?;
            if value == 0 || value > width {
                return Err(ParseError::BadStep {
                    field,
                    offset: slash_pos + 1,
                });
            }
            value
        } else {
            1
        };

        let _ = end_number_start;
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

    Ok((
        selected
            .iter()
            .enumerate()
            .filter_map(|(value, &yes)| yes.then_some(value as u8))
            .collect(),
        day_star,
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
    let mut values: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());
    let mut day_stars = [false; 2];

    for (index, (&text, &field)) in fields.iter().zip(kinds.iter()).enumerate() {
        let (allowed, is_day_star) = parse_field(text, field)?;
        values[index] = allowed;
        if field == Field::DayOfMonth {
            day_stars[0] = is_day_star;
        } else if field == Field::DayOfWeek {
            day_stars[1] = is_day_star;
        }
    }

    Ok(Cron {
        allowed: values,
        day_stars,
    })
}

fn main() {}