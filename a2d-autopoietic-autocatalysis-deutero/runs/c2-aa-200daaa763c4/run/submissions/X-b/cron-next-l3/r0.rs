use std::str;

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
    selected: [Vec<u8>; 5],
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
struct FieldSpec {
    field: Field,
    min: u8,
    max: u8,
}

const SPECS: [FieldSpec; 5] = [
    FieldSpec { field: Field::Minute, min: 0, max: 59 },
    FieldSpec { field: Field::Hour, min: 0, max: 23 },
    FieldSpec { field: Field::DayOfMonth, min: 1, max: 31 },
    FieldSpec { field: Field::Month, min: 1, max: 12 },
    FieldSpec { field: Field::DayOfWeek, min: 0, max: 6 },
];

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

fn name_value(field: Field, bytes: &[u8]) -> Option<u8> {
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

fn is_digits(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit)
}

fn parse_number(
    bytes: &[u8],
    offset: usize,
    field: Field,
    min: u8,
    max: u8,
    step: bool,
) -> Result<u8, ParseError> {
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(parse_error(field, offset, 1));
    }
    let mut value: u32 = 0;
    for &b in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or_else(|| parse_error(field, offset, 2))?;
    }
    if step {
        if value < 1 || value > (max - min + 1) as u32 {
            return Err(parse_error(field, offset, 5));
        }
    } else if value < min as u32 || value > max as u32 {
        return Err(parse_error(field, offset, 3));
    }
    Ok(value as u8)
}

fn parse_base_value(
    bytes: &[u8],
    offset: usize,
    field: Field,
    min: u8,
    max: u8,
) -> Result<u8, ParseError> {
    if is_digits(bytes) {
        parse_number(bytes, offset, field, min, max, false)
    } else if let Some(value) = name_value(field, bytes) {
        Ok(value)
    } else {
        Err(parse_error(field, offset, 0))
    }
}

