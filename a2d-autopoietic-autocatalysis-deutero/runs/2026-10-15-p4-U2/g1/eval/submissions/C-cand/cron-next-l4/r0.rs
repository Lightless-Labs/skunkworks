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
    sets: [BTreeSet<u8>; 5],
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

const FIELDS: [Field; 5] = [
    Field::Minute,
    Field::Hour,
    Field::DayOfMonth,
    Field::Month,
    Field::DayOfWeek,
];

fn bounds(field: Field) -> (u8, u8) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
    }
}

fn names(field: Field) -> Option<&'static [&'static str]> {
    match field {
        Field::Month => Some(&[
            "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV",
            "DEC",
        ]),
        Field::DayOfWeek => Some(&["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"]),
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct Atom {
    value: u8,
    numeric: bool,
    offset: usize,
}

fn parse_atom(s: &str, field: Field, offset: usize) -> Result<Atom, ParseError> {
    if s.is_empty() {
        return Err(ParseError::InvalidSyntax { field, offset });
    }
    if s.bytes().all(|b| b.is_ascii_digit()) {
        let bytes = s.as_bytes();
        if bytes.len() > 1 && bytes[0] == b'0' {
            return Err(ParseError::LeadingZero { field, offset });
        }
        let mut n = 0u32;
        for &b in bytes {
            n = n
                .checked_mul(10)
                .and_then(|v| v.checked_add((b - b'0') as u32))
                .ok_or(ParseError::NumberOverflow { field, offset })?;
        }
        let (lo, hi) = bounds(field);
        if n < lo as u32 || n > hi as u32 {
            return Err(ParseError::OutOfRange { field, offset });
        }
        return Ok(Atom {
            value: n as u8,
            numeric: true,
            offset,
        });
    }
    if s.bytes().all(|b| b.is_ascii_alphabetic()) {
        if let Some(table) = names(field) {
            let upper = s.to_ascii_uppercase();
            if let Some(i) = table.iter().position(|name| *name == upper) {
                let value = if field == Field::Month {
                    (i + 1) as u8
                } else {
                    i as u8
                };
                return Ok(Atom {
                    value,
                    numeric: false,
                    offset,
                });
            }
        }
    }
    Err(ParseError::InvalidSyntax { field, offset })
}

fn parse_step(s: &str, field: Field, offset: usize) -> Result<u8, ParseError> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ParseError::InvalidSyntax { field, offset });
    }
    let bytes = s.as_bytes();
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }
    let mut n = 0u32;
    for &b in bytes {
        n = n
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    let (lo, hi) = bounds(field);
    let width = hi as u32 - lo as u32 + 1;
    if n == 0 || n > width {
        return Err(ParseError::BadStep { field, offset });
    }
    Ok(n as u8)
}

