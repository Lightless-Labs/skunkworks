use std::ops::RangeInclusive;

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

#[derive(Clone, Copy)]
struct Domain {
    min: u32,
    max: u32,
}

impl Domain {
    fn width(self) -> u32 {
        self.max - self.min + 1
    }
}

fn field_domain(field: Field) -> Domain {
    match field {
        Field::Minute => Domain { min: 0, max: 59 },
        Field::Hour => Domain { min: 0, max: 23 },
        Field::DayOfMonth => Domain { min: 1, max: 31 },
        Field::Month => Domain { min: 1, max: 12 },
        Field::DayOfWeek => Domain { min: 0, max: 6 },
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

fn name_value(field: Field, token: &[u8]) -> Option<u32> {
    if !token.iter().all(u8::is_ascii_alphabetic) {
        return None;
    }
    let upper: Vec<u8> = token.iter().map(u8::to_ascii_uppercase).collect();
    match field {
        Field::Month => {
            const NAMES: [&[u8]; 12] = [
                b"JAN", b"FEB", b"MAR", b"APR", b"MAY", b"JUN", b"JUL", b"AUG", b"SEP", b"OCT",
                b"NOV", b"DEC",
            ];
            NAMES
                .iter()
                .position(|name| *name == upper.as_slice())
                .map(|i| i as u32 + 1)
        }
        Field::DayOfWeek => {
            const NAMES: [&[u8]; 7] =
                [b"SUN", b"MON", b"TUE", b"WED", b"THU", b"FRI", b"SAT"];
            NAMES
                .iter()
                .position(|name| *name == upper.as_slice())
                .map(|i| i as u32)
        }
        _ => None,
    }
}

fn parse_number(
    field: Field,
    bytes: &[u8],
    offset: usize,
    step: bool,
) -> Result<u32, ParseError> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(parse_error(field, offset, 0));
    }
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

    if step {
        if value == 0 || value > field_domain(field).width() {
            return Err(parse_error(field, offset, 5));
        }
    } else {
        let domain = field_domain(field);
        if value < domain.min || value > domain.max {
            return Err(parse_error(field, offset, 3));
        }
    }
    Ok(value)
}

fn parse_atom(
    field: Field,
    bytes: &[u8],
    offset: usize,
) -> Result<(u32, bool), ParseError> {
    if bytes.iter().all(u8::is_ascii_digit) {
        Ok((parse_number(field, bytes, offset, false)?, true))
    } else if let Some(value) = name_value(field, bytes) {
        Ok((value, false))
    } else {
        Err(parse_error(field, offset, 0))
    }
}

