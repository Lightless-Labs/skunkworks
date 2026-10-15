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
    sets: [Vec<u8>; 5],
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

#[derive(Clone, Copy)]
struct Domain {
    min: u8,
    max: u8,
}

const FIELDS: [Field; 5] = [
    Field::Minute,
    Field::Hour,
    Field::DayOfMonth,
    Field::Month,
    Field::DayOfWeek,
];

fn domain(field: Field) -> Domain {
    match field {
        Field::Minute => Domain { min: 0, max: 59 },
        Field::Hour => Domain { min: 0, max: 23 },
        Field::DayOfMonth => Domain { min: 1, max: 31 },
        Field::Month => Domain { min: 1, max: 12 },
        Field::DayOfWeek => Domain { min: 0, max: 6 },
    }
}

fn parse_name(field: Field, bytes: &[u8]) -> Option<u8> {
    let names: &[&[u8]] = match field {
        Field::Month => &[
            b"JAN", b"FEB", b"MAR", b"APR", b"MAY", b"JUN",
            b"JUL", b"AUG", b"SEP", b"OCT", b"NOV", b"DEC",
        ],
        Field::DayOfWeek => &[
            b"SUN", b"MON", b"TUE", b"WED", b"THU", b"FRI", b"SAT",
        ],
        _ => return None,
    };

    names.iter().position(|name| {
        bytes.len() == name.len()
            && bytes
                .iter()
                .zip(name.iter())
                .all(|(a, b)| a.to_ascii_uppercase() == *b)
    }).map(|i| match field {
        Field::Month => (i + 1) as u8,
        Field::DayOfWeek => i as u8,
        _ => unreachable!(),
    })
}

fn is_digits(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit)
}

fn is_valid_atom(field: Field, bytes: &[u8]) -> bool {
    is_digits(bytes) || parse_name(field, bytes).is_some()
}

fn invalid_syntax(field: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field, offset }
}

fn parse_number(
    field: Field,
    bytes: &[u8],
    offset: usize,
    step: bool,
) -> Result<u32, ParseError> {
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }

    let mut value = 0u32;
    for &byte in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }

    let d = domain(field);
    if step {
        let width = (d.max - d.min + 1) as u32;
        if value == 0 || value > width {
            return Err(ParseError::BadStep { field, offset });
        }
    } else if value < d.min as u32 || value > d.max as u32 {
        return Err(ParseError::OutOfRange { field, offset });
    }

    Ok(value)
}

fn parse_endpoint(
    field: Field,
    bytes: &[u8],
    offset: usize,
) -> Result<u8, ParseError> {
    if is_digits(bytes) {
        Ok(parse_number(field, bytes, offset, false)? as u8)
    } else {
        parse_name(field, bytes).ok_or_else(|| invalid_syntax(field, offset))
    }
}

fn parse_field(field: Field, text: &str) -> Result<Vec<u8>, ParseError> {
    let bytes = text.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !(byte.is_ascii_digit()
            || byte.is_ascii_alphabetic()
            || matches!(byte, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let d = domain(field);
    let mut selected = vec![false; (d.max - d.min + 1) as usize];
    let mut item_start = 0;

    for item in text.split(',') {
        let item_len = item.len();
        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let item_bytes = item.as_bytes();
        let slash_positions: Vec<usize> = item_bytes
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'/').then_some(i))
            .collect();

        if slash_positions.len() > 1 {
            return Err(invalid_syntax(field, item_start));
        }

        let (base, step_part, step_offset) = if let Some(&slash) = slash_positions.first() {
            (&item_bytes[..slash], Some(&item_bytes[slash + 1..]), item_start + slash + 1)
        } else {
            (item_bytes, None, item_start)
        };

        let hyphen_positions: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();

        let is_star = base == b"*";
        let range = hyphen_positions.len() == 1;
        let syntax_ok = if is_star {
            true
        } else if range {
            let hyphen = hyphen_positions[0];
            is_valid_atom(field, &base[..hyphen])
                && is_valid_atom(field, &base[hyphen + 1..])
        } else {
            hyphen_positions.is_empty() && is_valid_atom(field, base)
        };

        if !syntax_ok || step_part.is_some_and(|s| s.is_empty() || !is_digits(s)) {
            return Err(invalid_syntax(field, item_start));
        }

        let mut start_value = d.min;
        let mut end_value = d.max;
        let mut start_number_offset = item_start;
        let mut end_number_offset = item_start;

        if !is_star {
            if range {
                let hyphen = hyphen_positions[0];
                let left = &base[..hyphen];
                let right = &base[hyphen + 1..];
                start_number_offset = item_start;
                end_number_offset = item_start + hyphen + 1;

                start_value = parse_endpoint(field, left, start_number_offset)?;
                end_value = parse_endpoint(field, right, end_number_offset)?;

                if start_value > end_value {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: end_number_offset,
                    });
                }
            } else {
                start_value = parse_endpoint(field, base, item_start)?;
                end_value = if step_part.is_some() { d.max } else { start_value };
            }
        }

        let step = if let Some(step_bytes) = step_part {
            Some(parse_number(field, step_bytes, step_offset, true)? as u8)
        } else {
            None
        };

        if let Some(step) = step {
            let mut value = start_value;
            while value <= end_value {
                selected[(value - d.min) as usize] = true;
                match value.checked_add(step) {
                    Some(next) if next > value => value = next,
                    _ => break,
                }
            }
        } else {
            for value in start_value..=end_value {
                selected[(value - d.min) as usize] = true;
            }
        }

        item_start += item_len + 1;
    }

    Ok(selected
        .iter()
        .enumerate()
        .filter_map(|(i, &yes)| yes.then_some(d.min + i as u8))
        .collect())
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let pieces: Vec<&str> = input.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount { found: pieces.len() });
    }

    let mut sets: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());
    for (i, field) in FIELDS.iter().copied().enumerate() {
        sets[i] = parse_field(field, pieces[i])?;
    }

    Ok(Cron {
        sets,
        unrestricted_days: [pieces[2] == "*", pieces[4] == "*"],
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.sets[field_index(field)].clone()
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

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        if !self.sets[0].contains(&at.minute)
            || !self.sets[1].contains(&at.hour)
            || !self.sets[3].contains(&at.month)
        {
            return false;
        }

        let dom = self.sets[2].contains(&at.day);
        let dow = self.sets[4].contains(&weekday(at.year, at.month, at.day));

        match self.unrestricted_days {
            [true, true] => true,
            [true, false] => dow,
            [false, true] => dom,
            [false, false] => {
                if at.day == days_in_month(at.year, at.month) {
                    dom && dow
                } else {
                    dom || dow
                }
            }
        }
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
    let mut days = 0u32;
    for y in 2000..year {
        days += if is_leap_year(y) { 366 } else { 365 };
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