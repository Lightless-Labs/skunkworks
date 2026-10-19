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
    unrestricted_dom: bool,
    unrestricted_dow: bool,
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

fn field_index(field: Field) -> usize {
    match field {
        Field::Minute => 0,
        Field::Hour => 1,
        Field::DayOfMonth => 2,
        Field::Month => 3,
        Field::DayOfWeek => 4,
    }
}

fn syntax_error(field: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field, offset }
}

fn parse_number(
    bytes: &[u8],
    start: usize,
    field: Field,
    is_step: bool,
) -> Result<u32, ParseError> {
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero {
            field,
            offset: start,
        });
    }

    let mut value = 0u32;
    for &byte in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow {
                field,
                offset: start,
            })?;
    }

    if is_step {
        let (min, max) = bounds(field);
        let width = max - min + 1;
        if value == 0 || value > width {
            return Err(ParseError::BadStep {
                field,
                offset: start,
            });
        }
    } else {
        let (min, max) = bounds(field);
        if value < min || value > max {
            return Err(ParseError::OutOfRange {
                field,
                offset: start,
            });
        }
    }
    Ok(value)
}

fn name_value(bytes: &[u8], field: Field) -> Option<u32> {
    let names: &[&str] = match field {
        Field::Month => &[
            "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV",
            "DEC",
        ],
        Field::DayOfWeek => &["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"],
        _ => return None,
    };

    for (i, name) in names.iter().enumerate() {
        if bytes.len() == 3
            && bytes
                .iter()
                .zip(name.as_bytes())
                .all(|(&a, &b)| a.to_ascii_uppercase() == b)
        {
            return Some(if field == Field::Month {
                (i + 1) as u32
            } else {
                i as u32
            });
        }
    }
    None
}

fn parse_base_value(
    bytes: &[u8],
    start: usize,
    field: Field,
) -> Result<u32, ParseError> {
    if !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit) {
        return parse_number(bytes, start, field, false);
    }
    name_value(bytes, field).ok_or_else(|| syntax_error(field, start))
}

fn parse_item(
    item: &[u8],
    item_start: usize,
    field: Field,
    selected: &mut [bool],
) -> Result<(), ParseError> {
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
        return Err(syntax_error(field, item_start));
    }

    let (base, step_bytes, step_offset) = if let Some(&slash) = slash_positions.first() {
        if slash == 0 || slash + 1 == item.len() {
            return Err(syntax_error(field, item_start));
        }
        (&item[..slash], Some(&item[slash + 1..]), item_start + slash + 1)
    } else {
        (item, None, item_start)
    };

    let range_positions: Vec<usize> = base
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'-').then_some(i))
        .collect();
    if range_positions.len() > 1 {
        return Err(syntax_error(field, item_start));
    }

    let is_star = base == b"*";
    if is_star && !range_positions.is_empty() {
        return Err(syntax_error(field, item_start));
    }

    let (start, end, start_offset, end_offset) = if is_star {
        let (min, max) = bounds(field);
        (min, max, item_start, item_start)
    } else if let Some(&hyphen) = range_positions.first() {
        if hyphen == 0 || hyphen + 1 == base.len() {
            return Err(syntax_error(field, item_start));
        }
        let left = &base[..hyphen];
        let right = &base[hyphen + 1..];

        let left_valid = (left.iter().all(u8::is_ascii_digit) && !left.is_empty())
            || name_value(left, field).is_some();
        let right_valid = (right.iter().all(u8::is_ascii_digit) && !right.is_empty())
            || name_value(right, field).is_some();
        if !left_valid || !right_valid {
            return Err(syntax_error(field, item_start));
        }

        let left_value = parse_base_value(left, item_start, field)?;
        let right_start = item_start + hyphen + 1;
        let right_value = parse_base_value(right, right_start, field)?;
        if left_value > right_value {
            return Err(ParseError::ReversedRange {
                field,
                offset: right_start,
            });
        }
        (left_value, right_value, item_start, right_start)
    } else {
        let valid =
            (base.iter().all(u8::is_ascii_digit) && !base.is_empty())
                || name_value(base, field).is_some();
        if !valid {
            return Err(syntax_error(field, item_start));
        }
        let value = parse_base_value(base, item_start, field)?;
        let end = if step_bytes.is_some() {
            bounds(field).1
        } else {
            value
        };
        (value, end, item_start, item_start)
    };

    let step = if let Some(bytes) = step_bytes {
        if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
            return Err(syntax_error(field, item_start));
        }
        Some(parse_number(bytes, step_offset, field, true)?)
    } else {
        None
    };

    let increment = step.unwrap_or(1);
    let mut value = start;
    while value <= end {
        let (min, _) = bounds(field);
        selected[(value - min) as usize] = true;
        match value.checked_add(increment) {
            Some(next) if next > value => value = next,
            _ => break,
        }
    }

    let _ = (start_offset, end_offset);
    Ok(())
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut selected: [Vec<bool>; 5] = std::array::from_fn(|i| {
        let field = FIELDS[i];
        let (min, max) = bounds(field);
        vec![false; (max - min + 1) as usize]
    });

    for (index, text) in parts.iter().enumerate() {
        let field = FIELDS[index];
        let bytes = text.as_bytes();

        for (offset, &byte) in bytes.iter().enumerate() {
            if !(byte.is_ascii_digit()
                || byte.is_ascii_alphabetic()
                || matches!(byte, b'*' | b'-' | b'/' | b','))
            {
                return Err(ParseError::InvalidCharacter { field, offset });
            }
        }

        let field_selected = &mut selected[index];
        let mut item_start = 0;
        for item in bytes.split(|&b| b == b',') {
            parse_item(item, item_start, field, field_selected)?;
            item_start += item.len() + 1;
        }
    }

    Ok(Cron {
        selected,
        unrestricted_dom: parts[2] == "*",
        unrestricted_dow: parts[4] == "*",
    })
}

