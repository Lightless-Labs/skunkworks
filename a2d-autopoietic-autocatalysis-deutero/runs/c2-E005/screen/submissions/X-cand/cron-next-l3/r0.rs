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

fn bounds(field: Field) -> (u32, u32) {
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

fn syntax_error(field: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field, offset }
}

fn parse_number(
    bytes: &[u8],
    field: Field,
    offset: usize,
) -> Result<u32, ParseError> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(syntax_error(field, offset));
    }
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

fn named_value(bytes: &[u8], field: Field) -> Option<u32> {
    let upper: Vec<u8> = bytes.iter().map(u8::to_ascii_uppercase).collect();
    let names: &[&[u8]] = match field {
        Field::Month => &[
            b"JAN", b"FEB", b"MAR", b"APR", b"MAY", b"JUN", b"JUL", b"AUG",
            b"SEP", b"OCT", b"NOV", b"DEC",
        ],
        Field::DayOfWeek => &[
            b"SUN", b"MON", b"TUE", b"WED", b"THU", b"FRI", b"SAT",
        ],
        _ => return None,
    };
    names
        .iter()
        .position(|name| *name == upper.as_slice())
        .map(|index| {
            if field == Field::Month {
                index as u32 + 1
            } else {
                index as u32
            }
        })
}

fn is_atom_syntax(bytes: &[u8], field: Field) -> bool {
    if bytes.is_empty() {
        return false;
    }
    if bytes.iter().all(u8::is_ascii_digit) {
        return true;
    }
    named_value(bytes, field).is_some()
}

fn atom_value(
    bytes: &[u8],
    field: Field,
    offset: usize,
) -> Result<u32, ParseError> {
    if bytes.iter().all(u8::is_ascii_digit) {
        return parse_number(bytes, field, offset);
    }
    named_value(bytes, field).ok_or_else(|| syntax_error(field, offset))
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

    let (base, step_bytes, step_offset) = match slash_positions.as_slice() {
        [] => (item, None, 0),
        [slash] => (&item[..*slash], Some(&item[*slash + 1..]), item_offset + slash + 1),
        _ => return Err(syntax_error(field, item_offset)),
    };

    let hyphen_positions: Vec<usize> = base
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'-').then_some(i))
        .collect();

    let (wildcard, start_bytes, end_bytes) = match (base, hyphen_positions.as_slice()) {
        (b"*", []) => (true, None, None),
        (_, []) => (false, Some(base), None),
        (_, [hyphen]) => {
            let start = &base[..*hyphen];
            let end = &base[*hyphen + 1..];
            if start == b"*" || end == b"*" {
                return Err(syntax_error(field, item_offset));
            }
            (false, Some(start), Some(end))
        }
        _ => return Err(syntax_error(field, item_offset)),
    };

    if let Some(start) = start_bytes {
        if !is_atom_syntax(start, field)
            || end_bytes.is_some_and(|end| !is_atom_syntax(end, field))
        {
            return Err(syntax_error(field, item_offset));
        }
    }
    if let Some(step) = step_bytes {
        if step.is_empty() || !step.iter().all(u8::is_ascii_digit) {
            return Err(syntax_error(field, item_offset));
        }
    }

    let (minimum, maximum) = bounds(field);
    let start = if wildcard {
        minimum
    } else {
        atom_value(start_bytes.unwrap(), field, item_offset)?
    };
    if !wildcard && start_bytes.unwrap().iter().all(u8::is_ascii_digit) {
        if start < minimum || start > maximum {
            return Err(ParseError::OutOfRange {
                field,
                offset: item_offset,
            });
        }
    }

    let end = if let Some(end_bytes) = end_bytes {
        let end_offset = item_offset + (end_bytes.as_ptr() as usize - item.as_ptr() as usize);
        let value = atom_value(end_bytes, field, end_offset)?;
        if end_bytes.iter().all(u8::is_ascii_digit) && (value < minimum || value > maximum) {
            return Err(ParseError::OutOfRange {
                field,
                offset: end_offset,
            });
        }
        if start > value {
            return Err(ParseError::ReversedRange {
                field,
                offset: end_offset,
            });
        }
        value
    } else if wildcard || step_bytes.is_some() {
        maximum
    } else {
        start
    };

    let step = if let Some(step_bytes) = step_bytes {
        let value = parse_number(step_bytes, field, step_offset)?;
        let width = maximum - minimum + 1;
        if value == 0 || value > width {
            return Err(ParseError::BadStep {
                field,
                offset: step_offset,
            });
        }
        value
    } else {
        1
    };

    let mut value = start;
    while value <= end {
        selected[(value - minimum) as usize] = true;
        match value.checked_add(step) {
            Some(next) => value = next,
            None => break,
        }
    }
    Ok(())
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut selected: [Vec<bool>; 5] = std::array::from_fn(|index| {
        let (minimum, maximum) = bounds(FIELDS[index]);
        vec![false; (maximum - minimum + 1) as usize]
    });

    for (index, part) in parts.iter().enumerate() {
        let field = FIELDS[index];
        let bytes = part.as_bytes();
        for (offset, &byte) in bytes.iter().enumerate() {
            if !(byte.is_ascii_digit()
                || byte.is_ascii_alphabetic()
                || matches!(byte, b'*' | b'-' | b'/' | b','))
            {
                return Err(ParseError::InvalidCharacter { field, offset });
            }
        }

        let is_unrestricted = *part == "*";
        for (item, offset) in part.split(',').scan(0usize, |position, item| {
            let current = *position;
            *position += item.len() + 1;
            Some((item, current))
        }) {
            parse_item(item.as_bytes(), offset, field, &mut selected[index])?;
        }

        if field == Field::DayOfMonth && is_unrestricted {
            // Stored below.
        }
    }

    Ok(Cron {
        selected,
        day_of_month_unrestricted: parts[2] == "*",
        day_of_week_unrestricted: parts[4] == "*",
    })
}

