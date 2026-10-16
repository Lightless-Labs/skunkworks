use std::collections::BTreeSet;

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
    UnknownSpecial,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    values: [BTreeSet<u8>; 5],
    unrestricted_dom: bool,
    unrestricted_dow: bool,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowError {
    InvalidStart { component: DatePart },
    InvalidEnd { component: DatePart },
    ReversedWindow,
    ZeroCap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FireWindow {
    pub fires: Vec<CivilDateTime>,
    pub truncated: bool,
}

const FIELDS: [Field; 5] = [
    Field::Minute,
    Field::Hour,
    Field::DayOfMonth,
    Field::Month,
    Field::DayOfWeek,
];

fn index(field: Field) -> usize {
    match field {
        Field::Minute => 0,
        Field::Hour => 1,
        Field::DayOfMonth => 2,
        Field::Month => 3,
        Field::DayOfWeek => 4,
    }
}

fn bounds(field: Field) -> (u8, u8) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
    }
}

fn name_value(field: Field, s: &str) -> Option<u8> {
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
        .position(|name| name.eq_ignore_ascii_case(s))
        .map(|i| if field == Field::Month { i as u8 + 1 } else { i as u8 })
}

fn parse_number(
    field: Field,
    bytes: &[u8],
    offset: usize,
    is_step: bool,
) -> Result<u8, ParseError> {
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }
    let mut n: u32 = 0;
    for &b in bytes {
        n = n
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    if is_step {
        let (_, max) = bounds(field);
        let width = max as u32 - bounds(field).0 as u32 + 1;
        if n == 0 || n > width {
            return Err(ParseError::BadStep { field, offset });
        }
    } else {
        let (min, max) = bounds(field);
        if n < min as u32 || n > max as u32 {
            return Err(ParseError::OutOfRange { field, offset });
        }
    }
    Ok(n as u8)
}

fn atom_value(field: Field, bytes: &[u8], offset: usize) -> Result<u8, ParseError> {
    if bytes.iter().all(u8::is_ascii_digit) {
        return parse_number(field, bytes, offset, false);
    }
    if bytes.iter().all(u8::is_ascii_alphabetic) {
        if let Some(value) = name_value(field, std::str::from_utf8(bytes).unwrap()) {
            return Ok(value);
        }
    }
    Err(ParseError::InvalidSyntax { field, offset })
}

