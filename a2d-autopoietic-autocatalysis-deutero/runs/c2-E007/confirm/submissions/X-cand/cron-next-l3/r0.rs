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
    day_of_month_star: bool,
    day_of_week_star: bool,
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

fn invalid_syntax(field: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field, offset }
}

fn parse_number(bytes: &[u8], start: usize, field: Field) -> Result<u32, ParseError> {
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero {
            field,
            offset: start,
        });
    }

    let mut value = 0u32;
    for &b in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow {
                field,
                offset: start,
            })?;
    }
    Ok(value)
}

fn is_numeric_atom(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit)
}

fn named_value(bytes: &[u8], field: Field) -> Option<u32> {
    let names: &[&str] = match field {
        Field::Month => &[
            "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV",
            "DEC",
        ],
        Field::DayOfWeek => &["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"],
        _ => return None,
    };

    names
        .iter()
        .position(|name| {
            bytes.len() == name.len()
                && bytes
                    .iter()
                    .zip(name.as_bytes())
                    .all(|(&a, &b)| a.eq_ignore_ascii_case(&b))
        })
        .map(|index| {
            if field == Field::Month {
                index as u32 + 1
            } else {
                index as u32
            }
        })
}

fn is_valid_atom(bytes: &[u8], field: Field) -> bool {
    is_numeric_atom(bytes) || named_value(bytes, field).is_some()
}

fn parse_atom_value(bytes: &[u8], start: usize, field: Field) -> Result<u32, ParseError> {
    if is_numeric_atom(bytes) {
        parse_number(bytes, start, field)
    } else {
        named_value(bytes, field).ok_or_else(|| invalid_syntax(field, start))
    }
}

fn validate_domain(value: u32, field: Field, offset: usize) -> Result<(), ParseError> {
    let (min, max) = domain(field);
    if value < min || value > max {
        Err(ParseError::OutOfRange { field, offset })
    } else {
        Ok(())
    }
}

fn parse_item(
    item: &[u8],
    item_start: usize,
    field: Field,
    selected: &mut [bool],
) -> Result<(), ParseError> {
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
    if slash_positions.len() > 1 {
        return Err(invalid_syntax(field, item_start));
    }

    let (base, step_bytes, step_start) = if let Some(&slash) = slash_positions.first() {
        (&item[..slash], Some(&item[slash + 1..]), item_start + slash + 1)
    } else {
        (item, None, item_start)
    };

    if base.is_empty() || step_bytes.is_some_and(|s| s.is_empty()) {
        return Err(invalid_syntax(field, item_start));
    }

    let is_star = base == b"*";
    let hyphens: Vec<usize> = base
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'-').then_some(i))
        .collect();
    if (!is_star && hyphens.len() > 1) || (is_star && !hyphens.is_empty()) {
        return Err(invalid_syntax(field, item_start));
    }

    let (start_bytes, end_bytes, end_offset) = if is_star {
        (None, None, item_start)
    } else if let Some(&hyphen) = hyphens.first() {
        if hyphen == 0 || hyphen + 1 == base.len() {
            return Err(invalid_syntax(field, item_start));
        }
        (
            Some(&base[..hyphen]),
            Some(&base[hyphen + 1..]),
            item_start + hyphen + 1,
        )
    } else {
        (Some(base), None, item_start)
    };

    // Validate the complete structure of the item before reading any values.
    if let Some(start) = start_bytes {
        if !is_valid_atom(start, field) {
            return Err(invalid_syntax(field, item_start));
        }
    }
    if let Some(end) = end_bytes {
        if !is_valid_atom(end, field) {
            return Err(invalid_syntax(field, item_start));
        }
    }
    if let Some(step) = step_bytes {
        if !is_numeric_atom(step) {
            return Err(invalid_syntax(field, item_start));
        }
    }

    let (min, max) = domain(field);
    let mut start_value = min;
    let mut end_value = max;

    if let Some(start) = start_bytes {
        start_value = parse_atom_value(start, item_start, field)?;
        if is_numeric_atom(start) {
            validate_domain(start_value, field, item_start)?;
        }

        if let Some(end) = end_bytes {
            end_value = parse_atom_value(end, end_offset, field)?;
            if is_numeric_atom(end) {
                validate_domain(end_value, field, end_offset)?;
            }
            if start_value > end_value {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: end_offset,
                });
            }
        } else {
            end_value = if step_bytes.is_some() {
                max
            } else {
                start_value
            };
        }
    }

    let step = if let Some(bytes) = step_bytes {
        let value = parse_number(bytes, step_start, field)?;
        let width = max - min + 1;
        if value == 0 || value > width {
            return Err(ParseError::BadStep {
                field,
                offset: step_start,
            });
        }
        value
    } else {
        1
    };

    let mut value = start_value;
    while value <= end_value {
        selected[(value - min) as usize] = true;
        match value.checked_add(step) {
            Some(next) if next > value => value = next,
            _ => break,
        }
    }
    Ok(())
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut selected: [Vec<bool>; 5] = std::array::from_fn(|i| {
        let (min, max) = domain(FIELDS[i]);
        vec![false; (max - min + 1) as usize]
    });

    for (i, part) in parts.iter().enumerate() {
        let field = FIELDS[i];
        for (offset, &byte) in part.as_bytes().iter().enumerate() {
            if !(byte.is_ascii_digit()
                || byte.is_ascii_alphabetic()
                || matches!(byte, b'*' | b'-' | b'/' | b','))
            {
                return Err(ParseError::InvalidCharacter { field, offset });
            }
        }

        let mut item_start = 0;
        for item in part.as_bytes().split(|&b| b == b',') {
            parse_item(item, item_start, field, &mut selected[i])?;
            item_start += item.len() + 1;
        }
    }

    Ok(Cron {
        selected,
        day_of_month_star: parts[2] == "*",
        day_of_week_star: parts[4] == "*",
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
        let (min, _) = domain(field);
        self.selected[index]
            .iter()
            .enumerate()
            .filter_map(|(i, &yes)| yes.then_some((i as u8) + min as u8))
            .collect()
    }

    fn contains(&self, field: Field, value: u8) -> bool {
        let index = match field {
            Field::Minute => 0,
            Field::Hour => 1,
            Field::DayOfMonth => 2,
            Field::Month => 3,
            Field::DayOfWeek => 4,
        };
        let (min, max) = domain(field);
        (value as u32) >= min
            && (value as u32) <= max
            && self.selected[index][value as usize - min as usize]
    }

    fn date_matches(&self, at: CivilDateTime) -> bool {
        let dom = self.contains(Field::DayOfMonth, at.day);
        let dow = self.contains(Field::DayOfWeek, weekday(at.year, at.month, at.day));

        match (self.day_of_month_star, self.day_of_week_star) {
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

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        Ok(self.contains(Field::Minute, at.minute)
            && self.contains(Field::Hour, at.hour)
            && self.contains(Field::Month, at.month)
            && self.date_matches(at))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;

        let mut candidate = after;
        loop {
            if candidate.year == 2399
                && candidate.month == 12
                && candidate.day == 31
                && candidate.hour == 23
                && candidate.minute == 59
            {
                return Ok(None);
            }

            candidate = increment_minute(candidate);
            if self.contains(Field::Minute, candidate.minute)
                && self.contains(Field::Hour, candidate.hour)
                && self.contains(Field::Month, candidate.month)
                && self.date_matches(candidate)
            {
                return Ok(Some(candidate));
            }
        }
    }
}

fn is_leap_year(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days = 0u32;
    for y in 2000..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as u32;
    }
    days += day as u32 - 1;
    ((days + 6) % 7) as u8
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