fn leap_year(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 => if leap_year(year) { 29 } else { 28 },
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
    if at.day < 1 || at.day > days_in_month(at.year, at.month) {
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

fn weekday(at: CivilDateTime) -> u8 {
    let mut days = 0u32;
    for year in 2000..at.year {
        days += if leap_year(year) { 366 } else { 365 };
    }
    for month in 1..at.month {
        days += days_in_month(at.year, month) as u32;
    }
    days += at.day as u32 - 1;
    ((6 + days) % 7) as u8
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let index = field_index(field);
        let (minimum, _) = bounds(field);
        self.selected[index]
            .iter()
            .enumerate()
            .filter_map(|(i, &enabled)| enabled.then_some(minimum + i as u32))
            .map(|value| value as u8)
            .collect()
    }

    fn contains(&self, field: Field, value: u8) -> bool {
        let (minimum, maximum) = bounds(field);
        let value = value as u32;
        value >= minimum
            && value <= maximum
            && self.selected[field_index(field)][(value - minimum) as usize]
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        if !self.contains(Field::Minute, at.minute)
            || !self.contains(Field::Hour, at.hour)
            || !self.contains(Field::Month, at.month)
        {
            return false;
        }

        let dom = self.contains(Field::DayOfMonth, at.day);
        let dow = self.contains(Field::DayOfWeek, weekday(at));
        match (
            self.day_of_month_unrestricted,
            self.day_of_week_unrestricted,
        ) {
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

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        Ok(self.matches_valid(at))
    }

    pub fn next_fire(
        &self,
        after: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;

        let mut current = after;
        loop {
            if current.minute < 59 {
                current.minute += 1;
            } else {
                current.minute = 0;
                if current.hour < 23 {
                    current.hour += 1;
                } else {
                    current.hour = 0;
                    if current.day < days_in_month(current.year, current.month) {
                        current.day += 1;
                    } else {
                        current.day = 1;
                        if current.month < 12 {
                            current.month += 1;
                        } else {
                            current.month = 1;
                            if current.year == 2399 {
                                return Ok(None);
                            }
                            current.year += 1;
                        }
                    }
                }
            }

            if self.matches_valid(current) {
                return Ok(Some(current));
            }
        }
    }
}