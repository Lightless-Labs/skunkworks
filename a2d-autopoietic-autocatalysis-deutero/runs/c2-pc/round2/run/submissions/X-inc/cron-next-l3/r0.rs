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

fn names(field: Field) -> &'static [(&'static str, u32)] {
    match field {
        Field::Month => &[
            ("JAN", 1), ("FEB", 2), ("MAR", 3), ("APR", 4),
            ("MAY", 5), ("JUN", 6), ("JUL", 7), ("AUG", 8),
            ("SEP", 9), ("OCT", 10), ("NOV", 11), ("DEC", 12),
        ],
        Field::DayOfWeek => &[
            ("SUN", 0), ("MON", 1), ("TUE", 2), ("WED", 3),
            ("THU", 4), ("FRI", 5), ("SAT", 6),
        ],
        _ => &[],
    }
}

fn syntax_error(field: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field, offset }
}

fn parse_number(field: Field, bytes: &[u8], offset: usize, step: bool) -> Result<u32, ParseError> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(syntax_error(field, offset));
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }
    let mut value = 0u32;
    for &byte in bytes {
        value = value.checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    if step {
        let (min, max) = bounds(field);
        if value == 0 || value > max - min + 1 {
            return Err(ParseError::BadStep { field, offset });
        }
    } else {
        let (min, max) = bounds(field);
        if value < min || value > max {
            return Err(ParseError::OutOfRange { field, offset });
        }
    }
    Ok(value)
}

fn parse_endpoint(field: Field, bytes: &[u8], offset: usize) -> Result<u32, ParseError> {
    if bytes.iter().all(u8::is_ascii_digit) {
        return parse_number(field, bytes, offset, false);
    }
    if bytes.iter().all(u8::is_ascii_alphabetic) {
        let upper: Vec<u8> = bytes.iter().map(u8::to_ascii_uppercase).collect();
        for &(name, value) in names(field) {
            if upper == name.as_bytes() {
                return Ok(value);
            }
        }
    }
    Err(syntax_error(field, offset))
}

