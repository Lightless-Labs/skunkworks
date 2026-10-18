use std::str::FromStr;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    selected: [Vec<bool>; 5],
    unrestricted_dom: bool,
    unrestricted_dow: bool,
}

const FIELDS: [Field; 5] = [
    Field::Minute,
    Field::Hour,
    Field::DayOfMonth,
    Field::Month,
    Field::DayOfWeek,
];

fn domain(field: Field) -> (u8, u8) {
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

fn parse_error(field: Field, offset: usize, kind: fn(Field, usize) -> ParseError) -> ParseError {
    kind(field, offset)
}

fn invalid_syntax(field: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field, offset }
}

fn number(
    bytes: &[u8],
    start: usize,
    field: Field,
    is_step: bool,
) -> Result<u32, ParseError> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(invalid_syntax(field, start));
    }

    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero {
            field,
            offset: start,
        });
    }

    let mut value = 0u32;
    for &byte in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow {
                field,
                offset: start,
            })?;
    }

    let (min, max) = domain(field);
    if is_step {
        let width = (max as u32) - (min as u32) + 1;
        if value == 0 || value > width {
            return Err(ParseError::BadStep {
                field,
                offset: start,
            });
        }
    } else if value < min as u32 || value > max as u32 {
        return Err(ParseError::OutOfRange {
            field,
            offset: start,
        });
    }
    Ok(value)
}

fn name_value(field: Field, bytes: &[u8]) -> Option<u8> {
    if !bytes.iter().all(u8::is_ascii_alphabetic) {
        return None;
    }
    let upper: Vec<u8> = bytes.iter().map(u8::to_ascii_uppercase).collect();
    match field {
        Field::Month => {
            let names: [&[u8]; 12] = [
                b"JAN", b"FEB", b"MAR", b"APR", b"MAY", b"JUN", b"JUL", b"AUG", b"SEP",
                b"OCT", b"NOV", b"DEC",
            ];
            names
                .iter()
                .position(|name| *name == upper.as_slice())
                .map(|i| (i + 1) as u8)
        }
        Field::DayOfWeek => {
            let names: [&[u8]; 7] = [
                b"SUN", b"MON", b"TUE", b"WED", b"THU", b"FRI", b"SAT",
            ];
            names
                .iter()
                .position(|name| *name == upper.as_slice())
                .map(|i| i as u8)
        }
        _ => None,
    }
}

fn endpoint(
    bytes: &[u8],
    start: usize,
    field: Field,
) -> Result<u8, ParseError> {
    if bytes.iter().all(u8::is_ascii_digit) {
        return number(bytes, start, field, false).map(|v| v as u8);
    }
    name_value(field, bytes).ok_or_else(|| invalid_syntax(field, start))
}

