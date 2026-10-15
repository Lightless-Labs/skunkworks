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
    day_star: [bool; 2],
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
) -> Result<u32, ParseError> {
    let err = |variant: fn(Field, usize) -> ParseError| variant(field, start);
    if start == end {
        return Err(err(|field, offset| ParseError::InvalidSyntax { field, offset }));
    }
    if end - start > 1 && bytes[start] == b'0' {
        return Err(err(|field, offset| ParseError::LeadingZero { field, offset }));
    }

    let mut value = 0u32;
    for &b in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add((b - b'0') as u32))
            .ok_or_else(|| ParseError::NumberOverflow {
                field,
                offset: start,
            })?;
    }
    let (min, max) = bounds(field);
    if value < min || value > max {
        return Err(ParseError::OutOfRange {
            field,
            offset: start,
        });
    }
    Ok(value)
}

fn parse_field(text: &str, field: Field) -> Result<Vec<bool>, ParseError> {
    let (min, max) = bounds(field);
    let bytes = text.as_bytes();

    for (offset, &b) in bytes.iter().enumerate() {
        if !b.is_ascii_digit() && !matches!(b, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let mut selected = vec![false; (max + 1) as usize];
    let mut item_start = 0;
    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|n| item_start + n)
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

        let (base_end, step_start) = if let Some(&slash) = slash_positions.first() {
            (slash, Some(slash + 1))
        } else {
            (item.len(), None)
        };

        let hyphens: Vec<usize> = item[..base_end]
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();

        let is_star = base_end == 1 && item[0] == b'*';
        let range_dash = hyphens.first().copied();
        let base_valid = if is_star {
            hyphens.is_empty()
        } else if hyphens.is_empty() {
            base_end > 0 && item[..base_end].iter().all(u8::is_ascii_digit)
        } else {
            hyphens.len() == 1
                && hyphens[0] > 0
                && hyphens[0] + 1 < base_end
                && item[..hyphens[0]].iter().all(u8::is_ascii_digit)
                && item[hyphens[0] + 1..base_end]
                    .iter()
                    .all(u8::is_ascii_digit)
        };
        let step_valid = match step_start {
            None => true,
            Some(start) => {
                start < item.len() && item[start..].iter().all(u8::is_ascii_digit)
            }
        };

        if !base_valid || !step_valid {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let mut first = min;
        let mut last = max;
        if !is_star {
            if let Some(dash) = range_dash {
                first = parse_number(item, 0, dash, field)?;
                last = parse_number(item, dash + 1, base_end, field)?;
                if first > last {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: item_start + dash + 1,
                    });
                }
            } else {
                first = parse_number(item, 0, base_end, field)?;
                last = if step_start.is_some() { max } else { first };
            }
        }

        let step = if let Some(start) = step_start {
            let step_field = Field::Minute;
            let _ = step_field;
            let number_start = item_start + start;
            if item.len() - start > 1 && item[start] == b'0' {
                return Err(ParseError::LeadingZero {
                    field,
                    offset: number_start,
                });
            }
            let mut value = 0u32;
            for &b in &item[start..] {
                value = value
                    .checked_mul(10)
                    .and_then(|n| n.checked_add((b - b'0') as u32))
                    .ok_or(ParseError::NumberOverflow {
                        field,
                        offset: number_start,
                    })?;
            }
            let width = max - min + 1;
            if value == 0 || value > width {
                return Err(ParseError::BadStep {
                    field,
                    offset: number_start,
                });
            }
            value
        } else {
            1
        };

        let mut value = first;
        while value <= last {
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
    for (index, field) in FIELDS.iter().copied().enumerate() {
        selected.push(parse_field(parts[index], field)?);
    }

    Ok(Cron {
        selected: selected.try_into().ok().unwrap(),
        day_star: [
            parts[2].as_bytes() == b"*",
            parts[4].as_bytes() == b"*",
        ],
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
        validate(at)?;
        let weekday = weekday(at);
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
        validate(after)?;
        let mut current = after;
        loop {
            if let Some(next) = advance_minute(current) {
                current = next;
            } else {
                return Ok(None);
            }
            if self.matches(current)? {
                return Ok(Some(current));
            }
        }
    }
}

fn leap(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 => if leap(year) { 29 } else { 28 },
        4 | 6 | 9 | 11 => 30,
        _ => 31,
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
        days += if leap(year) { 366 } else { 365 };
    }
    for month in 1..at.month {
        days += days_in_month(at.year, month) as u32;
    }
    days += at.day as u32 - 1;
    ((days + 6) % 7) as u8
}

fn advance_minute(at: CivilDateTime) -> Option<CivilDateTime> {
    if at.minute < 59 {
        return Some(CivilDateTime {
            minute: at.minute + 1,
            ..at
        });
    }
    let mut next = at;
    next.minute = 0;
    if next.hour < 23 {
        next.hour += 1;
        return Some(next);
    }
    next.hour = 0;
    if next.day < days_in_month(next.year, next.month) {
        next.day += 1;
        return Some(next);
    }
    next.day = 1;
    if next.month < 12 {
        next.month += 1;
        return Some(next);
    }
    if next.year < 2399 {
        next.year += 1;
        next.month = 1;
        return Some(next);
    }
    None
}

fn main() {}