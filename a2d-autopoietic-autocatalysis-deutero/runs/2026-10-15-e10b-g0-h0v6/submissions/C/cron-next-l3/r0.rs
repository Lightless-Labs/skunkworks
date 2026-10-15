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
    sets: [Vec<bool>; 5],
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

fn width(field: Field) -> u8 {
    let (lo, hi) = domain(field);
    hi - lo + 1
}

fn err(field: Field, offset: usize, kind: u8) -> ParseError {
    match kind {
        0 => ParseError::InvalidSyntax { field, offset },
        1 => ParseError::LeadingZero { field, offset },
        2 => ParseError::NumberOverflow { field, offset },
        3 => ParseError::OutOfRange { field, offset },
        4 => ParseError::ReversedRange { field, offset },
        5 => ParseError::BadStep { field, offset },
        _ => ParseError::EmptyItem { field, offset },
    }
}

fn name_value(field: Field, token: &str) -> Option<u8> {
    let names: &[&str] = match field {
        Field::Month => &[
            "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV",
            "DEC",
        ],
        Field::DayOfWeek => &["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"],
        _ => return None,
    };
    names
        .iter()
        .position(|name| token.eq_ignore_ascii_case(name))
        .map(|i| if field == Field::DayOfWeek { i as u8 } else { i as u8 + 1 })
}

fn parse_atom(
    field: Field,
    token: &str,
    offset: usize,
    numeric_validation: bool,
) -> Result<u8, ParseError> {
    if token.is_empty() {
        return Err(err(field, offset, 0));
    }
    if token.bytes().all(|b| b.is_ascii_digit()) {
        if token.len() > 1 && token.as_bytes()[0] == b'0' {
            return Err(err(field, offset, 1));
        }
        let mut value: u32 = 0;
        for b in token.bytes() {
            value = value
                .checked_mul(10)
                .and_then(|v| v.checked_add((b - b'0') as u32))
                .ok_or_else(|| err(field, offset, 2))?;
        }
        if numeric_validation {
            let (lo, hi) = domain(field);
            if value < lo as u32 || value > hi as u32 {
                return Err(err(field, offset, 3));
            }
        }
        return Ok(value as u8);
    }
    name_value(field, token).ok_or_else(|| err(field, offset, 0))
}

