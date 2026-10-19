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
    day_unrestricted: [bool; 2],
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
struct Spec {
    field: Field,
    min: u32,
    max: u32,
}

const SPECS: [Spec; 5] = [
    Spec {
        field: Field::Minute,
        min: 0,
        max: 59,
    },
    Spec {
        field: Field::Hour,
        min: 0,
        max: 23,
    },
    Spec {
        field: Field::DayOfMonth,
        min: 1,
        max: 31,
    },
    Spec {
        field: Field::Month,
        min: 1,
        max: 12,
    },
    Spec {
        field: Field::DayOfWeek,
        min: 0,
        max: 6,
    },
];

fn invalid_syntax(field: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field, offset }
}

fn parse_number(bytes: &[u8], field: Field, offset: usize) -> Result<u32, ParseError> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(invalid_syntax(field, offset));
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }

    let mut value = 0u32;
    for &byte in bytes {
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    Ok(value)
}

fn name_value(field: Field, bytes: &[u8]) -> Option<u32> {
    let names: &[(&[u8], u32)] = match field {
        Field::Month => &[
            (b"JAN", 1),
            (b"FEB", 2),
            (b"MAR", 3),
            (b"APR", 4),
            (b"MAY", 5),
            (b"JUN", 6),
            (b"JUL", 7),
            (b"AUG", 8),
            (b"SEP", 9),
            (b"OCT", 10),
            (b"NOV", 11),
            (b"DEC", 12),
        ],
        Field::DayOfWeek => &[
            (b"SUN", 0),
            (b"MON", 1),
            (b"TUE", 2),
            (b"WED", 3),
            (b"THU", 4),
            (b"FRI", 5),
            (b"SAT", 6),
        ],
        _ => return None,
    };

    names.iter().find_map(|(name, value)| {
        if bytes.len() == name.len()
            && bytes
                .iter()
                .zip(name.iter())
                .all(|(a, b)| a.to_ascii_uppercase() == *b)
        {
            Some(*value)
        } else {
            None
        }
    })
}

fn valid_base_atom(field: Field, bytes: &[u8]) -> bool {
    (!bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit))
        || name_value(field, bytes).is_some()
}

fn parse_base_value(bytes: &[u8], field: Field, offset: usize) -> Result<u32, ParseError> {
    if !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit) {
        parse_number(bytes, field, offset)
    } else if let Some(value) = name_value(field, bytes) {
        Ok(value)
    } else {
        Err(invalid_syntax(field, offset))
    }
}

