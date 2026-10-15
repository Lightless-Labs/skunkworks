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
    allowed: [Vec<u8>; 5],
    day_of_month_is_star: bool,
    day_of_week_is_star: bool,
}

#[derive(Clone, Copy)]
struct Domain {
    min: u32,
    max: u32,
    width: u32,
}

fn field_info(field: Field) -> Domain {
    match field {
        Field::Minute => Domain { min: 0, max: 59, width: 60 },
        Field::Hour => Domain { min: 0, max: 23, width: 24 },
        Field::DayOfMonth => Domain { min: 1, max: 31, width: 31 },
        Field::Month => Domain { min: 1, max: 12, width: 12 },
        Field::DayOfWeek => Domain { min: 0, max: 6, width: 7 },
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

fn parse_number(bytes: &[u8], start: usize, field: Field) -> Result<u32, ParseError> {
    if bytes.is_empty() {
        return Err(ParseError::InvalidSyntax { field, offset: start });
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset: start });
    }

    let mut value = 0u32;
    for &byte in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset: start })?;
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
    domain: Domain,
    selected: &mut [bool],
) -> Result<(), ParseError> {
    if item.is_empty() {
        return Err(ParseError::EmptyItem { field, offset: item_offset });
    }

    // Validate overall structure before reading any number.
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
    let hyphen_positions: Vec<usize> = base
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'-').then_some(i))
        .collect();

    let range_parts = if is_star {
        if !hyphen_positions.is_empty() {
            return Err(invalid_syntax(field, item_offset));
        }
        None
    } else if hyphen_positions.is_empty() {
        if base.iter().any(|&b| !b.is_ascii_digit()) {
            return Err(invalid_syntax(field, item_offset));
        }
        Some((base, None, item_offset, 0usize))
    } else {
        if hyphen_positions.len() != 1 {
            return Err(invalid_syntax(field, item_offset));
        }
        let hyphen = hyphen_positions[0];
        if hyphen == 0 || hyphen + 1 == base.len() {
            return Err(invalid_syntax(field, item_offset));
        }
        let left = &base[..hyphen];
        let right = &base[hyphen + 1..];
        if left.iter().any(|&b| !b.is_ascii_digit())
            || right.iter().any(|&b| !b.is_ascii_digit())
        {
            return Err(invalid_syntax(field, item_offset));
        }
        Some((left, Some(right), item_offset, item_offset + hyphen + 1))
    };

    let step = if let Some(bytes) = step_bytes {
        if bytes.iter().any(|&b| !b.is_ascii_digit()) {
            return Err(invalid_syntax(field, item_offset));
        }
        let value = parse_number(bytes, step_offset, field)?;
        if value < 1 || value > domain.width {
            return Err(ParseError::BadStep { field, offset: step_offset });
        }
        Some(value)
    } else {
        None
    };

    let (start, end) = if is_star {
        (domain.min, domain.max)
    } else {
        let (start_bytes, end_bytes, start_offset, end_offset) = range_parts.unwrap();
        let start = parse_number(start_bytes, start_offset, field)?;
        if start < domain.min || start > domain.max {
            return Err(ParseError::OutOfRange { field, offset: start_offset });
        }
        if let Some(end_bytes) = end_bytes {
            let end = parse_number(end_bytes, end_offset, field)?;
            if end < domain.min || end > domain.max {
                return Err(ParseError::OutOfRange { field, offset: end_offset });
            }
            if start > end {
                return Err(ParseError::ReversedRange { field, offset: end_offset });
            }
            (start, end)
        } else if step.is_some() {
            (start, domain.max)
        } else {
            (start, start)
        }
    };

    let stride = step.unwrap_or(1);
    let mut value = start;
    while value <= end {
        selected[(value - domain.min) as usize] = true;
        match value.checked_add(stride) {
            Some(next) => value = next,
            None => break,
        }
    }
    Ok(())
}

fn parse_field(text: &str, field: Field) -> Result<Vec<u8>, ParseError> {
    let bytes = text.as_bytes();
    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let domain = field_info(field);
    let mut selected = vec![false; domain.width as usize];
    let mut offset = 0usize;
    for item in bytes.split(|&b| b == b',') {
        parse_item(item, offset, field, domain, &mut selected)?;
        offset += item.len() + 1;
    }

    Ok(selected
        .iter()
        .enumerate()
        .filter_map(|(i, &yes)| yes.then_some((domain.min as usize + i) as u8))
        .collect())
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let pieces: Vec<&str> = input.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount { found: pieces.len() });
    }

    let fields = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];
    let mut parsed: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());
    for (i, (&text, &field)) in pieces.iter().zip(fields.iter()).enumerate() {
        parsed[i] = parse_field(text, field)?;
    }

    Ok(Cron {
        allowed: parsed,
        day_of_month_is_star: pieces[2].as_bytes() == b"*",
        day_of_week_is_star: pieces[4].as_bytes() == b"*",
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.allowed[field_index(field)].clone()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        let weekday = weekday(at.year, at.month, at.day);
        Ok(self.allowed[0].contains(&at.minute)
            && self.allowed[1].contains(&at.hour)
            && self.allowed[2].contains(&at.day)
            && self.allowed[3].contains(&at.month)
            && self.allowed[4].contains(&weekday))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;
        let mut current = after;
        loop {
            if let Some(next) = advance_minute(current) {
                current = next;
            } else {
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

fn validate_datetime(at: CivilDateTime) -> Result<(), DateError> {
    if !(2000..=2399).contains(&at.year) {
        return Err(DateError::InvalidDateTime { component: DatePart::Year });
    }
    if !(1..=12).contains(&at.month) {
        return Err(DateError::InvalidDateTime { component: DatePart::Month });
    }
    if at.day < 1 || at.day > days_in_month(at.year, at.month) {
        return Err(DateError::InvalidDateTime { component: DatePart::Day });
    }
    if at.hour > 23 {
        return Err(DateError::InvalidDateTime { component: DatePart::Hour });
    }
    if at.minute > 59 {
        return Err(DateError::InvalidDateTime { component: DatePart::Minute });
    }
    Ok(())
}

fn is_leap_year(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 => if is_leap_year(year) { 29 } else { 28 },
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days = 0u32;
    for y in 2000..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as u32;
    }
    days += (day - 1) as u32;
    ((6 + days) % 7) as u8
}

fn advance_minute(at: CivilDateTime) -> Option<CivilDateTime> {
    let mut next = at;
    if next.minute < 59 {
        next.minute += 1;
        return Some(next);
    }
    next.minute = 0;
    if next.hour < 23 {
        next.hour += 1;
        return Some(next);
    }
    next.hour = 0;
    if next.day < days_in_month(next.year, next.month) {
        next.day += 1;
        return Some(next);
    }
    next.day = 1;
    if next.month < 12 {
        next.month += 1;
        return Some(next);
    }
    next.month = 1;
    if next.year < 2399 {
        next.year += 1;
        return Some(next);
    }
    None
}

fn main() {}