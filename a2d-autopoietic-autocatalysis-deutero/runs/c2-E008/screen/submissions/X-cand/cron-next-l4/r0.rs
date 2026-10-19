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
    sets: [Vec<bool>; 5],
    day_unrestricted: bool,
    weekday_unrestricted: bool,
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

fn index(field: Field) -> usize {
    match field {
        Field::Minute => 0,
        Field::Hour => 1,
        Field::DayOfMonth => 2,
        Field::Month => 3,
        Field::DayOfWeek => 4,
    }
}

fn invalid_syntax(field: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field, offset }
}

fn parse_number(s: &str, field: Field, offset: usize) -> Result<u32, ParseError> {
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
    Ok(n)
}

fn name_value(s: &str, field: Field) -> Option<u32> {
    let upper = s.to_ascii_uppercase();
    match field {
        Field::Month => [
            "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT",
            "NOV", "DEC",
        ]
        .iter()
        .position(|x| *x == upper)
        .map(|n| n as u32 + 1),
        Field::DayOfWeek => ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"]
            .iter()
            .position(|x| *x == upper)
            .map(|n| n as u32),
        _ => None,
    }
}

fn endpoint(s: &str, field: Field, offset: usize) -> Result<u32, ParseError> {
    if s.bytes().all(|b| b.is_ascii_digit()) && !s.is_empty() {
        let n = parse_number(s, field, offset)?;
        let (lo, hi) = bounds(field);
        if n < lo as u32 || n > hi as u32 {
            return Err(ParseError::OutOfRange { field, offset });
        }
        return Ok(n);
    }
    name_value(s, field).ok_or_else(|| invalid_syntax(field, offset))
}

fn parse_field(text: &str, field: Field) -> Result<Vec<bool>, ParseError> {
    let (lo, hi) = bounds(field);
    let mut selected = vec![false; hi as usize + 1];

    for (i, b) in text.bytes().enumerate() {
        if !(b.is_ascii_digit()
            || b.is_ascii_alphabetic()
            || matches!(b, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset: i });
        }
    }

    let mut item_start = 0usize;
    for item in text.split(',') {
        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let slash_count = item.bytes().filter(|b| *b == b'/').count();
        if slash_count > 1 {
            return Err(invalid_syntax(field, item_start));
        }

        let (base, step_text) = if let Some(pos) = item.find('/') {
            (&item[..pos], Some((&item[pos + 1..], item_start + pos + 1)))
        } else {
            (item, None)
        };
        if base.is_empty() || step_text.is_some_and(|(s, _)| s.is_empty()) {
            return Err(invalid_syntax(field, item_start));
        }

        let hyphens = base.bytes().filter(|b| *b == b'-').count();
        if hyphens > 1 || base == "*" && hyphens != 0 {
            return Err(invalid_syntax(field, item_start));
        }

        let (start_text, end_text) = if let Some(pos) = base.find('-') {
            let a = &base[..pos];
            let b = &base[pos + 1..];
            if a.is_empty() || b.is_empty() || a == "*" || b == "*" {
                return Err(invalid_syntax(field, item_start));
            }
            (a, Some((b, item_start + pos + 1)))
        } else {
            (base, None)
        };

        if start_text != "*" && endpoint(start_text, field, item_start).is_err() {
            // Syntax errors must be found before numeric validation.
            let syntactically_valid = start_text.bytes().all(|b| b.is_ascii_digit())
                || name_value(start_text, field).is_some();
            if !syntactically_valid {
                return Err(invalid_syntax(field, item_start));
            }
        }
        if let Some((end, end_offset)) = end_text {
            if !(end.bytes().all(|b| b.is_ascii_digit()) || name_value(end, field).is_some()) {
                return Err(invalid_syntax(field, item_start));
            }
            let _ = end_offset;
        }
        if let Some((step, _)) = step_text {
            if !step.bytes().all(|b| b.is_ascii_digit()) || step.is_empty() {
                return Err(invalid_syntax(field, item_start));
            }
        }

        let start = if start_text == "*" {
            lo as u32
        } else {
            endpoint(start_text, field, item_start)?
        };
        let end = if let Some((end, off)) = end_text {
            endpoint(end, field, off)?
        } else if start_text == "*" || step_text.is_some() {
            hi as u32
        } else {
            start
        };

        if start > end {
            let off = end_text.map(|(_, off)| off).unwrap_or(item_start);
            return Err(ParseError::ReversedRange { field, offset: off });
        }

        let step = if let Some((s, off)) = step_text {
            let n = parse_number(s, field, off)?;
            if n == 0 || n > (hi - lo + 1) as u32 {
                return Err(ParseError::BadStep { field, offset: off });
            }
            n
        } else {
            1
        };

        let mut n = start;
        while n <= end {
            selected[n as usize] = true;
            match n.checked_add(step) {
                Some(next) => n = next,
                None => break,
            }
        }
        item_start += item.len() + 1;
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

    let fields: Vec<&str> = expanded.split(' ').collect();
    if fields.len() != 5 {
        return Err(ParseError::FieldCount { found: fields.len() });
    }

    let kinds = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];
    let mut parsed: [Vec<bool>; 5] = std::array::from_fn(|_| Vec::new());
    for i in 0..5 {
        parsed[i] = parse_field(fields[i], kinds[i])?;
    }

    Ok(Cron {
        sets: parsed,
        day_unrestricted: fields[2] == "*",
        weekday_unrestricted: fields[4] == "*",
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.sets[index(field)]
            .iter()
            .enumerate()
            .filter_map(|(n, yes)| if *yes { Some(n as u8) } else { None })
            .collect()
    }

    fn selected(&self, field: Field, value: u8) -> bool {
        self.sets[index(field)]
            .get(value as usize)
            .copied()
            .unwrap_or(false)
    }

    fn date_matches(&self, dt: CivilDateTime, weekday: u8) -> bool {
        let dom = self.selected(Field::DayOfMonth, dt.day);
        let dow = self.selected(Field::DayOfWeek, weekday);
        match (self.day_unrestricted, self.weekday_unrestricted) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) if dt.day == days_in_month(dt.year, dt.month) => dom && dow,
            (false, false) => dom || dow,
        }
    }

    fn matches_valid(&self, dt: CivilDateTime) -> bool {
        self.selected(Field::Minute, dt.minute)
            && self.selected(Field::Hour, dt.hour)
            && self.selected(Field::Month, dt.month)
            && self.date_matches(dt, weekday(dt.year, dt.month, dt.day))
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate(at)?;
        Ok(self.matches_valid(at))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate(after)?;
        let mut dt = after;
        loop {
            if !increment_minute(&mut dt) {
                return Ok(None);
            }
            if self.matches_valid(dt) {
                return Ok(Some(dt));
            }
        }
    }

    pub fn previous_fire(
        &self,
        before: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
        validate(before)?;
        let mut dt = before;
        loop {
            if !decrement_minute(&mut dt) {
                return Ok(None);
            }
            if self.matches_valid(dt) {
                return Ok(Some(dt));
            }
        }
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
        let mut cursor = Some(start);
        while let Some(current) = cursor {
            if current >= end {
                break;
            }
            if self.matches_valid(current) {
                if fires.len() == cap {
                    return Ok(FireWindow {
                        fires,
                        truncated: true,
                    });
                }
                fires.push(current);
            }
            cursor = if current == end {
                None
            } else {
                let mut next = current;
                if increment_minute(&mut next) {
                    Some(next)
                } else {
                    None
                }
            };
        }
        Ok(FireWindow {
            fires,
            truncated: false,
        })
    }

    pub fn canonical(&self) -> String {
        let fields = [
            canonical_field(&self.sets[0], 0, 59, false),
            canonical_field(&self.sets[1], 0, 23, false),
            canonical_field(&self.sets[2], 1, 31, self.day_unrestricted),
            canonical_field(&self.sets[3], 1, 12, false),
            canonical_field(&self.sets[4], 0, 6, self.weekday_unrestricted),
        ];
        fields.join(" ")
    }
}