fn parse_item(
    field: Field,
    text: &str,
    item_offset: usize,
    selected: &mut [bool],
) -> Result<(), ParseError> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return Err(ParseError::EmptyItem { field, offset: item_offset });
    }

    let slash_positions: Vec<usize> = bytes.iter().enumerate()
        .filter_map(|(i, &b)| (b == b'/').then_some(i))
        .collect();
    if slash_positions.len() > 1 {
        return Err(syntax_error(field, item_offset));
    }

    let (base, step_bytes, step_offset) = if let Some(&slash) = slash_positions.first() {
        if slash == 0 || slash + 1 >= bytes.len() {
            return Err(syntax_error(field, item_offset));
        }
        (&bytes[..slash], Some(&bytes[slash + 1..]), item_offset + slash + 1)
    } else {
        (bytes, None, 0)
    };

    let is_star = base == b"*";
    let hyphens: Vec<usize> = base.iter().enumerate()
        .filter_map(|(i, &b)| (b == b'-').then_some(i))
        .collect();
    if hyphens.len() > 1 || (is_star && !hyphens.is_empty()) {
        return Err(syntax_error(field, item_offset));
    }

    let (start, end, end_offset) = if is_star {
        let (min, max) = bounds(field);
        (min, max, item_offset)
    } else if let Some(&hyphen) = hyphens.first() {
        if hyphen == 0 || hyphen + 1 >= base.len() {
            return Err(syntax_error(field, item_offset));
        }
        let left = &base[..hyphen];
        let right = &base[hyphen + 1..];
        let valid_endpoint = |part: &[u8]| {
            part.iter().all(u8::is_ascii_digit)
                || (part.iter().all(u8::is_ascii_alphabetic)
                    && names(field).iter().any(|(name, _)| {
                        part.eq_ignore_ascii_case(name.as_bytes())
                    }))
        };
        if !valid_endpoint(left) || !valid_endpoint(right) {
            return Err(syntax_error(field, item_offset));
        }
        (
            parse_endpoint(field, left, item_offset)?,
            parse_endpoint(field, right, item_offset + hyphen + 1)?,
            item_offset + hyphen + 1,
        )
    } else {
        let valid_endpoint = base.iter().all(u8::is_ascii_digit)
            || (base.iter().all(u8::is_ascii_alphabetic)
                && names(field).iter().any(|(name, _)| {
                    base.eq_ignore_ascii_case(name.as_bytes())
                }));
        if !valid_endpoint {
            return Err(syntax_error(field, item_offset));
        }
        let value = parse_endpoint(field, base, item_offset)?;
        let end = if step_bytes.is_some() {
            bounds(field).1
        } else {
            value
        };
        (value, end, item_offset)
    };

    if start > end {
        return Err(ParseError::ReversedRange { field, offset: end_offset });
    }

    // Validate the complete step syntax before performing numeric base checks.
    if let Some(step) = step_bytes {
        if !step.iter().all(u8::is_ascii_digit) {
            return Err(syntax_error(field, item_offset));
        }
    }

    let step = if let Some(step_bytes) = step_bytes {
        Some(parse_number(field, step_bytes, step_offset, true)?)
    } else {
        None
    };

    let stride = step.unwrap_or(1);
    let (min, _) = bounds(field);
    let mut value = start;
    while value <= end {
        selected[(value - min) as usize] = true;
        match value.checked_add(stride) {
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
        let (min, max) = bounds(FIELDS[i]);
        vec![false; (max - min + 1) as usize]
    });

    for (index, (&field, text)) in FIELDS.iter().zip(parts.iter()).enumerate() {
        for (offset, &byte) in text.as_bytes().iter().enumerate() {
            if !(byte.is_ascii_digit()
                || byte.is_ascii_alphabetic()
                || matches!(byte, b'*' | b'-' | b'/' | b','))
            {
                return Err(ParseError::InvalidCharacter { field, offset });
            }
        }

        let mut item_start = 0usize;
        for item in text.split(',') {
            parse_item(field, item, item_start, &mut selected[index])?;
            item_start += item.len() + 1;
        }
    }

    Ok(Cron {
        selected,
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
        let (min, _) = bounds(field);
        self.selected[index].iter().enumerate()
            .filter_map(|(i, &yes)| yes.then_some((min + i as u32) as u8))
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
        let (min, _) = bounds(field);
        value >= min as u8
            && self.selected[index]
                .get((value as u32 - min) as usize)
                .copied()
                .unwrap_or(false)
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate(at)?;
        Ok(self.matches_valid(at))
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        if !self.contains(Field::Minute, at.minute)
            || !self.contains(Field::Hour, at.hour)
            || !self.contains(Field::Month, at.month)
        {
            return false;
        }

        let dom = self.contains(Field::DayOfMonth, at.day);
        let dow = self.contains(Field::DayOfWeek, weekday(at.year, at.month, at.day));
        match (self.day_of_month_unrestricted, self.day_of_week_unrestricted) {
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

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate(after)?;
        let mut current = after;
        loop {
            current = match advance_minute(current) {
                Some(next) => next,
                None => return Ok(None),
            };
            if self.matches_valid(current) {
                return Ok(Some(current));
            }
        }
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

fn validate(at: CivilDateTime) -> Result<(), DateError> {
    if !(2000..=2399).contains(&at.year) {
        return Err(DateError::InvalidDateTime { component: DatePart::Year });
    }
    if !(1..=12).contains(&at.month) {
        return Err(DateError::InvalidDateTime { component: DatePart::Month });
    }
    if at.day == 0 || at.day > days_in_month(at.year, at.month) {
        return Err(DateError::InvalidDateTime { component: DatePart::Day });
    }
    if at.hour > 23 {
        return Err(DateError::InvalidDateTime { component: DatePart::Hour });
    }
    if at.minute > 59 {
        return Err(DateError::InvalidDateTime { component: DatePart::Minute });
    }
    Ok(())
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

fn advance_minute(mut at: CivilDateTime) -> Option<CivilDateTime> {
    if at.minute < 59 {
        at.minute += 1;
        return Some(at);
    }
    at.minute = 0;
    if at.hour < 23 {
        at.hour += 1;
        return Some(at);
    }
    at.hour = 0;
    if at.day < days_in_month(at.year, at.month) {
        at.day += 1;
        return Some(at);
    }
    at.day = 1;
    if at.month < 12 {
        at.month += 1;
        return Some(at);
    }
    at.month = 1;
    if at.year < 2399 {
        at.year += 1;
        return Some(at);
    }
    None
}