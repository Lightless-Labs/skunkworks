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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    selected: [Vec<bool>; 5],
    day_of_month_unrestricted: bool,
    day_of_week_unrestricted: bool,
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

fn parse_atom(s: &str, field: Field, offset: usize) -> Result<u32, ParseError> {
    if s.is_empty() {
        return Err(ParseError::InvalidSyntax { field, offset });
    }

    let bytes = s.as_bytes();
    if bytes.iter().all(u8::is_ascii_digit) {
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

        let (min, max) = bounds(field);
        if value < min || value > max {
            return Err(ParseError::OutOfRange { field, offset });
        }
        return Ok(value);
    }

    let upper = s.to_ascii_uppercase();
    let value = match field {
        Field::Month => match upper.as_str() {
            "JAN" => 1,
            "FEB" => 2,
            "MAR" => 3,
            "APR" => 4,
            "MAY" => 5,
            "JUN" => 6,
            "JUL" => 7,
            "AUG" => 8,
            "SEP" => 9,
            "OCT" => 10,
            "NOV" => 11,
            "DEC" => 12,
            _ => return Err(ParseError::InvalidSyntax { field, offset }),
        },
        Field::DayOfWeek => match upper.as_str() {
            "SUN" => 0,
            "MON" => 1,
            "TUE" => 2,
            "WED" => 3,
            "THU" => 4,
            "FRI" => 5,
            "SAT" => 6,
            _ => return Err(ParseError::InvalidSyntax { field, offset }),
        },
        _ => return Err(ParseError::InvalidSyntax { field, offset }),
    };
    Ok(value)
}

fn syntax_shape(base: &str) -> bool {
    if base == "*" {
        return true;
    }
    let hyphens = base.bytes().filter(|&b| b == b'-').count();
    if hyphens == 0 {
        return !base.is_empty();
    }
    if hyphens == 1 {
        let (a, b) = base.split_once('-').unwrap();
        return !a.is_empty() && !b.is_empty();
    }
    false
}

fn parse_field(text: &str, field: Field) -> Result<Vec<bool>, ParseError> {
    for (offset, &b) in text.as_bytes().iter().enumerate() {
        if !(b.is_ascii_digit()
            || b.is_ascii_alphabetic()
            || matches!(b, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (min, max) = bounds(field);
    let mut selected = vec![false; (max + 1) as usize];

    let mut item_start = 0usize;
    for item in text.split(',') {
        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let slash_count = item.bytes().filter(|&b| b == b'/').count();
        let (base, step_text) = if slash_count == 0 {
            (item, None)
        } else if slash_count == 1 {
            let (b, s) = item.split_once('/').unwrap();
            (b, Some(s))
        } else {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        };

        if !syntax_shape(base) || step_text == Some("") {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let range_parts = if base == "*" {
            None
        } else if let Some((a, b)) = base.split_once('-') {
            Some((a, b))
        } else {
            Some((base, ""))
        };

        let (start, end, end_offset) = if let Some((a, b)) = range_parts {
            let a_offset = item_start;
            let a_value = parse_atom(a, field, a_offset)?;
            if b.is_empty() {
                (a_value, a_value, a_offset)
            } else {
                let b_offset = item_start + a.len() + 1;
                let b_value = parse_atom(b, field, b_offset)?;
                if a_value > b_value {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: b_offset,
                    });
                }
                (a_value, b_value, b_offset)
            }
        } else {
            (min, max, item_start)
        };

        let step = if let Some(step_str) = step_text {
            let step_offset = item_start + base.len() + 1;
            if step_str.is_empty() || !step_str.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            if step_str.len() > 1 && step_str.as_bytes()[0] == b'0' {
                return Err(ParseError::LeadingZero {
                    field,
                    offset: step_offset,
                });
            }
            let mut value = 0u32;
            for &b in step_str.as_bytes() {
                value = value
                    .checked_mul(10)
                    .and_then(|n| n.checked_add((b - b'0') as u32))
                    .ok_or(ParseError::NumberOverflow {
                        field,
                        offset: step_offset,
                    })?;
            }
            if value < 1 || value > max - min + 1 {
                return Err(ParseError::BadStep {
                    field,
                    offset: step_offset,
                });
            }
            value
        } else {
            1
        };

        let (selection_start, selection_end) = if base == "*" {
            (min, max)
        } else if let Some((a, b)) = base.split_once('-') {
            let _ = (a, b);
            (start, end)
        } else if step_text.is_some() {
            (start, max)
        } else {
            (start, start)
        };

        let mut value = selection_start;
        while value <= selection_end {
            selected[value as usize] = true;
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
            }
        }

        item_start += item.len() + 1;
        let _ = end_offset;
    }

    Ok(selected)
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut selected = [
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ];
    for (i, (&field, text)) in FIELDS.iter().zip(parts.iter()).enumerate() {
        selected[i] = parse_field(text, field)?;
    }

    Ok(Cron {
        selected,
        day_of_month_unrestricted: parts[2] == "*",
        day_of_week_unrestricted: parts[4] == "*",
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let (min, max) = bounds(field);
        self.selected[field_index(field)]
            .iter()
            .enumerate()
            .filter_map(|(value, &is_selected)| {
                if is_selected && (value as u32) >= min && (value as u32) <= max {
                    Some(value as u8)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        Ok(self.matches_valid(at))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;

        let mut current = after;
        loop {
            if !increment_minute(&mut current) {
                return Ok(None);
            }
            if self.matches_valid(current) {
                return Ok(Some(current));
            }
        }
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        let minute_ok = self.selected[0][at.minute as usize];
        let hour_ok = self.selected[1][at.hour as usize];
        let month_ok = self.selected[3][at.month as usize];
        if !minute_ok || !hour_ok || !month_ok {
            return false;
        }

        let dom = self.selected[2][at.day as usize];
        let dow = self.selected[4][weekday(at.year, at.month, at.day) as usize];
        match (
            self.day_of_month_unrestricted,
            self.day_of_week_unrestricted,
        ) {
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
    if at.day < 1 || at.day > days_in_month(at.year, at.month) {
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

fn leap_year(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 => if leap_year(year) { 29 } else { 28 },
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
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
    ((days + 6) % 7) as u8
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