fn canonical_field(set: &[bool], lo: u8, hi: u8, unrestricted: bool) -> String {
    let count = (lo..=hi).filter(|n| set[*n as usize]).count();
    if unrestricted {
        return "*".to_string();
    }
    if count == (hi - lo + 1) as usize {
        if lo == 0 && hi == 59 || lo == 0 && hi == 23 || lo == 1 && hi == 12 {
            return "*".to_string();
        }
        return "*/1".to_string();
    }
    (lo..=hi)
        .filter(|n| set[*n as usize])
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn leap(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn validate(dt: CivilDateTime) -> Result<(), DateError> {
    if !(2000..=2399).contains(&dt.year) {
        return Err(DateError::InvalidDateTime {
            component: DatePart::Year,
        });
    }
    if !(1..=12).contains(&dt.month) {
        return Err(DateError::InvalidDateTime {
            component: DatePart::Month,
        });
    }
    if dt.day == 0 || dt.day > days_in_month(dt.year, dt.month) {
        return Err(DateError::InvalidDateTime {
            component: DatePart::Day,
        });
    }
    if dt.hour > 23 {
        return Err(DateError::InvalidDateTime {
            component: DatePart::Hour,
        });
    }
    if dt.minute > 59 {
        return Err(DateError::InvalidDateTime {
            component: DatePart::Minute,
        });
    }
    Ok(())
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
    ((6 + days) % 7) as u8
}

fn increment_minute(dt: &mut CivilDateTime) -> bool {
    if dt.minute < 59 {
        dt.minute += 1;
        return true;
    }
    dt.minute = 0;
    if dt.hour < 23 {
        dt.hour += 1;
        return true;
    }
    dt.hour = 0;
    if dt.day < days_in_month(dt.year, dt.month) {
        dt.day += 1;
        return true;
    }
    dt.day = 1;
    if dt.month < 12 {
        dt.month += 1;
        return true;
    }
    dt.month = 1;
    if dt.year < 2399 {
        dt.year += 1;
        return true;
    }
    false
}

fn decrement_minute(dt: &mut CivilDateTime) -> bool {
    if dt.minute > 0 {
        dt.minute -= 1;
        return true;
    }
    dt.minute = 59;
    if dt.hour > 0 {
        dt.hour -= 1;
        return true;
    }
    dt.hour = 23;
    if dt.day > 1 {
        dt.day -= 1;
        return true;
    }
    if dt.month > 1 {
        dt.month -= 1;
    } else if dt.year > 2000 {
        dt.year -= 1;
        dt.month = 12;
    } else {
        return false;
    }
    dt.day = days_in_month(dt.year, dt.month);
    true
}