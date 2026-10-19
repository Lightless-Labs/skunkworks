use std::cmp::Ordering;

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
    selected: [Vec<bool>; 5],
    unrestricted_day: [bool; 2],
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

fn bounds(field: Field) -> (u32, u32) {
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

fn parse_error(field: Field, offset: usize, make: fn(Field, usize) -> ParseError) -> ParseError {
    make(field, offset)
}

fn syntax(field: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field, offset }
}

fn parse_atom(field: Field, bytes: &[u8], offset: usize) -> Result<u32, ParseError> {
    let names: &[(&[u8], u32)] = match field {
        Field::Month => &[
            (b"JAN", 1), (b"FEB", 2), (b"MAR", 3), (b"APR", 4),
            (b"MAY", 5), (b"JUN", 6), (b"JUL", 7), (b"AUG", 8),
            (b"SEP", 9), (b"OCT", 10), (b"NOV", 11), (b"DEC", 12),
        ],
        Field::DayOfWeek => &[
            (b"SUN", 0), (b"MON", 1), (b"TUE", 2), (b"WED", 3),
            (b"THU", 4), (b"FRI", 5), (b"SAT", 6),
        ],
        _ => &[],
    };

    if !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit) {
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
        let (min, max) = bounds(field);
        if value < min || value > max {
            return Err(ParseError::OutOfRange { field, offset });
        }
        return Ok(value);
    }

    if !bytes.is_empty() {
        for (name, value) in names {
            if bytes.len() == name.len()
                && bytes.iter().zip(name.iter()).all(|(a, b)| a.to_ascii_uppercase() == *b)
            {
                return Ok(*value);
            }
        }
    }
    Err(syntax(field, offset))
}

fn parse_step(field: Field, bytes: &[u8], offset: usize) -> Result<u32, ParseError> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(syntax(field, offset));
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
    let (_, max) = bounds(field);
    if value == 0 || value > max + 1 {
        return Err(ParseError::BadStep { field, offset });
    }
    Ok(value)
}

