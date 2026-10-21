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

fn domain(field: Field) -> (u32, u32) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
    }
}

fn parse_number(bytes: &[u8], start: usize, field: Field, step: bool) -> Result<u32, ParseError> {
    let fail = |kind: fn(Field, usize) -> ParseError| kind(field, start);
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(fail(|field, offset| ParseError::LeadingZero { field, offset }));
    }

    let mut value = 0u32;
    for &byte in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or_else(|| fail(|field, offset| ParseError::NumberOverflow { field, offset }))?;
    }

    let (min, max) = domain(field);
    if step {
        let width = max - min + 1;
        if value == 0 || value > width {
            return Err(fail(|field, offset| ParseError::BadStep { field, offset }));
        }
    } else if value < min || value > max {
        return Err(fail(|field, offset| ParseError::OutOfRange { field, offset }));
    }
    Ok(value)
}

fn syntax_error(field: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field, offset }
}

fn parse_field(text: &str, field: Field) -> Result<Vec<bool>, ParseError> {
    let bytes = text.as_bytes();
    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (min, max) = domain(field);
    let mut selected = vec![false; (max + 1) as usize];

    let mut item_start = 0usize;
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

        let (base, step_bytes, step_offset) = match slash_positions.as_slice() {
            [] => (item, None, 0),
            [slash] => (&item[..*slash], Some(&item[*slash + 1..]), item_start + slash + 1),
            _ => return Err(syntax_error(field, item_start)),
        };

        let star = base == b"*";
        let hyphens: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();

        let (range_start, range_end, start_num_offset, end_num_offset) = if star {
            (None, None, 0, 0)
        } else {
            match hyphens.as_slice() {
                [] if !base.is_empty() && base.iter().all(u8::is_ascii_digit) => {
                    (Some(base), None, item_start, 0)
                }
                [hyphen]
                    if *hyphen > 0
                        && *hyphen + 1 < base.len()
                        && base[..*hyphen].iter().all(u8::is_ascii_digit)
                        && base[*hyphen + 1..].iter().all(u8::is_ascii_digit) =>
                {
                    (
                        Some(&base[..*hyphen]),
                        Some(&base[*hyphen + 1..]),
                        item_start,
                        item_start + hyphen + 1,
                    )
                }
                _ => return Err(syntax_error(field, item_start)),
            }
        };

        if step_bytes.is_some_and(|s| s.is_empty() || !s.iter().all(u8::is_ascii_digit)) {
            return Err(syntax_error(field, item_start));
        }

        let (start, end) = if star {
            (min, max)
        } else {
            let start = parse_number(
                range_start.unwrap(),
                start_num_offset,
                field,
                false,
            )?;
            if let Some(end_bytes) = range_end {
                let end = parse_number(end_bytes, end_num_offset, field, false)?;
                if start > end {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: end_num_offset,
                    });
                }
                (start, end)
            } else if step_bytes.is_some() {
                (start, max)
            } else {
                (start, start)
            }
        };

        let step = if let Some(step_bytes) = step_bytes {
            parse_number(step_bytes, step_offset, field, true)?
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
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut selected = Vec::with_capacity(5);
    for (i, field) in FIELDS.iter().copied().enumerate() {
        selected.push(parse_field(parts[i], field)?);
    }
    let selected: [Vec<bool>; 5] = selected.try_into().ok().unwrap();

    Ok(Cron {
        selected,
        day_of_month_is_star: parts[2].as_bytes() == b"*",
        day_of_week_is_star: parts[4].as_bytes() == b"*",
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
            .filter_map(|(value, &yes)| yes.then_some(value as u8))
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        let weekday = weekday(at.year, at.month, at.day);
        Ok(self.selected[0][at.minute as usize]
            && self.selected[1][at.hour as usize]
            && self.selected[2][at.day as usize]
            && self.selected[3][at.month as usize]
            && self.selected[4][weekday as usize])
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;
        let mut year = after.year;
        let mut month = after.month;
        let mut day = after.day;
        let mut hour = after.hour;
        let mut minute = after.minute;

        loop {
            if minute < 59 {
                minute += 1;
            } else {
                minute = 0;
                if hour < 23 {
                    hour += 1;
                } else {
                    hour = 0;
                    if day < days_in_month(year, month) {
                        day += 1;
                    } else {
                        day = 1;
                        if month < 12 {
                            month += 1;
                        } else {
                            month = 1;
                            if year == 2399 {
                                return Ok(None);
                            }
                            year += 1;
                        }
                    }
                }
            }

            let candidate = CivilDateTime {
                year,
                month,
                day,
                hour,
                minute,
            };
            if self.matches(candidate).unwrap_or(false) {
                return Ok(Some(candidate));
            }
            if year == 2399 && month == 12 && day == 31 && hour == 23 && minute == 59 {
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

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days = 0u32;
    for y in 2000..year {
        days += if leap_year(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as u32;
    }
    days += (day - 1) as u32;
    ((days + 6) % 7) as u8
}