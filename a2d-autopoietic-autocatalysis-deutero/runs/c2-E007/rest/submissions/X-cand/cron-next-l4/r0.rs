use std::str;

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

fn field_index(field: Field) -> usize {
    match field {
        Field::Minute => 0,
        Field::Hour => 1,
        Field::DayOfMonth => 2,
        Field::Month => 3,
        Field::DayOfWeek => 4,
    }
}

fn field_bounds(field: Field) -> (u32, u32) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
    }
}

fn field_width(field: Field) -> u32 {
    let (lo, hi) = field_bounds(field);
    hi - lo + 1
}

fn names(field: Field) -> &'static [(&'static str, u32)] {
    match field {
        Field::Month => &[
            ("JAN", 1), ("FEB", 2), ("MAR", 3), ("APR", 4), ("MAY", 5), ("JUN", 6),
            ("JUL", 7), ("AUG", 8), ("SEP", 9), ("OCT", 10), ("NOV", 11), ("DEC", 12),
        ],
        Field::DayOfWeek => &[
            ("SUN", 0), ("MON", 1), ("TUE", 2), ("WED", 3), ("THU", 4), ("FRI", 5),
            ("SAT", 6),
        ],
        _ => &[],
    }
}

fn parse_atom(s: &str, field: Field, offset: usize) -> Result<u32, ParseError> {
    if s.is_empty() {
        return Err(ParseError::InvalidSyntax { field, offset });
    }
    if s.bytes().all(|b| b.is_ascii_digit()) {
        if s.len() > 1 && s.as_bytes()[0] == b'0' {
            return Err(ParseError::LeadingZero { field, offset });
        }
        let mut value = 0u32;
        for b in s.bytes() {
            value = value
                .checked_mul(10)
                .and_then(|v| v.checked_add((b - b'0') as u32))
                .ok_or(ParseError::NumberOverflow { field, offset })?;
        }
        let (lo, hi) = field_bounds(field);
        if value < lo || value > hi {
            return Err(ParseError::OutOfRange { field, offset });
        }
        Ok(value)
    } else if s.bytes().all(|b| b.is_ascii_alphabetic()) {
        let upper = s.to_ascii_uppercase();
        names(field)
            .iter()
            .find_map(|(name, value)| (*name == upper).then_some(*value))
            .ok_or(ParseError::InvalidSyntax { field, offset })
    } else {
        Err(ParseError::InvalidSyntax { field, offset })
    }
}

fn parse_step(s: &str, field: Field, offset: usize) -> Result<u32, ParseError> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ParseError::InvalidSyntax { field, offset });
    }
    if s.len() > 1 && s.as_bytes()[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }
    let mut value = 0u32;
    for b in s.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    if value == 0 || value > field_width(field) {
        return Err(ParseError::BadStep { field, offset });
    }
    Ok(value)
}

fn parse_field(
    text: &str,
    field: Field,
    values: &mut [bool],
) -> Result<(), ParseError> {
    for (i, b) in text.bytes().enumerate() {
        if !(b.is_ascii_digit()
            || b.is_ascii_alphabetic()
            || matches!(b, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset: i });
        }
    }

    for (item_offset, item) in split_with_offsets(text, b',') {
        if item.is_empty() {
            return Err(ParseError::EmptyItem { field, offset: item_offset });
        }

        let slash_count = item.bytes().filter(|&b| b == b'/').count();
        let base = item.split('/').next().unwrap_or("");
        let step_text = if slash_count == 1 {
            item.split_once('/').map(|(_, s)| s)
        } else {
            None
        };

        let hyphen_count = base.bytes().filter(|&b| b == b'-').count();
        let syntax_ok = slash_count <= 1
            && !base.is_empty()
            && step_text.map_or(true, |s| !s.is_empty())
            && (base == "*"
                || hyphen_count == 0
                || (hyphen_count == 1
                    && base.split_once('-').map_or(false, |(a, b)| !a.is_empty() && !b.is_empty())))
            && !base.contains('*') == (base != "*");
        if !syntax_ok {
            return Err(ParseError::InvalidSyntax { field, offset: item_offset });
        }

        let (start, end, end_offset) = if base == "*" {
            let (lo, hi) = field_bounds(field);
            (lo, hi, item_offset)
        } else if let Some((left, right)) = base.split_once('-') {
            let left_offset = item_offset;
            let right_offset = item_offset + left.len() + 1;
            let start = parse_atom(left, field, left_offset)?;
            let end = parse_atom(right, field, right_offset)?;
            if start > end {
                return Err(ParseError::ReversedRange { field, offset: right_offset });
            }
            (start, end, right_offset)
        } else {
            let start = parse_atom(base, field, item_offset)?;
            let end = if step_text.is_some() {
                field_bounds(field).1
            } else {
                start
            };
            (start, end, item_offset)
        };

        let step = match step_text {
            Some(s) => {
                let step_offset = item_offset + item.len() - s.len();
                Some(parse_step(s, field, step_offset)?)
            }
            None => None,
        };

        let stride = step.unwrap_or(1);
        let mut value = start;
        loop {
            values[(value - field_bounds(field).0) as usize] = true;
            if value > end || end - value < stride {
                break;
            }
            value += stride;
        }
        let _ = end_offset;
    }
    Ok(())
}

