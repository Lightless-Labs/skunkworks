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
    sets: [Vec<bool>; 5],
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

fn name_value(field: Field, bytes: &[u8]) -> Option<u32> {
    if bytes.len() != 3 || !bytes.iter().all(u8::is_ascii_alphabetic) {
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

fn syntax_error(field: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field, offset }
}

fn parse_base(field: Field, bytes: &[u8], offset: usize) -> Result<u32, ParseError> {
    if bytes.is_empty() {
        return Err(syntax_error(field, offset));
    }
    if bytes.iter().all(u8::is_ascii_digit) {
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
        Ok(value)
    } else if let Some(value) = name_value(field, bytes) {
        Ok(value)
    } else {
        Err(syntax_error(field, offset))
    }
}

fn parse_step(field: Field, bytes: &[u8], offset: usize) -> Result<u32, ParseError> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(syntax_error(field, offset));
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
    let (min, max) = bounds(field);
    let width = max - min + 1;
    if value == 0 || value > width {
        return Err(ParseError::BadStep { field, offset });
    }
    Ok(value)
}

fn parse_item(
    field: Field,
    item: &[u8],
    item_offset: usize,
    selected: &mut [bool],
) -> Result<(), ParseError> {
    if item.is_empty() {
        return Err(ParseError::EmptyItem {
            field,
            offset: item_offset,
        });
    }

    let slash_positions: Vec<usize> = item
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'/').then_some(i))
        .collect();

    let (base, step_bytes, step_offset) = match slash_positions.as_slice() {
        [] => (item, None, 0),
        [slash] => (&item[..*slash], Some(&item[*slash + 1..]), *slash + 1),
        _ => return Err(syntax_error(field, item_offset)),
    };

    let (min, max) = bounds(field);
    let (start, end) = if base == b"*" {
        (min, max)
    } else {
        let hyphens: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();

        match hyphens.as_slice() {
            [] => {
                let value = parse_base(field, base, item_offset)?;
                let end = if step_bytes.is_some() { max } else { value };
                (value, end)
            }
            [dash] => {
                let left = &base[..*dash];
                let right = &base[*dash + 1..];
                if left.is_empty() || right.is_empty() || left == b"*" || right == b"*" {
                    return Err(syntax_error(field, item_offset));
                }
                let start = parse_base(field, left, item_offset)?;
                let right_offset = item_offset + dash + 1;
                let end = parse_base(field, right, right_offset)?;
                if start > end {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: right_offset,
                    });
                }
                (start, end)
            }
            _ => return Err(syntax_error(field, item_offset)),
        }
    };

    let step = match step_bytes {
        None => None,
        Some(bytes) => Some(parse_step(field, bytes, item_offset + step_offset)?),
    };

    if step.is_none() && base != b"*" && start == end {
        selected[(start - min) as usize] = true;
        return Ok(());
    }

    let increment = step.unwrap_or(1);
    let mut value = start;
    loop {
        selected[(value - min) as usize] = true;
        if value + increment > end {
            break;
        }
        value += increment;
    }
    Ok(())
}

