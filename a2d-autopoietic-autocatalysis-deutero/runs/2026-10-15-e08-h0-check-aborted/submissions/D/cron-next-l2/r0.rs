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

fn bounds(field: Field) -> (u32, u32) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
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
        return Err(ParseError::LeadingZero { field, offset: start });
    }

    let mut value = 0u32;
    for &byte in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset: start })?;
    }

    if is_step {
        let (_, max) = bounds(field);
        if value == 0 || value > max - bounds(field).0 + 1 {
            return Err(ParseError::BadStep { field, offset: start });
        }
    } else {
        let (min, max) = bounds(field);
        if value < min || value > max {
            return Err(ParseError::OutOfRange { field, offset: start });
        }
    }

    Ok(value)
}

fn parse_field(input: &str, field: Field) -> Result<Vec<bool>, ParseError> {
    let bytes = input.as_bytes();
    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (min, max) = bounds(field);
    let mut selected = vec![false; (max + 1) as usize];

    let mut item_start = 0;
    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|p| item_start + p)
            .unwrap_or(bytes.len());

        if item_start == item_end {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let item = &bytes[item_start..item_end];
        let slash_positions: Vec<usize> = item
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'/').then_some(i))
            .collect();

        let (base_end, step_range) = match slash_positions.as_slice() {
            [] => (item.len(), None),
            [slash] => (*slash, Some((*slash + 1, item.len()))),
            _ => {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                })
            }
        };

        let base = &item[..base_end];
        let (is_star, number_ranges) = if base == b"*" {
            (true, None)
        } else {
            let hyphens: Vec<usize> = base
                .iter()
                .enumerate()
                .filter_map(|(i, &b)| (b == b'-').then_some(i))
                .collect();
            match hyphens.as_slice() {
                [] if !base.is_empty() && base.iter().all(u8::is_ascii_digit) => {
                    (false, Some(vec![(0, base.len())]))
                }
                [hyphen]
                    if *hyphen > 0
                        && *hyphen + 1 < base.len()
                        && base[..*hyphen].iter().all(u8::is_ascii_digit)
                        && base[*hyphen + 1..].iter().all(u8::is_ascii_digit) =>
                {
                    (false, Some(vec![(0, *hyphen), (*hyphen + 1, base.len())]))
                }
                _ => {
                    return Err(ParseError::InvalidSyntax {
                        field,
                        offset: item_start,
                    })
                }
            }
        };

        let step = if let Some((start, end)) = step_range {
            if start == end || !item[start..end].iter().all(u8::is_ascii_digit) {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            Some(parse_number(
                item,
                start,
                end,
                field,
                true,
            )?)
        } else {
            None
        };

        let (start_value, end_value) = if is_star {
            (min, max)
        } else {
            let ranges = number_ranges.unwrap();
            let start_value = parse_number(
                item,
                ranges[0].0,
                ranges[0].1,
                field,
                false,
            )?;
            if ranges.len() == 1 {
                if step.is_some() {
                    (start_value, max)
                } else {
                    (start_value, start_value)
                }
            } else {
                let end_value = parse_number(
                    item,
                    ranges[1].0,
                    ranges[1].1,
                    field,
                    false,
                )?;
                if start_value > end_value {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: item_start + ranges[1].0,
                    });
                }
                (start_value, end_value)
            }
        };

        let stride = step.unwrap_or(1);
        let mut value = start_value;
        while value <= end_value {
            selected[value as usize] = true;
            match value.checked_add(stride) {
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
    let pieces: Vec<&str> = input.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount {
            found: pieces.len(),
        });
    }

    let mut selected = Vec::with_capacity(5);
    for (piece, field) in pieces.iter().zip(FIELDS) {
        selected.push(parse_field(piece, field)?);
    }

    Ok(Cron {
        selected: selected.try_into().ok().unwrap(),
        day_of_month_is_star: pieces[2] == "*",
        day_of_week_is_star: pieces[4] == "*",
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
            .filter_map(|(value, &is_selected)| is_selected.then_some(value as u8))
            .collect()
    }

    fn contains(&self, field: Field, value: u8) -> bool {
        let index = match field {
            Field::Minute => 0,
            Field::Hour => 1,
            Field::DayOfMonth => 2,
            Field::Month => 3,
            Field::DayOfWeek => 4,
        };
        self.selected[index]
            .get(value as usize)
            .copied()
            .unwrap_or(false)
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        let weekday = weekday(at.year, at.month, at.day);
        Ok(self.contains(Field::Minute, at.minute)
            && self.contains(Field::Hour, at.hour)
            && self.contains(Field::Month, at.month)
            && self.contains(Field::DayOfMonth, at.day)
            && self.contains(Field::DayOfWeek, weekday))
    }

    pub fn next_fire(
        &self,
        after: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
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
    days += (day - 1) as u32;
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