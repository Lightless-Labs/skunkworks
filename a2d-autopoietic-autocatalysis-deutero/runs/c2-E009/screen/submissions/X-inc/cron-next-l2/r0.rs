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

fn parse_number(bytes: &[u8], start: usize, field: Field) -> Result<u32, ParseError> {
    let err = |kind: fn(Field, usize) -> ParseError| kind(field, start);
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(err(|field, offset| ParseError::LeadingZero { field, offset }));
    }

    let mut value = 0u32;
    for &byte in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or_else(|| err(|field, offset| ParseError::NumberOverflow { field, offset }))?;
    }
    Ok(value)
}

fn parse_field(text: &str, field: Field) -> Result<Vec<bool>, ParseError> {
    let bytes = text.as_bytes();
    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (minimum, maximum) = bounds(field);
    let width = maximum - minimum + 1;
    let mut selected = vec![false; width as usize];
    let mut item_start = 0;

    for item_end in (0..=bytes.len()).filter(|&i| i == bytes.len() || bytes[i] == b',') {
        let item = &bytes[item_start..item_end];
        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let slashes: Vec<usize> = item
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'/').then_some(i))
            .collect();
        let (base, step_bytes, step_start) = match slashes.as_slice() {
            [] => (item, None, 0),
            [slash] => (&item[..*slash], Some(&item[*slash + 1..]), *slash + 1),
            _ => {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                })
            }
        };

        let is_star = base == b"*";
        let hyphens: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();

        let (start_value, end_value, start_number, end_number) = if is_star {
            (minimum, maximum, None, None)
        } else {
            match hyphens.as_slice() {
                [] if !base.is_empty() && base.iter().all(u8::is_ascii_digit) => {
                    (0, 0, Some((base, item_start)), None)
                }
                [hyphen]
                    if *hyphen > 0
                        && *hyphen + 1 < base.len()
                        && base[..*hyphen].iter().all(u8::is_ascii_digit)
                        && base[*hyphen + 1..].iter().all(u8::is_ascii_digit) =>
                {
                    (
                        0,
                        0,
                        Some((&base[..*hyphen], item_start)),
                        Some((&base[*hyphen + 1..], item_start + *hyphen + 1)),
                    )
                }
                _ => {
                    return Err(ParseError::InvalidSyntax {
                        field,
                        offset: item_start,
                    })
                }
            }
        };

        let (start, end) = if let Some((number, offset)) = start_number {
            let value = parse_number(number, offset, field)?;
            if value < minimum || value > maximum {
                return Err(ParseError::OutOfRange { field, offset });
            }
            if let Some((end_num, end_offset)) = end_number {
                let end_value = parse_number(end_num, end_offset, field)?;
                if end_value < minimum || end_value > maximum {
                    return Err(ParseError::OutOfRange {
                        field,
                        offset: end_offset,
                    });
                }
                if value > end_value {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: end_offset,
                    });
                }
                (value, end_value)
            } else if step_bytes.is_some() {
                (value, maximum)
            } else {
                (value, value)
            }
        } else {
            (start_value, end_value)
        };

        let step = if let Some(step) = step_bytes {
            if step.is_empty() || !step.iter().all(u8::is_ascii_digit) {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            let value = parse_number(step, item_start + step_start, field).map_err(|error| {
                match error {
                    ParseError::LeadingZero { .. } | ParseError::NumberOverflow { .. } => error,
                    _ => ParseError::InvalidSyntax {
                        field,
                        offset: item_start,
                    },
                }
            })?;
            if value == 0 || value > width {
                return Err(ParseError::BadStep {
                    field,
                    offset: item_start + step_start,
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
    for (text, field) in pieces.iter().zip(FIELDS) {
        selected.push(parse_field(text, field)?);
    }

    Ok(Cron {
        selected: selected.try_into().ok().unwrap(),
        day_of_month_is_star: pieces[2].as_bytes() == b"*",
        day_of_week_is_star: pieces[4].as_bytes() == b"*",
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
        let (minimum, _) = bounds(field);
        self.selected[index]
            .iter()
            .enumerate()
            .filter_map(|(i, &yes)| yes.then_some(minimum as u8 + i as u8))
            .collect()
    }

    fn matches_valid(&self, at: CivilDateTime, weekday: u8) -> bool {
        self.selected[0][at.minute as usize]
            && self.selected[1][at.hour as usize]
            && self.selected[2][(at.day - 1) as usize]
            && self.selected[3][(at.month - 1) as usize]
            && self.selected[4][weekday as usize]
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        Ok(self.matches_valid(at, weekday(at.year, at.month, at.day)))
    }

    pub fn next_fire(
        &self,
        after: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;
        let mut current = after;
        loop {
            if current.minute == 59 {
                current.minute = 0;
                if current.hour == 23 {
                    current.hour = 0;
                    if current.day == days_in_month(current.year, current.month) {
                        current.day = 1;
                        if current.month == 12 {
                            if current.year == 2399 {
                                return Ok(None);
                            }
                            current.year += 1;
                            current.month = 1;
                        } else {
                            current.month += 1;
                        }
                    } else {
                        current.day += 1;
                    }
                } else {
                    current.hour += 1;
                }
            } else {
                current.minute += 1;
            }

            if self.matches_valid(current, weekday(current.year, current.month, current.day)) {
                return Ok(Some(current));
            }
            if current.year == 2399
                && current.month == 12
                && current.day == 31
                && current.hour == 23
                && current.minute == 59
            {
                return Ok(None);
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