fn parse_field(field: Field, text: &str) -> Result<(Vec<bool>, bool), ParseError> {
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if !(b.is_ascii_digit()
            || b.is_ascii_alphabetic()
            || matches!(b, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset: i });
        }
    }

    let (min, max) = bounds(field);
    let mut selected = vec![false; (max + 1) as usize];
    let unrestricted = matches!(field, Field::DayOfMonth | Field::DayOfWeek) && text == "*";

    let mut item_start = 0;
    for item in text.split(',') {
        if item.is_empty() {
            return Err(ParseError::EmptyItem { field, offset: item_start });
        }
        let item_bytes = item.as_bytes();
        let slash_positions: Vec<usize> = item_bytes
            .iter()
            .enumerate()
            .filter_map(|(i, b)| (*b == b'/').then_some(i))
            .collect();
        if slash_positions.len() > 1 {
            return Err(syntax(field, item_start));
        }

        let (base, step_bytes, step_offset) = if let Some(&slash) = slash_positions.first() {
            (&item_bytes[..slash], Some(&item_bytes[slash + 1..]), Some(item_start + slash + 1))
        } else {
            (item_bytes, None, None)
        };

        let is_star = base == b"*";
        let hyphens: Vec<usize> = base.iter().enumerate()
            .filter_map(|(i, b)| (*b == b'-').then_some(i))
            .collect();

        if (!is_star && hyphens.len() > 1)
            || (is_star && !hyphens.is_empty())
            || (base.is_empty())
        {
            return Err(syntax(field, item_start));
        }
        if is_star && step_bytes.is_none() {
            for n in min..=max {
                selected[n as usize] = true;
            }
        } else {
            let (start_bytes, end_bytes, end_offset) = if let Some(&hyphen) = hyphens.first() {
                let left = &base[..hyphen];
                let right = &base[hyphen + 1..];
                if left.is_empty() || right.is_empty() {
                    return Err(syntax(field, item_start));
                }
                (left, Some(right), Some(item_start + hyphen + 1))
            } else {
                (base, None, None)
            };

            let start = parse_atom(field, start_bytes, item_start)?;
            let end = if let (Some(end_b), Some(end_o)) = (end_bytes, end_offset) {
                parse_atom(field, end_b, end_o)?
            } else if is_star {
                max
            } else {
                start
            };

            if end_bytes.is_some() && start > end {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: end_offset.unwrap(),
                });
            }

            let step = if let (Some(step_b), Some(step_o)) = (step_bytes, step_offset) {
                parse_step(field, step_b, step_o)?
            } else {
                1
            };

            let effective_end = if step_bytes.is_some() && end_bytes.is_none() && !is_star {
                max
            } else {
                end
            };
            let mut n = start;
            while n <= effective_end {
                selected[n as usize] = true;
                match n.checked_add(step) {
                    Some(next) if next > n => n = next,
                    _ => break,
                }
            }
        }
        item_start += item.len() + 1;
    }

    Ok((selected, unrestricted))
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    if input.as_bytes().first() == Some(&b'@') {
        let expansion = match input {
            "@hourly" => "0 * * * *",
            "@daily" => "0 0 * * *",
            "@weekly" => "0 0 * * 0",
            "@monthly" => "0 0 1 * *",
            "@yearly" => "0 0 1 1 *",
            _ => return Err(ParseError::UnknownSpecial),
        };
        return parse_cron(expansion);
    }

    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut parsed = Vec::with_capacity(5);
    for (field, text) in FIELDS.into_iter().zip(parts) {
        parsed.push(parse_field(field, text)?);
    }
    let mut iter = parsed.into_iter();
    let (m, _) = iter.next().unwrap();
    let (h, _) = iter.next().unwrap();
    let (dom, dom_unrestricted) = iter.next().unwrap();
    let (month, _) = iter.next().unwrap();
    let (dow, dow_unrestricted) = iter.next().unwrap();
    Ok(Cron {
        selected: [m, h, dom, month, dow],
        unrestricted_day: [dom_unrestricted, dow_unrestricted],
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.selected[field_index(field)]
            .iter()
            .enumerate()
            .filter_map(|(n, yes)| (*yes).then_some(n as u8))
            .collect()
    }

    fn valid_datetime(at: CivilDateTime) -> Result<(), DateError> {
        if !(2000..=2399).contains(&at.year) {
            return Err(DateError::InvalidDateTime { component: DatePart::Year });
        }
        if !(1..=12).contains(&at.month) {
            return Err(DateError::InvalidDateTime { component: DatePart::Month });
        }
        if !(1..=days_in_month(at.year, at.month)).contains(&at.day) {
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

    fn date_matches(&self, at: CivilDateTime) -> bool {
        if !self.selected[2][at.day as usize] || !self.selected[4][weekday(at.year, at.month, at.day) as usize] {
            match (self.unrestricted_day[0], self.unrestricted_day[1]) {
                (true, true) => {}
                (true, false) if self.selected[4][weekday(at.year, at.month, at.day) as usize] => {}
                (false, true) if self.selected[2][at.day as usize] => {}
                (false, false) => {
                    let dom = self.selected[2][at.day as usize];
                    let dow = self.selected[4][weekday(at.year, at.month, at.day) as usize];
                    if at.day == days_in_month(at.year, at.month) {
                        return dom && dow;
                    }
                    return dom || dow;
                }
                _ => return false,
            }
        }
        true
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        self.selected[0][at.minute as usize]
            && self.selected[1][at.hour as usize]
            && self.selected[3][at.month as usize]
            && self.date_matches(at)
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        Self::valid_datetime(at)?;
        Ok(self.matches_valid(at))
    }

    fn search(&self, from: CivilDateTime, inclusive: bool, forward: bool) -> Option<CivilDateTime> {
        let mut date = CivilDateTime { hour: 0, minute: 0, ..from };
        let mut first_date = true;
        loop {
            if self.selected[3][date.month as usize]
                && self.date_matches(date)
            {
                let hours = self.allowed(Field::Hour);
                let minutes = self.allowed(Field::Minute);
                if forward {
                    for &hour in &hours {
                        for &minute in &minutes {
                            let candidate = CivilDateTime { hour, minute, ..date };
                            let cmp = candidate.cmp(&from);
                            if cmp == Ordering::Greater || (inclusive && cmp == Ordering::Equal) {
                                return Some(candidate);
                            }
                        }
                    }
                } else {
                    for &hour in hours.iter().rev() {
                        for &minute in minutes.iter().rev() {
                            let candidate = CivilDateTime { hour, minute, ..date };
                            let cmp = candidate.cmp(&from);
                            if cmp == Ordering::Less || (inclusive && cmp == Ordering::Equal) {
                                return Some(candidate);
                            }
                        }
                    }
                }
            }

            if forward {
                if date.year == 2399 && date.month == 12 && date.day == 31 {
                    return None;
                }
                let (year, month, day) = next_date(date.year, date.month, date.day);
                date = CivilDateTime { year, month, day, hour: 0, minute: 0 };
            } else {
                if date.year == 2000 && date.month == 1 && date.day == 1 {
                    return None;
                }
                let (year, month, day) = previous_date(date.year, date.month, date.day);
                date = CivilDateTime { year, month, day, hour: 0, minute: 0 };
            }
            first_date = false;
            let _ = first_date;
        }
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        Self::valid_datetime(after)?;
        Ok(self.search(after, false, true))
    }

    pub fn previous_fire(&self, before: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        Self::valid_datetime(before)?;
        Ok(self.search(before, false, false))
    }

    pub fn fires_between(
        &self,
        start: CivilDateTime,
        end: CivilDateTime,
        cap: usize,
    ) -> Result<FireWindow, WindowError> {
        Self::valid_datetime(start).map_err(|e| match e {
            DateError::InvalidDateTime { component } => WindowError::InvalidStart { component },
        })?;
        Self::valid_datetime(end).map_err(|e| match e {
            DateError::InvalidDateTime { component } => WindowError::InvalidEnd { component },
        })?;
        if start > end {
            return Err(WindowError::ReversedWindow);
        }
        if cap == 0 {
            return Err(WindowError::ZeroCap);
        }

        let mut fires = Vec::new();
        if start == end {
            return Ok(FireWindow { fires, truncated: false });
        }
        let mut cursor = start;
        loop {
            if self.matches_valid(cursor) {
                if fires.len() == cap {
                    return Ok(FireWindow { fires, truncated: true });
                }
                fires.push(cursor);
            }
            let Some(next) = self.search(cursor, false, true) else {
                break;
            };
            if next >= end {
                break;
            }
            cursor = next;
        }
        Ok(FireWindow { fires, truncated: false })
    }

    pub fn canonical(&self) -> String {
        let mut fields = Vec::new();
        for field in FIELDS {
            let vals = self.allowed(field);
            let (_, max) = bounds(field);
            let full = vals.len() == (max - bounds(field).0 + 1) as usize;
            let text = match field {
                Field::DayOfMonth if self.unrestricted_day[0] => "*".to_string(),
                Field::DayOfWeek if self.unrestricted_day[1] => "*".to_string(),
                Field::DayOfMonth | Field::DayOfWeek if full => "*/1".to_string(),
                Field::Minute | Field::Hour | Field::Month if full => "*".to_string(),
                _ => vals.iter().map(u8::to_string).collect::<Vec<_>>().join(","),
            };
            fields.push(text);
        }
        fields.join(" ")
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
    let mut days = 0u32;
    for y in 2000..year {
        days += if leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as u32;
    }
    days += (day - 1) as u32;
    ((days + 6) % 7) as u8
}

fn next_date(year: u16, month: u8, day: u8) -> (u16, u8, u8) {
    if day < days_in_month(year, month) {
        (year, month, day + 1)
    } else if month < 12 {
        (year, month + 1, 1)
    } else {
        (year + 1, 1, 1)
    }
}

fn previous_date(year: u16, month: u8, day: u8) -> (u16, u8, u8) {
    if day > 1 {
        (year, month, day - 1)
    } else if month > 1 {
        (year, month - 1, days_in_month(year, month - 1))
    } else {
        (year - 1, 12, 31)
    }
}