fn split_with_offsets(s: &str, delim: u8) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, b) in s.bytes().enumerate() {
        if b == delim {
            out.push((start, &s[start..i]));
            start = i + 1;
        }
    }
    out.push((start, &s[start..]));
    out
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let expanded;
    let text = if input.as_bytes().first() == Some(&b'@') {
        expanded = match input {
            "@hourly" => "0 * * * *",
            "@daily" => "0 0 * * *",
            "@weekly" => "0 0 * * 0",
            "@monthly" => "0 0 1 * *",
            "@yearly" => "0 0 1 1 *",
            _ => return Err(ParseError::UnknownSpecial),
        };
        expanded
    } else {
        input
    };

    let pieces: Vec<&str> = text.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount { found: pieces.len() });
    }

    let fields = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];
    let mut selected: [Vec<bool>; 5] = std::array::from_fn(|i| {
        let (lo, hi) = field_bounds(fields[i]);
        vec![false; (hi - lo + 1) as usize]
    });
    let mut unrestricted_days = [false; 2];

    for i in 0..5 {
        if i == 2 {
            unrestricted_days[0] = pieces[i] == "*";
        } else if i == 4 {
            unrestricted_days[1] = pieces[i] == "*";
        }
        parse_field(pieces[i], fields[i], &mut selected[i])?;
    }

    Ok(Cron { selected, unrestricted_days })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let (lo, _) = field_bounds(field);
        self.selected[field_index(field)]
            .iter()
            .enumerate()
            .filter_map(|(i, &yes)| yes.then_some((i as u32 + lo) as u8))
            .collect()
    }

    fn has(&self, field: Field, value: u8) -> bool {
        let (lo, hi) = field_bounds(field);
        value as u32 >= lo
            && value as u32 <= hi
            && self.selected[field_index(field)][(value as u32 - lo) as usize]
    }

    fn day_matches(&self, at: CivilDateTime, weekday: u8) -> bool {
        let dom_unrestricted = self.unrestricted_days[0];
        let dow_unrestricted = self.unrestricted_days[1];
        let dom = self.has(Field::DayOfMonth, at.day);
        let dow = self.has(Field::DayOfWeek, weekday);
        match (dom_unrestricted, dow_unrestricted) {
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

    fn time_matches(&self, at: CivilDateTime, weekday: u8) -> bool {
        self.has(Field::Minute, at.minute)
            && self.has(Field::Hour, at.hour)
            && self.has(Field::Month, at.month)
            && self.day_matches(at, weekday)
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        let weekday = weekday_of(at.year, at.month, at.day);
        Ok(self.time_matches(at, weekday))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;
        let mut cursor = after;
        while increment_minute(&mut cursor) {
            let weekday = weekday_of(cursor.year, cursor.month, cursor.day);
            if self.time_matches(cursor, weekday) {
                return Ok(Some(cursor));
            }
        }
        Ok(None)
    }

    pub fn previous_fire(&self, before: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(before)?;
        let mut cursor = before;
        while decrement_minute(&mut cursor) {
            let weekday = weekday_of(cursor.year, cursor.month, cursor.day);
            if self.time_matches(cursor, weekday) {
                return Ok(Some(cursor));
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
            let weekday = weekday_of(cursor.year, cursor.month, cursor.day);
            if self.time_matches(cursor, weekday) {
                if fires.len() == cap {
                    return Ok(FireWindow { fires, truncated: true });
                }
                fires.push(cursor);
            }
            if !increment_minute(&mut cursor) {
                break;
            }
        }
        Ok(FireWindow { fires, truncated: false })
    }

    pub fn canonical(&self) -> String {
        let fields = [
            Field::Minute,
            Field::Hour,
            Field::DayOfMonth,
            Field::Month,
            Field::DayOfWeek,
        ];
        let mut rendered = Vec::new();
        for field in fields {
            let i = field_index(field);
            let (lo, hi) = field_bounds(field);
            let values = self.allowed(field);
            let full = values.len() == (hi - lo + 1) as usize;
            if field == Field::DayOfMonth && self.unrestricted_days[0] {
                rendered.push("*".to_string());
            } else if field == Field::DayOfWeek && self.unrestricted_days[1] {
                rendered.push("*".to_string());
            } else if full {
                if matches!(field, Field::DayOfMonth | Field::DayOfWeek) {
                    rendered.push("*/1".to_string());
                } else {
                    rendered.push("*".to_string());
                }
            } else {
                let _ = i;
                rendered.push(values.iter().map(u8::to_string).collect::<Vec<_>>().join(","));
            }
        }
        rendered.join(" ")
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

fn weekday_of(year: u16, month: u8, day: u8) -> u8 {
    let mut elapsed = 0u32;
    for y in 2000..year {
        elapsed += if is_leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        elapsed += days_in_month(year, m) as u32;
    }
    elapsed += (day - 1) as u32;
    ((6 + elapsed) % 7) as u8
}

fn increment_minute(at: &mut CivilDateTime) -> bool {
    if at.minute < 59 {
        at.minute += 1;
        return true;
    }
    at.minute = 0;
    if at.hour < 23 {
        at.hour += 1;
        return true;
    }
    at.hour = 0;
    if at.day < days_in_month(at.year, at.month) {
        at.day += 1;
        return true;
    }
    at.day = 1;
    if at.month < 12 {
        at.month += 1;
        return true;
    }
    at.month = 1;
    if at.year < 2399 {
        at.year += 1;
        return true;
    }
    false
}

fn decrement_minute(at: &mut CivilDateTime) -> bool {
    if at.minute > 0 {
        at.minute -= 1;
        return true;
    }
    at.minute = 59;
    if at.hour > 0 {
        at.hour -= 1;
        return true;
    }
    at.hour = 23;
    if at.day > 1 {
        at.day -= 1;
        return true;
    }
    if at.month > 1 {
        at.month -= 1;
    } else {
        if at.year == 2000 {
            return false;
        }
        at.year -= 1;
        at.month = 12;
    }
    at.day = days_in_month(at.year, at.month);
    true
}