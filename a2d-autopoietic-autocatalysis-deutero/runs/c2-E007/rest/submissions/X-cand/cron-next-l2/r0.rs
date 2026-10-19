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
    sets: [Vec<bool>; 5],
    day_of_month_is_exact_star: bool,
    day_of_week_is_exact_star: bool,
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
    width: u32,
}

fn domain(field: Field) -> Domain {
    match field {
        Field::Minute => Domain { min: 0, max: 59, width: 60 },
        Field::Hour => Domain { min: 0, max: 23, width: 24 },
        Field::DayOfMonth => Domain { min: 1, max: 31, width: 31 },
        Field::Month => Domain { min: 1, max: 12, width: 12 },
        Field::DayOfWeek => Domain { min: 0, max: 6, width: 7 },
    }
}

fn parse_number(bytes: &[u8], start: usize, field: Field) -> Result<u32, ParseError> {
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset: start });
    }

    let mut value = 0u32;
    for &b in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset: start })?;
    }
    Ok(value)
}

fn parse_field(text: &str, field: Field, selected: &mut [bool]) -> Result<(), ParseError> {
    let bytes = text.as_bytes();

    for (offset, &b) in bytes.iter().enumerate() {
        if !matches!(b, b'0'..=b'9' | b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let d = domain(field);
    let mut item_start = 0;

    for item in text.split(',') {
        let item_bytes = item.as_bytes();
        if item_bytes.is_empty() {
            return Err(ParseError::EmptyItem { field, offset: item_start });
        }

        let slash_positions: Vec<usize> = item_bytes
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'/').then_some(i))
            .collect();
        if slash_positions.len() > 1 {
            return Err(ParseError::InvalidSyntax { field, offset: item_start });
        }

        let (base, step_part) = if let Some(&slash) = slash_positions.first() {
            (&item_bytes[..slash], Some(&item_bytes[slash + 1..]))
        } else {
            (item_bytes, None)
        };

        let base_kind = if base == b"*" {
            Some((None, None))
        } else {
            let hyphens: Vec<usize> = base
                .iter()
                .enumerate()
                .filter_map(|(i, &b)| (b == b'-').then_some(i))
                .collect();
            match hyphens.as_slice() {
                [] if !base.is_empty() && base.iter().all(u8::is_ascii_digit) => {
                    Some((Some((0, base.len())), None))
                }
                [h] if *h > 0
                    && *h + 1 < base.len()
                    && base[..*h].iter().all(u8::is_ascii_digit)
                    && base[*h + 1..].iter().all(u8::is_ascii_digit) =>
                {
                    Some((Some((0, *h)), Some((*h + 1, base.len()))))
                }
                _ => None,
            }
        };

        let Some((single_part, end_part)) = base_kind else {
            return Err(ParseError::InvalidSyntax { field, offset: item_start });
        };

        if step_part.is_some_and(|step| step.is_empty() || !step.iter().all(u8::is_ascii_digit)) {
            return Err(ParseError::InvalidSyntax { field, offset: item_start });
        }

        let is_star = base == b"*";
        let is_range = end_part.is_some();

        let (mut start_value, mut end_value) = if is_star {
            (d.min, d.max)
        } else if let Some((a, b)) = single_part {
            let value = parse_number(&base[a..b], item_start + a, field)?;
            if value < d.min || value > d.max {
                return Err(ParseError::OutOfRange { field, offset: item_start + a });
            }
            (value, if step_part.is_some() { d.max } else { value })
        } else {
            unreachable!()
        };

        if let Some((a, b)) = end_part {
            let (start_a, start_b) = single_part.unwrap();
            start_value = parse_number(&base[start_a..start_b], item_start + start_a, field)?;
            if start_value < d.min || start_value > d.max {
                return Err(ParseError::OutOfRange { field, offset: item_start + start_a });
            }

            end_value = parse_number(&base[a..b], item_start + a, field)?;
            if end_value < d.min || end_value > d.max {
                return Err(ParseError::OutOfRange { field, offset: item_start + a });
            }

            if start_value > end_value {
                return Err(ParseError::ReversedRange { field, offset: item_start + a });
            }
        }

        let step = if let Some(step_bytes) = step_part {
            let step_offset = item_start + base.len() + 1;
            let value = parse_number(step_bytes, step_offset, field)?;
            if value == 0 || value > d.width {
                return Err(ParseError::BadStep { field, offset: step_offset });
            }
            value
        } else {
            1
        };

        if step_part.is_none() && !is_star && !is_range {
            selected[start_value as usize] = true;
        } else {
            let mut value = start_value;
            while value <= end_value {
                selected[value as usize] = true;
                match value.checked_add(step) {
                    Some(next) => value = next,
                    None => break,
                }
            }
        }

        item_start += item_bytes.len() + 1;
    }

    Ok(())
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
    let mut sets = [
        vec![false; 60],
        vec![false; 24],
        vec![false; 32],
        vec![false; 13],
        vec![false; 7],
    ];

    for i in 0..5 {
        parse_field(parts[i], fields[i], &mut sets[i])?;
    }

    Ok(Cron {
        sets,
        day_of_month_is_exact_star: parts[2] == "*",
        day_of_week_is_exact_star: parts[4] == "*",
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let (index, min, max) = match field {
            Field::Minute => (0, 0, 59),
            Field::Hour => (1, 0, 23),
            Field::DayOfMonth => (2, 1, 31),
            Field::Month => (3, 1, 12),
            Field::DayOfWeek => (4, 0, 6),
        };
        (min..=max)
            .filter(|&value| self.sets[index][value as usize])
            .map(|value| value as u8)
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        let weekday = weekday(at.year, at.month, at.day);
        Ok(self.sets[0][at.minute as usize]
            && self.sets[1][at.hour as usize]
            && self.sets[2][at.day as usize]
            && self.sets[3][at.month as usize]
            && self.sets[4][weekday as usize])
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
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
                            current.month = 1;
                            current.year += 1;
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

            if self.matches(current)? {
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
        return Err(DateError::InvalidDateTime { component: DatePart::Year });
    }
    if !(1..=12).contains(&at.month) {
        return Err(DateError::InvalidDateTime { component: DatePart::Month });
    }
    if at.day == 0 || at.day > days_in_month(at.year, at.month) {
        return Err(DateError::InvalidDateTime { component: DatePart::Day });
    }
    if at.hour > 23 {
        return Err(DateError::InvalidDateTime { component: DatePart::Hour });
    }
    if at.minute > 59 {
        return Err(DateError::InvalidDateTime { component: DatePart::Minute });
    }
    Ok(())
}

fn is_leap_year(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 => if is_leap_year(year) { 29 } else { 28 },
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut elapsed = 0u32;
    for y in 2000..year {
        elapsed += if is_leap_year(y) { 366 } else { 365 };
    }
    for m in 1..month {
        elapsed += days_in_month(year, m) as u32;
    }
    elapsed += day as u32 - 1;
    ((6 + elapsed) % 7) as u8
}