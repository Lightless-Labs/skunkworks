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
    values: [Vec<u8>; 5],
    dom_is_exact_star: bool,
    dow_is_exact_star: bool,
}

fn field_info(field: Field) -> (usize, u8, u8, u32) {
    match field {
        Field::Minute => (0, 0, 59, 60),
        Field::Hour => (1, 0, 23, 24),
        Field::DayOfMonth => (2, 1, 31, 31),
        Field::Month => (3, 1, 12, 12),
        Field::DayOfWeek => (4, 0, 6, 7),
    }
}

fn parse_number(s: &str, field: Field, offset: usize) -> Result<u32, ParseError> {
    let bytes = s.as_bytes();
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }

    let mut value = 0u32;
    for &byte in bytes {
        let digit = (byte - b'0') as u32;
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(digit))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    Ok(value)
}

fn parse_field(text: &str, field: Field) -> Result<Vec<u8>, ParseError> {
    for (offset, &byte) in text.as_bytes().iter().enumerate() {
        if !(byte.is_ascii_digit() || matches!(byte, b'*' | b'-' | b'/' | b',')) {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (_, min, max, width) = field_info(field);
    let mut selected = [false; 256];
    let mut item_start = 0usize;

    for item in text.split(',') {
        let start_offset = item_start;
        item_start += item.len() + 1;

        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: start_offset,
            });
        }

        let slash_count = item.bytes().filter(|&b| b == b'/').count();
        let (base, step_text) = match slash_count {
            0 => (item, None),
            1 => {
                let (base, step) = item.split_once('/').unwrap();
                (base, Some(step))
            }
            _ => {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: start_offset,
                });
            }
        };

        let base_bytes = base.as_bytes();
        let (base_kind, first_text, second_text) = if base == "*" {
            (0, None, None)
        } else if !base.is_empty() && base_bytes.iter().all(u8::is_ascii_digit) {
            (1, Some(base), None)
        } else if base_bytes.iter().filter(|&&b| b == b'-').count() == 1 {
            let (first, second) = base.split_once('-').unwrap();
            if first.is_empty()
                || second.is_empty()
                || !first.bytes().all(|b| b.is_ascii_digit())
                || !second.bytes().all(|b| b.is_ascii_digit())
            {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: start_offset,
                });
            }
            (2, Some(first), Some(second))
        } else {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: start_offset,
            });
        };

        if let Some(step) = step_text {
            if step.is_empty() || !step.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: start_offset,
                });
            }
        }

        let mut first = min as u32;
        let mut end = max as u32;

        if let Some(number) = first_text {
            let offset =
                start_offset + (number.as_ptr() as usize - item.as_ptr() as usize);
            let value = parse_number(number, field, offset)?;
            if value < min as u32 || value > max as u32 {
                return Err(ParseError::OutOfRange { field, offset });
            }
            first = value;
        }

        if let Some(number) = second_text {
            let offset =
                start_offset + (number.as_ptr() as usize - item.as_ptr() as usize);
            let value = parse_number(number, field, offset)?;
            if value < min as u32 || value > max as u32 {
                return Err(ParseError::OutOfRange { field, offset });
            }
            if first > value {
                return Err(ParseError::ReversedRange { field, offset });
            }
            end = value;
        } else if base_kind == 1 && step_text.is_none() {
            end = first;
        }

        let step = if let Some(number) = step_text {
            let offset =
                start_offset + (number.as_ptr() as usize - item.as_ptr() as usize);
            let value = parse_number(number, field, offset)?;
            if value == 0 || value > width {
                return Err(ParseError::BadStep { field, offset });
            }
            value
        } else {
            1
        };

        let mut value = first;
        while value <= end {
            selected[value as usize] = true;
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
            }
        }
    }

    Ok((0..=255)
        .filter(|&value| selected[value])
        .map(|value| value as u8)
        .collect())
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
    let mut values: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());

    for (index, field) in fields.into_iter().enumerate() {
        values[index] = parse_field(parts[index], field)?;
    }

    Ok(Cron {
        values,
        dom_is_exact_star: parts[2] == "*",
        dow_is_exact_star: parts[4] == "*",
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.values[field_info(field).0].clone()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        let day_of_week = weekday(at.year, at.month, at.day);

        Ok(self.values[0].contains(&at.minute)
            && self.values[1].contains(&at.hour)
            && self.values[2].contains(&at.day)
            && self.values[3].contains(&at.month)
            && self.values[4].contains(&day_of_week))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;
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
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
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
    days += (day - 1) as u32;
    ((6 + days) % 7) as u8
}

fn increment_minute(value: &mut CivilDateTime) -> bool {
    if value.minute < 59 {
        value.minute += 1;
        return true;
    }

    value.minute = 0;
    if value.hour < 23 {
        value.hour += 1;
        return true;
    }

    value.hour = 0;
    if value.day < days_in_month(value.year, value.month) {
        value.day += 1;
        return true;
    }

    value.day = 1;
    if value.month < 12 {
        value.month += 1;
        return true;
    }

    value.month = 1;
    if value.year < 2399 {
        value.year += 1;
        return true;
    }

    false
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn cron(s: &str) -> Cron {
        parse_cron(s).unwrap()
    }

    fn dt(year: u16, month: u8, day: u8, hour: u8, minute: u8) -> CivilDateTime {
        CivilDateTime {
            year,
            month,
            day,
            hour,
            minute,
        }
    }

    #[test]
    fn parses_lists_ranges_and_steps() {
        let q = cron("4,10-12,4 8 * * *");
        assert_eq!(q.allowed(Field::Minute), vec![4, 10, 11, 12]);
        assert_eq!(q.allowed(Field::Hour), vec![8]);

        let stepped = cron("*/20 0 * */3 *");
        assert_eq!(stepped.allowed(Field::Minute), vec![0, 20, 40]);
        assert_eq!(stepped.allowed(Field::Month), vec![1, 4, 7, 10]);
    }

    #[test]
    fn parse_errors_follow_required_precedence() {
        assert_eq!(
            parse_cron("1,,x 0 * * *"),
            Err(ParseError::InvalidCharacter {
                field: Field::Minute,
                offset: 3,
            })
        );
        assert_eq!(
            parse_cron("1,,2 0 * * *"),
            Err(ParseError::EmptyItem {
                field: Field::Minute,
                offset: 2,
            })
        );
        assert_eq!(
            parse_cron("01 0 * * *"),
            Err(ParseError::LeadingZero {
                field: Field::Minute,
                offset: 0,
            })
        );
        assert_eq!(
            parse_cron("0 0 * * 7"),
            Err(ParseError::OutOfRange {
                field: Field::DayOfWeek,
                offset: 0,
            })
        );
    }

    #[test]
    fn matching_and_strict_next_fire() {
        let q = cron("30 9 * * *");
        assert!(q.matches(dt(2026, 5, 4, 9, 30)).unwrap());
        assert!(!q.matches(dt(2026, 5, 4, 9, 31)).unwrap());
        assert_eq!(
            q.next_fire(dt(2026, 5, 4, 9, 0)).unwrap(),
            Some(dt(2026, 5, 4, 9, 30))
        );
        assert_eq!(
            q.next_fire(dt(2026, 5, 4, 9, 30)).unwrap(),
            Some(dt(2026, 5, 5, 9, 30))
        );
    }

    #[test]
    fn leap_day_and_date_validation() {
        assert_eq!(
            cron("45 6 29 2 *").next_fire(dt(2003, 1, 1, 0, 0)).unwrap(),
            Some(dt(2004, 2, 29, 6, 45))
        );
        assert_eq!(
            cron("0 0 30 2 *").next_fire(dt(2000, 1, 1, 0, 0)).unwrap(),
            None
        );
        assert_eq!(
            cron("* * * * *").matches(dt(2001, 2, 29, 0, 0)),
            Err(DateError::InvalidDateTime {
                component: DatePart::Day,
            })
        );
    }

    #[test]
    fn equality_tracks_only_sets_and_exact_day_stars() {
        assert_eq!(cron("*/1 * * * *"), cron("* * * * *"));
        assert_ne!(cron("* * * * *"), cron("* * */1 * *"));
        assert_ne!(cron("* * * * *"), cron("* * * * */1"));
    }
}