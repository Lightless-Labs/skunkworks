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
    sets: [Vec<bool>; 5],
    unrestricted_dom: bool,
    unrestricted_dow: bool,
}

impl Cron {
    fn index(field: Field) -> usize {
        match field {
            Field::Minute => 0,
            Field::Hour => 1,
            Field::DayOfMonth => 2,
            Field::Month => 3,
            Field::DayOfWeek => 4,
        }
    }

    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.sets[Self::index(field)]
            .iter()
            .enumerate()
            .filter_map(|(i, &yes)| if yes { Some(i as u8) } else { None })
            .collect()
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        let minute_ok = self.sets[0][at.minute as usize];
        let hour_ok = self.sets[1][at.hour as usize];
        let month_ok = self.sets[3][at.month as usize];

        let dom = self.sets[2][at.day as usize];
        let dow = self.sets[4][weekday(at.year, at.month, at.day) as usize];
        let last_day = at.day == days_in_month(at.year, at.month);

        let day_ok = if self.unrestricted_dom && self.unrestricted_dow {
            true
        } else if self.unrestricted_dom {
            dow
        } else if self.unrestricted_dow {
            dom
        } else if last_day {
            dom && dow
        } else {
            dom || dow
        };

        minute_ok && hour_ok && month_ok && day_ok
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        Ok(self.matches_valid(at))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;
        let mut date = Date { year: after.year, month: after.month, day: after.day };
        loop {
            if self.date_matches(date) {
                for hour in self.allowed(Field::Hour) {
                    for minute in self.allowed(Field::Minute) {
                        let candidate = CivilDateTime {
                            year: date.year,
                            month: date.month,
                            day: date.day,
                            hour,
                            minute,
                        };
                        if candidate > after {
                            return Ok(Some(candidate));
                        }
                    }
                }
            }
            if !date.increment() {
                return Ok(None);
            }
        }
    }

    pub fn previous_fire(&self, before: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(before)?;
        let mut date = Date { year: before.year, month: before.month, day: before.day };
        loop {
            if self.date_matches(date) {
                for hour in self.allowed(Field::Hour).into_iter().rev() {
                    for minute in self.allowed(Field::Minute).into_iter().rev() {
                        let candidate = CivilDateTime {
                            year: date.year,
                            month: date.month,
                            day: date.day,
                            hour,
                            minute,
                        };
                        if candidate < before {
                            return Ok(Some(candidate));
                        }
                    }
                }
            }
            if !date.decrement() {
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
        validate_datetime(start).map_err(|e| WindowError::InvalidStart {
            component: match e { DateError::InvalidDateTime { component } => component },
        })?;
        validate_datetime(end).map_err(|e| WindowError::InvalidEnd {
            component: match e { DateError::InvalidDateTime { component } => component },
        })?;

        if start > end {
            return Err(WindowError::ReversedWindow);
        }
        if cap == 0 {
            return Err(WindowError::ZeroCap);
        }
        if start == end {
            return Ok(FireWindow { fires: Vec::new(), truncated: false });
        }

        let mut fires = Vec::new();
        let mut cursor = Some(start);
        while let Some(at) = cursor {
            if at >= end {
                break;
            }
            let Some(fire) = self.next_fire(at).map_err(|_| unreachable!())? else {
                break;
            };
            if fire >= end {
                break;
            }
            if fires.len() == cap {
                return Ok(FireWindow { fires, truncated: true });
            }
            fires.push(fire);
            cursor = increment_minute(fire);
        }
        Ok(FireWindow { fires, truncated: false })
    }

    pub fn canonical(&self) -> String {
        let fields = [
            self.canonical_field(0, Field::Minute, false),
            self.canonical_field(1, Field::Hour, false),
            self.canonical_field(2, Field::DayOfMonth, self.unrestricted_dom),
            self.canonical_field(3, Field::Month, false),
            self.canonical_field(4, Field::DayOfWeek, self.unrestricted_dow),
        ];
        fields.join(" ")
    }

    fn canonical_field(&self, idx: usize, field: Field, unrestricted: bool) -> String {
        if unrestricted {
            return "*".to_string();
        }
        let min = domain_min(field);
        let max = domain_max(field);
        let values: Vec<usize> = self.sets[idx]
            .iter()
            .enumerate()
            .filter_map(|(i, &yes)| if yes && i >= min && i <= max { Some(i) } else { None })
            .collect();
        if values.len() == max - min + 1 {
            if matches!(field, Field::DayOfMonth | Field::DayOfWeek) {
                return "*/1".to_string();
            }
            return "*".to_string();
        }
        values.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",")
    }

    fn date_matches(&self, date: Date) -> bool {
        if !self.sets[3][date.month as usize] {
            return false;
        }
        let dom = self.sets[2][date.day as usize];
        let dow = self.sets[4][weekday(date.year, date.month, date.day) as usize];
        let last_day = date.day == days_in_month(date.year, date.month);

        if self.unrestricted_dom && self.unrestricted_dow {
            true
        } else if self.unrestricted_dom {
            dow
        } else if self.unrestricted_dow {
            dom
        } else if last_day {
            dom && dow
        } else {
            dom || dow
        }
    }
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

#[derive(Clone, Copy)]
struct Date {
    year: u16,
    month: u8,
    day: u8,
}

impl Date {
    fn increment(&mut self) -> bool {
        if self.day < days_in_month(self.year, self.month) {
            self.day += 1;
            true
        } else if self.month < 12 {
            self.month += 1;
            self.day = 1;
            true
        } else if self.year < 2399 {
            self.year += 1;
            self.month = 1;
            self.day = 1;
            true
        } else {
            false
        }
    }

    fn decrement(&mut self) -> bool {
        if self.day > 1 {
            self.day -= 1;
            true
        } else if self.month > 1 {
            self.month -= 1;
            self.day = days_in_month(self.year, self.month);
            true
        } else if self.year > 2000 {
            self.year -= 1;
            self.month = 12;
            self.day = 31;
            true
        } else {
            false
        }
    }
}

fn validate_datetime(at: CivilDateTime) -> Result<(), DateError> {
    if !(2000..=2399).contains(&at.year) {
        return Err(DateError::InvalidDateTime { component: DatePart::Year });
    }
    if !(1..=12).contains(&at.month) {
        return Err(DateError::InvalidDateTime { component: DatePart::Month });
    }
    if at.day < 1 || at.day > days_in_month(at.year, at.month) {
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

fn is_leap(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days: i64 = 0;
    for y in 2000..year {
        days += if is_leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as i64;
    }
    days += day as i64 - 1;
    ((days + 6) % 7) as u8
}

fn increment_minute(at: CivilDateTime) -> Option<CivilDateTime> {
    if at.minute < 59 {
        Some(CivilDateTime { minute: at.minute + 1, ..at })
    } else if at.hour < 23 {
        Some(CivilDateTime { hour: at.hour + 1, minute: 0, ..at })
    } else if at.day < days_in_month(at.year, at.month) {
        Some(CivilDateTime { day: at.day + 1, hour: 0, minute: 0, ..at })
    } else if at.month < 12 {
        Some(CivilDateTime { month: at.month + 1, day: 1, hour: 0, minute: 0, ..at })
    } else if at.year < 2399 {
        Some(CivilDateTime { year: at.year + 1, month: 1, day: 1, hour: 0, minute: 0, ..at })
    } else {
        None
    }
}

fn domain_min(field: Field) -> usize {
    match field {
        Field::Minute | Field::Hour | Field::DayOfWeek => 0,
        Field::DayOfMonth | Field::Month => 1,
    }
}

fn domain_max(field: Field) -> usize {
    match field {
        Field::Minute => 59,
        Field::Hour => 23,
        Field::DayOfMonth => 31,
        Field::Month => 12,
        Field::DayOfWeek => 6,
    }
}

fn domain_width(field: Field) -> usize {
    domain_max(field) - domain_min(field) + 1
}

fn names(field: Field) -> Option<&'static [(&'static str, u8)]> {
    match field {
        Field::Month => Some(&[
            ("JAN", 1), ("FEB", 2), ("MAR", 3), ("APR", 4), ("MAY", 5), ("JUN", 6),
            ("JUL", 7), ("AUG", 8), ("SEP", 9), ("OCT", 10), ("NOV", 11), ("DEC", 12),
        ]),
        Field::DayOfWeek => Some(&[
            ("SUN", 0), ("MON", 1), ("TUE", 2), ("WED", 3), ("THU", 4), ("FRI", 5), ("SAT", 6),
        ]),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum Base {
    Any,
    Atom(u32, usize),
    Range(u32, usize, u32, usize),
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
        return parse_fields(expansion);
    }
    let pieces: Vec<&str> = input.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount { found: pieces.len() });
    }
    parse_fields(input)
}