fn parse_field(
    text: &str,
    spec: FieldSpec,
) -> Result<(Vec<u8>, bool), ParseError> {
    let bytes = text.as_bytes();
    for (offset, &b) in bytes.iter().enumerate() {
        if !(b.is_ascii_digit()
            || b.is_ascii_alphabetic()
            || matches!(b, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field: spec.field, offset });
        }
    }

    let unrestricted_day =
        (spec.field == Field::DayOfMonth || spec.field == Field::DayOfWeek) && bytes == b"*";
    let mut selected = vec![false; (spec.max - spec.min + 1) as usize];

    let mut item_start = 0usize;
    for item in text.split(',') {
        let item_bytes = item.as_bytes();
        if item_bytes.is_empty() {
            return Err(ParseError::EmptyItem { field: spec.field, offset: item_start });
        }

        let slash_positions: Vec<usize> = item_bytes
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'/').then_some(i))
            .collect();
        if slash_positions.len() > 1 {
            return Err(parse_error(spec.field, item_start, 0));
        }

        let (base, step_part) = if let Some(&slash) = slash_positions.first() {
            (&item_bytes[..slash], Some((slash + 1, &item_bytes[slash + 1..])))
        } else {
            (item_bytes, None)
        };

        if base.is_empty() || step_part.is_some_and(|(_, s)| s.is_empty()) {
            return Err(parse_error(spec.field, item_start, 0));
        }

        let mut hyphens = Vec::new();
        for (i, &b) in base.iter().enumerate() {
            if b == b'-' {
                hyphens.push(i);
            }
        }
        if hyphens.len() > 1 {
            return Err(parse_error(spec.field, item_start, 0));
        }

        let is_star = base == b"*";
        let (start_bytes, end_bytes, start_offset, end_offset) = if is_star {
            (None, None, item_start, item_start)
        } else if let Some(&hyphen) = hyphens.first() {
            if hyphen == 0 || hyphen + 1 == base.len() {
                return Err(parse_error(spec.field, item_start, 0));
            }
            let left = &base[..hyphen];
            let right = &base[hyphen + 1..];
            if left == b"*" || right == b"*" {
                return Err(parse_error(spec.field, item_start, 0));
            }
            (
                Some(left),
                Some(right),
                item_start,
                item_start + hyphen + 1,
            )
        } else {
            (Some(base), None, item_start, item_start)
        };

        if !is_star {
            if !start_bytes.unwrap().iter().all(|b| b.is_ascii_digit() || b.is_ascii_alphabetic())
                || end_bytes.is_some_and(|s| !s.iter().all(|b| b.is_ascii_digit() || b.is_ascii_alphabetic()))
            {
                return Err(parse_error(spec.field, item_start, 0));
            }
            let valid_atom = |atom: &[u8]| is_digits(atom) || name_value(spec.field, atom).is_some();
            if !valid_atom(start_bytes.unwrap())
                || end_bytes.is_some_and(|s| !valid_atom(s))
            {
                return Err(parse_error(spec.field, item_start, 0));
            }
        }

        let mut step = None;
        if let Some((step_offset, step_bytes)) = step_part {
            if !is_digits(step_bytes) {
                return Err(parse_error(spec.field, item_start, 0));
            }
            step = Some(parse_number(
                step_bytes,
                item_start + step_offset,
                spec.field,
                spec.min,
                spec.max,
                true,
            )?);
        }

        let (start, end) = if is_star {
            (spec.min, spec.max)
        } else {
            let start = parse_base_value(start_bytes.unwrap(), start_offset, spec.field, spec.min, spec.max)?;
            let end = if let Some(bytes) = end_bytes {
                parse_base_value(bytes, end_offset, spec.field, spec.min, spec.max)?
            } else if step.is_some() {
                spec.max
            } else {
                start
            };
            if end_bytes.is_some() && start > end {
                return Err(ParseError::ReversedRange { field: spec.field, offset: end_offset });
            }
            (start, end)
        };

        let stride = step.unwrap_or(1);
        let mut value = start;
        while value <= end {
            selected[(value - spec.min) as usize] = true;
            match value.checked_add(stride) {
                Some(next) if next > value => value = next,
                _ => break,
            }
        }

        item_start += item_bytes.len() + 1;
    }

    Ok((
        selected
            .iter()
            .enumerate()
            .filter_map(|(i, &yes)| yes.then_some(spec.min + i as u8))
            .collect(),
        unrestricted_day,
    ))
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut selected: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());
    let mut unrestricted_days = [false; 2];
    for (index, (part, spec)) in parts.iter().zip(SPECS.iter()).enumerate() {
        let (values, unrestricted) = parse_field(part, *spec)?;
        selected[index] = values;
        if index == 2 {
            unrestricted_days[0] = unrestricted;
        } else if index == 4 {
            unrestricted_days[1] = unrestricted;
        }
    }
    Ok(Cron { selected, unrestricted_days })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.selected[field_index(field)].clone()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        Ok(self.matches_valid(at))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;
        let mut current = after;
        loop {
            if let Some(next) = increment_minute(current) {
                current = next;
            } else {
                return Ok(None);
            }
            if self.matches_valid(current) {
                return Ok(Some(current));
            }
        }
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        if !self.selected[0].contains(&at.minute)
            || !self.selected[1].contains(&at.hour)
            || !self.selected[3].contains(&at.month)
        {
            return false;
        }
        let dom = self.selected[2].contains(&at.day);
        let dow = self.selected[4].contains(&weekday(at.year, at.month, at.day));
        match (self.unrestricted_days[0], self.unrestricted_days[1]) {
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
    let mut days: u32 = 0;
    for y in 2000..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as u32;
    }
    days += (day - 1) as u32;
    ((6 + days) % 7) as u8
}

fn increment_minute(at: CivilDateTime) -> Option<CivilDateTime> {
    if at.minute < 59 {
        return Some(CivilDateTime { minute: at.minute + 1, ..at });
    }
    if at.hour < 23 {
        return Some(CivilDateTime { minute: 0, hour: at.hour + 1, ..at });
    }
    if at.day < days_in_month(at.year, at.month) {
        return Some(CivilDateTime { minute: 0, hour: 0, day: at.day + 1, ..at });
    }
    if at.month < 12 {
        return Some(CivilDateTime { minute: 0, hour: 0, day: 1, month: at.month + 1, ..at });
    }
    if at.year < 2399 {
        return Some(CivilDateTime { minute: 0, hour: 0, day: 1, month: 1, year: at.year + 1 });
    }
    None
}