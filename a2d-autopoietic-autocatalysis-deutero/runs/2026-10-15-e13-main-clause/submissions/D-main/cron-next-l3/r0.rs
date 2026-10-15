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
    unrestricted_day_of_month: bool,
    unrestricted_day_of_week: bool,
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

fn field_index(field: Field) -> usize {
    match field {
        Field::Minute => 0,
        Field::Hour => 1,
        Field::DayOfMonth => 2,
        Field::Month => 3,
        Field::DayOfWeek => 4,
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

fn name_value(field: Field, bytes: &[u8]) -> Option<u32> {
    let names: &[&str] = match field {
        Field::Month => &[
            "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV",
            "DEC",
        ],
        Field::DayOfWeek => &["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"],
        _ => return None,
    };

    if bytes.len() != 3 {
        return None;
    }

    let upper = [
        bytes[0].to_ascii_uppercase(),
        bytes[1].to_ascii_uppercase(),
        bytes[2].to_ascii_uppercase(),
    ];
    names
        .iter()
        .position(|name| name.as_bytes() == upper)
        .map(|index| {
            if field == Field::Month {
                index as u32 + 1
            } else {
                index as u32
            }
        })
}

fn atom_is_valid(field: Field, atom: &[u8]) -> bool {
    if atom.is_empty() {
        return false;
    }
    if atom.iter().all(u8::is_ascii_digit) {
        return true;
    }
    name_value(field, atom).is_some()
}

fn parse_number(
    field: Field,
    bytes: &[u8],
    offset: usize,
    is_step: bool,
) -> Result<u32, ParseError> {
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(parse_error(field, offset, 1));
    }

    let mut value = 0u32;
    for &byte in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or_else(|| parse_error(field, offset, 2))?;
    }

    if is_step {
        let (min, max) = domain(field);
        let width = max - min + 1;
        if value == 0 || value > width {
            return Err(parse_error(field, offset, 5));
        }
    } else {
        let (min, max) = domain(field);
        if value < min || value > max {
            return Err(parse_error(field, offset, 3));
        }
    }
    Ok(value)
}

fn parse_endpoint(field: Field, bytes: &[u8], offset: usize) -> Result<u32, ParseError> {
    if bytes.iter().all(u8::is_ascii_digit) {
        parse_number(field, bytes, offset, false)
    } else {
        name_value(field, bytes).ok_or_else(|| parse_error(field, offset, 0))
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
        .filter_map(|(i, &b)| (b == b'/').then_some(i))
        .collect();
    if slash_positions.len() > 1 {
        return Err(parse_error(field, item_offset, 0));
    }

    let (base, step_part, step_offset) = if let Some(&slash) = slash_positions.first() {
        (&item[..slash], Some(&item[slash + 1..]), item_offset + slash + 1)
    } else {
        (item, None, item_offset)
    };

    let (is_star, start_atom, end_atom, end_offset) = if base == b"*" {
        (true, None, None, item_offset)
    } else {
        let hyphens: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();
        if hyphens.len() > 1 {
            return Err(parse_error(field, item_offset, 0));
        }
        if let Some(&hyphen) = hyphens.first() {
            let left = &base[..hyphen];
            let right = &base[hyphen + 1..];
            if !atom_is_valid(field, left) || !atom_is_valid(field, right) {
                return Err(parse_error(field, item_offset, 0));
            }
            (
                false,
                Some(left),
                Some(right),
                item_offset + hyphen + 1,
            )
        } else {
            if !atom_is_valid(field, base) {
                return Err(parse_error(field, item_offset, 0));
            }
            (false, Some(base), None, item_offset)
        }
    };

    if let Some(step) = step_part {
        if step.is_empty() || !step.iter().all(u8::is_ascii_digit) {
            return Err(parse_error(field, item_offset, 0));
        }
    }

    let mut start = None;
    let mut end = None;

    if !is_star {
        start = Some(parse_endpoint(field, start_atom.unwrap(), item_offset)?);
        if let Some(atom) = end_atom {
            end = Some(parse_endpoint(field, atom, end_offset)?);
            if start.unwrap() > end.unwrap() {
                return Err(parse_error(field, end_offset, 4));
            }
        }
    }

    let step = if let Some(step_bytes) = step_part {
        Some(parse_number(field, step_bytes, step_offset, true)?)
    } else {
        None
    };

    let (domain_min, domain_max) = domain(field);
    let (range_start, range_end) = if is_star {
        (domain_min, domain_max)
    } else if let Some(end_value) = end {
        (start.unwrap(), end_value)
    } else if step.is_some() {
        (start.unwrap(), domain_max)
    } else {
        (start.unwrap(), start.unwrap())
    };
    let step_value = step.unwrap_or(1);

    let mut value = range_start;
    while value <= range_end {
        selected[(value - domain_min) as usize] = true;
        match value.checked_add(step_value) {
            Some(next) if next > value => value = next,
            _ => break,
        }
    }
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

    let (min, max) = domain(field);
    let mut selected = vec![false; (max - min + 1) as usize];
    let mut start = 0;
    for index in 0..=bytes.len() {
        if index == bytes.len() || bytes[index] == b',' {
            parse_item(field, &bytes[start..index], start, &mut selected)?;
            start = index + 1;
        }
    }
    Ok(selected)
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let pieces: Vec<&str> = input.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount {
            found: pieces.len(),
        });
    }

    let mut parsed: [Vec<bool>; 5] = std::array::from_fn(|_| Vec::new());
    for (index, field) in FIELDS.iter().copied().enumerate() {
        parsed[index] = parse_field(field, pieces[index])?;
    }

    Ok(Cron {
        selected: parsed,
        unrestricted_day_of_month: pieces[2] == "*",
        unrestricted_day_of_week: pieces[4] == "*",
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let (min, _) = domain(field);
        self.selected[field_index(field)]
            .iter()
            .enumerate()
            .filter_map(|(index, &is_selected)| {
                is_selected.then_some((min as usize + index) as u8)
            })
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        Ok(self.matches_valid(at))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;

        let mut candidate = after;
        loop {
            if !increment_minute(&mut candidate) {
                return Ok(None);
            }
            if self.matches_valid(candidate) {
                return Ok(Some(candidate));
            }
        }
    }

    fn is_selected(&self, field: Field, value: u8) -> bool {
        let (min, _) = domain(field);
        self.selected[field_index(field)][(value as u32 - min) as usize]
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        if !self.is_selected(Field::Minute, at.minute)
            || !self.is_selected(Field::Hour, at.hour)
            || !self.is_selected(Field::Month, at.month)
        {
            return false;
        }

        let dom_unrestricted = self.unrestricted_day_of_month;
        let dow_unrestricted = self.unrestricted_day_of_week;
        let dom_match = self.is_selected(Field::DayOfMonth, at.day);
        let weekday = weekday(at.year, at.month, at.day);
        let dow_match = self.is_selected(Field::DayOfWeek, weekday);
        let last_day = at.day == days_in_month(at.year, at.month);

        match (dom_unrestricted, dow_unrestricted) {
            (true, true) => true,
            (true, false) => dow_match,
            (false, true) => dom_match,
            (false, false) if last_day => dom_match && dow_match,
            (false, false) => dom_match || dow_match,
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

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days = 0u32;
    for y in 2000..year {
        days += if is_leap_year(y) { 366 } else { 365 };
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

fn main() {}