fn parse_fields(input: &str) -> Result<Cron, ParseError> {
    let pieces: Vec<&str> = input.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount { found: pieces.len() });
    }
    let fields = [Field::Minute, Field::Hour, Field::DayOfMonth, Field::Month, Field::DayOfWeek];
    let mut sets: [Vec<bool>; 5] = [
        vec![false; 60],
        vec![false; 24],
        vec![false; 32],
        vec![false; 13],
        vec![false; 7],
    ];
    let mut unrestricted_dom = false;
    let mut unrestricted_dow = false;

    for (idx, (text, field)) in pieces.iter().zip(fields).enumerate() {
        if idx == 2 {
            unrestricted_dom = *text == "*";
        } else if idx == 4 {
            unrestricted_dow = *text == "*";
        }
        for (offset, byte) in text.bytes().enumerate() {
            if !(byte.is_ascii_digit()
                || byte.is_ascii_alphabetic()
                || matches!(byte, b'*' | b'-' | b'/' | b','))
            {
                return Err(ParseError::InvalidCharacter { field, offset });
            }
        }

        let mut start = 0;
        for item in text.split(',') {
            if item.is_empty() {
                return Err(ParseError::EmptyItem { field, offset: start });
            }
            parse_item(item, start, field, &mut sets[idx])?;
            start += item.len() + 1;
        }
    }

    Ok(Cron { sets, unrestricted_dom, unrestricted_dow })
}

