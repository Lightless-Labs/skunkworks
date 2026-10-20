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
    bits: [u64; 5],
    day_wildcards: [bool; 2],
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

fn bounds(field: Field) -> (u32, u32, u32) {
    match field {
        Field::Minute => (0, 59, 60),
        Field::Hour => (0, 23, 24),
        Field::DayOfMonth => (1, 31, 31),
        Field::Month => (1, 12, 12),
        Field::DayOfWeek => (0, 6, 7),
    }
}

fn parse_num(bytes: &[u8], field: Field, offset: usize, step: bool) -> Result<u32, ParseError> {
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }
    let mut n = 0u32;
    for &b in bytes {
        n = n.checked_mul(10)
            .and_then(|x| x.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    let (min, max, width) = bounds(field);
    if step {
        if n == 0 || n > width {
            return Err(ParseError::BadStep { field, offset });
        }
    } else if n < min || n > max {
        return Err(ParseError::OutOfRange { field, offset });
    }
    Ok(n)
}

fn valid_base(b: &[u8]) -> bool {
    if b == b"*" { return true; }
    let mut dash = None;
    for (i, &x) in b.iter().enumerate() {
        if x == b'-' {
            if dash.is_some() { return false; }
            dash = Some(i);
        } else if !x.is_ascii_digit() {
            return false;
        }
    }
    match dash {
        None => !b.is_empty(),
        Some(i) => i > 0 && i + 1 < b.len(),
    }
}

fn parse_field(s: &str, field: Field) -> Result<u64, ParseError> {
    let b = s.as_bytes();
    for (i, &x) in b.iter().enumerate() {
        if !x.is_ascii_digit() && x != b'*' && x != b'-' && x != b'/' && x != b',' {
            return Err(ParseError::InvalidCharacter { field, offset: i });
        }
    }
    let (min, max, _) = bounds(field);
    let mut bits = 0u64;
    let mut offset = 0usize;
    for item in s.split(',') {
        let ib = item.as_bytes();
        if ib.is_empty() {
            return Err(ParseError::EmptyItem { field, offset });
        }
        let mut slash = None;
        for (i, &x) in ib.iter().enumerate() {
            if x == b'/' {
                if slash.is_some() {
                    return Err(ParseError::InvalidSyntax { field, offset });
                }
                slash = Some(i);
            }
        }
        let (base, step_bytes, step_offset) = match slash {
            Some(i) => (&ib[..i], Some(&ib[i + 1..]), offset + i + 1),
            None => (ib, None, 0),
        };
        if !valid_base(base)
            || step_bytes.map_or(false, |x| x.is_empty() || !x.iter().all(u8::is_ascii_digit))
        {
            return Err(ParseError::InvalidSyntax { field, offset });
        }

        let dash = base.iter().position(|&x| x == b'-');
        let (lo, hi) = if base == b"*" {
            (min, max)
        } else if let Some(i) = dash {
            let lo = parse_num(&base[..i], field, offset, false)?;
            let hi = parse_num(&base[i + 1..], field, offset + i + 1, false)?;
            if lo > hi {
                return Err(ParseError::ReversedRange { field, offset: offset + i + 1 });
            }
            (lo, hi)
        } else {
            let n = parse_num(base, field, offset, false)?;
            (n, if step_bytes.is_some() { max } else { n })
        };
        let step = match step_bytes {
            Some(x) => parse_num(x, field, step_offset, true)?,
            None => 1,
        };
        let mut v = lo;
        while v <= hi {
            bits |= 1u64 << v;
            v += step;
        }
        offset += ib.len() + 1;
    }
    Ok(bits)
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }
    let mut bits = [0u64; 5];
    for i in 0..5 {
        bits[i] = parse_field(parts[i], FIELDS[i])?;
    }
    Ok(Cron {
        bits,
        day_wildcards: [parts[2] == "*", parts[4] == "*"],
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let i = match field {
            Field::Minute => 0,
            Field::Hour => 1,
            Field::DayOfMonth => 2,
            Field::Month => 3,
            Field::DayOfWeek => 4,
        };
        let (min, max, _) = bounds(field);
        (min..=max)
            .filter(|&v| self.bits[i] & (1u64 << v) != 0)
            .map(|v| v as u8)
            .collect()
    }

    fn has(&self, field: Field, value: u8) -> bool {
        let i = match field {
            Field::Minute => 0,
            Field::Hour => 1,
            Field::DayOfMonth => 2,
            Field::Month => 3,
            Field::DayOfWeek => 4,
        };
        self.bits[i] & (1u64 << value) != 0
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate(at)?;
        Ok(self.has(Field::Minute, at.minute)
            && self.has(Field::Hour, at.hour)
            && self.has(Field::DayOfMonth, at.day)
            && self.has(Field::Month, at.month)
            && self.has(Field::DayOfWeek, weekday(at.year, at.month, at.day)))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate(after)?;
        let mut t = after;
        loop {
            if t.year == 2399 && t.month == 12 && t.day == 31 && t.hour == 23 && t.minute == 59 {
                return Ok(None);
            }
            t = tick(t);
            if self.matches(t)? {
                return Ok(Some(t));
            }
        }
    }
}

fn validate(t: CivilDateTime) -> Result<(), DateError> {
    if t.year < 2000 || t.year > 2399 {
        return Err(DateError::InvalidDateTime { component: DatePart::Year });
    }
    if t.month < 1 || t.month > 12 {
        return Err(DateError::InvalidDateTime { component: DatePart::Month });
    }
    if t.day < 1 || t.day > month_days(t.year, t.month) {
        return Err(DateError::InvalidDateTime { component: DatePart::Day });
    }
    if t.hour > 23 {
        return Err(DateError::InvalidDateTime { component: DatePart::Hour });
    }
    if t.minute > 59 {
        return Err(DateError::InvalidDateTime { component: DatePart::Minute });
    }
    Ok(())
}

fn is_leap(y: u16) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

fn month_days(y: u16, m: u8) -> u8 {
    match m {
        2 => if is_leap(y) { 29 } else { 28 },
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn weekday(y: u16, m: u8, d: u8) -> u8 {
    let mut n = 6u32;
    for year in 2000..y {
        n += if is_leap(year) { 366 } else { 365 };
    }
    for month in 1..m {
        n += month_days(y, month) as u32;
    }
    n += (d - 1) as u32;
    (n % 7) as u8
}

fn tick(mut t: CivilDateTime) -> CivilDateTime {
    if t.minute != 59 {
        t.minute += 1;
        return t;
    }
    t.minute = 0;
    if t.hour != 23 {
        t.hour += 1;
        return t;
    }
    t.hour = 0;
    if t.day < month_days(t.year, t.month) {
        t.day += 1;
        return t;
    }
    t.day = 1;
    if t.month != 12 {
        t.month += 1;
        return t;
    }
    t.month = 1;
    t.year += 1;
    t
}
