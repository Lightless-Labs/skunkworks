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
    unrestricted: [bool; 2],
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

fn name_value(field: Field, bytes: &[u8]) -> Option<u32> {
    let names: &[&[u8]] = match field {
        Field::Month => &[
            b"JAN", b"FEB", b"MAR", b"APR", b"MAY", b"JUN", b"JUL", b"AUG", b"SEP", b"OCT",
            b"NOV", b"DEC",
        ],
        Field::DayOfWeek => &[b"SUN", b"MON", b"TUE", b"WED", b"THU", b"FRI", b"SAT"],
        _ => return None,
    };
    names
        .iter()
        .position(|name| {
            name.len() == bytes.len()
                && name
                    .iter()
                    .zip(bytes)
                    .all(|(a, b)| a.eq_ignore_ascii_case(b))
        })
        .map(|i| {
            if field == Field::Month {
                (i + 1) as u32
            } else {
                i as u32
            }
        })
}

fn parse_u32_number(bytes: &[u8], field: Field, offset: usize) -> Result<u32, ParseError> {
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }
    let mut value = 0u32;
    for &b in bytes {
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    Ok(value)
}

fn parse_endpoint(bytes: &[u8], field: Field, offset: usize) -> Result<u32, ParseError> {
    if !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit) {
        let n = parse_u32_number(bytes, field, offset)?;
        let (min, max) = bounds(field);
        if n < min || n > max {
            return Err(ParseError::OutOfRange { field, offset });
        }
        return Ok(n);
    }
    name_value(field, bytes).ok_or(ParseError::InvalidSyntax { field, offset })
}

