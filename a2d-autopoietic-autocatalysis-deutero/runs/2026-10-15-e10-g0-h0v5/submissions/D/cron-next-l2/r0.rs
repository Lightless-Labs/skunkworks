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
    day_of_month_exact_star: bool,
    day_of_week_exact_star: bool,
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

fn domain(field: Field) -> (u32, u32) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
    }
}

fn parse_number(bytes: &[u8], start: usize, field: Field) -> Result<u32, ParseError> {
    let err = |e| e;
    if bytes.is_empty() {
        return Err(ParseError::InvalidSyntax { field, offset: start });
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset: start });
    }
    let mut value = 0u32;
    for &b in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or_else(|| err(ParseError::NumberOverflow { field, offset: start }))?;
    }
    Ok(value)
}

fn parse_field(text: &str, field: Field) -> Result<Vec<bool>, ParseError> {
    let bytes = text.as_bytes();
    for (offset, &b) in bytes.iter().enumerate() {
        if !b.is_ascii_digit() && !matches!(b, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (min, max) = domain(field);
    let width = max - min + 1;
    let mut selected = vec![false; (max + 1) as usize];

    let mut item_start = 0;
    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|p| item_start + p)
            .unwrap_or(bytes.len());
        let item = &bytes[item_start..item_end];

        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let slash_positions: Vec<usize> = item
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'/').then_some(i))
            .collect();
        let (base, step_bytes, step_offset) = match slash_positions.as_slice() {
            [] => (item, None, 0),
            [slash] => (&item[..*slash], Some(&item[*slash + 1..]), item_start + slash + 1),
            _ => {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                })
            }
        };

        let star = base == b"*";
        let hyphens: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();

        let base_kind = if star {
            if !hyphens.is_empty() {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            None
        } else {
            match hyphens.as_slice() {
                [] if !base.is_empty() && base.iter().all(u8::is_ascii_digit) => {
                    Some((0usize, base.len(), None))
                }
                [h]
                    if *h > 0
                        && *h + 1 < base.len()
                        && base[..*h].iter().all(u8::is_ascii_digit)
                        && base[*h + 1..].iter().all(u8::is_ascii_digit) =>
                {
                    Some((0usize, *h, Some(*h + 1)))
                }
                _ => {
                    return Err(ParseError::InvalidSyntax {
                        field,
                        offset: item_start,
                    })
                }
            }
        };

        let mut start_value = min;
        let mut end_value = max;
        let mut start_offset = item_start;

        if let Some((start_idx, start_end, end_idx)) = base_kind {
            start_offset = item_start + start_idx;
            start_value = parse_number(&base[start_idx..start_end], start_offset, field)?;
            if start_value < min || start_value > max {
                return Err(ParseError::OutOfRange {
                    field,
                    offset: start_offset,
                });
            }
            if let Some(end_idx) = end_idx {
                let end_offset = item_start + end_idx;
                end_value = parse_number(&base[end_idx..], end_offset, field)?;
                if end_value < min || end_value > max {
                    return Err(ParseError::OutOfRange {
                        field,
                        offset: end_offset,
                    });
                }
                if start_value > end_value {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: end_offset,
                    });
                }
            } else if step_bytes.is_none() {
                end_value = start_value;
            } else {
                end_value = max;
            }
        }

        let step = if let Some(step_bytes) = step_bytes {
            let value = parse_number(step_bytes, step_offset, field)?;
            if value < 1 || value > width {
                return Err(ParseError::BadStep {
                    field,
                    offset: step_offset,
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

        if item_end == bytes.len() {
            break;
        }
        item_start = item_end + 1;
    }

    Ok(selected)
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut parsed = Vec::with_capacity(5);
    for (text, field) in parts.iter().zip(FIELDS) {
        parsed.push(parse_field(text, field)?);
    }

    Ok(Cron {
        selected: parsed.try_into().ok().unwrap(),
        day_of_month_exact_star: parts[2] == "*",
        day_of_week_exact_star: parts[4] == "*",
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
        let weekday = weekday(at.year, at.month, at.day);
        Ok(self.selected[0][at.minute as usize]
            && self.selected[1][at.hour as usize]
            && self.selected[2][at.day as usize]
            && self.selected[3][at.month as usize]
            && self.selected[4][weekday as usize])
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;
        let mut current = after;
        loop {
            current = increment_minute(current);
            if current.year > 2399 {
                return Ok(None);
            }
            if self.matches(current)? {
                return Ok(Some(current));
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

fn is_leap(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days = 0u32;
    for y in 2000..year {
        days += if is_leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as u32;
    }
    days += (day - 1) as u32;
    ((6 + days) % 7) as u8
}

fn increment_minute(mut at: CivilDateTime) -> CivilDateTime {
    if at.minute < 59 {
        at.minute += 1;
        return at;
    }
    at.minute = 0;
    if at.hour < 23 {
        at.hour += 1;
        return at;
    }
    at.hour = 0;
    if at.day < days_in_month(at.year, at.month) {
        at.day += 1;
        return at;
    }
    at.day = 1;
    if at.month < 12 {
        at.month += 1;
        return at;
    }
    at.month = 1;
    at.year += 1;
    at
}