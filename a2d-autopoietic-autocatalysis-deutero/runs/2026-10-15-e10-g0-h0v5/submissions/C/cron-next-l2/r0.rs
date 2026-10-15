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
    day_of_month_wildcard: bool,
    day_of_week_wildcard: bool,
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

fn field_bounds(field: Field) -> (u32, u32, u32) {
    match field {
        Field::Minute => (0, 59, 60),
        Field::Hour => (0, 23, 24),
        Field::DayOfMonth => (1, 31, 31),
        Field::Month => (1, 12, 12),
        Field::DayOfWeek => (0, 6, 7),
    }
}

fn parse_error(field: Field, offset: usize, kind: u8) -> ParseError {
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

fn parse_number(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: Field,
    is_step: bool,
) -> Result<u32, ParseError> {
    if start == end {
        return Err(parse_error(field, start, 0));
    }
    if end - start > 1 && bytes[start] == b'0' {
        return Err(parse_error(field, start, 1));
    }
    let mut value = 0u32;
    for &byte in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or_else(|| parse_error(field, start, 2))?;
    }
    let (min, max, width) = field_bounds(field);
    if is_step {
        if value == 0 || value > width {
            return Err(parse_error(field, start, 5));
        }
    } else if value < min || value > max {
        return Err(parse_error(field, start, 3));
    }
    Ok(value)
}

fn parse_field(text: &str, field: Field) -> Result<Vec<bool>, ParseError> {
    let bytes = text.as_bytes();
    for (offset, &byte) in bytes.iter().enumerate() {
        if !(byte.is_ascii_digit() || matches!(byte, b'*' | b'-' | b'/' | b',')) {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (min, max, _) = field_bounds(field);
    let mut selected = vec![false; (max + 1) as usize];

    let mut item_start = 0usize;
    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|p| item_start + p)
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
            return Err(parse_error(field, item_start, 0));
        }

        let base_end = slash_positions.first().copied().unwrap_or(item.len());
        let step = if let Some(&slash) = slash_positions.first() {
            if slash + 1 == item.len() {
                return Err(parse_error(field, item_start, 0));
            }
            Some(parse_number(
                bytes,
                item_start + slash + 1,
                item_end,
                field,
                true,
            )?)
        } else {
            None
        };

        if base_end == 0 {
            return Err(parse_error(field, item_start, 0));
        }

        let base = &item[..base_end];
        let (start, end, range_end_offset) = if base == b"*" {
            (min, max, item_start)
        } else {
            let hyphens: Vec<usize> = base
                .iter()
                .enumerate()
                .filter_map(|(i, &b)| (b == b'-').then_some(i))
                .collect();

            if hyphens.len() > 1 {
                return Err(parse_error(field, item_start, 0));
            }
            if let Some(&hyphen) = hyphens.first() {
                if hyphen == 0 || hyphen + 1 == base.len() {
                    return Err(parse_error(field, item_start, 0));
                }
                let a = parse_number(
                    bytes,
                    item_start,
                    item_start + hyphen,
                    field,
                    false,
                )?;
                let b = parse_number(
                    bytes,
                    item_start + hyphen + 1,
                    item_start + base_end,
                    field,
                    false,
                )?;
                (a, b, item_start + hyphen + 1)
            } else {
                let a = parse_number(bytes, item_start, item_start + base_end, field, false)?;
                let b = if step.is_some() { max } else { a };
                (a, b, item_start)
            }
        };

        if base != b"*" && base.iter().any(|&b| !b.is_ascii_digit() && b != b'-') {
            return Err(parse_error(field, item_start, 0));
        }

        if start > end {
            return Err(ParseError::ReversedRange {
                field,
                offset: range_end_offset,
            });
        }

        let increment = step.unwrap_or(1);
        let mut value = start;
        while value <= end {
            selected[value as usize] = true;
            match value.checked_add(increment) {
                Some(next) if next > value => value = next,
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
    let fields: Vec<&str> = input.split(' ').collect();
    if fields.len() != 5 {
        return Err(ParseError::FieldCount {
            found: fields.len(),
        });
    }

    let names = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];
    let mut parsed = Vec::with_capacity(5);
    for (text, field) in fields.iter().zip(names) {
        parsed.push(parse_field(text, field)?);
    }

    let selected: [Vec<bool>; 5] = parsed
        .try_into()
        .expect("exactly five fields were parsed");

    Ok(Cron {
        selected,
        day_of_month_wildcard: fields[2].as_bytes() == b"*",
        day_of_week_wildcard: fields[4].as_bytes() == b"*",
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let index = field_index(field);
        self.selected[index]
            .iter()
            .enumerate()
            .filter_map(|(value, &is_selected)| is_selected.then_some(value as u8))
            .collect()
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

fn is_leap_year(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn month_length(year: u16, month: u8) -> u8 {
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

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days = 0u32;
    for y in 2000..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += month_length(year, m) as u32;
    }
    days += (day - 1) as u32;
    ((6 + days) % 7) as u8
}

fn selected_at(cron: &Cron, field: Field, value: u8) -> bool {
    cron.selected[field_index(field)]
        .get(value as usize)
        .copied()
        .unwrap_or(false)
}

fn advance_minute(at: CivilDateTime) -> Option<CivilDateTime> {
    if at.minute < 59 {
        return Some(CivilDateTime {
            minute: at.minute + 1,
            ..at
        });
    }
    if at.hour < 23 {
        return Some(CivilDateTime {
            hour: at.hour + 1,
            minute: 0,
            ..at
        });
    }
    if at.day < month_length(at.year, at.month) {
        return Some(CivilDateTime {
            day: at.day + 1,
            hour: 0,
            minute: 0,
            ..at
        });
    }
    if at.month < 12 {
        return Some(CivilDateTime {
            month: at.month + 1,
            day: 1,
            hour: 0,
            minute: 0,
            ..at
        });
    }
    if at.year < 2399 {
        return Some(CivilDateTime {
            year: at.year + 1,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
        });
    }
    None
}

impl Cron {
    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        let dow = weekday(at.year, at.month, at.day);
        Ok(selected_at(self, Field::Minute, at.minute)
            && selected_at(self, Field::Hour, at.hour)
            && selected_at(self, Field::DayOfMonth, at.day)
            && selected_at(self, Field::Month, at.month)
            && selected_at(self, Field::DayOfWeek, dow))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;
        let mut candidate = match advance_minute(after) {
            Some(value) => value,
            None => return Ok(None),
        };

        loop {
            if self.matches(candidate)? {
                return Ok(Some(candidate));
            }
            candidate = match advance_minute(candidate) {
                Some(value) => value,
                None => return Ok(None),
            };
        }
    }
}

fn main() {}