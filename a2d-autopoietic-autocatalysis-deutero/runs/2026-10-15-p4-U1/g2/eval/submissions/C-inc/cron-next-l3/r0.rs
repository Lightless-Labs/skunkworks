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

fn field_index(field: Field) -> usize {
    match field {
        Field::Minute => 0,
        Field::Hour => 1,
        Field::DayOfMonth => 2,
        Field::Month => 3,
        Field::DayOfWeek => 4,
    }
}

fn field_bounds(field: Field) -> (u8, u8) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
    }
}

fn field_width(field: Field) -> u8 {
    let (min, max) = field_bounds(field);
    max - min + 1
}

fn parse_name(field: Field, bytes: &[u8]) -> Option<u8> {
    let names: &[(&[u8], u8)] = match field {
        Field::Month => &[
            (b"JAN", 1), (b"FEB", 2), (b"MAR", 3), (b"APR", 4),
            (b"MAY", 5), (b"JUN", 6), (b"JUL", 7), (b"AUG", 8),
            (b"SEP", 9), (b"OCT", 10), (b"NOV", 11), (b"DEC", 12),
        ],
        Field::DayOfWeek => &[
            (b"SUN", 0), (b"MON", 1), (b"TUE", 2), (b"WED", 3),
            (b"THU", 4), (b"FRI", 5), (b"SAT", 6),
        ],
        _ => return None,
    };

    names.iter().find_map(|(name, value)| {
        if bytes.len() == name.len()
            && bytes.iter().zip(name.iter()).all(|(a, b)| a.to_ascii_uppercase() == *b)
        {
            Some(*value)
        } else {
            None
        }
    })
}

fn syntax_base(field: Field, bytes: &[u8]) -> bool {
    if bytes == b"*" {
        return true;
    }

    let mut parts = bytes.split(|&b| b == b'-');
    let first = parts.next().unwrap_or_default();
    let second = parts.next();
    if parts.next().is_some() || first.is_empty() {
        return false;
    }

    let valid_atom = |atom: &[u8]| {
        atom.iter().all(u8::is_ascii_digit)
            || parse_name(field, atom).is_some()
    };

    match second {
        None => valid_atom(first),
        Some(end) => !end.is_empty() && valid_atom(first) && valid_atom(end),
    }
}

fn number_or_name(
    field: Field,
    bytes: &[u8],
    offset: usize,
    step: bool,
) -> Result<u8, ParseError> {
    if !step {
        if let Some(value) = parse_name(field, bytes) {
            return Ok(value);
        }
    }

    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(ParseError::InvalidSyntax { field, offset });
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }

    let mut value: u32 = 0;
    for &byte in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }

    if step {
        if value == 0 || value > field_width(field) as u32 {
            return Err(ParseError::BadStep { field, offset });
        }
    } else {
        let (min, max) = field_bounds(field);
        if value < min as u32 || value > max as u32 {
            return Err(ParseError::OutOfRange { field, offset });
        }
    }

    Ok(value as u8)
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

    let mut slash = item.split(|&b| b == b'/');
    let base = slash.next().unwrap_or_default();
    let step_bytes = slash.next();
    if slash.next().is_some() || base.is_empty() || step_bytes.is_some_and(|s| s.is_empty()) {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    }

    if !syntax_base(field, base) {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    }

    let (min, max) = field_bounds(field);
    let mut range_parts = base.split(|&b| b == b'-');
    let first = range_parts.next().unwrap_or_default();
    let second = range_parts.next();

    let (start, end, start_offset, end_offset) = if base == b"*" {
        (min, max, item_offset, item_offset)
    } else {
        let first_offset = item_offset;
        let start = number_or_name(field, first, first_offset, false)?;
        let second_offset = second.map(|_| first_offset + first.len() + 1);
        let end = match (second, second_offset) {
            (Some(bytes), Some(offset)) => number_or_name(field, bytes, offset, false)?,
            _ => start,
        };
        if second.is_some() && start > end {
            return Err(ParseError::ReversedRange {
                field,
                offset: second_offset.unwrap(),
            });
        }
        (start, end, first_offset, second_offset.unwrap_or(first_offset))
    };

    let step = match step_bytes {
        Some(bytes) => {
            let offset = item_offset + base.len() + 1;
            Some(number_or_name(field, bytes, offset, true)?)
        }
        None => None,
    };

    let selection_end = if step.is_some() && second.is_none() && base != b"*" {
        max
    } else {
        end
    };

    if let Some(step) = step {
        let mut value = start;
        while value <= selection_end {
            selected[value as usize] = true;
            match value.checked_add(step) {
                Some(next) if next > value => value = next,
                _ => break,
            }
        }
    } else {
        let mut value = start;
        while value <= end {
            selected[value as usize] = true;
            value += 1;
        }
    }

    let _ = (start_offset, end_offset);
    Ok(())
}

