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
    day_of_month_star: bool,
    day_of_week_star: bool,
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
    offset: usize,
    field: Field,
    step: bool,
) -> Result<u32, ParseError> {
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }

    let mut value = 0u32;
    for &byte in bytes {
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }

    let (min, max) = bounds(field);
    if step {
        let width = max - min + 1;
        if value < 1 || value > width {
            return Err(ParseError::BadStep { field, offset });
        }
    } else if value < min || value > max {
        return Err(ParseError::OutOfRange { field, offset });
    }

    Ok(value)
}

fn is_digits(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit)
}

fn parse_field(text: &str, field: Field) -> Result<Vec<bool>, ParseError> {
    let bytes = text.as_bytes();

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
            .map(|n| item_start + n)
            .unwrap_or(bytes.len());
        let item = &bytes[item_start..item_end];

        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let slash = item.iter().position(|&b| b == b'/');
        let (base, step_bytes, step_offset) = if let Some(slash) = slash {
            if item[slash + 1..].contains(&b'/') {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            (&item[..slash], Some(&item[slash + 1..]), item_start + slash + 1)
        } else {
            (item, None, item_end)
        };

        let base_kind = if base == b"*" {
            0u8
        } else if is_digits(base) {
            1u8
        } else {
            let hyphen = base.iter().position(|&b| b == b'-');
            match hyphen {
                Some(hyphen)
                    if !base[hyphen + 1..].contains(&b'-')
                        && is_digits(&base[..hyphen])
                        && is_digits(&base[hyphen + 1..]) =>
                {
                    2u8
                }
                _ => {
                    return Err(ParseError::InvalidSyntax {
                        field,
                        offset: item_start,
                    });
                }
            }
        };

        if let Some(step) = step_bytes {
            if !is_digits(step) {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
        }

        let (start, end) = match base_kind {
            0 => (min, max),
            1 => {
                let value = parse_number(base, item_start, field, false)?;
                (value, if step_bytes.is_some() { max } else { value })
            }
            _ => {
                let hyphen = base.iter().position(|&b| b == b'-').unwrap();
                let start = parse_number(&base[..hyphen], item_start, field, false)?;
                let end_offset = item_start + hyphen + 1;
                let end = parse_number(&base[hyphen + 1..], end_offset, field, false)?;
                if start > end {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: end_offset,
                    });
                }
                (start, end)
            }
        };

        let step = match step_bytes {
            Some(step) => parse_number(step, step_offset, field, true)?,
            None => 1,
        };

        let mut value = start;
        while value <= end {
            selected[value as usize] = true;
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
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

    let mut selected = [Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for (i, field) in FIELDS.iter().copied().enumerate() {
        selected[i] = parse_field(pieces[i], field)?;
    }

    Ok(Cron {
        selected,
        day_of_month_star: pieces[2] == "*",
        day_of_week_star: pieces[4] == "*",
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
            .filter_map(|(value, &yes)| if yes { Some(value as u8) } else { None })
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate(at)?;
        let weekday = weekday(at.year, at.month, at.day) as usize;
        Ok(self.selected[0][at.minute as usize]
            && self.selected[1][at.hour as usize]
            && self.selected[2][at.day as usize]
            && self.selected[3][at.month as usize]
            && self.selected[4][weekday])
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate(after)?;

        let mut year = after.year;
        let mut month = after.month;
        let mut day = after.day;

        loop {
            let date_matches = self.selected[3][month as usize]
                && self.selected[2][day as usize]
                && self.selected[4][weekday(year, month, day) as usize];

            if date_matches {
                let same_date = year == after.year && month == after.month && day == after.day;
                let min_time = if same_date {
                    (after.hour, after.minute)
                } else {
                    (0, 0)
                };

                for hour in self.allowed(Field::Hour) {
                    if hour < min_time.0 {
                        continue;
                    }
                    let minute_floor = if hour == min_time.0 && same_date {
                        min_time.1.saturating_add(1)
                    } else {
                        0
                    };

                    for minute in self.allowed(Field::Minute) {
                        if minute >= minute_floor {
                            return Ok(Some(CivilDateTime {
                                year,
                                month,
                                day,
                                hour,
                                minute,
                            }));
                        }
                    }
                }
            }

            if year == 2399 && month == 12 && day == 31 {
                return Ok(None);
            }

            if day < days_in_month(year, month) {
                day += 1;
            } else {
                day = 1;
                if month < 12 {
                    month += 1;
                } else {
                    month = 1;
                    year += 1;
                }
            }
        }
    }
}

fn validate(at: CivilDateTime) -> Result<(), DateError> {
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

fn leap(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 => {
            if leap(year) {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days = 0u32;
    for y in 2000..year {
        days += if leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as u32;
    }
    days += (day - 1) as u32;
    ((days + 6) % 7) as u8
}