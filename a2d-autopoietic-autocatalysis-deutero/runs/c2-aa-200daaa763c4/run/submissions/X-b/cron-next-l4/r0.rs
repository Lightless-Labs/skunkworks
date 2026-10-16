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

const MINS: [u32; 5] = [0, 0, 1, 1, 0];
const MAXS: [u32; 5] = [59, 23, 31, 12, 6];

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

fn name_value(field: Field, bytes: &[u8]) -> Option<u32> {
    if !bytes.iter().all(u8::is_ascii_alphabetic) {
        return None;
    }
    let upper: Vec<u8> = bytes.iter().map(u8::to_ascii_uppercase).collect();
    let names: &[&[u8]] = match field {
        Field::Month => &[
            b"JAN", b"FEB", b"MAR", b"APR", b"MAY", b"JUN", b"JUL", b"AUG", b"SEP",
            b"OCT", b"NOV", b"DEC",
        ],
        Field::DayOfWeek => &[b"SUN", b"MON", b"TUE", b"WED", b"THU", b"FRI", b"SAT"],
        _ => return None,
    };
    names
        .iter()
        .position(|name| *name == upper.as_slice())
        .map(|n| if field == Field::Month { (n + 1) as u32 } else { n as u32 })
}

fn is_number(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit)
}

fn valid_atom(field: Field, bytes: &[u8]) -> bool {
    is_number(bytes) || name_value(field, bytes).is_some()
}