fn parse_field(field: Field, text: &str) -> Result<Vec<bool>, ParseError> {
    let bytes = text.as_bytes();
    for (offset, &byte) in bytes.iter().enumerate() {
        if !(byte.is_ascii_digit()
            || byte.is_ascii_alphabetic()
            || matches!(byte, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let mut selected = vec![false; 256];
    let mut offset = 0;
    for item in bytes.split(|&b| b == b',') {
        parse_item(field, item, offset, &mut selected)?;
        offset += item.len() + 1;
    }
    Ok(selected)
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let fields: Vec<&str> = input.split(' ').collect();
    if fields.len() != 5 {
        return Err(ParseError::FieldCount { found: fields.len() });
    }

    let field_kinds = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];
    let mut parsed = Vec::with_capacity(5);
    for (field, text) in field_kinds.into_iter().zip(fields.iter()) {
        parsed.push(parse_field(field, text)?);
    }

    let mut selected: [Vec<bool>; 5] = std::array::from_fn(|_| Vec::new());
    for (index, mut values) in parsed.into_iter().enumerate() {
        let (min, max) = field_bounds(field_kinds[index]);
        values.truncate(max as usize + 1);
        if min > 0 && values.len() <= max as usize {
            values.resize(max as usize + 1, false);
        }
        selected[index] = values;
    }

    Ok(Cron {
        selected,
        day_of_month_unrestricted: fields[2] == "*",
        day_of_week_unrestricted: fields[4] == "*",
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let (min, max) = field_bounds(field);
        (min..=max)
            .filter(|&value| self.selected[field_index(field)].get(value as usize).copied().unwrap_or(false))
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
            current = advance_minute(current);
            if current.year > 2399 {
                return Ok(None);
            }
            if self.matches_valid(current) {
                return Ok(Some(current));
            }
        }
    }

    fn has(&self, field: Field, value: u8) -> bool {
        self.selected[field_index(field)]
            .get(value as usize)
            .copied()
            .unwrap_or(false)
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        if !self.has(Field::Minute, at.minute)
            || !self.has(Field::Hour, at.hour)
            || !self.has(Field::Month, at.month)
        {
            return false;
        }

        let dom = self.has(Field::DayOfMonth, at.day);
        let dow = self.has(Field::DayOfWeek, weekday(at.year, at.month, at.day));
        match (self.day_of_month_unrestricted, self.day_of_week_unrestricted) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) if at.day == days_in_month(at.year, at.month) => dom && dow,
            (false, false) => dom || dow,
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
        return Err(DateError::InvalidDateTime { component: DatePart::Year });
    }
    if !(1..=12).contains(&at.month) {
        return Err(DateError::InvalidDateTime { component: DatePart::Month });
    }
    if at.day == 0 || at.day > days_in_month(at.year, at.month) {
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

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days: u32 = 0;
    for y in 2000..year {
        days += if leap_year(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as u32;
    }
    days += (day - 1) as u32;
    ((days + 6) % 7) as u8
}

fn advance_minute(mut at: CivilDateTime) -> CivilDateTime {
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

fn main() {}