fn parse_field(
    text: &str,
    field: Field,
    unrestricted: &mut bool,
) -> Result<Vec<bool>, ParseError> {
    let (min, max) = bounds(field);
    let mut selected = vec![false; (max + 1) as usize];

    for (offset, &b) in text.as_bytes().iter().enumerate() {
        if !(b.is_ascii_digit()
            || b.is_ascii_alphabetic()
            || matches!(b, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    *unrestricted = matches!(field, Field::DayOfMonth | Field::DayOfWeek)
        && text.as_bytes() == b"*";

    let mut item_start = 0usize;
    loop {
        let end = text.as_bytes()[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|p| item_start + p)
            .unwrap_or(text.len());
        let item = &text.as_bytes()[item_start..end];
        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let slash = item.iter().position(|&b| b == b'/');
        if slash.is_some_and(|s| item[s + 1..].contains(&b'/')) {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }
        let (base, step_bytes, step_offset) = if let Some(s) = slash {
            (&item[..s], Some(&item[s + 1..]), item_start + s + 1)
        } else {
            (item, None, 0)
        };

        let hyphen = base.iter().position(|&b| b == b'-');
        if hyphen.is_some_and(|h| base[h + 1..].contains(&b'-')) {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        if base.is_empty() || step_bytes.is_some_and(|s| s.is_empty()) {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let (left, right) = if let Some(h) = hyphen {
            if base[..h].is_empty()
                || base[h + 1..].is_empty()
                || &base[..h] == b"*"
                || &base[h + 1..] == b"*"
            {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            (&base[..h], Some(&base[h + 1..]))
        } else {
            (base, None)
        };

        let wildcard = left == b"*" && right.is_none();
        let valid_atom = |s: &[u8]| {
            (!s.is_empty() && s.iter().all(u8::is_ascii_digit))
                || name_value(field, s).is_some()
        };
        if (!wildcard && !valid_atom(left))
            || right.is_some_and(|r| !valid_atom(r))
            || step_bytes.is_some_and(|s| !s.iter().all(u8::is_ascii_digit))
        {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let (from, to, right_offset) = if wildcard {
            (min, max, item_start)
        } else {
            let a = parse_endpoint(left, field, item_start)?;
            let (b, ro) = if let Some(r) = right {
                let offset = item_start + r.as_ptr() as usize - item.as_ptr() as usize;
                (parse_endpoint(r, field, offset)?, offset)
            } else {
                (a, item_start)
            };
            if right.is_some() && a > b {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: ro,
                });
            }
            (a, b, ro)
        };

        let step = if let Some(s) = step_bytes {
            let n = parse_u32_number(s, field, step_offset)?;
            let width = max - min + 1;
            if n == 0 || n > width {
                return Err(ParseError::BadStep {
                    field,
                    offset: step_offset,
                });
            }
            n
        } else {
            0
        };

        if step == 0 {
            for n in from..=to {
                selected[n as usize] = true;
            }
        } else {
            let mut n = from;
            while n <= to {
                selected[n as usize] = true;
                match n.checked_add(step) {
                    Some(next) => n = next,
                    None => break,
                }
            }
        }

        let _ = right_offset;
        if end == text.len() {
            break;
        }
        item_start = end + 1;
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

    let pieces: Vec<&str> = expanded.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount { found: pieces.len() });
    }

    let mut unrestricted = [false; 2];
    let mut fields = Vec::with_capacity(5);
    for (i, piece) in pieces.iter().enumerate() {
        let mut flag = false;
        let parsed = parse_field(piece, FIELDS[i], &mut flag)?;
        if i == 2 {
            unrestricted[0] = flag;
        } else if i == 4 {
            unrestricted[1] = flag;
        }
        fields.push(parsed);
    }

    let mut it = fields.into_iter();
    Ok(Cron {
        selected: [
            it.next().unwrap(),
            it.next().unwrap(),
            it.next().unwrap(),
            it.next().unwrap(),
            it.next().unwrap(),
        ],
        unrestricted,
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let i = field_index(field);
        self.selected[i]
            .iter()
            .enumerate()
            .filter_map(|(n, &yes)| yes.then_some(n as u8))
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate(at)?;
        Ok(self.matches_valid(at))
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

        match (self.unrestricted[0], self.unrestricted[1]) {
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
        let mut day = CivilDateTime {
            hour: 0,
            minute: 0,
            ..after
        };
        loop {
            for &hour in &self.allowed(Field::Hour) {
                for &minute in &self.allowed(Field::Minute) {
                    let candidate = CivilDateTime { hour, minute, ..day };
                    if candidate > after && self.matches_valid(candidate) {
                        return Ok(Some(candidate));
                    }
                }
            }
            match increment_day(day) {
                Some(next) => day = next,
                None => return Ok(None),
            }
        }
    }

    pub fn previous_fire(&self, before: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate(before)?;
        let mut day = CivilDateTime {
            hour: 0,
            minute: 0,
            ..before
        };
        loop {
            let hours = self.allowed(Field::Hour);
            let minutes = self.allowed(Field::Minute);
            for &hour in hours.iter().rev() {
                for &minute in minutes.iter().rev() {
                    let candidate = CivilDateTime { hour, minute, ..day };
                    if candidate < before && self.matches_valid(candidate) {
                        return Ok(Some(candidate));
                    }
                }
            }
            match decrement_day(day) {
                Some(prev) => day = prev,
                None => return Ok(None),
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
        let mut cursor = start;
        while cursor < end {
            match self.next_fire_or_at(cursor) {
                Some(fire) if fire < end => {
                    if fires.len() == cap {
                        return Ok(FireWindow {
                            fires,
                            truncated: true,
                        });
                    }
                    fires.push(fire);
                    cursor = match increment_minute(fire) {
                        Some(next) => next,
                        None => break,
                    };
                }
                _ => break,
            }
        }
        Ok(FireWindow {
            fires,
            truncated: false,
        })
    }

    fn next_fire_or_at(&self, at: CivilDateTime) -> Option<CivilDateTime> {
        if self.matches_valid(at) {
            return Some(at);
        }
        self.next_fire(at).ok().flatten()
    }

    pub fn canonical(&self) -> String {
        let mut parts = Vec::new();
        for i in 0..5 {
            let field = FIELDS[i];
            let (min, max) = bounds(field);
            let all = (min..=max).all(|n| self.selected[i][n as usize]);
            if (i == 2 && self.unrestricted[0]) || (i == 4 && self.unrestricted[1]) {
                parts.push("*".to_string());
            } else if all {
                if i == 2 || i == 4 {
                    parts.push("*/1".to_string());
                } else {
                    parts.push("*".to_string());
                }
            } else {
                parts.push(
                    (min..=max)
                        .filter(|&n| self.selected[i][n as usize])
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>()
                        .join(","),
                );
            }
        }
        parts.join(" ")
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
    ((6 + count) % 7) as u8
}

fn increment_day(at: CivilDateTime) -> Option<CivilDateTime> {
    if at.day < days_in_month(at.year, at.month) {
        return Some(CivilDateTime {
            day: at.day + 1,
            ..at
        });
    }
    if at.month < 12 {
        return Some(CivilDateTime {
            day: 1,
            month: at.month + 1,
            ..at
        });
    }
    if at.year < 2399 {
        return Some(CivilDateTime {
            day: 1,
            month: 1,
            year: at.year + 1,
            ..at
        });
    }
    None
}

fn decrement_day(at: CivilDateTime) -> Option<CivilDateTime> {
    if at.day > 1 {
        return Some(CivilDateTime {
            day: at.day - 1,
            ..at
        });
    }
    if at.month > 1 {
        let month = at.month - 1;
        return Some(CivilDateTime {
            day: days_in_month(at.year, month),
            month,
            ..at
        });
    }
    if at.year > 2000 {
        let year = at.year - 1;
        return Some(CivilDateTime {
            day: 31,
            month: 12,
            year,
            ..at
        });
    }
    None
}

fn increment_minute(at: CivilDateTime) -> Option<CivilDateTime> {
    if at.minute < 59 {
        Some(CivilDateTime {
            minute: at.minute + 1,
            ..at
        })
    } else if at.hour < 23 {
        Some(CivilDateTime {
            minute: 0,
            hour: at.hour + 1,
            ..at
        })
    } else {
        increment_day(CivilDateTime {
            minute: 0,
            hour: 0,
            ..at
        })
    }
}