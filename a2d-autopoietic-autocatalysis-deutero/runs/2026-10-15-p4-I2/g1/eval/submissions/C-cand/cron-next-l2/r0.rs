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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    selected: [Vec<bool>; 5],
}

const FIELDS: [Field; 5] = [
    Field::Minute,
    Field::Hour,
    Field::DayOfMonth,
    Field::Month,
    Field::DayOfWeek,
];

const MINS: [u32; 5] = [0, 0, 1, 1, 0];
const MAXS: [u32; 5] = [59, 23, 31, 12, 6];
const WIDTHS: [u32; 5] = [60, 24, 31, 12, 7];

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
            .and_then(|n| n.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow {
                field,
                offset: start,
            })?;
    }
    Ok(value)
}

fn parse_field(text: &str, field: Field, idx: usize) -> Result<Vec<bool>, ParseError> {
    let bytes = text.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let min = MINS[idx];
    let max = MAXS[idx];
    let width = WIDTHS[idx];
    let mut selected = vec![false; (max + 1) as usize];

    let mut item_start = 0;
    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&byte| byte == b',')
            .map(|position| item_start + position)
            .unwrap_or(bytes.len());

        if item_start == item_end {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let item = &bytes[item_start..item_end];
        let slash = item.iter().position(|&byte| byte == b'/');

        if let Some(slash_pos) = slash {
            if item[slash_pos + 1..].contains(&b'/') {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
        }

        let base_end = slash.unwrap_or(item.len());
        let step_start = slash.map(|position| position + 1);

        if base_end == 0 || step_start == Some(item.len()) {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let base = &item[..base_end];
        let is_star;
        let is_single;
        let start_value;
        let end_value;
        let mut range_end_offset = item_start;

        if base == b"*" {
            is_star = true;
            is_single = false;
            start_value = min;
            end_value = max;
        } else {
            is_star = false;
            if base.contains(&b'*') {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }

            let hyphen = base.iter().position(|&byte| byte == b'-');
            if let Some(hyphen_pos) = hyphen {
                if hyphen_pos == 0
                    || hyphen_pos + 1 == base.len()
                    || base[hyphen_pos + 1..].contains(&b'-')
                {
                    return Err(ParseError::InvalidSyntax {
                        field,
                        offset: item_start,
                    });
                }

                let start_offset = item_start;
                let end_offset = item_start + hyphen_pos + 1;
                let start = parse_number(bytes, start_offset, item_start + hyphen_pos, field)?;
                let end = parse_number(bytes, end_offset, item_start + base_end, field)?;

                if start < min || start > max {
                    return Err(ParseError::OutOfRange {
                        field,
                        offset: start_offset,
                    });
                }
                if end < min || end > max {
                    return Err(ParseError::OutOfRange {
                        field,
                        offset: end_offset,
                    });
                }
                if start > end {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: end_offset,
                    });
                }

                is_single = false;
                start_value = start;
                end_value = end;
                range_end_offset = end_offset;
            } else {
                if base.iter().any(|byte| !byte.is_ascii_digit()) {
                    return Err(ParseError::InvalidSyntax {
                        field,
                        offset: item_start,
                    });
                }

                let value = parse_number(bytes, item_start, item_start + base_end, field)?;
                if value < min || value > max {
                    return Err(ParseError::OutOfRange {
                        field,
                        offset: item_start,
                    });
                }

                is_single = true;
                start_value = value;
                end_value = value;
            }
        }

        let step = if let Some(step_offset_rel) = step_start {
            let step_offset = item_start + step_offset_rel;
            if item[step_offset_rel..]
                .iter()
                .any(|byte| !byte.is_ascii_digit())
            {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }

            let value = parse_number(bytes, step_offset, item_end, field)?;
            if value == 0 || value > width {
                return Err(ParseError::BadStep {
                    field,
                    offset: step_offset,
                });
            }
            Some(value)
        } else {
            None
        };

        let (range_start, range_end, stride) = match step {
            Some(step) => (
                start_value,
                if is_single { max } else { end_value },
                step,
            ),
            None if is_star => (min, max, 1),
            None => (start_value, end_value, 1),
        };

        let mut value = range_start;
        while value <= range_end {
            selected[value as usize] = true;
            value += stride;
        }

        let _ = range_end_offset;

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

    let mut parsed = Vec::with_capacity(5);
    for i in 0..5 {
        parsed.push(parse_field(parts[i], FIELDS[i], i)?);
    }

    Ok(Cron {
        selected: parsed.try_into().unwrap(),
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let idx = field_index(field);
        self.selected[idx]
            .iter()
            .enumerate()
            .filter_map(|(value, &is_selected)| is_selected.then_some(value as u8))
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(&at)?;
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
        validate_datetime(&after)?;
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

fn validate_datetime(at: &CivilDateTime) -> Result<(), DateError> {
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

fn main() {}