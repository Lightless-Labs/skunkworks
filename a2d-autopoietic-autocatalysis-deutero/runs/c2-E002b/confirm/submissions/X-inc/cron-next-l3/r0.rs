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
    minute: Vec<bool>,
    hour: Vec<bool>,
    day_of_month: Vec<bool>,
    month: Vec<bool>,
    day_of_week: Vec<bool>,
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

fn domain(field: Field) -> Domain {
    match field {
        Field::Minute => Domain { min: 0, max: 59 },
        Field::Hour => Domain { min: 0, max: 23 },
        Field::DayOfMonth => Domain { min: 1, max: 31 },
        Field::Month => Domain { min: 1, max: 12 },
        Field::DayOfWeek => Domain { min: 0, max: 6 },
    }
}

fn invalid_syntax(field: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field, offset }
}

fn name_value(field: Field, bytes: &[u8]) -> Option<u32> {
    if !bytes.iter().all(u8::is_ascii_alphabetic) {
        return None;
    }

    let upper: Vec<u8> = bytes.iter().map(u8::to_ascii_uppercase).collect();
    match field {
        Field::Month => match upper.as_slice() {
            b"JAN" => Some(1),
            b"FEB" => Some(2),
            b"MAR" => Some(3),
            b"APR" => Some(4),
            b"MAY" => Some(5),
            b"JUN" => Some(6),
            b"JUL" => Some(7),
            b"AUG" => Some(8),
            b"SEP" => Some(9),
            b"OCT" => Some(10),
            b"NOV" => Some(11),
            b"DEC" => Some(12),
            _ => None,
        },
        Field::DayOfWeek => match upper.as_slice() {
            b"SUN" => Some(0),
            b"MON" => Some(1),
            b"TUE" => Some(2),
            b"WED" => Some(3),
            b"THU" => Some(4),
            b"FRI" => Some(5),
            b"SAT" => Some(6),
            _ => None,
        },
        _ => None,
    }
}

fn is_atom(field: Field, bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && (bytes.iter().all(u8::is_ascii_digit) || name_value(field, bytes).is_some())
}

fn parse_number(field: Field, bytes: &[u8], offset: usize) -> Result<u32, ParseError> {
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
    Ok(value)
}

fn parse_endpoint(
    field: Field,
    bytes: &[u8],
    offset: usize,
    domain: Domain,
) -> Result<u32, ParseError> {
    if bytes.iter().all(u8::is_ascii_digit) {
        let value = parse_number(field, bytes, offset)?;
        if value < domain.min || value > domain.max {
            return Err(ParseError::OutOfRange { field, offset });
        }
        Ok(value)
    } else {
        name_value(field, bytes).ok_or_else(|| invalid_syntax(field, offset))
    }
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

    let domain = domain(field);
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
            return Err(invalid_syntax(field, item_start));
        }

        let (base, step_part) = if let Some(&slash) = slash_positions.first() {
            (&item[..slash], Some((slash + 1, &item[slash + 1..])))
        } else {
            (item, None)
        };

        let mut hyphens = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i));
        let first_hyphen = hyphens.next();
        let second_hyphen = hyphens.next();
        if second_hyphen.is_some() {
            return Err(invalid_syntax(field, item_start));
        }

        let (start_part, end_part) = if let Some(hyphen) = first_hyphen {
            let left = &base[..hyphen];
            let right = &base[hyphen + 1..];
            if !is_atom(field, left) || !is_atom(field, right) {
                return Err(invalid_syntax(field, item_start));
            }
            (Some(left), Some((hyphen + 1, right)))
        } else if base == b"*" {
            (None, None)
        } else if is_atom(field, base) {
            (Some(base), None)
        } else {
            return Err(invalid_syntax(field, item_start));
        };

        let (start, end) = if let Some(start_bytes) = start_part {
            let start = parse_endpoint(field, start_bytes, item_start, domain)?;
            let end = if let Some((end_offset, end_bytes)) = end_part {
                parse_endpoint(field, end_bytes, item_start + end_offset, domain)?
            } else if step_part.is_some() {
                domain.max
            } else {
                start
            };

            if end_part.is_some() && start > end {
                let (end_offset, _) = end_part.unwrap();
                return Err(ParseError::ReversedRange {
                    field,
                    offset: item_start + end_offset,
                });
            }
            (start, end)
        } else {
            (domain.min, domain.max)
        };

        let step = if let Some((step_offset, step_bytes)) = step_part {
            if step_bytes.is_empty() || !step_bytes.iter().all(u8::is_ascii_digit) {
                return Err(invalid_syntax(field, item_start));
            }
            let offset = item_start + step_offset;
            let value = parse_number(field, step_bytes, offset)?;
            if value == 0 || value > domain.width() {
                return Err(ParseError::BadStep { field, offset });
            }
            value
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

    let minute = parse_field(Field::Minute, fields[0])?;
    let hour = parse_field(Field::Hour, fields[1])?;
    let day_of_month = parse_field(Field::DayOfMonth, fields[2])?;
    let month = parse_field(Field::Month, fields[3])?;
    let day_of_week = parse_field(Field::DayOfWeek, fields[4])?;

    Ok(Cron {
        minute,
        hour,
        day_of_month,
        month,
        day_of_week,
        day_of_month_unrestricted: fields[2] == "*",
        day_of_week_unrestricted: fields[4] == "*",
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let selected = match field {
            Field::Minute => &self.minute,
            Field::Hour => &self.hour,
            Field::DayOfMonth => &self.day_of_month,
            Field::Month => &self.month,
            Field::DayOfWeek => &self.day_of_week,
        };
        selected
            .iter()
            .enumerate()
            .filter_map(|(value, &yes)| yes.then_some(value as u8))
            .collect()
    }

    fn day_matches(&self, date: CivilDateTime, weekday: u8) -> bool {
        let dom = self.day_of_month[date.day as usize];
        let dow = self.day_of_week[weekday as usize];
        let last_day = date.day == days_in_month(date.year, date.month);

        match (
            self.day_of_month_unrestricted,
            self.day_of_week_unrestricted,
        ) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) if last_day => dom && dow,
            (false, false) => dom || dow,
        }
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        self.minute[at.minute as usize]
            && self.hour[at.hour as usize]
            && self.month[at.month as usize]
            && self.day_matches(at, weekday(at.year, at.month, at.day))
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        Ok(self.matches_valid(at))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;

        let mut year = after.year;
        let mut month = after.month;
        let mut day = after.day;

        loop {
            if self.month[month as usize] {
                let candidate_date = CivilDateTime {
                    year,
                    month,
                    day,
                    hour: 0,
                    minute: 0,
                };
                let weekday = weekday(year, month, day);
                if self.day_matches(candidate_date, weekday) {
                    for (hour, &hour_selected) in self.hour.iter().enumerate() {
                        if !hour_selected {
                            continue;
                        }
                        for (minute, &minute_selected) in self.minute.iter().enumerate() {
                            if !minute_selected {
                                continue;
                            }
                            let candidate = CivilDateTime {
                                year,
                                month,
                                day,
                                hour: hour as u8,
                                minute: minute as u8,
                            };
                            if candidate > after {
                                return Ok(Some(candidate));
                            }
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
    ((6 + days) % 7) as u8
}