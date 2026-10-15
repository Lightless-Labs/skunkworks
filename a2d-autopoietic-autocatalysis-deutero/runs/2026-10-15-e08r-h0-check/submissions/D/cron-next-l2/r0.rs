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
    day_of_month_is_star: bool,
    day_of_week_is_star: bool,
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

fn parse_number(bytes: &[u8], start: usize, field: Field, is_step: bool) -> Result<u32, ParseError> {
    let err = |kind: fn(Field, usize) -> ParseError| kind(field, start);
    if bytes.is_empty() {
        return Err(err(if is_step {
            |field, offset| ParseError::BadStep { field, offset }
        } else {
            |field, offset| ParseError::InvalidSyntax { field, offset }
        }));
    }

    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset: start });
    }

    let mut value = 0u32;
    for &b in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset: start })?;
    }

    if is_step {
        let (min, max) = domain(field);
        let width = max - min + 1;
        if value == 0 || value > width {
            return Err(ParseError::BadStep { field, offset: start });
        }
    } else {
        let (min, max) = domain(field);
        if value < min || value > max {
            return Err(ParseError::OutOfRange { field, offset: start });
        }
    }
    Ok(value)
}

fn invalid_syntax(field: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field, offset }
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
    if slash_positions.len() > 1 {
        return Err(invalid_syntax(field, item_offset));
    }

    let (base, step_bytes, step_offset) = if let Some(&slash) = slash_positions.first() {
        if slash == 0 || slash + 1 == item.len() {
            return Err(invalid_syntax(field, item_offset));
        }
        (&item[..slash], Some(&item[slash + 1..]), item_offset + slash + 1)
    } else {
        (item, None, 0)
    };

    let is_star = base == b"*";
    let hyphens: Vec<usize> = base
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'-').then_some(i))
        .collect();

    if !is_star {
        if hyphens.len() > 1 {
            return Err(invalid_syntax(field, item_offset));
        }
        if hyphens.len() == 1 {
            let h = hyphens[0];
            if h == 0 || h + 1 == base.len() {
                return Err(invalid_syntax(field, item_offset));
            }
        }
    }

    if is_star {
        // Valid.
    } else if hyphens.is_empty() {
        if base.iter().any(|&b| !b.is_ascii_digit()) {
            return Err(invalid_syntax(field, item_offset));
        }
    } else {
        let h = hyphens[0];
        if base[..h].iter().any(|&b| !b.is_ascii_digit())
            || base[h + 1..].iter().any(|&b| !b.is_ascii_digit())
        {
            return Err(invalid_syntax(field, item_offset));
        }
    }

    let step = if let Some(bytes) = step_bytes {
        if bytes.iter().any(|&b| !b.is_ascii_digit()) {
            return Err(invalid_syntax(field, item_offset));
        }
        Some(parse_number(bytes, step_offset, field, true)?)
    } else {
        None
    };

    let (min, max) = domain(field);
    let (start, end, single_unstepped) = if is_star {
        (min, max, false)
    } else if hyphens.is_empty() {
        let value = parse_number(base, item_offset, field, false)?;
        if step.is_some() {
            (value, max, false)
        } else {
            (value, value, true)
        }
    } else {
        let h = hyphens[0];
        let start = parse_number(&base[..h], item_offset, field, false)?;
        let end_offset = item_offset + h + 1;
        let end = parse_number(&base[h + 1..], end_offset, field, false)?;
        if start > end {
            return Err(ParseError::ReversedRange {
                field,
                offset: end_offset,
            });
        }
        (start, end, false)
    };

    if single_unstepped {
        selected[start as usize] = true;
    } else if let Some(step) = step {
        let mut value = start;
        while value <= end {
            selected[value as usize] = true;
            value += step;
        }
    } else {
        for value in start..=end {
            selected[value as usize] = true;
        }
    }
    Ok(())
}

fn parse_field(text: &str, field: Field) -> Result<Vec<bool>, ParseError> {
    let bytes = text.as_bytes();
    for (offset, &b) in bytes.iter().enumerate() {
        if !(b.is_ascii_digit() || matches!(b, b'*' | b'-' | b'/' | b',')) {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (_, max) = domain(field);
    let mut selected = vec![false; max as usize + 1];
    let mut item_start = 0usize;
    for i in 0..=bytes.len() {
        if i == bytes.len() || bytes[i] == b',' {
            parse_item(&bytes[item_start..i], item_start, field, &mut selected)?;
            item_start = i + 1;
        }
    }
    Ok(selected)
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut parsed = Vec::with_capacity(5);
    for (text, field) in parts.iter().zip(FIELDS) {
        parsed.push(parse_field(text, field)?);
    }

    Ok(Cron {
        selected: parsed.try_into().ok().unwrap(),
        day_of_month_is_star: parts[2].as_bytes() == b"*",
        day_of_week_is_star: parts[4].as_bytes() == b"*",
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

        let mut current = after;
        loop {
            if current.year == 2399 && current.month == 12 && current.day == 31
                && current.hour == 23 && current.minute == 59
            {
                return Ok(None);
            }
            current = next_minute(current);
            if self.matches(current)? {
                return Ok(Some(current));
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
    days += (day - 1) as u32;
    ((6 + days) % 7) as u8
}

fn next_minute(mut at: CivilDateTime) -> CivilDateTime {
    if at.minute < 59 {
        at.minute += 1;
        return at;
    }
    at.minute = 0;
    if at.hour < 23 {
        at.hour += 1;
        return at;
    }
    at.hour = 0;
    if at.day < days_in_month(at.year, at.month) {
        at.day += 1;
        return at;
    }
    at.day = 1;
    if at.month < 12 {
        at.month += 1;
        return at;
    }
    at.month = 1;
    at.year += 1;
    at
}