fn parse_field(field: Field, text: &str) -> Result<BTreeSet<u8>, ParseError> {
    let bytes = text.as_bytes();
    for (offset, &b) in bytes.iter().enumerate() {
        if !(b.is_ascii_digit()
            || b.is_ascii_alphabetic()
            || matches!(b, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (domain_min, domain_max) = bounds(field);
    let mut selected = BTreeSet::new();
    let mut item_start = 0usize;
    for item in text.split(',') {
        let item_bytes = item.as_bytes();
        if item_bytes.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let slash_count = item_bytes.iter().filter(|&&b| b == b'/').count();
        let base_end = item_bytes.iter().position(|&b| b == b'/').unwrap_or(item_bytes.len());
        let base = &item_bytes[..base_end];
        let step_bytes = if slash_count == 1 {
            Some(&item_bytes[base_end + 1..])
        } else {
            None
        };

        let hyphen_count = base.iter().filter(|&&b| b == b'-').count();
        let syntax_ok = slash_count <= 1
            && (base == b"*"
                || (hyphen_count == 0 && !base.is_empty())
                || (hyphen_count == 1
                    && !base.is_empty()
                    && base[0] != b'-'
                    && base[base.len() - 1] != b'-'));
        if !syntax_ok || step_bytes.is_some_and(|s| s.is_empty()) {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }
        if base != b"*" && base.iter().any(|&b| b == b'*') {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let dash = base.iter().position(|&b| b == b'-');
        let (start, end, start_offset, end_offset) = if base == b"*" {
            (domain_min, domain_max, item_start, item_start)
        } else if let Some(dash) = dash {
            let left = &base[..dash];
            let right = &base[dash + 1..];
            if left.is_empty()
                || right.is_empty()
                || left.iter().any(|&b| b == b'-')
                || right.iter().any(|&b| b == b'-')
            {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            let s = atom_value(field, left, item_start)?;
            let e = atom_value(field, right, item_start + dash + 1)?;
            if s > e {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: item_start + dash + 1,
                });
            }
            (s, e, item_start, item_start + dash + 1)
        } else {
            let s = atom_value(field, base, item_start)?;
            let e = if step_bytes.is_some() { domain_max } else { s };
            (s, e, item_start, item_start)
        };

        let step = if let Some(step) = step_bytes {
            if !step.iter().all(u8::is_ascii_digit) {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            Some(parse_number(
                field,
                step,
                item_start + base_end + 1,
                true,
            )?)
        } else {
            None
        };

        let _ = (start_offset, end_offset);
        if let Some(step) = step {
            let mut v = start;
            loop {
                selected.insert(v);
                if end - v < step {
                    break;
                }
                v += step;
            }
        } else {
            for v in start..=end {
                selected.insert(v);
            }
        }
        item_start += item_bytes.len() + 1;
    }
    Ok(selected)
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let expanded = if input.starts_with('@') {
        match input {
            "@hourly" => "0 * * * *",
            "@daily" => "0 0 * * *",
            "@weekly" => "0 0 * * 0",
            "@monthly" => "0 0 1 * *",
            "@yearly" => "0 0 1 1 *",
            _ => return Err(ParseError::UnknownSpecial),
        }
    } else {
        input
    };

    let pieces: Vec<&str> = expanded.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount {
            found: pieces.len(),
        });
    }
    let mut values: [BTreeSet<u8>; 5] = std::array::from_fn(|_| BTreeSet::new());
    for i in 0..5 {
        values[i] = parse_field(FIELDS[i], pieces[i])?;
    }
    Ok(Cron {
        values,
        unrestricted_dom: pieces[2] == "*",
        unrestricted_dow: pieces[4] == "*",
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.values[index(field)].iter().copied().collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate(at)?;
        Ok(self.matches_valid(at))
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        if !self.values[0].contains(&at.minute)
            || !self.values[1].contains(&at.hour)
            || !self.values[3].contains(&at.month)
        {
            return false;
        }
        let dom = self.values[2].contains(&at.day);
        let dow = self.values[4].contains(&weekday(at.year, at.month, at.day));
        match (self.unrestricted_dom, self.unrestricted_dow) {
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
        let mut t = after;
        while increment_minute(&mut t) {
            if self.matches_valid(t) {
                return Ok(Some(t));
            }
        }
        Ok(None)
    }

    pub fn previous_fire(
        &self,
        before: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
        validate(before)?;
        let mut t = before;
        while decrement_minute(&mut t) {
            if self.matches_valid(t) {
                return Ok(Some(t));
            }
        }
        Ok(None)
    }

    pub fn fires_between(
        &self,
        start: CivilDateTime,
        end: CivilDateTime,
        cap: usize,
    ) -> Result<FireWindow, WindowError> {
        validate(start).map_err(|e| match e {
            DateError::InvalidDateTime { component } => WindowError::InvalidStart { component },
        })?;
        validate(end).map_err(|e| match e {
            DateError::InvalidDateTime { component } => WindowError::InvalidEnd { component },
        })?;
        if start > end {
            return Err(WindowError::ReversedWindow);
        }
        if cap == 0 {
            return Err(WindowError::ZeroCap);
        }
        let mut fires = Vec::new();
        let mut current = start;
        while current < end {
            if self.matches_valid(current) {
                if fires.len() == cap {
                    return Ok(FireWindow {
                        fires,
                        truncated: true,
                    });
                }
                fires.push(current);
            }
            if !increment_minute(&mut current) {
                break;
            }
        }
        Ok(FireWindow {
            fires,
            truncated: false,
        })
    }

    pub fn canonical(&self) -> String {
        let mut parts = Vec::with_capacity(5);
        for i in 0..5 {
            let field = FIELDS[i];
            let (min, max) = bounds(field);
            let full = self.values[i].len() == (max - min + 1) as usize;
            let part = if full {
                if field == Field::DayOfMonth && !self.unrestricted_dom
                    || field == Field::DayOfWeek && !self.unrestricted_dow
                {
                    "*/1".to_string()
                } else {
                    "*".to_string()
                }
            } else {
                self.values[i]
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            };
            parts.push(part);
        }
        parts.join(" ")
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

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days = 0u32;
    for y in 2000..year {
        days += if is_leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as u32;
    }
    days += day as u32 - 1;
    ((days + 6) % 7) as u8
}

fn increment_minute(t: &mut CivilDateTime) -> bool {
    if t.minute < 59 {
        t.minute += 1;
        return true;
    }
    t.minute = 0;
    if t.hour < 23 {
        t.hour += 1;
        return true;
    }
    t.hour = 0;
    if t.day < days_in_month(t.year, t.month) {
        t.day += 1;
        return true;
    }
    t.day = 1;
    if t.month < 12 {
        t.month += 1;
        return true;
    }
    t.month = 1;
    if t.year < 2399 {
        t.year += 1;
        return true;
    }
    false
}

fn decrement_minute(t: &mut CivilDateTime) -> bool {
    if t.minute > 0 {
        t.minute -= 1;
        return true;
    }
    t.minute = 59;
    if t.hour > 0 {
        t.hour -= 1;
        return true;
    }
    t.hour = 23;
    if t.day > 1 {
        t.day -= 1;
        return true;
    }
    if t.month > 1 {
        t.month -= 1;
    } else {
        if t.year == 2000 {
            return false;
        }
        t.year -= 1;
        t.month = 12;
    }
    t.day = days_in_month(t.year, t.month);
    true
}