use std::collections::BTreeSet;

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
    minutes: Vec<u8>,
    hours: Vec<u8>,
    days_of_month: Vec<u8>,
    months: Vec<u8>,
    days_of_week: Vec<u8>,
    day_of_month_is_star: bool,
    day_of_week_is_star: bool,
}

#[derive(Clone, Copy)]
struct Domain {
    min: u32,
    max: u32,
    width: u32,
}

impl Domain {
    fn for_field(field: Field) -> Self {
        match field {
            Field::Minute => Self {
                min: 0,
                max: 59,
                width: 60,
            },
            Field::Hour => Self {
                min: 0,
                max: 23,
                width: 24,
            },
            Field::DayOfMonth => Self {
                min: 1,
                max: 31,
                width: 31,
            },
            Field::Month => Self {
                min: 1,
                max: 12,
                width: 12,
            },
            Field::DayOfWeek => Self {
                min: 0,
                max: 6,
                width: 7,
            },
        }
    }
}

fn parse_number(
    bytes: &[u8],
    start: usize,
    field: Field,
    domain: Domain,
) -> Result<u32, ParseError> {
    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }

    if end == start {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: start,
        });
    }

    if end - start > 1 && bytes[start] == b'0' {
        return Err(ParseError::LeadingZero {
            field,
            offset: start,
        });
    }

    let mut value = 0u32;
    for &byte in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow {
                field,
                offset: start,
            })?;
    }

    if value < domain.min || value > domain.max {
        return Err(ParseError::OutOfRange {
            field,
            offset: start,
        });
    }

    Ok(value)
}

fn parse_step(
    bytes: &[u8],
    start: usize,
    field: Field,
    domain: Domain,
) -> Result<u32, ParseError> {
    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }

    if end == start {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: start,
        });
    }

    if end - start > 1 && bytes[start] == b'0' {
        return Err(ParseError::LeadingZero {
            field,
            offset: start,
        });
    }

    let mut value = 0u32;
    for &byte in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow {
                field,
                offset: start,
            })?;
    }

    if value == 0 || value > domain.width {
        return Err(ParseError::BadStep {
            field,
            offset: start,
        });
    }

    Ok(value)
}

fn parse_item(
    item: &[u8],
    item_offset: usize,
    field: Field,
    domain: Domain,
    selected: &mut BTreeSet<u8>,
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

    if slash_positions.len() > 1 {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    }

    let (base, step_start) = if let Some(&slash) = slash_positions.first() {
        (&item[..slash], Some(slash + 1))
    } else {
        (item, None)
    };

    let is_star = base == b"*";
    let hyphens: Vec<usize> = base
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'-').then_some(i))
        .collect();

    let (base_kind, first_start, second_start) = if is_star {
        (0u8, None, None)
    } else if hyphens.is_empty() {
        if base.iter().all(u8::is_ascii_digit) && !base.is_empty() {
            (1u8, Some(0usize), None)
        } else {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
    } else if hyphens.len() == 1 {
        let h = hyphens[0];
        if h > 0
            && h + 1 < base.len()
            && base[..h].iter().all(u8::is_ascii_digit)
            && base[h + 1..].iter().all(u8::is_ascii_digit)
        {
            (2u8, Some(0usize), Some(h + 1))
        } else {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
    } else {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    };

    if let Some(step_start) = step_start {
        if step_start >= item.len()
            || !item[step_start..].iter().all(u8::is_ascii_digit)
        {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
    }

    let (start, end) = match base_kind {
        0 => (domain.min, domain.max),
        1 => {
            let start = parse_number(
                base,
                first_start.expect("numeric base"),
                field,
                domain,
            )?;
            let end = if step_start.is_some() {
                domain.max
            } else {
                start
            };
            (start, end)
        }
        _ => {
            let start = parse_number(
                base,
                first_start.expect("range start"),
                field,
                domain,
            )?;
            let end = parse_number(
                base,
                second_start.expect("range end"),
                field,
                domain,
            )?;
            if start > end {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: second_start.expect("range end"),
                });
            }
            (start, end)
        }
    };

    let step = if let Some(step_start) = step_start {
        parse_step(item, step_start, field, domain)?
    } else {
        1
    };

    let mut value = start;
    while value <= end {
        selected.insert(value as u8);
        match value.checked_add(step) {
            Some(next) => value = next,
            None => break,
        }
    }

    Ok(())
}

fn parse_field(text: &str, field: Field) -> Result<Vec<u8>, ParseError> {
    let bytes = text.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let domain = Domain::for_field(field);
    let mut selected = BTreeSet::new();
    let mut item_start = 0usize;

    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b',' {
            parse_item(
                &bytes[item_start..index],
                item_start,
                field,
                domain,
                &mut selected,
            )?;
            item_start = index + 1;
        }
    }

    parse_item(
        &bytes[item_start..],
        item_start,
        field,
        domain,
        &mut selected,
    )?;

    Ok(selected.into_iter().collect())
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let minutes = parse_field(parts[0], Field::Minute)?;
    let hours = parse_field(parts[1], Field::Hour)?;
    let day_of_month_is_star = parts[2].as_bytes() == b"*";
    let days_of_month = parse_field(parts[2], Field::DayOfMonth)?;
    let months = parse_field(parts[3], Field::Month)?;
    let day_of_week_is_star = parts[4].as_bytes() == b"*";
    let days_of_week = parse_field(parts[4], Field::DayOfWeek)?;

    Ok(Cron {
        minutes,
        hours,
        days_of_month,
        months,
        days_of_week,
        day_of_month_is_star,
        day_of_week_is_star,
    })
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
    let mut day_index = 0u32;
    for year in 2000..at.year {
        day_index += if is_leap_year(year) { 366 } else { 365 };
    }
    for month in 1..at.month {
        day_index += days_in_month(at.year, month) as u32;
    }
    day_index += (at.day - 1) as u32;
    ((day_index + 6) % 7) as u8
}

fn contains(values: &[u8], value: u8) -> bool {
    values.binary_search(&value).is_ok()
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        match field {
            Field::Minute => self.minutes.clone(),
            Field::Hour => self.hours.clone(),
            Field::DayOfMonth => self.days_of_month.clone(),
            Field::Month => self.months.clone(),
            Field::DayOfWeek => self.days_of_week.clone(),
        }
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        let weekday = weekday(at);
        Ok(contains(&self.minutes, at.minute)
            && contains(&self.hours, at.hour)
            && contains(&self.months, at.month)
            && contains(&self.days_of_month, at.day)
            && contains(&self.days_of_week, weekday))
    }

    pub fn next_fire(
        &self,
        after: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;

        let mut year = after.year;
        let mut month = after.month;

        while year <= 2399 {
            if !contains(&self.months, month) {
                if month == 12 {
                    year += 1;
                    month = 1;
                } else {
                    month += 1;
                }
                continue;
            }

            let month_days = days_in_month(year, month);
            for day in 1..=month_days {
                if !contains(&self.days_of_month, day) {
                    continue;
                }
                let date = CivilDateTime {
                    year,
                    month,
                    day,
                    hour: 0,
                    minute: 0,
                };
                if !contains(&self.days_of_week, weekday(date)) {
                    continue;
                }

                for &hour in &self.hours {
                    for &minute in &self.minutes {
                        let candidate = CivilDateTime {
                            year,
                            month,
                            day,
                            hour,
                            minute,
                        };
                        if candidate > after {
                            return Ok(Some(candidate));
                        }
                    }
                }
            }

            if month == 12 {
                year += 1;
                month = 1;
            } else {
                month += 1;
            }
        }

        Ok(None)
    }
}