fn parse_field(text: &str, field: Field) -> Result<BTreeSet<u8>, ParseError> {
    for (offset, byte) in text.bytes().enumerate() {
        if !(byte.is_ascii_digit()
            || byte.is_ascii_alphabetic()
            || matches!(byte, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (minimum, maximum) = bounds(field);
    let mut result = BTreeSet::new();
    let mut start = 0usize;
    for item in text.split(',') {
        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: start,
            });
        }
        let item_offset = start;
        start += item.len() + 1;

        let slash_parts: Vec<&str> = item.split('/').collect();
        if slash_parts.len() > 2
            || slash_parts[0].is_empty()
            || (slash_parts.len() == 2 && slash_parts[1].is_empty())
        {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
        let base = slash_parts[0];
        let step_offset = item_offset + base.len() + 1;
        let step = if slash_parts.len() == 2 {
            Some(parse_step(slash_parts[1], field, step_offset)?)
        } else {
            None
        };

        let (range_start, range_end, end_offset) = if base == "*" {
            (minimum, maximum, item_offset)
        } else {
            let hyphens: Vec<usize> = base
                .match_indices('-')
                .map(|(idx, _)| idx)
                .collect();
            if hyphens.len() > 1 {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_offset,
                });
            }
            if let Some(&hyphen) = hyphens.first() {
                let left = &base[..hyphen];
                let right = &base[hyphen + 1..];
                if left.is_empty() || right.is_empty() || left == "*" || right == "*" {
                    return Err(ParseError::InvalidSyntax {
                        field,
                        offset: item_offset,
                    });
                }
                let a = parse_atom(left, field, item_offset)?;
                let b = parse_atom(right, field, item_offset + hyphen + 1)?;
                if a.value > b.value {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: b.offset,
                    });
                }
                (a.value, b.value, b.offset)
            } else {
                let atom = parse_atom(base, field, item_offset)?;
                (
                    atom.value,
                    if step.is_some() { maximum } else { atom.value },
                    atom.offset,
                )
            }
        };
        let _ = end_offset;
        let increment = step.unwrap_or(1);
        let mut value = range_start;
        while value <= range_end {
            result.insert(value);
            match value.checked_add(increment) {
                Some(next) => value = next,
                None => break,
            }
        }
    }
    Ok(result)
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
    let parts: Vec<&str> = expanded.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }
    let mut sets: [BTreeSet<u8>; 5] = std::array::from_fn(|_| BTreeSet::new());
    for i in 0..5 {
        sets[i] = parse_field(parts[i], FIELDS[i])?;
    }
    Ok(Cron {
        sets,
        unrestricted_dom: parts[2] == "*",
        unrestricted_dow: parts[4] == "*",
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.sets[field_index(field)].iter().copied().collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate(at)?;
        Ok(self.matches_valid(at))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate(after)?;
        let mut date = (after.year, after.month, after.day);
        let mut hour_minute = after.hour as u16 * 60 + after.minute as u16;
        loop {
            if self.date_matches(date.0, date.1, date.2) {
                let day_minutes: Vec<u16> = self.sets[1]
                    .iter()
                    .flat_map(|h| self.sets[0].iter().map(move |m| *h as u16 * 60 + *m as u16))
                    .collect();
                if let Some(&minute) = day_minutes.iter().find(|&&v| v > hour_minute) {
                    return Some(make_datetime(
                        date.0,
                        date.1,
                        date.2,
                        (minute / 60) as u8,
                        (minute % 60) as u8,
                    ))
                    .pipe(Ok);
                }
            }
            if !advance_date(&mut date) {
                return Ok(None);
            }
            hour_minute = 0;
        }
    }

    pub fn previous_fire(&self, before: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate(before)?;
        let mut date = (before.year, before.month, before.day);
        let mut limit = before.hour as u16 * 60 + before.minute as u16;
        loop {
            if self.date_matches(date.0, date.1, date.2) {
                for hour in self.sets[1].iter().rev() {
                    for minute in self.sets[0].iter().rev() {
                        let value = *hour as u16 * 60 + *minute as u16;
                        if value < limit {
                            return Ok(Some(make_datetime(
                                date.0,
                                date.1,
                                date.2,
                                *hour,
                                *minute,
                            )));
                        }
                    }
                }
            }
            if !retreat_date(&mut date) {
                return Ok(None);
            }
            limit = 24 * 60;
        }
    }

    pub fn fires_between(
        &self,
        start: CivilDateTime,
        end: CivilDateTime,
        cap: usize,
    ) -> Result<FireWindow, WindowError> {
        validate(start).map_err(|e| WindowError::InvalidStart {
            component: match e {
                DateError::InvalidDateTime { component } => component,
            },
        })?;
        validate(end).map_err(|e| WindowError::InvalidEnd {
            component: match e {
                DateError::InvalidDateTime { component } => component,
            },
        })?;
        if start > end {
            return Err(WindowError::ReversedWindow);
        }
        if cap == 0 {
            return Err(WindowError::ZeroCap);
        }
        let mut fires = Vec::new();
        let mut cursor = start;
        while cursor < end {
            if self.matches_valid(cursor) {
                if fires.len() == cap {
                    return Ok(FireWindow {
                        fires,
                        truncated: true,
                    });
                }
                fires.push(cursor);
            }
            match self.next_fire(cursor) {
                Ok(Some(next)) if next < end => cursor = next,
                _ => break,
            }
        }
        Ok(FireWindow {
            fires,
            truncated: false,
        })
    }

    pub fn canonical(&self) -> String {
        let mut fields = Vec::new();
        for i in 0..5 {
            let field = FIELDS[i];
            let (lo, hi) = bounds(field);
            let full = self.sets[i].len() == (hi - lo + 1) as usize
                && (lo..=hi).all(|n| self.sets[i].contains(&n));
            if i == 2 && self.unrestricted_dom {
                fields.push("*".to_string());
            } else if i == 4 && self.unrestricted_dow {
                fields.push("*".to_string());
            } else if full && matches!(field, Field::Minute | Field::Hour | Field::Month) {
                fields.push("*".to_string());
            } else if full && matches!(field, Field::DayOfMonth | Field::DayOfWeek) {
                fields.push("*/1".to_string());
            } else {
                fields.push(
                    self.sets[i]
                        .iter()
                        .map(u8::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                );
            }
        }
        fields.join(" ")
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        self.sets[0].contains(&at.minute)
            && self.sets[1].contains(&at.hour)
            && self.sets[3].contains(&at.month)
            && self.date_matches(at.year, at.month, at.day)
    }

    fn date_matches(&self, year: u16, month: u8, day: u8) -> bool {
        let dom = self.sets[2].contains(&day);
        let dow = self.sets[4].contains(&weekday(year, month, day));
        match (self.unrestricted_dom, self.unrestricted_dow) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) => {
                if day == days_in_month(year, month) {
                    dom && dow
                } else {
                    dom || dow
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FireWindow {
    pub fires: Vec<CivilDateTime>,
    pub truncated: bool,
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

fn leap(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 => if leap(year) { 29 } else { 28 },
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut count = 0u32;
    for y in 2000..year {
        count += if leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        count += days_in_month(year, m) as u32;
    }
    count += day as u32 - 1;
    ((count + 6) % 7) as u8
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

fn make_datetime(year: u16, month: u8, day: u8, hour: u8, minute: u8) -> CivilDateTime {
    CivilDateTime {
        year,
        month,
        day,
        hour,
        minute,
    }
}

fn advance_date(date: &mut (u16, u8, u8)) -> bool {
    if date.2 < days_in_month(date.0, date.1) {
        date.2 += 1;
    } else if date.1 < 12 {
        date.1 += 1;
        date.2 = 1;
    } else if date.0 < 2399 {
        date.0 += 1;
        date.1 = 1;
        date.2 = 1;
    } else {
        return false;
    }
    true
}

fn retreat_date(date: &mut (u16, u8, u8)) -> bool {
    if date.2 > 1 {
        date.2 -= 1;
    } else if date.1 > 1 {
        date.1 -= 1;
        date.2 = days_in_month(date.0, date.1);
    } else if date.0 > 2000 {
        date.0 -= 1;
        date.1 = 12;
        date.2 = 31;
    } else {
        return false;
    }
    true
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}

fn main() {}