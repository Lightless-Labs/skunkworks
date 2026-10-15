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
    fields: [Vec<bool>; 5],
    unrestricted_days: [bool; 2],
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

#[derive(Clone, Copy)]
struct Domain {
    min: u32,
    max: u32,
}

const FIELD_ORDER: [Field; 5] = [
    Field::Minute,
    Field::Hour,
    Field::DayOfMonth,
    Field::Month,
    Field::DayOfWeek,
];

fn domain(field: Field) -> Domain {
    match field {
        Field::Minute => Domain { min: 0, max: 59 },
        Field::Hour => Domain { min: 0, max: 23 },
        Field::DayOfMonth => Domain { min: 1, max: 31 },
        Field::Month => Domain { min: 1, max: 12 },
        Field::DayOfWeek => Domain { min: 0, max: 6 },
    }
}

fn name_value(field: Field, bytes: &[u8]) -> Option<u32> {
    if !bytes.iter().all(u8::is_ascii_alphabetic) {
        return None;
    }
    let upper: Vec<u8> = bytes.iter().map(u8::to_ascii_uppercase).collect();
    let names: &[&[u8]] = match field {
        Field::Month => &[
            b"JAN", b"FEB", b"MAR", b"APR", b"MAY", b"JUN",
            b"JUL", b"AUG", b"SEP", b"OCT", b"NOV", b"DEC",
        ],
        Field::DayOfWeek => &[
            b"SUN", b"MON", b"TUE", b"WED", b"THU", b"FRI", b"SAT",
        ],
        _ => return None,
    };
    names.iter().position(|n| *n == upper.as_slice()).map(|i| {
        if field == Field::Month {
            (i + 1) as u32
        } else {
            i as u32
        }
    })
}

fn invalid_syntax(field: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field, offset }
}

fn parse_number(field: Field, bytes: &[u8], offset: usize, is_step: bool) -> Result<u32, ParseError> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(invalid_syntax(field, offset));
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }
    let mut value = 0u32;
    for &b in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    if is_step {
        let width = domain(field).max - domain(field).min + 1;
        if value < 1 || value > width {
            return Err(ParseError::BadStep { field, offset });
        }
    } else {
        let d = domain(field);
        if value < d.min || value > d.max {
            return Err(ParseError::OutOfRange { field, offset });
        }
    }
    Ok(value)
}

fn parse_endpoint(field: Field, bytes: &[u8], offset: usize) -> Result<u32, ParseError> {
    if bytes.iter().all(u8::is_ascii_digit) {
        return parse_number(field, bytes, offset, false);
    }
    name_value(field, bytes).ok_or_else(|| invalid_syntax(field, offset))
}

fn parse_field(field: Field, source: &str) -> Result<Vec<bool>, ParseError> {
    let bytes = source.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if !(b.is_ascii_digit()
            || b.is_ascii_alphabetic()
            || matches!(b, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset: i });
        }
    }

    let d = domain(field);
    let mut selected = vec![false; (d.max + 1) as usize];

    let mut item_start = 0usize;
    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|n| item_start + n)
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
            return Err(invalid_syntax(field, item_start));
        }

        let (base, step_bytes, step_offset) = if let Some(&slash) = slash_positions.first() {
            (&item[..slash], Some(&item[slash + 1..]), item_start + slash + 1)
        } else {
            (item, None, item_start)
        };

        let hyphen_positions: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();

        let syntax_ok = if base == b"*" {
            true
        } else if hyphen_positions.len() == 1 {
            let h = hyphen_positions[0];
            h > 0 && h + 1 < base.len()
        } else {
            hyphen_positions.is_empty() && !base.is_empty()
        };

        if !syntax_ok || (step_bytes.is_some() && step_bytes.unwrap().is_empty()) {
            return Err(invalid_syntax(field, item_start));
        }

        let (start, end, end_offset) = if base == b"*" {
            (d.min, d.max, item_start)
        } else if let Some(&h) = hyphen_positions.first() {
            let left_offset = item_start;
            let right_offset = item_start + h + 1;
            let left = parse_endpoint(field, &base[..h], left_offset)?;
            let right = parse_endpoint(field, &base[h + 1..], right_offset)?;
            if left > right {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: right_offset,
                });
            }
            (left, right, right_offset)
        } else {
            let value = parse_endpoint(field, base, item_start)?;
            let end = if step_bytes.is_some() { d.max } else { value };
            (value, end, item_start)
        };

        let step = if let Some(step) = step_bytes {
            parse_number(field, step, step_offset, true)?
        } else {
            1
        };

        let _ = end_offset;
        let mut value = start;
        while value <= end {
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
    let pieces: Vec<&str> = input.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount { found: pieces.len() });
    }

    let mut parsed = Vec::with_capacity(5);
    for (i, field) in FIELD_ORDER.iter().copied().enumerate() {
        parsed.push(parse_field(field, pieces[i])?);
    }

    Ok(Cron {
        fields: parsed.try_into().ok().unwrap(),
        unrestricted_days: [pieces[2] == "*", pieces[4] == "*"],
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
        self.fields[index]
            .iter()
            .enumerate()
            .filter_map(|(value, &yes)| yes.then_some(value as u8))
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        Ok(self.matches_valid(at))
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        if !self.fields[0][at.minute as usize]
            || !self.fields[1][at.hour as usize]
            || !self.fields[3][at.month as usize]
        {
            return false;
        }

        let dom = self.fields[2][at.day as usize];
        let dow = self.fields[4][weekday(at.year, at.month, at.day) as usize];
        match (self.unrestricted_days[0], self.unrestricted_days[1]) {
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
        validate_datetime(after)?;
        let mut current = after;
        loop {
            if current.year == 2399 && current.month == 12 && current.day == 31
                && current.hour == 23 && current.minute == 59
            {
                return Ok(None);
            }
            current = next_minute(current);
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

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days = 0u32;
    for y in 2000..year {
        days += if is_leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as u32;
    }
    days += (day - 1) as u32;
    ((days + 6) % 7) as u8
}

fn validate_datetime(at: CivilDateTime) -> Result<(), DateError> {
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

fn next_minute(mut at: CivilDateTime) -> CivilDateTime {
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

fn main() {}