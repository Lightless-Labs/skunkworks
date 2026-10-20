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
    day_of_month_is_star: bool,
    day_of_week_is_star: bool,
}

fn domain(field: Field) -> (u8, u8) {
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

fn parse_number(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: Field,
    is_step: bool,
) -> Result<u32, ParseError> {
    if end - start > 1 && bytes[start] == b'0' {
        return Err(ParseError::LeadingZero {
            field,
            offset: start,
        });
    }

    let mut value: u32 = 0;
    for &b in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow {
                field,
                offset: start,
            })?;
    }

    if is_step {
        let (_, max) = domain(field);
        let width = max as u32 - domain(field).0 as u32 + 1;
        if value == 0 || value > width {
            return Err(ParseError::BadStep {
                field,
                offset: start,
            });
        }
    } else {
        let (min, max) = domain(field);
        if value < min as u32 || value > max as u32 {
            return Err(ParseError::OutOfRange {
                field,
                offset: start,
            });
        }
    }
    Ok(value)
}

fn parse_item(
    item: &[u8],
    item_offset: usize,
    field: Field,
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
        .filter_map(|(i, &b)| (b == b'/').then_some(i))
        .collect();
    let hyphen_positions: Vec<usize> = item
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'-').then_some(i))
        .collect();

    if slash_positions.len() > 1 || hyphen_positions.len() > 1 {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    }

    let (base_end, step_start) = if let Some(&slash) = slash_positions.first() {
        if slash == 0 || slash + 1 >= item.len() {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
        (slash, Some(slash + 1))
    } else {
        (item.len(), None)
    };

    let is_star = base_end == 1 && item[0] == b'*';
    let range = hyphen_positions.first().copied().filter(|&h| h < base_end);

    if !is_star {
        if base_end == 0 {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
        if let Some(h) = range {
            if h == 0 || h + 1 == base_end {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_offset,
                });
            }
            if item[..h].iter().any(|b| !b.is_ascii_digit())
                || item[h + 1..base_end].iter().any(|b| !b.is_ascii_digit())
            {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_offset,
                });
            }
        } else if item[..base_end].iter().any(|b| !b.is_ascii_digit()) {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
    } else if range.is_some() {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    }

    if let Some(step_start) = step_start {
        if item[step_start..].iter().any(|b| !b.is_ascii_digit()) {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
    }

    let step = if let Some(start) = step_start {
        Some(parse_number(item, start, item.len(), field, true)?)
    } else {
        None
    };

    let (min, max) = domain(field);
    let (start_value, end_value) = if is_star {
        (min as u32, max as u32)
    } else if let Some(h) = range {
        let a = parse_number(item, 0, h, field, false)?;
        let b = parse_number(item, h + 1, base_end, field, false)?;
        if a > b {
            return Err(ParseError::ReversedRange {
                field,
                offset: item_offset + h + 1,
            });
        }
        (a, b)
    } else {
        let n = parse_number(item, 0, base_end, field, false)?;
        (n, if step.is_some() { max as u32 } else { n })
    };

    let increment = step.unwrap_or(1);
    let mut value = start_value;
    while value <= end_value {
        selected[value as usize] = true;
        match value.checked_add(increment) {
            Some(next) => value = next,
            None => break,
        }
    }
    Ok(())
}

fn parse_field(text: &str, field: Field) -> Result<Vec<bool>, ParseError> {
    let (min, max) = domain(field);
    let mut selected = vec![false; max as usize + 1];
    let bytes = text.as_bytes();

    for (i, &b) in bytes.iter().enumerate() {
        if !(b.is_ascii_digit() || matches!(b, b'*' | b'-' | b'/' | b',')) {
            return Err(ParseError::InvalidCharacter { field, offset: i });
        }
    }

    let mut start = 0;
    for i in 0..=bytes.len() {
        if i == bytes.len() || bytes[i] == b',' {
            parse_item(&bytes[start..i], start, field, &mut selected)?;
            start = i + 1;
        }
    }

    for value in 0..min as usize {
        selected[value] = false;
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

    let kinds = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];
    let mut sets = Vec::with_capacity(5);
    for (text, field) in fields.iter().zip(kinds) {
        sets.push(parse_field(text, field)?);
    }

    Ok(Cron {
        sets: sets.try_into().ok().unwrap(),
        day_of_month_is_star: fields[2] == "*",
        day_of_week_is_star: fields[4] == "*",
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let set = &self.sets[field_index(field)];
        set.iter()
            .enumerate()
            .filter_map(|(value, &yes)| yes.then_some(value as u8))
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        let weekday = weekday(at.year, at.month, at.day);
        Ok(self.sets[0][at.minute as usize]
            && self.sets[1][at.hour as usize]
            && self.sets[2][at.day as usize]
            && self.sets[3][at.month as usize]
            && self.sets[4][weekday as usize])
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;
        let mut current = after;
        loop {
            if !increment_minute(&mut current) {
                return Ok(None);
            }
            if self.matches(current)? {
                return Ok(Some(current));
            }
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
        days += if is_leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as u32;
    }
    days += day as u32 - 1;
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