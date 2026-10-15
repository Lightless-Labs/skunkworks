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
    sets: [Vec<u8>; 5],
    day_unrestricted: [bool; 2],
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

fn field_index(field: Field) -> usize {
    match field {
        Field::Minute => 0,
        Field::Hour => 1,
        Field::DayOfMonth => 2,
        Field::Month => 3,
        Field::DayOfWeek => 4,
    }
}

fn invalid_syntax(field: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field, offset }
}

fn name_value(field: Field, bytes: &[u8]) -> Option<u32> {
    if !bytes.iter().all(u8::is_ascii_alphabetic) {
        return None;
    }
    let upper: Vec<u8> = bytes.iter().map(u8::to_ascii_uppercase).collect();
    match field {
        Field::Month => match upper.as_slice() {
            b"JAN" => Some(1),
            b"FEB" => Some(2),
            b"MAR" => Some(3),
            b"APR" => Some(4),
            b"MAY" => Some(5),
            b"JUN" => Some(6),
            b"JUL" => Some(7),
            b"AUG" => Some(8),
            b"SEP" => Some(9),
            b"OCT" => Some(10),
            b"NOV" => Some(11),
            b"DEC" => Some(12),
            _ => None,
        },
        Field::DayOfWeek => match upper.as_slice() {
            b"SUN" => Some(0),
            b"MON" => Some(1),
            b"TUE" => Some(2),
            b"WED" => Some(3),
            b"THU" => Some(4),
            b"FRI" => Some(5),
            b"SAT" => Some(6),
            _ => None,
        },
        _ => None,
    }
}

fn is_atom(field: Field, bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && (bytes.iter().all(u8::is_ascii_digit) || name_value(field, bytes).is_some())
}

fn parse_number(
    field: Field,
    bytes: &[u8],
    offset: usize,
    is_step: bool,
) -> Result<u32, ParseError> {
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }
    let mut value: u32 = 0;
    for &b in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    if is_step {
        let (min, max) = domain(field);
        let width = max - min + 1;
        if value == 0 || value > width {
            return Err(ParseError::BadStep { field, offset });
        }
    } else {
        let (min, max) = domain(field);
        if value < min || value > max {
            return Err(ParseError::OutOfRange { field, offset });
        }
    }
    Ok(value)
}

fn parse_endpoint(
    field: Field,
    bytes: &[u8],
    offset: usize,
) -> Result<u32, ParseError> {
    if bytes.iter().all(u8::is_ascii_digit) {
        parse_number(field, bytes, offset, false)
    } else {
        name_value(field, bytes).ok_or_else(|| invalid_syntax(field, offset))
    }
}