fn parse_number(bytes: &[u8], field: Field, offset: usize) -> Result<u32, ParseError> {
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

fn parse_base_atom(bytes: &[u8], field: Field, offset: usize) -> Result<u32, ParseError> {
    if is_number(bytes) {
        let n = parse_number(bytes, field, offset)?;
        let i = index(field);
        if n < MINS[i] || n > MAXS[i] {
            return Err(ParseError::OutOfRange { field, offset });
        }
        Ok(n)
    } else {
        name_value(field, bytes).ok_or_else(|| invalid_syntax(field, offset))
    }
}

fn parse_field(text: &str, field: Field) -> Result<Vec<bool>, ParseError> {
    let bytes = text.as_bytes();

    for (offset, &b) in bytes.iter().enumerate() {
        if !(b.is_ascii_digit()
            || b.is_ascii_alphabetic()
            || matches!(b, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let i = index(field);
    let mut selected = vec![false; (MAXS[i] + 1) as usize];
    let mut item_start = 0usize;

    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|p| item_start + p)
            .unwrap_or(bytes.len());
        let item = &bytes[item_start..item_end];

        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let slash_positions: Vec<usize> = item
            .iter()
            .enumerate()
            .filter_map(|(p, &b)| (b == b'/').then_some(p))
            .collect();

        let (base, step_bytes, step_offset) = match slash_positions.as_slice() {
            [] => (item, None, 0),
            [slash] => (
                &item[..*slash],
                Some(&item[*slash + 1..]),
                item_start + *slash + 1,
            ),
            _ => return Err(invalid_syntax(field, item_start)),
        };

        let wildcard = base == b"*";
        let hyphens: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(p, &b)| (b == b'-').then_some(p))
            .collect();

        let (first, last, end_offset) = if wildcard {
            if !hyphens.is_empty() {
                return Err(invalid_syntax(field, item_start));
            }
            (MINS[i], MAXS[i], item_start)
        } else {
            match hyphens.as_slice() {
                [] => {
                    if !valid_atom(field, base) {
                        return Err(invalid_syntax(field, item_start));
                    }
                    let value = parse_base_atom(base, field, item_start)?;
                    (value, value, item_start)
                }
                [hyphen] => {
                    let left = &base[..*hyphen];
                    let right = &base[*hyphen + 1..];
                    if !valid_atom(field, left) || !valid_atom(field, right) {
                        return Err(invalid_syntax(field, item_start));
                    }
                    let left_offset = item_start;
                    let right_offset = item_start + *hyphen + 1;
                    let first = parse_base_atom(left, field, left_offset)?;
                    let last = parse_base_atom(right, field, right_offset)?;
                    if first > last {
                        return Err(ParseError::ReversedRange {
                            field,
                            offset: right_offset,
                        });
                    }
                    (first, last, right_offset)
                }
                _ => return Err(invalid_syntax(field, item_start)),
            }
        };

        let step = if let Some(step_bytes) = step_bytes {
            if !is_number(step_bytes) {
                return Err(invalid_syntax(field, item_start));
            }
            let n = parse_number(step_bytes, field, step_offset)?;
            let width = MAXS[i] - MINS[i] + 1;
            if n == 0 || n > width {
                return Err(ParseError::BadStep {
                    field,
                    offset: step_offset,
                });
            }
            Some(n)
        } else {
            None
        };

        let effective_end = if step.is_some() && !wildcard && first == last {
            MAXS[i]
        } else {
            last
        };
        let stride = step.unwrap_or(1);
        let mut value = first;
        while value <= effective_end {
            selected[value as usize] = true;
            value += stride;
        }

        let _ = end_offset;
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

    let selected: [Vec<bool>; 5] = [
        parse_field(parts[0], FIELDS[0])?,
        parse_field(parts[1], FIELDS[1])?,
        parse_field(parts[2], FIELDS[2])?,
        parse_field(parts[3], FIELDS[3])?,
        parse_field(parts[4], FIELDS[4])?,
    ];

    Ok(Cron {
        selected,
        unrestricted_dom: parts[2] == "*",
        unrestricted_dow: parts[4] == "*",
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.selected[index(field)]
            .iter()
            .enumerate()
            .filter_map(|(n, &on)| on.then_some(n as u8))
            .collect()
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
        if !(1..=days_in_month(at.year, at.month)).contains(&at.day) {
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

    fn selected_value(&self, field: Field, value: u8) -> bool {
        self.selected[index(field)][value as usize]
    }

    fn day_matches(&self, at: CivilDateTime) -> bool {
        let dom = self.selected_value(Field::DayOfMonth, at.day);
        let dow = self.selected_value(Field::DayOfWeek, weekday(at.year, at.month, at.day));
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

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        self.selected_value(Field::Minute, at.minute)
            && self.selected_value(Field::Hour, at.hour)
            && self.selected_value(Field::Month, at.month)
            && self.day_matches(at)
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        Self::validate(at)?;
        Ok(self.matches_valid(at))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        Self::validate(after)?;
        let mut date = (after.year, after.month, after.day);
        while date <= (2399, 12, 31) {
            let (year, month, day) = date;
            if self.selected_value(Field::Month, month) {
                let probe = CivilDateTime {
                    year,
                    month,
                    day,
                    hour: 0,
                    minute: 0,
                };
                if self.day_matches(probe) {
                    for hour in self.allowed(Field::Hour) {
                        for minute in self.allowed(Field::Minute) {
                            let candidate = CivilDateTime {
                                year,
                                month,
                                day,
                                hour,
                                minute,
                            };
                            if candidate > after {
                                return Ok(Some(candidate));
                            }
                        }
                    }
                }
            }
            date = next_date(year, month, day);
        }
        Ok(None)
    }

    pub fn previous_fire(
        &self,
        before: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
        Self::validate(before)?;
        let mut date = (before.year, before.month, before.day);
        loop {
            let (year, month, day) = date;
            if self.selected_value(Field::Month, month) {
                let probe = CivilDateTime {
                    year,
                    month,
                    day,
                    hour: 0,
                    minute: 0,
                };
                if self.day_matches(probe) {
                    for hour in self.allowed(Field::Hour).into_iter().rev() {
                        for minute in self.allowed(Field::Minute).into_iter().rev() {
                            let candidate = CivilDateTime {
                                year,
                                month,
                                day,
                                hour,
                                minute,
                            };
                            if candidate < before {
                                return Ok(Some(candidate));
                            }
                        }
                    }
                }
            }
            if date == (2000, 1, 1) {
                return Ok(None);
            }
            date = previous_date(year, month, day);
        }
    }

    pub fn fires_between(
        &self,
        start: CivilDateTime,
        end: CivilDateTime,
        cap: usize,
    ) -> Result<FireWindow, WindowError> {
        Self::validate(start).map_err(|e| match e {
            DateError::InvalidDateTime { component } => WindowError::InvalidStart { component },
        })?;
        Self::validate(end).map_err(|e| match e {
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

        loop {
            if !self.matches_valid(cursor) {
                match self.next_fire(cursor) {
                    Ok(Some(next)) => cursor = next,
                    Ok(None) | Err(_) => break,
                }
            }

            if cursor >= end {
                break;
            }

            if fires.len() == cap {
                return Ok(FireWindow {
                    fires,
                    truncated: true,
                });
            }

            fires.push(cursor);

            // Continue from the next minute, preserving a matching start as inclusive.
            if cursor.minute < 59 {
                cursor.minute += 1;
            } else {
                cursor.minute = 0;
                if cursor.hour < 23 {
                    cursor.hour += 1;
                } else {
                    cursor.hour = 0;
                    let (year, month, day) = next_date(cursor.year, cursor.month, cursor.day);
                    if year > 2399 {
                        break;
                    }
                    cursor.year = year;
                    cursor.month = month;
                    cursor.day = day;
                }
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
            let full = self.selected[i]
                .iter()
                .skip(MINS[i] as usize)
                .take((MAXS[i] - MINS[i] + 1) as usize)
                .all(|&v| v);

            if full {
                if (field == Field::DayOfMonth && !self.unrestricted_dom)
                    || (field == Field::DayOfWeek && !self.unrestricted_dow)
                {
                    fields.push("*/1".to_string());
                } else {
                    fields.push("*".to_string());
                }
                continue;
            }

            fields.push(
                self.allowed(field)
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            );
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
    } else if year < 2399 {
        (year + 1, 1, 1)
    } else {
        (2400, 1, 1)
    }
}

fn previous_date(year: u16, month: u8, day: u8) -> (u16, u8, u8) {
    if day > 1 {
        (year, month, day - 1)
    } else if month > 1 {
        let m = month - 1;
        (year, m, days_in_month(year, m))
    } else {
        (year - 1, 12, 31)
    }
}