fn parse_field(field: Field, text: &str) -> Result<Vec<bool>, ParseError> {
    let bytes = text.as_bytes();
    for (offset, &b) in bytes.iter().enumerate() {
        if !(b.is_ascii_digit()
            || b.is_ascii_alphabetic()
            || matches!(b, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (min, max) = bounds(field);
    let mut selected = vec![false; (max - min + 1) as usize];
    let mut start = 0;
    for i in 0..=bytes.len() {
        if i == bytes.len() || bytes[i] == b',' {
            parse_item(field, &bytes[start..i], start, &mut selected)?;
            start = i + 1;
        }
    }
    Ok(selected)
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let expanded = if input.as_bytes().first() == Some(&b'@') {
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

    let fields = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];
    let mut sets = Vec::with_capacity(5);
    for (field, part) in fields.into_iter().zip(parts.iter()) {
        sets.push(parse_field(field, part)?);
    }
    let sets: [Vec<bool>; 5] = sets.try_into().ok().unwrap();

    Ok(Cron {
        sets,
        unrestricted_dom: parts[2] == "*",
        unrestricted_dow: parts[4] == "*",
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let (min, _) = bounds(field);
        self.sets[field_index(field)]
            .iter()
            .enumerate()
            .filter_map(|(i, &allowed)| allowed.then_some(min + i as u32))
            .map(|v| v as u8)
            .collect()
    }

    fn selected(&self, field: Field, value: u8) -> bool {
        let (min, max) = bounds(field);
        let value = value as u32;
        value >= min
            && value <= max
            && self.sets[field_index(field)][(value - min) as usize]
    }

    fn date_matches(&self, date: CivilDateTime, weekday: u8) -> bool {
        let dom = self.selected(Field::DayOfMonth, date.day);
        let dow = self.selected(Field::DayOfWeek, weekday);
        match (self.unrestricted_dom, self.unrestricted_dow) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) => {
                if date.day == days_in_month(date.year, date.month) {
                    dom && dow
                } else {
                    dom || dow
                }
            }
        }
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        let weekday = weekday(at.year, at.month, at.day);
        Ok(self.selected(Field::Minute, at.minute)
            && self.selected(Field::Hour, at.hour)
            && self.selected(Field::Month, at.month)
            && self.date_matches(at, weekday))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;
        let mut date = CivilDateTime {
            hour: 0,
            minute: 0,
            ..after
        };
        let mut wd = weekday(date.year, date.month, date.day);

        loop {
            if self.selected(Field::Month, date.month) && self.date_matches(date, wd) {
                for hour in self.allowed(Field::Hour) {
                    for minute in self.allowed(Field::Minute) {
                        let candidate = CivilDateTime { hour, minute, ..date };
                        if candidate > after {
                            return Ok(Some(candidate));
                        }
                    }
                }
            }
            if !advance_day(&mut date) {
                return Ok(None);
            }
            wd = (wd + 1) % 7;
        }
    }

    pub fn previous_fire(&self, before: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(before)?;
        let mut date = CivilDateTime {
            hour: 0,
            minute: 0,
            ..before
        };
        let mut wd = weekday(date.year, date.month, date.day);

        loop {
            if self.selected(Field::Month, date.month) && self.date_matches(date, wd) {
                let hours = self.allowed(Field::Hour);
                let minutes = self.allowed(Field::Minute);
                for &hour in hours.iter().rev() {
                    for &minute in minutes.iter().rev() {
                        let candidate = CivilDateTime { hour, minute, ..date };
                        if candidate < before {
                            return Ok(Some(candidate));
                        }
                    }
                }
            }
            if !retreat_day(&mut date) {
                return Ok(None);
            }
            wd = (wd + 6) % 7;
        }
    }

    pub fn fires_between(
        &self,
        start: CivilDateTime,
        end: CivilDateTime,
        cap: usize,
    ) -> Result<FireWindow, WindowError> {
        validate_datetime(start).map_err(|e| match e {
            DateError::InvalidDateTime { component } => WindowError::InvalidStart { component },
        })?;
        validate_datetime(end).map_err(|e| match e {
            DateError::InvalidDateTime { component } => WindowError::InvalidEnd { component },
        })?;
        if start > end {
            return Err(WindowError::ReversedWindow);
        }
        if cap == 0 {
            return Err(WindowError::ZeroCap);
        }

        let mut fires = Vec::new();
        let mut cursor = start;

        if self.matches(start).unwrap() {
            fires.push(start);
            if fires.len() == cap {
                let truncated = self
                    .next_fire(start)
                    .unwrap()
                    .is_some_and(|next| next < end);
                return Ok(FireWindow { fires, truncated });
            }
        }

        while let Some(next) = self.next_fire(cursor).unwrap() {
            if next >= end {
                break;
            }
            if fires.len() == cap {
                return Ok(FireWindow {
                    fires,
                    truncated: true,
                });
            }
            fires.push(next);
            cursor = next;
        }

        Ok(FireWindow {
            fires,
            truncated: false,
        })
    }

    pub fn canonical(&self) -> String {
        let fields = [
            Field::Minute,
            Field::Hour,
            Field::DayOfMonth,
            Field::Month,
            Field::DayOfWeek,
        ];
        fields
            .iter()
            .map(|&field| {
                let (min, max) = bounds(field);
                let vals = self.allowed(field);
                let unrestricted = match field {
                    Field::DayOfMonth => self.unrestricted_dom,
                    Field::DayOfWeek => self.unrestricted_dow,
                    _ => false,
                };
                if unrestricted {
                    return "*".to_string();
                }
                if vals.len() == (max - min + 1) as usize {
                    return match field {
                        Field::DayOfMonth | Field::DayOfWeek => "*/1".to_string(),
                        _ => "*".to_string(),
                    };
                }
                vals.iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn leap(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 => {
            if leap(year) {
                29
            } else {
                28
            }
        }
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

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days: i64 = 0;
    for y in 2000..year {
        days += if leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as i64;
    }
    days += (day - 1) as i64;
    ((days + 6) % 7) as u8
}

fn advance_day(date: &mut CivilDateTime) -> bool {
    if date.day < days_in_month(date.year, date.month) {
        date.day += 1;
    } else if date.month < 12 {
        date.day = 1;
        date.month += 1;
    } else if date.year < 2399 {
        date.day = 1;
        date.month = 1;
        date.year += 1;
    } else {
        return false;
    }
    true
}

fn retreat_day(date: &mut CivilDateTime) -> bool {
    if date.day > 1 {
        date.day -= 1;
    } else if date.month > 1 {
        date.month -= 1;
        date.day = days_in_month(date.year, date.month);
    } else if date.year > 2000 {
        date.year -= 1;
        date.month = 12;
        date.day = 31;
    } else {
        return false;
    }
    true
}