fn parse_field(field: Field, text: &str) -> Result<Vec<u8>, ParseError> {
    let bytes = text.as_bytes();

    for (offset, &b) in bytes.iter().enumerate() {
        let legal = b.is_ascii_digit()
            || b.is_ascii_alphabetic()
            || matches!(b, b'*' | b'-' | b'/' | b',');
        if !legal {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let mut selected = vec![false; (domain(field).1 + 1) as usize];
    let mut item_start = 0usize;

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
            .filter_map(|(i, &b)| if b == b'/' { Some(i) } else { None })
            .collect();
        if slash_positions.len() > 1 {
            return Err(invalid_syntax(field, item_start));
        }

        let (base, step_bytes, step_offset) = if let Some(&slash) = slash_positions.first() {
            (&item_bytes[..slash], Some(&item_bytes[slash + 1..]), item_start + slash + 1)
        } else {
            (item_bytes, None, 0)
        };

        let (star, range_parts) = if base == b"*" {
            (true, None)
        } else {
            let hyphens: Vec<usize> = base
                .iter()
                .enumerate()
                .filter_map(|(i, &b)| if b == b'-' { Some(i) } else { None })
                .collect();
            if hyphens.len() > 1 {
                return Err(invalid_syntax(field, item_start));
            }
            if let Some(&hyphen) = hyphens.first() {
                let left = &base[..hyphen];
                let right = &base[hyphen + 1..];
                if !is_atom(field, left) || !is_atom(field, right) {
                    return Err(invalid_syntax(field, item_start));
                }
                (false, Some((left, right, hyphen)))
            } else if is_atom(field, base) {
                (false, None)
            } else {
                return Err(invalid_syntax(field, item_start));
            }
        };

        if let Some(step) = step_bytes {
            if step.is_empty() || !step.iter().all(u8::is_ascii_digit) {
                return Err(invalid_syntax(field, item_start));
            }
        }

        let (minimum, maximum) = domain(field);
        let (start, end) = if star {
            (minimum, maximum)
        } else if let Some((left, right, hyphen)) = range_parts {
            let start = parse_endpoint(field, left, item_start)?;
            let right_offset = item_start + hyphen + 1;
            let end = parse_endpoint(field, right, right_offset)?;
            if start > end {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: right_offset,
                });
            }
            (start, end)
        } else {
            let value = parse_endpoint(field, base, item_start)?;
            if step_bytes.is_some() {
                (value, maximum)
            } else {
                (value, value)
            }
        };

        let step = if let Some(step) = step_bytes {
            parse_number(field, step, step_offset, true)?
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

        item_start += item_bytes.len() + 1;
    }

    Ok(selected
        .iter()
        .enumerate()
        .filter_map(|(value, &yes)| if yes { Some(value as u8) } else { None })
        .collect())
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut parsed = Vec::with_capacity(5);
    for (i, field) in FIELDS.iter().copied().enumerate() {
        parsed.push(parse_field(field, parts[i])?);
    }

    Ok(Cron {
        sets: parsed.try_into().ok().unwrap(),
        day_unrestricted: [parts[2] == "*", parts[4] == "*"],
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.sets[field_index(field)].clone()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        Ok(matches_valid(self, at))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;
        if after.year == 2399 && after.month == 12 && after.day == 31
            && after.hour == 23 && after.minute == 59
        {
            return Ok(None);
        }

        let mut ordinal = to_ordinal(after);
        let max_ordinal = to_ordinal(CivilDateTime {
            year: 2399,
            month: 12,
            day: 31,
            hour: 23,
            minute: 59,
        });
        while ordinal < max_ordinal {
            ordinal += 1;
            let at = from_ordinal(ordinal);
            if matches_valid(self, at) {
                return Ok(Some(at));
            }
        }
        Ok(None)
    }
}

fn is_leap(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 => if is_leap(year) { 29 } else { 28 },
        4 | 6 | 9 | 11 => 30,
        _ => 31,
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

fn weekday(at: CivilDateTime) -> u8 {
    let mut days = 0u32;
    for year in 2000..at.year {
        days += if is_leap(year) { 366 } else { 365 };
    }
    for month in 1..at.month {
        days += days_in_month(at.year, month) as u32;
    }
    days += (at.day - 1) as u32;
    ((6 + days) % 7) as u8
}

fn contains(cron: &Cron, field: Field, value: u8) -> bool {
    cron.sets[field_index(field)].binary_search(&value).is_ok()
}

fn matches_valid(cron: &Cron, at: CivilDateTime) -> bool {
    if !contains(cron, Field::Minute, at.minute)
        || !contains(cron, Field::Hour, at.hour)
        || !contains(cron, Field::Month, at.month)
    {
        return false;
    }

    let dom = contains(cron, Field::DayOfMonth, at.day);
    let dow = contains(cron, Field::DayOfWeek, weekday(at));
    match cron.day_unrestricted {
        [true, true] => true,
        [false, true] => dom,
        [true, false] => dow,
        [false, false] => {
            if at.day == days_in_month(at.year, at.month) {
                dom && dow
            } else {
                dom || dow
            }
        }
    }
}

fn days_before_year(year: u16) -> u64 {
    let mut days = 0u64;
    for y in 2000..year {
        days += if is_leap(y) { 366 } else { 365 };
    }
    days
}

fn to_ordinal(at: CivilDateTime) -> u64 {
    let mut days = days_before_year(at.year);
    for month in 1..at.month {
        days += days_in_month(at.year, month) as u64;
    }
    days += (at.day - 1) as u64;
    days * 1440 + at.hour as u64 * 60 + at.minute as u64
}

fn from_ordinal(ordinal: u64) -> CivilDateTime {
    let mut days = ordinal / 1440;
    let minutes = (ordinal % 1440) as u16;
    let hour = (minutes / 60) as u8;
    let minute = (minutes % 60) as u8;

    let mut year = 2000u16;
    loop {
        let year_days = if is_leap(year) { 366 } else { 365 };
        if days < year_days {
            break;
        }
        days -= year_days;
        year += 1;
    }

    let mut month = 1u8;
    loop {
        let month_days = days_in_month(year, month) as u64;
        if days < month_days {
            break;
        }
        days -= month_days;
        month += 1;
    }

    CivilDateTime {
        year,
        month,
        day: days as u8 + 1,
        hour,
        minute,
    }
}

fn main() {}