fn parse_field(text: &str, spec: Spec) -> Result<(Vec<bool>, bool), ParseError> {
    let bytes = text.as_bytes();
    let mut selected = vec![false; (spec.max + 1) as usize];
    let unrestricted = matches!(spec.field, Field::DayOfMonth | Field::DayOfWeek)
        && bytes == b"*";

    for (i, &byte) in bytes.iter().enumerate() {
        if !(byte.is_ascii_digit()
            || byte.is_ascii_alphabetic()
            || matches!(byte, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter {
                field: spec.field,
                offset: i,
            });
        }
    }

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
                field: spec.field,
                offset: item_start,
            });
        }

        let slash_positions: Vec<usize> = item
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'/').then_some(i))
            .collect();
        if slash_positions.len() > 1 {
            return Err(invalid_syntax(spec.field, item_start));
        }

        let (base, step_bytes, step_offset) = if let Some(&slash) = slash_positions.first() {
            (&item[..slash], Some(&item[slash + 1..]), item_start + slash + 1)
        } else {
            (item, None, 0)
        };
        if base.is_empty() || step_bytes.is_some_and(|s| s.is_empty()) {
            return Err(invalid_syntax(spec.field, item_start));
        }

        let hyphens: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();
        if hyphens.len() > 1 {
            return Err(invalid_syntax(spec.field, item_start));
        }

        let (start_bytes, end_bytes, start_offset, end_offset, is_star) =
            if base == b"*" {
                (&[][..], None, 0, 0, true)
            } else if let Some(&hyphen) = hyphens.first() {
                if hyphen == 0 || hyphen + 1 == base.len() {
                    return Err(invalid_syntax(spec.field, item_start));
                }
                let start = &base[..hyphen];
                let end = &base[hyphen + 1..];
                if !valid_base_atom(spec.field, start) || !valid_base_atom(spec.field, end) {
                    return Err(invalid_syntax(spec.field, item_start));
                }
                (
                    start,
                    Some(end),
                    item_start,
                    item_start + hyphen + 1,
                    false,
                )
            } else {
                if !valid_base_atom(spec.field, base) {
                    return Err(invalid_syntax(spec.field, item_start));
                }
                (base, None, item_start, 0, false)
            };

        if is_star && !base.iter().all(|&b| b == b'*') {
            return Err(invalid_syntax(spec.field, item_start));
        }

        if let Some(step) = step_bytes {
            if !step.iter().all(u8::is_ascii_digit) {
                return Err(invalid_syntax(spec.field, item_start));
            }
        }

        let start_value = if is_star {
            spec.min
        } else {
            let value = parse_base_value(start_bytes, spec.field, start_offset)?;
            if !(spec.min..=spec.max).contains(&value) {
                return Err(ParseError::OutOfRange {
                    field: spec.field,
                    offset: start_offset,
                });
            }
            value
        };

        let end_value = if let Some(end) = end_bytes {
            let value = parse_base_value(end, spec.field, end_offset)?;
            if !(spec.min..=spec.max).contains(&value) {
                return Err(ParseError::OutOfRange {
                    field: spec.field,
                    offset: end_offset,
                });
            }
            if start_value > value {
                return Err(ParseError::ReversedRange {
                    field: spec.field,
                    offset: end_offset,
                });
            }
            value
        } else if is_star || step_bytes.is_some() {
            spec.max
        } else {
            start_value
        };

        let step = if let Some(step) = step_bytes {
            let value = parse_number(step, spec.field, step_offset)?;
            if value == 0 || value > spec.max - spec.min + 1 {
                return Err(ParseError::BadStep {
                    field: spec.field,
                    offset: step_offset,
                });
            }
            value
        } else {
            1
        };

        let mut value = start_value;
        while value <= end_value {
            selected[value as usize] = true;
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
            }
        }

        if item_end == bytes.len() {
            break;
        }
        item_start = item_end + 1;
    }

    Ok((selected, unrestricted))
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let expansion = if input.starts_with('@') {
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

    let parts: Vec<&str> = expansion.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut sets = Vec::with_capacity(5);
    let mut unrestricted = [false; 2];
    for (index, (part, spec)) in parts.iter().zip(SPECS.iter()).enumerate() {
        let (set, is_unrestricted) = parse_field(part, *spec)?;
        if index == 2 {
            unrestricted[0] = is_unrestricted;
        } else if index == 4 {
            unrestricted[1] = is_unrestricted;
        }
        sets.push(set);
    }

    Ok(Cron {
        sets: sets.try_into().ok().unwrap(),
        day_unrestricted: unrestricted,
    })
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
    days += day as u32 - 1;
    ((days + 6) % 7) as u8
}

fn next_date(at: CivilDateTime) -> Option<(u16, u8, u8)> {
    if at.day < days_in_month(at.year, at.month) {
        Some((at.year, at.month, at.day + 1))
    } else if at.month < 12 {
        Some((at.year, at.month + 1, 1))
    } else if at.year < 2399 {
        Some((at.year + 1, 1, 1))
    } else {
        None
    }
}

