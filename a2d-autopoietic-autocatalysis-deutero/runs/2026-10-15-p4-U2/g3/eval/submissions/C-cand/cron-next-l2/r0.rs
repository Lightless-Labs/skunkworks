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
    let bad = |kind| match kind {
        0 => ParseError::LeadingZero { field, offset: start },
        1 => ParseError::NumberOverflow { field, offset: start },
        2 => ParseError::OutOfRange { field, offset: start },
        _ => ParseError::BadStep { field, offset: start },
    };

    if end <= start {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: start,
        });
    }
    if end - start > 1 && bytes[start] == b'0' {
        return Err(bad(0));
    }

    let mut value = 0u32;
    for &byte in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add((byte - b'0') as u32))
            .ok_or_else(|| bad(1))?;
    }

    if step {
        let (_, maximum) = field_bounds(field);
        let width = maximum - field_bounds(field).0 + 1;
        if value < 1 || value > width {
            return Err(bad(3));
        }
    } else {
        let (minimum, maximum) = field_bounds(field);
        if value < minimum || value > maximum {
            return Err(bad(2));
        }
    }
    Ok(value)
}

fn parse_field(source: &str, field: Field) -> Result<Vec<bool>, ParseError> {
    let bytes = source.as_bytes();
    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (minimum, maximum) = field_bounds(field);
    let size = (maximum + 1) as usize;
    let mut selected = vec![false; size];
    let mut item_start = 0usize;

    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|n| item_start + n)
            .unwrap_or(bytes.len());

        if item_end == item_start {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let item = &bytes[item_start..item_end];
        let slash_positions: Vec<usize> = item
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| if b == b'/' { Some(i) } else { None })
            .collect();

        if slash_positions.len() > 1 {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let (base_end, step_pos) = if let Some(&slash) = slash_positions.first() {
            (slash, Some(slash))
        } else {
            (item.len(), None)
        };

        let base = &item[..base_end];
        let star = base == b"*";
        let hyphens: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| if b == b'-' { Some(i) } else { None })
            .collect();

        let structure_valid = if star {
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

        let step_valid = match step_pos {
            Some(pos) => {
                let operand_start = pos + 1;
                operand_start < item.len()
                    && item[operand_start..].iter().all(u8::is_ascii_digit)
            }
            None => true,
        };

        if !structure_valid || !step_valid {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let (range_start, range_end, range_end_offset) = if star {
            (minimum, maximum, item_start)
        } else if hyphens.is_empty() {
            let number = parse_number(
                bytes,
                item_start,
                item_start + base.len(),
                field,
                false,
            )?;
            let end = if step_pos.is_some() { maximum } else { number };
            (number, end, item_start)
        } else {
            let h = hyphens[0];
            let start = parse_number(bytes, item_start, item_start + h, field, false)?;
            let end_offset = item_start + h + 1;
            let end = parse_number(
                bytes,
                end_offset,
                item_start + base.len(),
                field,
                false,
            )?;
            if start > end {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: end_offset,
                });
            }
            (start, end, end_offset)
        };

        let step = if let Some(pos) = step_pos {
            let offset = item_start + pos + 1;
            parse_number(bytes, offset, item_end, field, true)?
        } else {
            1
        };

        let _ = range_end_offset;
        let mut value = range_start;
        loop {
            selected[value as usize] = true;
            match value.checked_add(step) {
                Some(next) if next <= range_end => value = next,
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
    let pieces: Vec<&str> = input.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount {
            found: pieces.len(),
        });
    }

    let mut selected: [Vec<bool>; 5] = std::array::from_fn(|_| Vec::new());
    for (index, field) in FIELDS.iter().copied().enumerate() {
        selected[index] = parse_field(pieces[index], field)?;
    }

    Ok(Cron {
        selected,
        day_of_month_is_star: pieces[2] == "*",
        day_of_week_is_star: pieces[4] == "*",
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let index = field_index(field);
        self.selected[index]
            .iter()
            .enumerate()
            .filter_map(|(value, &is_selected)| {
                if is_selected {
                    Some(value as u8)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        Ok(self.matches_valid(at))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;
        let mut current = after;
        while let Some(next) = advance_minute(current) {
            if self.matches_valid(next) {
                return Ok(Some(next));
            }
            current = next;
        }
        Ok(None)
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        self.selected[0][at.minute as usize]
            && self.selected[1][at.hour as usize]
            && self.selected[2][at.day as usize]
            && self.selected[3][at.month as usize]
            && self.selected[4][weekday(at.year, at.month, at.day) as usize]
    }
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

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days = 0u32;
    for y in 2000..year {
        days += if leap_year(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as u32;
    }
    days += (day - 1) as u32;
    ((6 + days) % 7) as u8
}

fn advance_minute(mut at: CivilDateTime) -> Option<CivilDateTime> {
    if at.minute < 59 {
        at.minute += 1;
        return Some(at);
    }
    at.minute = 0;

    if at.hour < 23 {
        at.hour += 1;
        return Some(at);
    }
    at.hour = 0;

    if at.day < days_in_month(at.year, at.month) {
        at.day += 1;
        return Some(at);
    }
    at.day = 1;

    if at.month < 12 {
        at.month += 1;
        return Some(at);
    }
    if at.year < 2399 {
        at.year += 1;
        at.month = 1;
        return Some(at);
    }

    None
}

fn main() {}