fn parse_field(field: Field, text: &str) -> Result<Vec<bool>, ParseError> {
    let bytes = text.as_bytes();
    for (offset, &byte) in bytes.iter().enumerate() {
        let legal = byte.is_ascii_digit()
            || byte.is_ascii_alphabetic()
            || matches!(byte, b'*' | b'-' | b'/' | b',');
        if !legal {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let domain = field_domain(field);
    let mut selected = vec![false; (domain.max + 1) as usize];

    let mut item_start = 0usize;
    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|p| item_start + p)
            .unwrap_or(bytes.len());
        let item = &bytes[item_start..item_end];

        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let slash_positions: Vec<usize> = item
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'/').then_some(i))
            .collect();
        if slash_positions.len() > 1 {
            return Err(parse_error(field, item_start, 0));
        }

        let (base, step_part) = if let Some(&slash) = slash_positions.first() {
            (&item[..slash], Some((slash + 1, &item[slash + 1..])))
        } else {
            (item, None)
        };

        if base.is_empty() || step_part.is_some_and(|(_, part)| part.is_empty()) {
            return Err(parse_error(field, item_start, 0));
        }

        let hyphen_positions: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();

        let base_is_star = base == b"*";
        let range_parts = if hyphen_positions.len() == 1 {
            let hyphen = hyphen_positions[0];
            if base[..hyphen].is_empty() || base[hyphen + 1..].is_empty() {
                return Err(parse_error(field, item_start, 0));
            }
            Some((&base[..hyphen], &base[hyphen + 1..], hyphen))
        } else if hyphen_positions.len() > 1 || (base_is_star && !hyphen_positions.is_empty()) {
            return Err(parse_error(field, item_start, 0));
        } else {
            None
        };

        if !base_is_star && range_parts.is_none() {
            let valid_atom = base.iter().all(u8::is_ascii_digit)
                || name_value(field, base).is_some();
            if !valid_atom {
                return Err(parse_error(field, item_start, 0));
            }
        }

        if let Some((left, right, _)) = range_parts {
            if left == b"*" || right == b"*" {
                return Err(parse_error(field, item_start, 0));
            }
            let left_valid =
                left.iter().all(u8::is_ascii_digit) || name_value(field, left).is_some();
            let right_valid =
                right.iter().all(u8::is_ascii_digit) || name_value(field, right).is_some();
            if !left_valid || !right_valid {
                return Err(parse_error(field, item_start, 0));
            }
        }

        if let Some((_, step_bytes)) = step_part {
            if !step_bytes.iter().all(u8::is_ascii_digit) {
                return Err(parse_error(field, item_start, 0));
            }
        }

        let mut start = domain.min;
        let mut end = domain.max;
        let mut end_offset = item_start;

        if !base_is_star {
            if let Some((left, right, hyphen)) = range_parts {
                let left_offset = item_start;
                let right_offset = item_start + hyphen + 1;

                let left_value = if left.iter().all(u8::is_ascii_digit) {
                    parse_number(field, left, left_offset, false)?
                } else {
                    name_value(field, left).unwrap()
                };
                let right_value = if right.iter().all(u8::is_ascii_digit) {
                    parse_number(field, right, right_offset, false)?
                } else {
                    name_value(field, right).unwrap()
                };

                start = left_value;
                end = right_value;
                end_offset = right_offset;
                if start > end {
                    return Err(parse_error(field, end_offset, 4));
                }
            } else {
                let (value, numeric) = parse_atom(field, base, item_start)?;
                start = value;
                if step_part.is_none() {
                    end = value;
                } else {
                    end = domain.max;
                }
                if numeric && step_part.is_some() {
                    end = domain.max;
                }
            }
        }

        let step = if let Some((step_offset, step_bytes)) = step_part {
            parse_number(field, step_bytes, item_start + step_offset, true)?
        } else {
            1
        };

        let mut value = start;
        while value <= end {
            selected[value as usize] = true;
            match value.checked_add(step) {
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

    let field_order = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];
    let mut sets: [Vec<bool>; 5] = std::array::from_fn(|_| Vec::new());

    for (i, field) in field_order.into_iter().enumerate() {
        sets[i] = parse_field(field, fields[i])?;
    }

    Ok(Cron {
        selected: sets,
        day_of_month_unrestricted: fields[2] == "*",
        day_of_week_unrestricted: fields[4] == "*",
    })
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

fn weekday(at: CivilDateTime) -> u8 {
    let mut days = 0u32;
    for year in 2000..at.year {
        days += if is_leap_year(year) { 366 } else { 365 };
    }
    for month in 1..at.month {
        days += days_in_month(at.year, month) as u32;
    }
    days += (at.day - 1) as u32;
    ((6 + days) % 7) as u8
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.selected[field_index(field)]
            .iter()
            .enumerate()
            .filter_map(|(value, &is_selected)| {
                (is_selected && value <= u8::MAX as usize).then_some(value as u8)
            })
            .collect()
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        if !self.selected[0][at.minute as usize]
            || !self.selected[1][at.hour as usize]
            || !self.selected[3][at.month as usize]
        {
            return false;
        }

        let dom = self.selected[2][at.day as usize];
        let dow = self.selected[4][weekday(at) as usize];

        match (
            self.day_of_month_unrestricted,
            self.day_of_week_unrestricted,
        ) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) if at.day == days_in_month(at.year, at.month) => dom && dow,
            (false, false) => dom || dow,
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

        let mut candidate = after;
        loop {
            if candidate.year == 2399
                && candidate.month == 12
                && candidate.day == 31
                && candidate.hour == 23
                && candidate.minute == 59
            {
                return Ok(None);
            }

            if candidate.minute < 59 {
                candidate.minute += 1;
            } else {
                candidate.minute = 0;
                if candidate.hour < 23 {
                    candidate.hour += 1;
                } else {
                    candidate.hour = 0;
                    if candidate.day < days_in_month(candidate.year, candidate.month) {
                        candidate.day += 1;
                    } else {
                        candidate.day = 1;
                        if candidate.month < 12 {
                            candidate.month += 1;
                        } else {
                            candidate.month = 1;
                            if candidate.year == 2399 {
                                return Ok(None);
                            }
                            candidate.year += 1;
                        }
                    }
                }
            }

            if self.matches_valid(candidate) {
                return Ok(Some(candidate));
            }
        }
    }
}