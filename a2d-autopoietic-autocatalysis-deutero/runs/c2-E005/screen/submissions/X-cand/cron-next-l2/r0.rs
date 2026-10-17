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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    selected: [Vec<bool>; 5],
    full_day_of_month: bool,
    full_day_of_week: bool,
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

fn domain(field: Field) -> (u32, u32) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
    }
}

fn parse_number(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: Field,
    step: bool,
) -> Result<u32, ParseError> {
    if end - start > 1 && bytes[start] == b'0' {
        return Err(ParseError::LeadingZero {
            field,
            offset: start,
        });
    }

    let mut value = 0u32;
    for &byte in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(u32::from(byte - b'0')))
            .ok_or(ParseError::NumberOverflow {
                field,
                offset: start,
            })?;
    }

    let (minimum, maximum) = domain(field);
    if step {
        if value < 1 || value > maximum - minimum + 1 {
            return Err(ParseError::BadStep {
                field,
                offset: start,
            });
        }
    } else if value < minimum || value > maximum {
        return Err(ParseError::OutOfRange {
            field,
            offset: start,
        });
    }

    Ok(value)
}

fn invalid_syntax(field: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field, offset }
}

fn parse_field(text: &str, field: Field) -> Result<Vec<bool>, ParseError> {
    let bytes = text.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (minimum, maximum) = domain(field);
    let mut selected = vec![false; (maximum + 1) as usize];

    let mut item_start = 0usize;
    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|n| item_start + n)
            .unwrap_or(bytes.len());

        if item_start == item_end {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let item = &bytes[item_start..item_end];
        let slash_positions: Vec<usize> = item
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'/').then_some(i))
            .collect();

        if slash_positions.len() > 1 {
            return Err(invalid_syntax(field, item_start));
        }

        let (base_end, step_part) = if let Some(&slash) = slash_positions.first() {
            if slash == 0 || slash + 1 == item.len() {
                return Err(invalid_syntax(field, item_start));
            }
            (slash, Some(slash + 1))
        } else {
            (item.len(), None)
        };

        let base = &item[..base_end];
        let hyphens: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();

        let base_kind = if base == b"*" {
            0
        } else if hyphens.is_empty() && !base.is_empty() && base.iter().all(u8::is_ascii_digit) {
            1
        } else if hyphens.len() == 1 {
            let hyphen = hyphens[0];
            if hyphen == 0
                || hyphen + 1 == base.len()
                || !base[..hyphen].iter().all(u8::is_ascii_digit)
                || !base[hyphen + 1..].iter().all(u8::is_ascii_digit)
            {
                return Err(invalid_syntax(field, item_start));
            }
            2
        } else {
            return Err(invalid_syntax(field, item_start));
        };

        let step = if let Some(step_start_in_item) = step_part {
            if item[step_start_in_item..].iter().any(|&b| !b.is_ascii_digit()) {
                return Err(invalid_syntax(field, item_start));
            }
            Some(parse_number(
                item,
                step_start_in_item,
                item.len(),
                field,
                true,
            )?)
        } else {
            None
        };

        let (start, end) = match base_kind {
            0 => (minimum, maximum),
            1 => {
                let value = parse_number(item, 0, base_end, field, false)?;
                (value, if step.is_some() { maximum } else { value })
            }
            _ => {
                let hyphen = hyphens[0];
                let start = parse_number(item, 0, hyphen, field, false)?;
                let end = parse_number(item, hyphen + 1, base_end, field, false)?;
                if start > end {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: item_start + hyphen + 1,
                    });
                }
                (start, end)
            }
        };

        let stride = step.unwrap_or(1);
        let mut value = start;
        loop {
            selected[value as usize] = true;
            match value.checked_add(stride) {
                Some(next) if next <= end => value = next,
                _ => break,
            }
        }

        if item_end == bytes.len() {
            break;
        }
        item_start = item_end + 1;
    }

    Ok(selected)
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut selected: [Vec<bool>; 5] = std::array::from_fn(|_| Vec::new());
    for (index, (&field, part)) in FIELDS.iter().zip(parts.iter()).enumerate() {
        selected[index] = parse_field(part, field)?;
    }

    Ok(Cron {
        selected,
        full_day_of_month: parts[2] == "*",
        full_day_of_week: parts[4] == "*",
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let index = field_index(field);
        self.selected[index]
            .iter()
            .enumerate()
            .filter_map(|(value, &yes)| yes.then_some(value as u8))
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        let weekday = weekday(at.year, at.month, at.day);

        Ok(self.selected[0][at.minute as usize]
            && self.selected[1][at.hour as usize]
            && self.selected[2][at.day as usize]
            && self.selected[3][at.month as usize]
            && self.selected[4][weekday as usize])
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;

        let mut date = after;
        let mut first_day = true;
        while date.year <= 2399 {
            let day_matches = self.selected[2][date.day as usize]
                && self.selected[3][date.month as usize]
                && self.selected[4][weekday(date.year, date.month, date.day) as usize];

            if day_matches {
                let start_hour = if first_day { after.hour } else { 0 };
                for hour in start_hour..24 {
                    if !self.selected[1][hour as usize] {
                        continue;
                    }
                    let start_minute = if first_day && hour == after.hour {
                        after.minute.saturating_add(1)
                    } else {
                        0
                    };
                    for minute in start_minute..60 {
                        if self.selected[0][minute as usize] {
                            return Ok(Some(CivilDateTime {
                                year: date.year,
                                month: date.month,
                                day: date.day,
                                hour,
                                minute,
                            }));
                        }
                    }
                }
            }

            first_day = false;
            if !advance_day(&mut date) {
                break;
            }
        }

        Ok(None)
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

fn is_leap_year(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if is_leap_year(year) => 29,
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
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += u32::from(days_in_month(year, m));
    }
    days += u32::from(day - 1);
    ((6 + days) % 7) as u8
}

fn advance_day(date: &mut CivilDateTime) -> bool {
    if date.day < days_in_month(date.year, date.month) {
        date.day += 1;
        date.hour = 0;
        date.minute = 0;
        return true;
    }
    if date.month < 12 {
        date.month += 1;
        date.day = 1;
        date.hour = 0;
        date.minute = 0;
        return true;
    }
    if date.year < 2399 {
        date.year += 1;
        date.month = 1;
        date.day = 1;
        date.hour = 0;
        date.minute = 0;
        return true;
    }
    false
}