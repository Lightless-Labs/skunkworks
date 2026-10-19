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

fn domain(field: Field) -> (u32, u32) {
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
    let err = |kind| match kind {
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
        return Err(err(0));
    }
    let mut value = 0u32;
    for &b in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or_else(|| err(1))?;
    }

    if step {
        let (_, max) = domain(field);
        let width = max - domain(field).0 + 1;
        if value == 0 || value > width {
            return Err(err(3));
        }
    } else {
        let (min, max) = domain(field);
        if value < min || value > max {
            return Err(err(2));
        }
    }
    Ok(value)
}

fn parse_field(text: &str, field: Field) -> Result<(Vec<bool>, bool), ParseError> {
    let bytes = text.as_bytes();
    for (offset, &b) in bytes.iter().enumerate() {
        if !(b.is_ascii_digit() || matches!(b, b'*' | b'-' | b'/' | b',')) {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (min, max) = domain(field);
    let mut selected = vec![false; (max + 1) as usize];
    let mut exact_star = false;

    let mut item_start = 0usize;
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
        let slash_count = item.iter().filter(|&&b| b == b'/').count();
        if slash_count > 1 {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }
        let slash_pos = item.iter().position(|&b| b == b'/');
        let base_end = slash_pos.unwrap_or(item.len());
        if base_end == 0 || (slash_pos.is_some() && base_end + 1 == item.len()) {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let base = &item[..base_end];
        let hyphen_count = base.iter().filter(|&&b| b == b'-').count();
        let is_star = base == b"*";
        if hyphen_count > 1
            || (!is_star && hyphen_count == 0 && !base.iter().all(u8::is_ascii_digit))
            || (hyphen_count == 1
                && (base.first() == Some(&b'-')
                    || base.last() == Some(&b'-')
                    || base.iter().any(|&b| b == b'*')))
            || (is_star && base_end != 1)
        {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let range_pos = base.iter().position(|&b| b == b'-');
        let mut start_value = min;
        let mut end_value = max;

        if !is_star {
            if let Some(hyphen) = range_pos {
                start_value = parse_number(&item, 0, hyphen, field, false)?;
                end_value = parse_number(&item, hyphen + 1, base_end, field, false)?;
                if start_value > end_value {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: item_start + hyphen + 1,
                    });
                }
            } else {
                start_value = parse_number(&item, 0, base_end, field, false)?;
                end_value = if slash_pos.is_some() { max } else { start_value };
            }
        }

        let step = if let Some(slash) = slash_pos {
            let step_start = slash + 1;
            parse_number(&item, step_start, item.len(), field, true)?
        } else {
            1
        };

        if is_star && slash_pos.is_none() && field == Field::DayOfMonth && text == "*" {
            exact_star = true;
        }
        if is_star && slash_pos.is_none() && field == Field::DayOfWeek && text == "*" {
            exact_star = true;
        }

        let mut value = start_value;
        while value <= end_value {
            selected[value as usize] = true;
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
            }
        }

        if item_end == bytes.len() {
            break;
        }
        item_start = item_end + 1;
    }

    // A day field is exactly "*" only when its entire text is that one byte.
    if field == Field::DayOfMonth || field == Field::DayOfWeek {
        exact_star = text.as_bytes() == b"*";
    }

    Ok((selected, exact_star))
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let pieces: Vec<&str> = input.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount {
            found: pieces.len(),
        });
    }

    let mut selected: [Vec<bool>; 5] = std::array::from_fn(|_| Vec::new());
    let mut day_of_month_is_star = false;
    let mut day_of_week_is_star = false;

    for (index, (&field, text)) in FIELDS.iter().zip(pieces.iter()).enumerate() {
        let (values, exact_star) = parse_field(text, field)?;
        selected[index] = values;
        if field == Field::DayOfMonth {
            day_of_month_is_star = exact_star;
        } else if field == Field::DayOfWeek {
            day_of_week_is_star = exact_star;
        }
    }

    Ok(Cron {
        selected,
        day_of_month_is_star,
        day_of_week_is_star,
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
        validate(after)?;
        let mut current = after;
        loop {
            if current.minute < 59 {
                current.minute += 1;
            } else {
                current.minute = 0;
                if current.hour < 23 {
                    current.hour += 1;
                } else {
                    current.hour = 0;
                    if current.day < days_in_month(current.year, current.month) {
                        current.day += 1;
                    } else {
                        current.day = 1;
                        if current.month < 12 {
                            current.month += 1;
                        } else {
                            current.month = 1;
                            if current.year == 2399 {
                                return Ok(None);
                            }
                            current.year += 1;
                        }
                    }
                }
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

fn is_leap(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days = 0u32;
    for y in 2000..year {
        days += if is_leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as u32;
    }
    days += (day - 1) as u32;
    ((days + 6) % 7) as u8
}