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

fn parse_number(s: &str, field: Field, offset: usize) -> Result<u32, ParseError> {
    let bytes = s.as_bytes();
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
    Ok(value)
}

fn name_value(s: &str, field: Field) -> Option<u32> {
    if field == Field::Month {
        const NAMES: [&str; 12] = [
            "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
        ];
        NAMES
            .iter()
            .position(|n| s.eq_ignore_ascii_case(n))
            .map(|i| (i + 1) as u32)
    } else if field == Field::DayOfWeek {
        const NAMES: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
        NAMES
            .iter()
            .position(|n| s.eq_ignore_ascii_case(n))
            .map(|i| i as u32)
    } else {
        None
    }
}

fn atom_value(s: &str, field: Field, offset: usize) -> Result<u32, ParseError> {
    if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
        let value = parse_number(s, field, offset)?;
        let (min, max) = domain(field);
        if value < min || value > max {
            return Err(ParseError::OutOfRange { field, offset });
        }
        Ok(value)
    } else if let Some(v) = name_value(s, field) {
        Ok(v)
    } else {
        Err(ParseError::InvalidSyntax { field, offset })
    }
}

fn parse_field(text: &str, field: Field) -> Result<(Vec<bool>, bool), ParseError> {
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if !(b.is_ascii_digit()
            || b.is_ascii_alphabetic()
            || matches!(b, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset: i });
        }
    }

    let (min, max) = domain(field);
    let mut selected = vec![false; (max + 1) as usize];
    let mut item_offset = 0usize;

    for item in text.split(',') {
        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_offset,
            });
        }
        let this_offset = item_offset;
        item_offset += item.len() + 1;

        let slash_count = item.bytes().filter(|&b| b == b'/').count();
        let (base, step_text) = if slash_count == 0 {
            (item, None)
        } else if slash_count == 1 {
            let p = item.find('/').unwrap();
            (&item[..p], Some(&item[p + 1..]))
        } else {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: this_offset,
            });
        };

        let (is_star, left, right) = if base == "*" {
            (true, None, None)
        } else {
            let hyphens = base.bytes().filter(|&b| b == b'-').count();
            if hyphens == 0 {
                (false, Some(base), None)
            } else if hyphens == 1 {
                let p = base.find('-').unwrap();
                if p == 0 || p + 1 == base.len() {
                    return Err(ParseError::InvalidSyntax {
                        field,
                        offset: this_offset,
                    });
                }
                (false, Some(&base[..p]), Some(&base[p + 1..]))
            } else {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: this_offset,
                });
            }
        };

        if base.is_empty() || step_text == Some("") {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: this_offset,
            });
        }

        let a;
        let b;
        if is_star {
            a = min;
            b = max;
        } else {
            let left = left.unwrap();
            let av = atom_value(left, field, this_offset)?;
            a = av;

            if let Some(right) = right {
                let right_offset = this_offset + base.find('-').unwrap() + 1;
                let bv = atom_value(right, field, right_offset)?;
                b = bv;
                if a > b {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: right_offset,
                    });
                }
            } else {
                b = if step_text.is_some() { max } else { av };
            }
        }

        let step = if let Some(st) = step_text {
            if !st.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: this_offset,
                });
            }
            let step_offset = this_offset + item.find('/').unwrap() + 1;
            let n = parse_number(st, field, step_offset)?;
            if n < 1 || n > max - min + 1 {
                return Err(ParseError::BadStep {
                    field,
                    offset: step_offset,
                });
            }
            n
        } else {
            1
        };

        let mut value = a;
        while value <= b {
            selected[value as usize] = true;
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
            }
        }
    }

    Ok((selected, text == "*"))
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

    let pieces: Vec<&str> = expanded.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount {
            found: pieces.len(),
        });
    }

    let mut selected = Vec::with_capacity(5);
    let mut flags = Vec::with_capacity(5);
    for (i, text) in pieces.iter().enumerate() {
        let (set, unrestricted) = parse_field(text, FIELDS[i])?;
        selected.push(set);
        flags.push(unrestricted);
    }

    Ok(Cron {
        selected: selected.try_into().ok().unwrap(),
        unrestricted_dom: flags[2],
        unrestricted_dow: flags[4],
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.selected[field_index(field)]
            .iter()
            .enumerate()
            .filter_map(|(i, &yes)| yes.then_some(i as u8))
            .collect()
    }

    fn contains(&self, field: Field, value: u8) -> bool {
        self.selected[field_index(field)]
            .get(value as usize)
            .copied()
            .unwrap_or(false)
    }

    fn day_matches(&self, date: CivilDateTime, weekday: u8) -> bool {
        let dom = self.contains(Field::DayOfMonth, date.day);
        let dow = self.contains(Field::DayOfWeek, weekday);
        match (self.unrestricted_dom, self.unrestricted_dow) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) => {
                if date.day == month_length(date.year, date.month) {
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
            && self.day_matches(at, weekday(at.year, at.month, at.day)))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;
        let mut date = CivilDateTime {
            hour: 0,
            minute: 0,
            ..after
        };
        loop {
            if self.contains(Field::Month, date.month)
                && self.day_matches(date, weekday(date.year, date.month, date.day))
            {
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
        }
    }

    pub fn previous_fire(&self, before: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(before)?;
        let mut date = CivilDateTime {
            hour: 23,
            minute: 59,
            ..before
        };
        loop {
            if self.contains(Field::Month, date.month)
                && self.day_matches(date, weekday(date.year, date.month, date.day))
            {
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

        // next_fire is strictly later than its argument. Include a matching
        // start explicitly, then continue strictly after each discovered fire.
        let mut fires = Vec::new();
        if start < end && self.matches(start).unwrap() {
            fires.push(start);
            if fires.len() == cap {
                if self.next_fire(start).unwrap().map_or(false, |fire| fire < end) {
                    return Ok(FireWindow {
                        fires,
                        truncated: true,
                    });
                }
                return Ok(FireWindow {
                    fires,
                    truncated: false,
                });
            }
        }

        let mut cursor = start;
        loop {
            let Some(fire) = self.next_fire(cursor).unwrap() else {
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
            cursor = fire;
        }

        Ok(FireWindow {
            fires,
            truncated: false,
        })
    }

    pub fn canonical(&self) -> String {
        let mut parts = Vec::with_capacity(5);
        for field in FIELDS {
            let values = self.allowed(field);
            let (min, max) = domain(field);
            let full = values.len() == (max - min + 1) as usize;
            let text = match field {
                Field::DayOfMonth if self.unrestricted_dom => "*".to_string(),
                Field::DayOfWeek if self.unrestricted_dow => "*".to_string(),
                Field::DayOfMonth | Field::DayOfWeek if full => "*/1".to_string(),
                _ if full => "*".to_string(),
                _ => values
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            };
            parts.push(text);
        }
        parts.join(" ")
    }
}

fn leap_year(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn month_length(year: u16, month: u8) -> u8 {
    match month {
        2 => {
            if leap_year(year) {
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
    if at.day < 1 || at.day > month_length(at.year, at.month) {
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
        days += month_length(year, m) as u32;
    }
    days += (day - 1) as u32;
    ((6 + days) % 7) as u8
}

fn advance_day(date: &mut CivilDateTime) -> bool {
    if date.day < month_length(date.year, date.month) {
        date.day += 1;
    } else if date.month < 12 {
        date.month += 1;
        date.day = 1;
    } else if date.year < 2399 {
        date.year += 1;
        date.month = 1;
        date.day = 1;
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
        date.day = month_length(date.year, date.month);
    } else if date.year > 2000 {
        date.year -= 1;
        date.month = 12;
        date.day = 31;
    } else {
        return false;
    }
    true
}

fn main() {}