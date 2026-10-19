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
    day_of_month_unrestricted: bool,
    day_of_week_unrestricted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CivilDateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatePart {
    Year,
    Month,
    Day,
    Hour,
    Minute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateError {
    InvalidDateTime { component: DatePart },
}

const FIELDS: [Field; 5] = [
    Field::Minute,
    Field::Hour,
    Field::DayOfMonth,
    Field::Month,
    Field::DayOfWeek,
];

fn bounds(field: Field) -> (u32, u32) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
    }
}

fn name_value(field: Field, bytes: &[u8]) -> Option<u32> {
    let names: &[&[u8]] = match field {
        Field::Month => &[
            b"JAN", b"FEB", b"MAR", b"APR", b"MAY", b"JUN",
            b"JUL", b"AUG", b"SEP", b"OCT", b"NOV", b"DEC",
        ],
        Field::DayOfWeek => &[b"SUN", b"MON", b"TUE", b"WED", b"THU", b"FRI", b"SAT"],
        _ => return None,
    };
    names.iter().position(|name| {
        bytes.len() == name.len()
            && bytes
                .iter()
                .zip(name.iter())
                .all(|(a, b)| a.to_ascii_uppercase() == *b)
    }).map(|i| {
        if field == Field::Month {
            (i + 1) as u32
        } else {
            i as u32
        }
    })
}

fn is_number(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit)
}

fn parse_number(
    field: Field,
    bytes: &[u8],
    offset: usize,
    low: u32,
    high: u32,
    step: bool,
) -> Result<u32, ParseError> {
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }
    let mut value = 0u32;
    for &b in bytes {
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    if step {
        if value < 1 || value > high {
            return Err(ParseError::BadStep { field, offset });
        }
    } else if value < low || value > high {
        return Err(ParseError::OutOfRange { field, offset });
    }
    Ok(value)
}

fn parse_endpoint(
    field: Field,
    bytes: &[u8],
    offset: usize,
) -> Result<u32, ParseError> {
    let (low, high) = bounds(field);
    if is_number(bytes) {
        parse_number(field, bytes, offset, low, high, false)
    } else {
        name_value(field, bytes).ok_or(ParseError::InvalidSyntax { field, offset })
    }
}

