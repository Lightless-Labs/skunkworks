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

fn field_index(field: Field) -> usize {
    match field {
        Field::Minute => 0,
        Field::Hour => 1,
        Field::DayOfMonth => 2,
        Field::Month => 3,
        Field::DayOfWeek => 4,
    }
}

fn field_bounds(field: Field) -> (u32, u32) {
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
    step: bool,
) -> Result<u32, ParseError> {
    if start == end {
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

    if step {
        let (_, max) = field_bounds(field);
        if value == 0 || value > max + 1 {
            return Err(ParseError::BadStep {
                field,
                offset: start,
            });
        }
    } else {
        let (min, max) = field_bounds(field);
        if value < min || value > max {
            return Err(ParseError::OutOfRange {
                field,
                offset: start,
            });
        }
    }

    Ok(value)
}

fn parse_field(
    text: &str,
    field: Field,
) -> Result<(Vec<bool>, bool), ParseError> {
    let bytes = text.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (domain_min, domain_max) = field_bounds(field);
    let mut selected = vec![false; (domain_max + 1) as usize];

    for (item_start, item) in split_with_offsets(bytes, b',') {
        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let slash_positions: Vec<usize> = item
            .iter()
            .enumerate()
            .filter_map(|(i, &byte)| (byte == b'/').then_some(i))
            .collect();

        if slash_positions.len() > 1 {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let (base_end, step_range) = if let Some(&slash) = slash_positions.first() {
            (slash, Some((slash + 1, item.len())))
        } else {
            (item.len(), None)
        };

        let base = &item[..base_end];
        let is_star = base == b"*";
        let hyphens: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &byte)| (byte == b'-').then_some(i))
            .collect();

        let base_kind = if is_star && hyphens.is_empty() {
            0
        } else if hyphens.is_empty() && !base.is_empty() {
            if base.iter().all(u8::is_ascii_digit) {
                1
            } else {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
        } else if hyphens.len() == 1 {
            let hyphen = hyphens[0];
            if hyphen == 0
                || hyphen + 1 == base.len()
                || !base[..hyphen].iter().all(u8::is_ascii_digit)
                || !base[hyphen + 1..].iter().all(u8::is_ascii_digit)
            {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            2
        } else {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        };

        if let Some((step_start, step_end)) = step_range {
            if step_start == step_end
                || !item[step_start..step_end]
                    .iter()
                    .all(u8::is_ascii_digit)
            {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
        }

        let (start, end) = match base_kind {
            0 => (domain_min, domain_max),
            1 => {
                let number = parse_number(
                    base,
                    0,
                    base.len(),
                    field,
                    false,
                )?;
                (number, number)
            }
            _ => {
                let hyphen = hyphens[0];
                let start = parse_number(base, 0, hyphen, field, false)?;
                let end = parse_number(
                    base,
                    hyphen + 1,
                    base.len(),
                    field,
                    false,
                )?;
                if start > end {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: item_start + hyphen + 1,
                    });
                }
                (start, end)
            }
        };

        let step = if let Some((step_start, step_end)) = step_range {
            parse_number(
                item,
                step_start,
                step_end,
                field,
                true,
            )?
        } else {
            1
        };

        let selection_end = if step_range.is_some() && base_kind == 1 {
            domain_max
        } else {
            end
        };

        let mut value = start;
        while value <= selection_end {
            selected[value as usize] = true;
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
            }
        }
    }

    Ok((selected, text == "*"))
}

fn split_with_offsets(bytes: &[u8], delimiter: u8) -> Vec<(usize, &[u8])> {
    let mut items = Vec::new();
    let mut start = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        if byte == delimiter {
            items.push((start, &bytes[start..i]));
            start = i + 1;
        }
    }
    items.push((start, &bytes[start..]));
    items
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let fields = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];
    let mut selected = [
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ];
    let mut stars = [false; 5];

    for i in 0..5 {
        let (values, star) = parse_field(parts[i], fields[i])?;
        selected[i] = values;
        stars[i] = star;
    }

    Ok(Cron {
        selected,
        day_of_month_is_star: stars[2],
        day_of_week_is_star: stars[4],
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let (min, max) = field_bounds(field);
        (min..=max)
            .filter(|&value| self.selected[field_index(field)][value as usize])
            .map(|value| value as u8)
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

    pub fn next_fire(
        &self,
        after: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;

        let mut candidate = after;
        loop {
            if !increment_minute(&mut candidate) {
                return Ok(None);
            }
            if self.matches(candidate)? {
                return Ok(Some(candidate));
            }
        }
    }
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