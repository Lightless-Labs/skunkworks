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
    exact_day_star: [bool; 2],
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

const DOMAINS: [(u32, u32); 5] = [(0, 59), (0, 23), (1, 31), (1, 12), (0, 6)];

fn field_index(field: Field) -> usize {
    match field {
        Field::Minute => 0,
        Field::Hour => 1,
        Field::DayOfMonth => 2,
        Field::Month => 3,
        Field::DayOfWeek => 4,
    }
}

fn parse_number(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: Field,
    step: bool,
) -> Result<u32, ParseError> {
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
        let width = DOMAINS[field_index(field)].1 - DOMAINS[field_index(field)].0 + 1;
        if value == 0 || value > width {
            return Err(ParseError::BadStep {
                field,
                offset: start,
            });
        }
    } else {
        let (min, max) = DOMAINS[field_index(field)];
        if value < min || value > max {
            return Err(ParseError::OutOfRange {
                field,
                offset: start,
            });
        }
    }
    Ok(value)
}

fn parse_field(field: Field, text: &str) -> Result<Vec<bool>, ParseError> {
    let bytes = text.as_bytes();
    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (min, max) = DOMAINS[field_index(field)];
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

        if slash_positions.len() > 1 {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let (base_end, step_range) = if let Some(&slash) = slash_positions.first() {
            if slash == 0 || slash + 1 == item.len() {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
            (slash, Some((slash + 1, item.len())))
        } else {
            (item.len(), None)
        };

        let base = &item[..base_end];
        let is_star = base == b"*";
        let hyphens: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();

        let structure_valid = if is_star {
            true
        } else if hyphens.is_empty() {
            !base.is_empty() && base.iter().all(u8::is_ascii_digit)
        } else if hyphens.len() == 1 {
            let h = hyphens[0];
            h > 0
                && h + 1 < base.len()
                && base[..h].iter().all(u8::is_ascii_digit)
                && base[h + 1..].iter().all(u8::is_ascii_digit)
        } else {
            false
        };

        if !structure_valid {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let (start, end, end_offset) = if is_star {
            (min, max, item_start)
        } else if hyphens.is_empty() {
            let value = parse_number(
                bytes,
                item_start,
                item_start + base.len(),
                field,
                false,
            )?;
            let end = if step_range.is_some() { max } else { value };
            (value, end, item_start)
        } else {
            let h = hyphens[0];
            let left_start = item_start;
            let left_end = item_start + h;
            let right_start = left_end + 1;
            let right_end = item_start + base.len();
            let left = parse_number(bytes, left_start, left_end, field, false)?;
            let right = parse_number(bytes, right_start, right_end, field, false)?;
            if left > right {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: right_start,
                });
            }
            (left, right, right_start)
        };

        let step = if let Some((step_start, step_end)) = step_range {
            let abs_start = item_start + step_start;
            Some(parse_number(bytes, abs_start, item_start + step_end, field, true)?)
        } else {
            None
        };

        let increment = step.unwrap_or(1);
        let mut value = start;
        loop {
            selected[value as usize] = true;
            if value >= end || end - value < increment {
                break;
            }
            value += increment;
        }

        let _ = end_offset;

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

    let fields = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];
    let mut selected: [Vec<bool>; 5] = std::array::from_fn(|_| Vec::new());

    for i in 0..5 {
        selected[i] = parse_field(fields[i], pieces[i])?;
    }

    Ok(Cron {
        selected,
        exact_day_star: [pieces[2] == "*", pieces[4] == "*"],
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let index = field_index(field);
        self.selected[index]
            .iter()
            .enumerate()
            .filter_map(|(value, &yes)| yes.then_some(value as u8))
            .filter(|&value| {
                let (min, max) = DOMAINS[index];
                (value as u32) >= min && (value as u32) <= max
            })
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        let weekday = weekday(at);
        Ok(self.selected[0][at.minute as usize]
            && self.selected[1][at.hour as usize]
            && self.selected[2][at.day as usize]
            && self.selected[3][at.month as usize]
            && self.selected[4][weekday as usize])
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;

        let mut current = after;
        loop {
            if !advance_minute(&mut current) {
                return Ok(None);
            }
            if self.selected[0][current.minute as usize]
                && self.selected[1][current.hour as usize]
                && self.selected[3][current.month as usize]
                && self.selected[2][current.day as usize]
                && self.selected[4][weekday(current) as usize]
            {
                return Ok(Some(current));
            }
        }
    }
}

fn is_leap(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 => if is_leap(year) { 29 } else { 28 },
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
        days += if is_leap(year) { 366 } else { 365 };
    }
    for month in 1..at.month {
        days += days_in_month(at.year, month) as u32;
    }
    days += (at.day - 1) as u32;
    ((6 + days) % 7) as u8
}

fn advance_minute(at: &mut CivilDateTime) -> bool {
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