fn parse_item(item: &str, item_start: usize, field: Field, set: &mut [bool]) -> Result<(), ParseError> {
    let bytes = item.as_bytes();
    let slash_count = bytes.iter().filter(|&&b| b == b'/').count();
    if slash_count > 1 {
        return Err(ParseError::InvalidSyntax { field, offset: item_start });
    }

    let (base_text, step_text, step_offset) = if let Some(pos) = bytes.iter().position(|&b| b == b'/') {
        (&item[..pos], Some(&item[pos + 1..]), Some(item_start + pos + 1))
    } else {
        (item, None, None)
    };
    if base_text.is_empty() || step_text == Some("") {
        return Err(ParseError::InvalidSyntax { field, offset: item_start });
    }

    let hyphens = base_text.bytes().filter(|&b| b == b'-').count();
    let base = if base_text == "*" {
        if hyphens != 0 { return Err(ParseError::InvalidSyntax { field, offset: item_start }); }
        Base::Any
    } else if hyphens == 0 {
        let value = parse_atom(base_text, item_start, field)?;
        Base::Atom(value, item_start)
    } else if hyphens == 1 {
        let pos = base_text.find('-').unwrap();
        let left = &base_text[..pos];
        let right = &base_text[pos + 1..];
        if left.is_empty() || right.is_empty() {
            return Err(ParseError::InvalidSyntax { field, offset: item_start });
        }
        let left_value = parse_atom(left, item_start, field)?;
        let right_start = item_start + pos + 1;
        let right_value = parse_atom(right, right_start, field)?;
        Base::Range(left_value, item_start, right_value, right_start)
    } else {
        return Err(ParseError::InvalidSyntax { field, offset: item_start });
    };

    let step = if let Some(text) = step_text {
        Some(parse_step(text, step_offset.unwrap(), field)?)
    } else {
        None
    };

    let min = domain_min(field) as u32;
    let max = domain_max(field) as u32;

    let (start, end, stepped) = match base {
        Base::Any => (min, max, step.is_some()),
        Base::Atom(value, offset) => {
            check_domain(value, offset, field, min, max)?;
            if let Some(_) = step {
                (value, max, true)
            } else {
                (value, value, false)
            }
        }
        Base::Range(a, aoff, b, boff) => {
            check_domain(a, aoff, field, min, max)?;
            check_domain(b, boff, field, min, max)?;
            if a > b {
                return Err(ParseError::ReversedRange { field, offset: boff });
            }
            (a, b, step.is_some())
        }
    };

    let stride = if stepped { step.unwrap() } else { 1 };
    let mut value = start;
    while value <= end {
        set[value as usize] = true;
        match value.checked_add(stride) {
            Some(next) => value = next,
            None => break,
        }
    }
    Ok(())
}

fn parse_atom(text: &str, offset: usize, field: Field) -> Result<u32, ParseError> {
    if text.bytes().all(|b| b.is_ascii_digit()) {
        return parse_number(text, offset, field);
    }
    if let Some(table) = names(field) {
        let upper = text.to_ascii_uppercase();
        if let Some((_, value)) = table.iter().find(|(name, _)| *name == upper) {
            return Ok(*value as u32);
        }
    }
    Err(ParseError::InvalidSyntax { field, offset: offset_of_item(offset) })
}

fn offset_of_item(offset: usize) -> usize {
    offset
}

fn parse_number(text: &str, offset: usize, field: Field) -> Result<u32, ParseError> {
    if text.len() > 1 && text.as_bytes()[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }
    let mut value = 0u32;
    for b in text.bytes() {
        value = value.checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    Ok(value)
}

fn parse_step(text: &str, offset: usize, field: Field) -> Result<u32, ParseError> {
    if !text.bytes().all(|b| b.is_ascii_digit()) || text.is_empty() {
        return Err(ParseError::InvalidSyntax { field, offset: offset.saturating_sub(1) });
    }
    let value = parse_number(text, offset, field)?;
    if value == 0 || value > domain_width(field) as u32 {
        return Err(ParseError::BadStep { field, offset });
    }
    Ok(value)
}

fn check_domain(value: u32, offset: usize, field: Field, min: u32, max: u32) -> Result<(), ParseError> {
    if value < min || value > max {
        Err(ParseError::OutOfRange { field, offset })
    } else {
        Ok(())
    }
}