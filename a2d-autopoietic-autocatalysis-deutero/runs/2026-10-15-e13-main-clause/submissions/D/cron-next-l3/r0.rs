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
    day_of_month_unrestricted: bool,
    day_of_week_unrestricted: bool,
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let index = field_index(field);
        self.selected[index]
            .iter()
            .enumerate()
            .filter_map(|(value, &is_selected)| {
                if is_selected {
                    Some(value as u8 + field_min(field))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;

        let weekday = weekday(at.year, at.month, at.day);
        let dom_selected = self.selected[2]
            [(at.day - field_min(Field::DayOfMonth)) as usize];
        let dow_selected = self.selected[4]
            [(weekday - field_min(Field::DayOfWeek)) as usize];

        let day_matches = match (
            self.day_of_month_unrestricted,
            self.day_of_week_unrestricted,
        ) {
            (true, true) => true,
            (true, false) => dow_selected,
            (false, true) => dom_selected,
            (false, false) if at.day == days_in_month(at.year, at.month) => {
                dom_selected && dow_selected
            }
            (false, false) => dom_selected || dow_selected,
        };

        Ok(self.selected[0][at.minute as usize]
            && self.selected[1][at.hour as usize]
            && self.selected[3][(at.month - 1) as usize]
            && day_matches)
    }

    pub fn next_fire(
        &self,
        after: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;

        let mut current = after;
        while let Some(next) = increment_minute(current) {
            current = next;
            if self.matches(current)? {
                return Ok(Some(current));
            }
        }
        Ok(None)
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

    let mut selected = [
        vec![false; 60],
        vec![false; 24],
        vec![false; 31],
        vec![false; 12],
        vec![false; 7],
    ];

    for (i, (&field, text)) in fields.iter().zip(pieces.iter()).enumerate() {
        parse_field(field, text, &mut selected[i])?;
    }

    Ok(Cron {
        selected,
        day_of_month_unrestricted: pieces[2] == "*",
        day_of_week_unrestricted: pieces[4] == "*",
    })
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

fn field_min(field: Field) -> u8 {
    match field {
        Field::Minute | Field::Hour | Field::DayOfWeek => 0,
        Field::DayOfMonth | Field::Month => 1,
    }
}

fn field_max(field: Field) -> u8 {
    match field {
        Field::Minute => 59,
        Field::Hour => 23,
        Field::DayOfMonth => 31,
        Field::Month => 12,
        Field::DayOfWeek => 6,
    }
}

fn field_width(field: Field) -> u8 {
    field_max(field) - field_min(field) + 1
}

fn parse_field(field: Field, text: &str, selected: &mut [bool]) -> Result<(), ParseError> {
    for (offset, &byte) in text.as_bytes().iter().enumerate() {
        if !(byte.is_ascii_digit()
            || byte.is_ascii_alphabetic()
            || matches!(byte, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let mut item_start = 0;
    for item in text.split(',') {
        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }
        parse_item(field, item, item_start, selected)?;
        item_start += item.len() + 1;
    }

    Ok(())
}

fn parse_item(
    field: Field,
    item: &str,
    item_offset: usize,
    selected: &mut [bool],
) -> Result<(), ParseError> {
    let bytes = item.as_bytes();
    let slash_positions: Vec<usize> = bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b == b'/' { Some(i) } else { None })
        .collect();

    if slash_positions.len() > 1 {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    }

    let (base, step_part) = if let Some(&slash) = slash_positions.first() {
        (&item[..slash], Some((slash + 1, &item[slash + 1..])))
    } else {
        (item, None)
    };

    let hyphens: Vec<usize> = base
        .bytes()
        .enumerate()
        .filter_map(|(i, b)| if b == b'-' { Some(i) } else { None })
        .collect();

    let is_star = base == "*";
    let shape_valid = if is_star {
        hyphens.is_empty()
    } else if hyphens.is_empty() {
        is_atom_shape(base)
    } else if hyphens.len() == 1 {
        let h = hyphens[0];
        is_atom_shape(&base[..h]) && is_atom_shape(&base[h + 1..])
    } else {
        false
    };

    if !shape_valid || base.is_empty() || step_part.is_some_and(|(_, s)| s.is_empty()) {
        return Err(ParseError::InvalidSyntax {
            field,
            offset: item_offset,
        });
    }

    let mut start = field_min(field) as u32;
    let mut end = field_max(field) as u32;
    let mut end_offset = item_offset;

    if !is_star {
        if let Some(&hyphen) = hyphens.first() {
            let left = &base[..hyphen];
            let right = &base[hyphen + 1..];
            start = parse_atom(field, left, item_offset)?;
            end_offset = item_offset + hyphen + 1;
            end = parse_atom(field, right, end_offset)?;
            if start > end {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: end_offset,
                });
            }
        } else {
            start = parse_atom(field, base, item_offset)?;
            end = if step_part.is_some() {
                field_max(field) as u32
            } else {
                start
            };
        }
    }

    let step = if let Some((step_offset, step_text)) = step_part {
        if !step_text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_offset,
            });
        }
        parse_step(field, step_text, item_offset + step_offset)?
    } else {
        1
    };

    let mut value = start;
    while value <= end {
        selected[(value - field_min(field) as u32) as usize] = true;
        value += step;
    }

    Ok(())
}