fn previous_date(at: CivilDateTime) -> Option<(u16, u8, u8)> {
    if at.day > 1 {
        Some((at.year, at.month, at.day - 1))
    } else if at.month > 1 {
        let month = at.month - 1;
        Some((at.year, month, days_in_month(at.year, month)))
    } else if at.year > 2000 {
        Some((at.year - 1, 12, 31))
    } else {
        None
    }
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let index = match field {
            Field::Minute => 0,
            Field::Hour => 1,
            Field::DayOfMonth => 2,
            Field::Month => 3,
            Field::DayOfWeek => 4,
        };
        self.sets[index]
            .iter()
            .enumerate()
            .filter_map(|(value, &is_selected)| is_selected.then_some(value as u8))
            .collect()
    }

    fn date_matches(&self, at: CivilDateTime) -> bool {
        let dom = self.sets[2][at.day as usize];
        let dow = self.sets[4][weekday(at.year, at.month, at.day) as usize];

        match self.day_unrestricted {
            [true, true] => true,
            [true, false] => dow,
            [false, true] => dom,
            [false, false] if at.day == days_in_month(at.year, at.month) => dom && dow,
            [false, false] => dom || dow,
        }
    }

    fn time_matches(&self, at: CivilDateTime) -> bool {
        self.sets[0][at.minute as usize]
            && self.sets[1][at.hour as usize]
            && self.sets[3][at.month as usize]
            && self.date_matches(at)
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        Ok(self.time_matches(at))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;

        let mut date = (after.year, after.month, after.day);
        let mut first_day = true;
        loop {
            let (year, month, day) = date;
            if self.sets[3][month as usize] {
                for hour in 0..24u8 {
                    if !self.sets[1][hour as usize] {
                        continue;
                    }
                    for minute in 0..60u8 {
                        if !self.sets[0][minute as usize] {
                            continue;
                        }
                        let candidate = CivilDateTime {
                            year,
                            month,
                            day,
                            hour,
                            minute,
                        };
                        if first_day && candidate <= after {
                            continue;
                        }
                        if self.time_matches(candidate) {
                            return Ok(Some(candidate));
                        }
                    }
                }
            }
            first_day = false;
            let probe = CivilDateTime {
                year,
                month,
                day,
                hour: 0,
                minute: 0,
            };
            match next_date(probe) {
                Some(next) => date = next,
                None => return Ok(None),
            }
        }
    }

    pub fn previous_fire(
        &self,
        before: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(before)?;

        let mut date = (before.year, before.month, before.day);
        let mut first_day = true;
        loop {
            let (year, month, day) = date;
            if self.sets[3][month as usize] {
                for hour in (0..24u8).rev() {
                    if !self.sets[1][hour as usize] {
                        continue;
                    }
                    for minute in (0..60u8).rev() {
                        if !self.sets[0][minute as usize] {
                            continue;
                        }
                        let candidate = CivilDateTime {
                            year,
                            month,
                            day,
                            hour,
                            minute,
                        };
                        if first_day && candidate >= before {
                            continue;
                        }
                        if self.time_matches(candidate) {
                            return Ok(Some(candidate));
                        }
                    }
                }
            }
            first_day = false;
            let probe = CivilDateTime {
                year,
                month,
                day,
                hour: 0,
                minute: 0,
            };
            match previous_date(probe) {
                Some(previous) => date = previous,
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
        validate_datetime(start).map_err(|error| match error {
            DateError::InvalidDateTime { component } => WindowError::InvalidStart { component },
        })?;
        validate_datetime(end).map_err(|error| match error {
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
            let candidate = if self.matches(cursor).unwrap() {
                Some(cursor)
            } else {
                self.next_fire(cursor).unwrap()
            };

            let Some(candidate) = candidate else {
                break;
            };
            if candidate >= end {
                break;
            }
            if fires.len() == cap {
                return Ok(FireWindow {
                    fires,
                    truncated: true,
                });
            }
            fires.push(candidate);
            cursor = candidate;
        }

        Ok(FireWindow {
            fires,
            truncated: false,
        })
    }

    pub fn canonical(&self) -> String {
        let mut fields = Vec::with_capacity(5);
        for (index, spec) in SPECS.iter().enumerate() {
            let selected: Vec<u32> = (spec.min..=spec.max)
                .filter(|&value| self.sets[index][value as usize])
                .collect();
            let is_full = selected.len() as u32 == spec.max - spec.min + 1;
            let text = if (index == 2 && self.day_unrestricted[0])
                || (index == 4 && self.day_unrestricted[1])
            {
                "*".to_string()
            } else if is_full && (index == 2 || index == 4) {
                "*/1".to_string()
            } else if is_full {
                "*".to_string()
            } else {
                selected
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            };
            fields.push(text);
        }
        fields.join(" ")
    }
}