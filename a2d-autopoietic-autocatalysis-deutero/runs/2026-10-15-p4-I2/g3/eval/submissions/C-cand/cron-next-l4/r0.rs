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

const FIELDS: [Field; 5] = [
    Field::Minute,
    Field::Hour,
    Field::DayOfMonth,
    Field::Month,
    Field::DayOfWeek,
];

fn bounds(field: Field) -> (u32, u32, u32) {
    match field {
        Field::Minute => (0, 59, 60),
        Field::Hour => (0, 23, 24),
        Field::DayOfMonth => (1, 31, 31),
        Field::Month => (1, 12, 12),
        Field::DayOfWeek => (0, 6, 7),
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
    if !bytes.iter().all(u8::is_ascii_alphabetic) {
        return None;
    }
    let upper: Vec<u8> = bytes.iter().map(u8::to_ascii_uppercase).collect();
    match field {
        Field::Month => {
            const NAMES: [&[u8]; 12] = [
                b"JAN", b"FEB", b"MAR", b"APR", b"MAY", b"JUN", b"JUL", b"AUG", b"SEP",
                b"OCT", b"NOV", b"DEC",
            ];
            NAMES.iter().position(|n| *n == upper).map(|i| i as u32 + 1)
        }
        Field::DayOfWeek => {
            const NAMES: [&[u8]; 7] =
                [b"SUN", b"MON", b"TUE", b"WED", b"THU", b"FRI", b"SAT"];
            NAMES.iter().position(|n| *n == upper).map(|i| i as u32)
        }
        _ => None,
    }
}

fn atom_value(field: Field, bytes: &[u8], offset: usize) -> Result<u32, ParseError> {
    if bytes.is_empty() {
        return Err(ParseError::InvalidSyntax { field, offset });
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
        let (min, max, _) = bounds(field);
        if value < min || value > max {
            return Err(ParseError::OutOfRange { field, offset });
        }
        Ok(value)
    } else if let Some(value) = name_value(field, bytes) {
        Ok(value)
    } else {
        Err(ParseError::InvalidSyntax { field, offset })
    }
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
        .filter_map(|(i, b)| (*b == b'/').then_some(i))
        .collect();
    if slash_positions.len() > 1 {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    }

    let (base_end, step_part) = if let Some(&slash) = slash_positions.first() {
        (&item[..slash], Some((slash + 1, &item[slash + 1..])))
    } else {
        (item, None)
    };
    if base_end.is_empty() || step_part.is_some_and(|(_, s)| s.is_empty()) {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    }

    let is_star = base_end == b"*";
    let hyphens: Vec<usize> = base_end
        .iter()
        .enumerate()
        .filter_map(|(i, b)| (*b == b'-').then_some(i))
        .collect();
    if is_star && !hyphens.is_empty() || hyphens.len() > 1 {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    }

    let (start_bytes, end_bytes, start_rel, end_rel) = if is_star {
        (None, None, 0usize, 0usize)
    } else if let Some(&h) = hyphens.first() {
        if h == 0 || h + 1 == base_end.len() {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
        (
            Some(&base_end[..h]),
            Some(&base_end[h + 1..]),
            0,
            h + 1,
        )
    } else {
        (Some(base_end), None, 0, 0)
    };

    for part in [start_bytes, end_bytes].into_iter().flatten() {
        if !(part.iter().all(u8::is_ascii_digit)
            || name_value(field, part).is_some())
        {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
    }

    let step = if let Some((step_rel, bytes)) = step_part {
        if !bytes.iter().all(u8::is_ascii_digit) {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
        if bytes.len() > 1 && bytes[0] == b'0' {
            return Err(ParseError::LeadingZero {
                field,
                offset: item_offset + step_rel,
            });
        }
        let mut value = 0u32;
        for &b in bytes {
            value = value
                .checked_mul(10)
                .and_then(|v| v.checked_add((b - b'0') as u32))
                .ok_or(ParseError::NumberOverflow {
                    field,
                    offset: item_offset + step_rel,
                })?;
        }
        let (_, _, width) = bounds(field);
        if value == 0 || value > width {
            return Err(ParseError::BadStep {
                field,
                offset: item_offset + step_rel,
            });
        }
        Some(value)
    } else {
        None
    };

    let (min, max, _) = bounds(field);
    let start = if is_star {
        min
    } else {
        atom_value(
            field,
            start_bytes.unwrap(),
            item_offset + start_rel,
        )?
    };
    let end = if is_star {
        max
    } else if let Some(bytes) = end_bytes {
        atom_value(field, bytes, item_offset + end_rel)?
    } else if step.is_some() {
        max
    } else {
        start
    };

    if end_bytes.is_some() && start > end {
        return Err(ParseError::ReversedRange {
            field,
            offset: item_offset + end_rel,
        });
    }

    let increment = step.unwrap_or(1);
    let mut value = start;
    while value <= end {
        selected[(value - min) as usize] = true;
        match value.checked_add(increment) {
            Some(next) if next > value => value = next,
            _ => break,
        }
    }
    Ok(())
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let expanded = match input {
        "@hourly" => Some("0 * * * *"),
        "@daily" => Some("0 0 * * *"),
        "@weekly" => Some("0 0 * * 0"),
        "@monthly" => Some("0 0 1 * *"),
        "@yearly" => Some("0 0 1 1 *"),
        _ => None,
    };
    if input.as_bytes().first() == Some(&b'@') {
        if let Some(expanded) = expanded {
            return parse_cron(expanded);
        }
        return Err(ParseError::UnknownSpecial);
    }

    let pieces: Vec<&str> = input.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount {
            found: pieces.len(),
        });
    }

    let mut selected: [Vec<bool>; 5] = [
        vec![false; 60],
        vec![false; 24],
        vec![false; 31],
        vec![false; 12],
        vec![false; 7],
    ];
    let mut unrestricted_days = [false; 2];

    for (index, text) in pieces.iter().enumerate() {
        let field = FIELDS[index];
        let bytes = text.as_bytes();
        for (offset, &byte) in bytes.iter().enumerate() {
            if !(byte.is_ascii_digit()
                || byte.is_ascii_alphabetic()
                || matches!(byte, b'*' | b'-' | b'/' | b','))
            {
                return Err(ParseError::InvalidCharacter { field, offset });
            }
        }

        if index == 2 {
            unrestricted_days[0] = *text == "*";
        } else if index == 4 {
            unrestricted_days[1] = *text == "*";
        }

        let mut start = 0usize;
        for (i, part) in bytes.iter().enumerate() {
            if *part == b',' {
                parse_item(field, &bytes[start..i], start, &mut selected[index])?;
                start = i + 1;
            }
        }
        parse_item(field, &bytes[start..], start, &mut selected[index])?;
    }

    Ok(Cron {
        selected,
        unrestricted_days,
    })
}

fn is_leap(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn month_length(year: u16, month: u8) -> u8 {
    match month {
        2 => if is_leap(year) { 29 } else { 28 },
        4 | 6 | 9 | 11 => 30,
        _ => 31,
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
    if at.day == 0 || at.day > month_length(at.year, at.month) {
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

fn weekday(at: CivilDateTime) -> u8 {
    let mut days = 0u32;
    for year in 2000..at.year {
        days += if is_leap(year) { 366 } else { 365 };
    }
    for month in 1..at.month {
        days += month_length(at.year, month) as u32;
    }
    days += at.day as u32 - 1;
    ((days + 6) % 7) as u8
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let index = field_index(field);
        let (min, _, _) = bounds(field);
        self.selected[index]
            .iter()
            .enumerate()
            .filter_map(|(i, &yes)| yes.then_some(min as u8 + i as u8))
            .collect()
    }

    fn date_matches(&self, at: CivilDateTime) -> bool {
        let dom = self.selected[2][at.day as usize - 1];
        let dow = self.selected[4][weekday(at) as usize];
        match (self.unrestricted_days[0], self.unrestricted_days[1]) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) => {
                if at.day == month_length(at.year, at.month) {
                    dom && dow
                } else {
                    dom || dow
                }
            }
        }
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        self.selected[0][at.minute as usize]
            && self.selected[1][at.hour as usize]
            && self.selected[3][at.month as usize - 1]
            && self.date_matches(at)
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate(at)?;
        Ok(self.matches_valid(at))
    }

    fn next_valid(&self, after: CivilDateTime, inclusive: bool) -> Option<CivilDateTime> {
        if !self.selected[0].iter().any(|&v| v)
            || !self.selected[1].iter().any(|&v| v)
            || !self.selected[3].iter().any(|&v| v)
        {
            return None;
        }
        let mut date = CivilDateTime {
            year: after.year,
            month: after.month,
            day: after.day,
            hour: 0,
            minute: 0,
        };
        loop {
            if self.selected[3][date.month as usize - 1] && self.date_matches(date) {
                let hours = self.allowed(Field::Hour);
                let minutes = self.allowed(Field::Minute);
                for hour in hours {
                    for minute in &minutes {
                        let candidate = CivilDateTime {
                            year: date.year,
                            month: date.month,
                            day: date.day,
                            hour,
                            minute: *minute,
                        };
                        if (candidate > after || (inclusive && candidate == after))
                            && self.matches_valid(candidate)
                        {
                            return Some(candidate);
                        }
                    }
                }
            }
            if date.year == 2399 && date.month == 12 && date.day == 31 {
                return None;
            }
            if date.day < month_length(date.year, date.month) {
                date.day += 1;
            } else if date.month < 12 {
                date.month += 1;
                date.day = 1;
            } else {
                date.year += 1;
                date.month = 1;
                date.day = 1;
            }
        }
    }

    fn previous_valid(&self, before: CivilDateTime) -> Option<CivilDateTime> {
        if !self.selected[0].iter().any(|&v| v)
            || !self.selected[1].iter().any(|&v| v)
            || !self.selected[3].iter().any(|&v| v)
        {
            return None;
        }
        let mut date = CivilDateTime {
            year: before.year,
            month: before.month,
            day: before.day,
            hour: 0,
            minute: 0,
        };
        loop {
            if self.selected[3][date.month as usize - 1] && self.date_matches(date) {
                let hours = self.allowed(Field::Hour);
                let minutes = self.allowed(Field::Minute);
                for &hour in hours.iter().rev() {
                    for &minute in minutes.iter().rev() {
                        let candidate = CivilDateTime {
                            year: date.year,
                            month: date.month,
                            day: date.day,
                            hour,
                            minute,
                        };
                        if candidate < before && self.matches_valid(candidate) {
                            return Some(candidate);
                        }
                    }
                }
            }
            if date.year == 2000 && date.month == 1 && date.day == 1 {
                return None;
            }
            if date.day > 1 {
                date.day -= 1;
            } else if date.month > 1 {
                date.month -= 1;
                date.day = month_length(date.year, date.month);
            } else {
                date.year -= 1;
                date.month = 12;
                date.day = 31;
            }
        }
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate(after)?;
        Ok(self.next_valid(after, false))
    }

    pub fn previous_fire(
        &self,
        before: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
        validate(before)?;
        Ok(self.previous_valid(before))
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
        if start == end {
            return Ok(FireWindow {
                fires: Vec::new(),
                truncated: false,
            });
        }

        let mut fires = Vec::new();
        let mut cursor = self.next_valid(start, true);
        while let Some(found) = cursor {
            if found >= end {
                break;
            }
            if fires.len() == cap {
                return Ok(FireWindow {
                    fires,
                    truncated: true,
                });
            }
            fires.push(found);
            cursor = self.next_valid(found, false);
        }
        Ok(FireWindow {
            fires,
            truncated: false,
        })
    }

    pub fn canonical(&self) -> String {
        let mut parts = Vec::with_capacity(5);
        for field in FIELDS {
            let index = field_index(field);
            let values = self.allowed(field);
            let (_, _, width) = bounds(field);
            let unrestricted = match field {
                Field::DayOfMonth => self.unrestricted_days[0],
                Field::DayOfWeek => self.unrestricted_days[1],
                _ => false,
            };
            if unrestricted || (values.len() as u32 == width && index != 2 && index != 4) {
                parts.push("*".to_string());
            } else if (index == 2 || index == 4) && values.len() as u32 == width {
                parts.push("*/1".to_string());
            } else {
                parts.push(
                    values
                        .iter()
                        .map(u8::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                );
            }
        }
        parts.join(" ")
    }
}

fn main() {}