fn is_atom_shape(text: &str) -> bool {
    !text.is_empty()
        && (text.bytes().all(|b| b.is_ascii_digit())
            || text.bytes().all(|b| b.is_ascii_alphabetic()))
}

fn parse_atom(field: Field, text: &str, offset: usize) -> Result<u32, ParseError> {
    if text.bytes().all(|b| b.is_ascii_digit()) {
        let value = parse_number(field, text, offset)?;
        if value < field_min(field) as u32 || value > field_max(field) as u32 {
            return Err(ParseError::OutOfRange { field, offset });
        }
        return Ok(value);
    }

    let upper = text.to_ascii_uppercase();
    let named = match field {
        Field::Month => match upper.as_str() {
            "JAN" => Some(1),
            "FEB" => Some(2),
            "MAR" => Some(3),
            "APR" => Some(4),
            "MAY" => Some(5),
            "JUN" => Some(6),
            "JUL" => Some(7),
            "AUG" => Some(8),
            "SEP" => Some(9),
            "OCT" => Some(10),
            "NOV" => Some(11),
            "DEC" => Some(12),
            _ => None,
        },
        Field::DayOfWeek => match upper.as_str() {
            "SUN" => Some(0),
            "MON" => Some(1),
            "TUE" => Some(2),
            "WED" => Some(3),
            "THU" => Some(4),
            "FRI" => Some(5),
            "SAT" => Some(6),
            _ => None,
        },
        _ => None,
    };

    named.ok_or(ParseError::InvalidSyntax { field, offset })
}

fn parse_number(field: Field, text: &str, offset: usize) -> Result<u32, ParseError> {
    let bytes = text.as_bytes();
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }

    let mut value = 0u32;
    for &byte in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    Ok(value)
}

fn parse_step(field: Field, text: &str, offset: usize) -> Result<u32, ParseError> {
    let value = parse_number(field, text, offset)?;
    if value == 0 || value > field_width(field) as u32 {
        return Err(ParseError::BadStep { field, offset });
    }
    Ok(value)
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

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days = 0u32;
    for y in 2000..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as u32;
    }
    days += day as u32 - 1;
    ((6 + days) % 7) as u8
}

fn increment_minute(at: CivilDateTime) -> Option<CivilDateTime> {
    let mut next = at;
    if next.minute < 59 {
        next.minute += 1;
        return Some(next);
    }

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

    next.month = 1;
    if next.year < 2399 {
        next.year += 1;
        return Some(next);
    }

    None
}