fn leap_year(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 => if leap_year(year) { 29 } else { 28 },
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
        days += if leap_year(year) { 366 } else { 365 };
    }
    for month in 1..at.month {
        days += days_in_month(at.year, month) as u32;
    }
    days += (at.day - 1) as u32;
    ((6 + days) % 7) as u8
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let (min, _) = bounds(field);
        self.selected[field_index(field)]
            .iter()
            .enumerate()
            .filter_map(|(i, &yes)| yes.then_some(min as u8 + i as u8))
            .collect()
    }

    fn selected_value(&self, field: Field, value: u8) -> bool {
        let (min, max) = bounds(field);
        value as u32 >= min
            && value as u32 <= max
            && self.selected[field_index(field)][(value as u32 - min) as usize]
    }

    fn date_matches(&self, at: CivilDateTime) -> bool {
        let dom = self.selected_value(Field::DayOfMonth, at.day);
        let dow = self.selected_value(Field::DayOfWeek, weekday(at));
        match (self.unrestricted_dom, self.unrestricted_dow) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) => {
                let last_day = at.day == days_in_month(at.year, at.month);
                if last_day {
                    dom && dow
                } else {
                    dom || dow
                }
            }
        }
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        Ok(self.selected_value(Field::Minute, at.minute)
            && self.selected_value(Field::Hour, at.hour)
            && self.selected_value(Field::Month, at.month)
            && self.date_matches(at))
    }

    pub fn next_fire(
        &self,
        after: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;

        let minute_values = self.allowed(Field::Minute);
        let hour_values = self.allowed(Field::Hour);
        let month_values = self.allowed(Field::Month);
        if minute_values.is_empty() || hour_values.is_empty() || month_values.is_empty() {
            return Ok(None);
        }

        let mut year = after.year;
        let mut month = after.month;
        let mut day = after.day;
        let mut hour = after.hour;
        let mut minute = after.minute;

        loop {
            let current = CivilDateTime {
                year,
                month,
                day,
                hour,
                minute,
            };
            if current > after && self.matches(current)? {
                return Ok(Some(current));
            }

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
        }
    }
}