fn parse_field(text: &str, field: Field) -> Result<Vec<bool>, ParseError> {
    let bytes = text.as_bytes();
    for (offset, &byte) in bytes.iter().enumerate() {
        if !(byte.is_ascii_digit()
            || byte.is_ascii_alphabetic()
            || matches!(byte, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (min, max) = domain(field);
    let mut selected = vec![false; max as usize + 1];

    let mut item_start = 0;
    loop {
        let relative_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',');
        let item_end = relative_end.map_or(bytes.len(), |n| item_start + n);
        let item = &bytes[item_start..item_end];

        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let slash_count = item.iter().filter(|&&b| b == b'/').count();
        if slash_count > 1 {
            return Err(invalid_syntax(field, item_start));
        }
        let (base, step_bytes, step_start) = if let Some(pos) = item.iter().position(|&b| b == b'/') {
            let step_start = item_start + pos + 1;
            if pos == 0 || pos + 1 == item.len() {
                return Err(invalid_syntax(field, item_start));
            }
            (&item[..pos], Some(&item[pos + 1..]), step_start)
        } else {
            (item, None, item_start)
        };

        let starred = base == b"*";
        let hyphens = base.iter().filter(|&&b| b == b'-').count();
        if !starred && hyphens > 1 {
            return Err(invalid_syntax(field, item_start));
        }
        let range_pos = base.iter().position(|&b| b == b'-');
        let (start_value, end_value, start_offset, end_offset) = if starred {
            (min, max, item_start, item_start)
        } else if let Some(pos) = range_pos {
            if pos == 0 || pos + 1 == base.len() {
                return Err(invalid_syntax(field, item_start));
            }
            if base[..pos].contains(&b'-') || base[pos + 1..].contains(&b'-') {
                return Err(invalid_syntax(field, item_start));
            }

            // Validate the complete base structure before reading numeric values.
            let left = &base[..pos];
            let right = &base[pos + 1..];
            let valid_endpoint = |s: &[u8]| {
                s.iter().all(u8::is_ascii_digit)
                    || name_value(field, s).is_some()
            };
            if !valid_endpoint(left) || !valid_endpoint(right) {
                return Err(invalid_syntax(field, item_start));
            }

            let left_offset = item_start;
            let right_offset = item_start + pos + 1;
            let a = endpoint(left, left_offset, field)?;
            let b = endpoint(right, right_offset, field)?;
            if a > b {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: right_offset,
                });
            }
            (a, b, left_offset, right_offset)
        } else {
            let valid_endpoint = base.iter().all(u8::is_ascii_digit)
                || name_value(field, base).is_some();
            if !valid_endpoint {
                return Err(invalid_syntax(field, item_start));
            }
            let a = endpoint(base, item_start, field)?;
            (a, a, item_start, item_start)
        };

        let step = if let Some(step_bytes) = step_bytes {
            if !step_bytes.iter().all(u8::is_ascii_digit) {
                return Err(invalid_syntax(field, item_start));
            }
            number(step_bytes, step_start, field, true)? as u8
        } else {
            0
        };

        let end = if step_bytes.is_some() && !starred && range_pos.is_none() {
            max
        } else {
            end_value
        };
        let stride = if step == 0 { 1 } else { step };
        let mut value = start_value;
        while value <= end {
            selected[value as usize] = true;
            match value.checked_add(stride) {
                Some(next) if next > value => value = next,
                _ => break,
            }
        }

        let _ = (start_offset, end_offset);
        if item_end == bytes.len() {
            break;
        }
        item_start = item_end + 1;
    }

    Ok(selected)
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

    let mut selected = Vec::with_capacity(5);
    for (text, field) in parts.iter().zip(FIELDS) {
        selected.push(parse_field(text, field)?);
    }

    Ok(Cron {
        selected: selected.try_into().ok().unwrap(),
        unrestricted_dom: parts[2] == "*",
        unrestricted_dow: parts[4] == "*",
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let idx = field_index(field);
        self.selected[idx]
            .iter()
            .enumerate()
            .filter_map(|(n, &yes)| {
                let (min, max) = domain(field);
                if yes && n >= min as usize && n <= max as usize {
                    Some(n as u8)
                } else {
                    None
                }
            })
            .collect()
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        if !self.selected[0][at.minute as usize]
            || !self.selected[1][at.hour as usize]
            || !self.selected[3][at.month as usize]
        {
            return false;
        }

        let dom = self.selected[2][at.day as usize];
        let dow = self.selected[4][weekday(at.year, at.month, at.day) as usize];

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

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        Ok(self.matches_valid(at))
    }

    pub fn next_fire(
        &self,
        after: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;
        let mut date = (after.year, after.month, after.day);
        let mut minute_of_day = after.hour as u16 * 60 + after.minute as u16 + 1;

        loop {
            if minute_of_day >= 1440 {
                minute_of_day = 0;
                if !advance_date(&mut date.0, &mut date.1, &mut date.2) {
                    return Ok(None);
                }
            }

            let at = CivilDateTime {
                year: date.0,
                month: date.1,
                day: date.2,
                hour: (minute_of_day / 60) as u8,
                minute: (minute_of_day % 60) as u8,
            };
            if self.matches_valid(at) {
                return Ok(Some(at));
            }

            let hour = (minute_of_day / 60) as usize;
            let minute = (minute_of_day % 60) as usize;
            if self.selected[0][minute] {
                minute_of_day += 1;
            } else {
                let next_minute = (minute + 1..60).find(|&m| self.selected[0][m]);
                if let Some(m) = next_minute {
                    minute_of_day = (hour * 60 + m) as u16;
                } else {
                    let next_hour = (hour + 1..24).find(|&h| self.selected[1][h]);
                    if let Some(h) = next_hour {
                        let first_minute = self.allowed(Field::Minute)[0];
                        minute_of_day = (h * 60 + first_minute as usize) as u16;
                    } else {
                        let first_hour = self.allowed(Field::Hour)[0];
                        let first_minute = self.allowed(Field::Minute)[0];
                        minute_of_day = (first_hour as u16) * 60 + first_minute as u16;
                        minute_of_day = 1440;
                    }
                }
            }

            if minute_of_day >= 1440 {
                // The current date is exhausted; skip directly to the next date.
                minute_of_day = 1440;
            }
        }
    }

    pub fn previous_fire(
        &self,
        before: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(before)?;
        let mut date = (before.year, before.month, before.day);
        let mut minute_of_day = before.hour as i16 * 60 + before.minute as i16 - 1;

        loop {
            if minute_of_day < 0 {
                if !retreat_date(&mut date.0, &mut date.1, &mut date.2) {
                    return Ok(None);
                }
                minute_of_day = 1439;
            }

            let at = CivilDateTime {
                year: date.0,
                month: date.1,
                day: date.2,
                hour: (minute_of_day / 60) as u8,
                minute: (minute_of_day % 60) as u8,
            };
            if self.matches_valid(at) {
                return Ok(Some(at));
            }

            let hour = (minute_of_day / 60) as usize;
            let minute = (minute_of_day % 60) as usize;
            if self.selected[0][minute] {
                minute_of_day -= 1;
            } else {
                let prior_minute = (0..minute).rev().find(|&m| self.selected[0][m]);
                if let Some(m) = prior_minute {
                    minute_of_day = (hour * 60 + m) as i16;
                } else {
                    let prior_hour = (0..hour).rev().find(|&h| self.selected[1][h]);
                    if let Some(h) = prior_hour {
                        let last_minute = *self.allowed(Field::Minute).last().unwrap();
                        minute_of_day = (h * 60 + last_minute as usize) as i16;
                    } else {
                        if date == (2000, 1, 1) {
                            return Ok(None);
                        }
                        minute_of_day = -1;
                    }
                }
            }
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
        while cursor < end {
            let Some(fire) = self.next_fire_before_or_at(cursor, end) else {
                break;
            };
            if fire >= end {
                break;
            }
            if fires.len() == cap {
                return Ok(FireWindow {
                    fires,
                    truncated: true,
                });
            }
            fires.push(fire);
            match self.next_fire(fire) {
                Ok(Some(next)) => cursor = next,
                Ok(None) | Err(_) => break,
            }
        }
        Ok(FireWindow {
            fires,
            truncated: false,
        })
    }

    fn next_fire_before_or_at(
        &self,
        at: CivilDateTime,
        _end: CivilDateTime,
    ) -> Option<CivilDateTime> {
        if self.matches_valid(at) {
            Some(at)
        } else {
            self.next_fire(at).ok().flatten()
        }
    }

    pub fn canonical(&self) -> String {
        let mut fields = Vec::with_capacity(5);
        for field in FIELDS {
            let values = self.allowed(field);
            let full = values.len() == (domain(field).1 - domain(field).0 + 1) as usize;
            let idx = field_index(field);
            let unrestricted = match field {
                Field::DayOfMonth => self.unrestricted_dom,
                Field::DayOfWeek => self.unrestricted_dow,
                _ => false,
            };

            if unrestricted {
                fields.push("*".to_string());
            } else if full && matches!(field, Field::DayOfMonth | Field::DayOfWeek) {
                fields.push("*/1".to_string());
            } else if full {
                fields.push("*".to_string());
            } else {
                let _ = idx;
                fields.push(
                    values
                        .iter()
                        .map(u8::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                );
            }
        }
        fields.join(" ")
    }
}

impl FromStr for Field {
    type Err = ();

    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        Err(())
    }
}

fn leap_year(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if leap_year(year) => 29,
        2 => 28,
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
    let mut days = 0u32;
    for y in 2000..year {
        days += if leap_year(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as u32;
    }
    days += (day - 1) as u32;
    ((6 + days) % 7) as u8
}

fn advance_date(year: &mut u16, month: &mut u8, day: &mut u8) -> bool {
    if *day < days_in_month(*year, *month) {
        *day += 1;
    } else if *month < 12 {
        *month += 1;
        *day = 1;
    } else if *year < 2399 {
        *year += 1;
        *month = 1;
        *day = 1;
    } else {
        return false;
    }
    true
}

fn retreat_date(year: &mut u16, month: &mut u8, day: &mut u8) -> bool {
    if *day > 1 {
        *day -= 1;
    } else if *month > 1 {
        *month -= 1;
        *day = days_in_month(*year, *month);
    } else if *year > 2000 {
        *year -= 1;
        *month = 12;
        *day = 31;
    } else {
        return false;
    }
    true
}