fn parse_field(field: Field, text: &str) -> Result<Vec<bool>, ParseError> {
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if !(b.is_ascii_digit()
            || b.is_ascii_alphabetic()
            || matches!(b, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset: i });
        }
    }

    let (low, high) = bounds(field);
    let mut selected = vec![false; (high + 1) as usize];

    let mut item_start = 0;
    for item in text.split(',') {
        let item_bytes = item.as_bytes();
        if item_bytes.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let slash_positions: Vec<usize> = item_bytes
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

        let (base, step_part, step_offset) = if let Some(&slash) = slash_positions.first() {
            (&item_bytes[..slash], Some(&item_bytes[slash + 1..]), item_start + slash + 1)
        } else {
            (item_bytes, None, 0)
        };

        let hyphens: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();

        if hyphens.len() > 1 || base.is_empty() {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let (start_bytes, end_bytes) = if base == b"*" {
            (None, None)
        } else if let Some(&hyphen) = hyphens.first() {
            if hyphen == 0 || hyphen + 1 == base.len() {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            let left = &base[..hyphen];
            let right = &base[hyphen + 1..];
            if left == b"*" || right == b"*" {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            (Some(left), Some(right))
        } else {
            (Some(base), None)
        };

        if let Some(step) = step_part {
            if step.is_empty() || !is_number(step) {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
        }

        for endpoint in [start_bytes, end_bytes].into_iter().flatten() {
            if !is_number(endpoint) && name_value(field, endpoint).is_none() {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
        }

        let start_value = match start_bytes {
            None => low,
            Some(part) => {
                let offset = item_start + (part.as_ptr() as usize - item_bytes.as_ptr() as usize);
                parse_endpoint(field, part, offset)?
            }
        };

        let end_value = match end_bytes {
            Some(part) => {
                let offset = item_start + (part.as_ptr() as usize - item_bytes.as_ptr() as usize);
                parse_endpoint(field, part, offset)?
            }
            None => {
                if start_bytes.is_none() || step_part.is_some() {
                    high
                } else {
                    start_value
                }
            }
        };

        if let (Some(start), Some(end)) = (start_bytes, end_bytes) {
            let start_offset =
                item_start + (start.as_ptr() as usize - item_bytes.as_ptr() as usize);
            let end_offset = item_start + (end.as_ptr() as usize - item_bytes.as_ptr() as usize);
            let _ = start_offset;
            if start_value > end_value {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: end_offset,
                });
            }
        }

        let step = match step_part {
            Some(part) => parse_number(
                field,
                part,
                step_offset,
                1,
                high - low + 1,
                true,
            )?,
            None => 1,
        };

        let mut value = start_value;
        while value <= end_value {
            selected[value as usize] = true;
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
            }
        }

        item_start += item_bytes.len() + 1;
    }

    Ok(selected)
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut parsed = Vec::with_capacity(5);
    for (field, part) in FIELDS.into_iter().zip(parts.iter().copied()) {
        parsed.push(parse_field(field, part)?);
    }

    Ok(Cron {
        selected: parsed.try_into().ok().unwrap(),
        day_of_month_unrestricted: parts[2] == "*",
        day_of_week_unrestricted: parts[4] == "*",
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let index = match field {
            Field::Minute => 0,
            Field::Hour => 1,
            Field::DayOfMonth => 2,
            Field::Month => 3,
            Field::DayOfWeek => 4,
        };
        self.selected[index]
            .iter()
            .enumerate()
            .filter_map(|(i, &yes)| yes.then_some(i as u8))
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        Ok(self.matches_valid(at))
    }

    pub fn next_fire(
        &self,
        after: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;
        let mut current = after;
        loop {
            if let Some(next) = increment_minute(current) {
                current = next;
            } else {
                return Ok(None);
            }
            if self.matches_valid(current) {
                return Ok(Some(current));
            }
        }
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        if !self.selected[0][at.minute as usize]
            || !self.selected[1][at.hour as usize]
            || !self.selected[3][at.month as usize]
        {
            return false;
        }

        let dom = self.selected[2][at.day as usize];
        let dow = self.selected[4][weekday(at.year, at.month, at.day) as usize];

        match (
            self.day_of_month_unrestricted,
            self.day_of_week_unrestricted,
        ) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) => {
                if at.day == days_in_month(at.year, at.month) {
                    dom && dow
                } else {
                    dom || dow
                }
            }
        }
    }
}

fn validate_datetime(at: CivilDateTime) -> Result<(), DateError> {
    if !(2000..=2399).contains(&at.year) {
        return Err(DateError::InvalidDateTime {
            component: DatePart::Year,
        });
    }
    if !(1..=12).contains(&at.month) {
        return Err(DateError::InvalidDateTime {
            component: DatePart::Month,
        });
    }
    if at.day == 0 || at.day > days_in_month(at.year, at.month) {
        return Err(DateError::InvalidDateTime {
            component: DatePart::Day,
        });
    }
    if at.hour > 23 {
        return Err(DateError::InvalidDateTime {
            component: DatePart::Hour,
        });
    }
    if at.minute > 59 {
        return Err(DateError::InvalidDateTime {
            component: DatePart::Minute,
        });
    }
    Ok(())
}

fn leap_year(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days = 0u32;
    for y in 2000..year {
        days += if leap_year(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as u32;
    }
    days += (day - 1) as u32;
    ((days + 6) % 7) as u8
}

fn increment_minute(at: CivilDateTime) -> Option<CivilDateTime> {
    if at.minute < 59 {
        return Some(CivilDateTime {
            minute: at.minute + 1,
            ..at
        });
    }
    if at.hour < 23 {
        return Some(CivilDateTime {
            hour: at.hour + 1,
            minute: 0,
            ..at
        });
    }
    if at.day < days_in_month(at.year, at.month) {
        return Some(CivilDateTime {
            day: at.day + 1,
            hour: 0,
            minute: 0,
            ..at
        });
    }
    if at.month < 12 {
        return Some(CivilDateTime {
            month: at.month + 1,
            day: 1,
            hour: 0,
            minute: 0,
            ..at
        });
    }
    if at.year < 2399 {
        return Some(CivilDateTime {
            year: at.year + 1,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            ..at
        });
    }
    None
}