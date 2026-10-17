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

fn bounds(field: Field) -> (u32, u32, usize) {
    match field {
        Field::Minute => (0, 59, 60),
        Field::Hour => (0, 23, 24),
        Field::DayOfMonth => (1, 31, 31),
        Field::Month => (1, 12, 12),
        Field::DayOfWeek => (0, 6, 7),
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

fn name_value(field: Field, bytes: &[u8]) -> Option<u32> {
    let names: &[(&[u8], u32)] = match field {
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
        if bytes.len() != name.len() {
            return None;
        }
        bytes
            .iter()
            .zip(name.iter())
            .all(|(a, b)| a.to_ascii_uppercase() == *b)
            .then_some(*value)
    })
}

fn atom_syntax_ok(field: Field, bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    if bytes.iter().all(u8::is_ascii_digit) {
        return true;
    }
    name_value(field, bytes).is_some()
}

fn parse_number(field: Field, bytes: &[u8], offset: usize) -> Result<u32, ParseError> {
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
    Ok(value)
}

fn endpoint_value(
    field: Field,
    bytes: &[u8],
    offset: usize,
) -> Result<u32, ParseError> {
    if bytes.iter().all(u8::is_ascii_digit) {
        let value = parse_number(field, bytes, offset)?;
        let (min, max, _) = bounds(field);
        if value < min || value > max {
            return Err(ParseError::OutOfRange { field, offset });
        }
        Ok(value)
    } else {
        name_value(field, bytes)
            .ok_or(ParseError::InvalidSyntax { field, offset })
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

    let slashes: Vec<usize> = item
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'/').then_some(i))
        .collect();

    let (base, step_part, step_offset) = match slashes.as_slice() {
        [] => (item, None, 0),
        [slash] => (&item[..*slash], Some(&item[*slash + 1..]), slash + 1),
        _ => {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
    };

    let hyphens: Vec<usize> = base
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'-').then_some(i))
        .collect();

    let base_shape_valid = match hyphens.as_slice() {
        [] => base == b"*" || atom_syntax_ok(field, base),
        [hyphen] => {
            base[0..*hyphen] != *b"*"
                && base[*hyphen + 1..] != *b"*"
                && atom_syntax_ok(field, &base[..*hyphen])
                && atom_syntax_ok(field, &base[*hyphen + 1..])
        }
        _ => false,
    };

    if !base_shape_valid || base.is_empty() {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    }
    if step_part.is_some_and(|part| part.is_empty() || !part.iter().all(u8::is_ascii_digit)) {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    }

    let (start, range_end, end_offset) = match hyphens.as_slice() {
        [] if base == b"*" => {
            let (min, max, _) = bounds(field);
            (min, Some(max), item_offset)
        }
        [] => {
            let value = endpoint_value(field, base, item_offset)?;
            (value, None, item_offset)
        }
        [hyphen] => {
            let start = endpoint_value(field, &base[..*hyphen], item_offset)?;
            let end_at = item_offset + hyphen + 1;
            let end = endpoint_value(field, &base[*hyphen + 1..], end_at)?;
            if start > end {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: end_at,
                });
            }
            (start, Some(end), end_at)
        }
        _ => unreachable!(),
    };

    let step = if let Some(part) = step_part {
        let step_at = item_offset + step_offset;
        if !part.iter().all(u8::is_ascii_digit) {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
        let value = parse_number(field, part, step_at)?;
        let (_, _, width) = bounds(field);
        if value == 0 || value > width as u32 {
            return Err(ParseError::BadStep {
                field,
                offset: step_at,
            });
        }
        Some(value)
    } else {
        None
    };

    let (_, domain_max, _) = bounds(field);
    let end = match (step, range_end) {
        (Some(_), None) => domain_max,
        (_, Some(value)) => value,
        (None, None) => start,
    };

    let stride = step.unwrap_or(1);
    let mut value = start;
    while value <= end {
        selected[value as usize] = true;
        match value.checked_add(stride) {
            Some(next) if next > value => value = next,
            _ => break,
        }
    }

    let _ = end_offset;
    Ok(())
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut selected: [Vec<bool>; 5] = [
        vec![false; 60],
        vec![false; 24],
        vec![false; 32],
        vec![false; 13],
        vec![false; 7],
    ];
    let mut unrestricted_days = [false; 2];

    for (index, field_text) in parts.iter().enumerate() {
        let field = FIELDS[index];
        for (offset, &byte) in field_text.as_bytes().iter().enumerate() {
            if !(byte.is_ascii_digit()
                || byte.is_ascii_alphabetic()
                || matches!(byte, b'*' | b'-' | b'/' | b','))
            {
                return Err(ParseError::InvalidCharacter { field, offset });
            }
        }

        if index == 2 {
            unrestricted_days[0] = *field_text == "*";
        } else if index == 4 {
            unrestricted_days[1] = *field_text == "*";
        }

        let mut offset = 0usize;
        for item in field_text.split(',') {
            parse_item(field, item.as_bytes(), offset, &mut selected[index])?;
            offset += item.len() + 1;
        }
    }

    Ok(Cron {
        selected,
        unrestricted_days,
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

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;

        let weekday = weekday(at.year, at.month, at.day);
        let day_of_month = self.selected[2][at.day as usize];
        let day_of_week = self.selected[4][weekday as usize];

        let day_matches = match self.unrestricted_days {
            [true, true] => true,
            [false, true] => day_of_month,
            [true, false] => day_of_week,
            [false, false] => {
                if at.day == days_in_month(at.year, at.month) {
                    day_of_month && day_of_week
                } else {
                    day_of_month || day_of_week
                }
            }
        };

        Ok(self.selected[0][at.minute as usize]
            && self.selected[1][at.hour as usize]
            && self.selected[3][at.month as usize]
            && day_matches)
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;

        let mut current = after;
        loop {
            if current.year == 2399
                && current.month == 12
                && current.day == 31
                && current.hour == 23
                && current.minute == 59
            {
                return Ok(None);
            }

            current = advance_minute(current);
            if self.matches(current)? {
                return Ok(Some(current));
            }
        }
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
    if !(1..=days_in_month(at.year, at.month)).contains(&at.day) {
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
        days += days_in_month(year, m) as u32;
    }
    days += day as u32 - 1;
    ((6 + days) % 7) as u8
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

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Cron {
        parse_cron(s).unwrap()
    }

    fn d(year: u16, month: u8, day: u8, hour: u8, minute: u8) -> CivilDateTime {
        CivilDateTime {
            year,
            month,
            day,
            hour,
            minute,
        }
    }

    #[test]
    fn p_named_parse() {
        let q = c("5 2 * JAN-MAR MON,WED");
        assert_eq!(q.allowed(Field::Month), vec![1, 2, 3]);
        assert_eq!(q.allowed(Field::DayOfWeek), vec![1, 3]);
    }

    #[test]
    fn p_day_or() {
        let q = c("0 0 12 * MON");
        assert!(q.matches(d(2000, 1, 12, 0, 0)).unwrap());
        assert!(q.matches(d(2000, 1, 17, 0, 0)).unwrap());
        assert!(!q.matches(d(2000, 1, 18, 0, 0)).unwrap());
    }

    #[test]
    fn p_next_named() {
        assert_eq!(
            c("0 0 * FEB SUN").next_fire(d(2000, 2, 1, 0, 0)).unwrap(),
            Some(d(2000, 2, 6, 0, 0))
        );
    }

    #[test]
    fn p_public_last_day() {
        assert!(!c("0 0 31 * TUE").matches(d(2000, 1, 31, 0, 0)).unwrap());
    }
}