fn parse_field(field: Field, text: &str, set: &mut [bool]) -> Result<(), ParseError> {
    let bytes = text.as_bytes();
    for (offset, &b) in bytes.iter().enumerate() {
        if !(b.is_ascii_digit()
            || b.is_ascii_alphabetic()
            || matches!(b, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (minimum, maximum) = domain(field);
    let mut item_start = 0;
    for item in text.split(',') {
        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let item_bytes = item.as_bytes();
        let slash_count = item_bytes.iter().filter(|&&b| b == b'/').count();
        if slash_count > 1 {
            return Err(err(field, item_start, 0));
        }
        let (base, step_text, step_offset) = if let Some(pos) = item.find('/') {
            (&item[..pos], Some(&item[pos + 1..]), item_start + pos + 1)
        } else {
            (item, None, 0)
        };
        if base.is_empty() || step_text == Some("") {
            return Err(err(field, item_start, 0));
        }

        let star = base == "*";
        let hyphen_count = base.bytes().filter(|&b| b == b'-').count();
        if (!star && hyphen_count > 1)
            || (star && hyphen_count != 0)
            || (base.contains('*') && !star)
        {
            return Err(err(field, item_start, 0));
        }

        let (start_token, end_token, end_offset) = if star {
            (None, None, 0)
        } else if let Some(pos) = base.find('-') {
            let left = &base[..pos];
            let right = &base[pos + 1..];
            if left.is_empty() || right.is_empty() || left.contains('*') || right.contains('*') {
                return Err(err(field, item_start, 0));
            }
            (Some(left), Some(right), item_start + pos + 1)
        } else {
            (Some(base), None, 0)
        };

        // Validate token shape before reading numeric values.
        for token in [start_token, end_token].into_iter().flatten() {
            let digits = token.bytes().all(|b| b.is_ascii_digit());
            let named = name_value(field, token).is_some();
            if !digits && !named {
                return Err(err(field, item_start, 0));
            }
        }

        let (start, end) = if star {
            (minimum, maximum)
        } else {
            let st = start_token.unwrap();
            let start_offset = item_start;
            let start_value = parse_atom(field, st, start_offset, true)?;
            if let Some(et) = end_token {
                let end_value = parse_atom(field, et, end_offset, true)?;
                if start_value > end_value {
                    return Err(err(field, end_offset, 4));
                }
                (start_value, end_value)
            } else {
                (start_value, if step_text.is_some() { maximum } else { start_value })
            }
        };

        let step = if let Some(step) = step_text {
            if !step.bytes().all(|b| b.is_ascii_digit()) || step.is_empty() {
                return Err(err(field, item_start, 0));
            }
            if step.len() > 1 && step.as_bytes()[0] == b'0' {
                return Err(err(field, step_offset, 1));
            }
            let mut value: u32 = 0;
            for b in step.bytes() {
                value = value
                    .checked_mul(10)
                    .and_then(|v| v.checked_add((b - b'0') as u32))
                    .ok_or_else(|| err(field, step_offset, 2))?;
            }
            if value == 0 || value > width(field) as u32 {
                return Err(err(field, step_offset, 5));
            }
            value as u8
        } else {
            1
        };

        let mut value = start;
        while value <= end {
            set[value as usize] = true;
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
            }
        }
        item_start += item.len() + 1;
    }
    Ok(())
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut sets: [Vec<bool>; 5] = std::array::from_fn(|i| {
        let (lo, hi) = domain(FIELDS[i]);
        vec![false; hi as usize + 1.max(lo as usize)]
    });
    for (i, field) in FIELDS.iter().copied().enumerate() {
        let (lo, hi) = domain(field);
        sets[i].resize(hi as usize + 1, false);
        parse_field(field, parts[i], &mut sets[i])?;
        if lo > 0 {
            // Indexing uses the field value directly.
        }
    }

    Ok(Cron {
        sets,
        unrestricted_days: [parts[2] == "*", parts[4] == "*"],
    })
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
        let (lo, hi) = domain(field);
        (lo..=hi)
            .filter(|&value| self.sets[index][value as usize])
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate(at)?;
        Ok(self.matches_valid(at))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate(after)?;
        if after.year == 2399 && after.month == 12 && after.day == 31 && after.hour == 23 && after.minute == 59 {
            return Ok(None);
        }

        let mut current = after;
        loop {
            if current.minute < 59 {
                current.minute += 1;
            } else {
                current.minute = 0;
                if current.hour < 23 {
                    current.hour += 1;
                } else {
                    current.hour = 0;
                    if current.day < days_in_month(current.year, current.month) {
                        current.day += 1;
                    } else {
                        current.day = 1;
                        if current.month < 12 {
                            current.month += 1;
                        } else {
                            current.month = 1;
                            if current.year == 2399 {
                                return Ok(None);
                            }
                            current.year += 1;
                        }
                    }
                }
            }
            if self.matches_valid(current) {
                return Ok(Some(current));
            }
        }
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        if !self.sets[0][at.minute as usize]
            || !self.sets[1][at.hour as usize]
            || !self.sets[3][at.month as usize]
        {
            return false;
        }

        let dom_unrestricted = self.unrestricted_days[0];
        let dow_unrestricted = self.unrestricted_days[1];
        let dom_match = self.sets[2][at.day as usize];
        let dow_match = self.sets[4][weekday(at.year, at.month, at.day) as usize];

        match (dom_unrestricted, dow_unrestricted) {
            (true, true) => true,
            (true, false) => dow_match,
            (false, true) => dom_match,
            (false, false) => {
                if at.day == days_in_month(at.year, at.month) {
                    dom_match && dow_match
                } else {
                    dom_match || dow_match
                }
            }
        }
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

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days = 0u32;
    for y in 2000..year {
        days += if is_leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as u32;
    }
    days += (day - 1) as u32;
    ((days + 6) % 7) as u8
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

fn main() {}