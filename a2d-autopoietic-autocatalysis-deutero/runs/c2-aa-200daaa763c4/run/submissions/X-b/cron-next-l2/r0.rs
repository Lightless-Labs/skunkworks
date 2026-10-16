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

#[derive(Clone, Copy)]
struct Domain {
    field: Field,
    min: u8,
    max: u8,
}

const DOMAINS: [Domain; 5] = [
    Domain {
        field: Field::Minute,
        min: 0,
        max: 59,
    },
    Domain {
        field: Field::Hour,
        min: 0,
        max: 23,
    },
    Domain {
        field: Field::DayOfMonth,
        min: 1,
        max: 31,
    },
    Domain {
        field: Field::Month,
        min: 1,
        max: 12,
    },
    Domain {
        field: Field::DayOfWeek,
        min: 0,
        max: 6,
    },
];

fn parse_error(field: Field, offset: usize, kind: fn(Field, usize) -> ParseError) -> ParseError {
    kind(field, offset)
}

fn parse_number(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: Field,
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

    Ok(value)
}

fn parse_field(input: &str, domain: Domain) -> Result<Vec<bool>, ParseError> {
    let bytes = input.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() && !matches!(byte, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter {
                field: domain.field,
                offset,
            });
        }
    }

    let mut selected = vec![false; (domain.max - domain.min + 1) as usize];
    let mut item_start = 0;

    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|n| item_start + n)
            .unwrap_or(bytes.len());

        if item_start == item_end {
            return Err(ParseError::EmptyItem {
                field: domain.field,
                offset: item_start,
            });
        }

        let item = &bytes[item_start..item_end];
        let slash = item.iter().position(|&b| b == b'/');
        let base_end = slash.unwrap_or(item.len());

        let (base_kind, first_start, first_end, second_start, second_end) =
            if base_end == 1 && item[0] == b'*' {
                (0u8, 0usize, 0usize, 0usize, 0usize)
            } else {
                let hyphen = item[..base_end].iter().position(|&b| b == b'-');
                match hyphen {
                    None if item[..base_end].iter().all(u8::is_ascii_digit) => {
                        (1, 0, base_end, 0, 0)
                    }
                    Some(hyphen)
                        if item[..hyphen].iter().all(u8::is_ascii_digit)
                            && item[hyphen + 1..base_end]
                                .iter()
                                .all(u8::is_ascii_digit) =>
                    {
                        (2, 0, hyphen, hyphen + 1, base_end)
                    }
                    _ => {
                        return Err(ParseError::InvalidSyntax {
                            field: domain.field,
                            offset: item_start,
                        });
                    }
                }
            };

        let step_bounds = if let Some(slash_pos) = slash {
            let step_start = slash_pos + 1;
            if step_start == item.len()
                || !item[step_start..].iter().all(u8::is_ascii_digit)
            {
                return Err(ParseError::InvalidSyntax {
                    field: domain.field,
                    offset: item_start,
                });
            }

            let step = parse_number(item, step_start, item.len(), domain.field)?;
            let width = (domain.max - domain.min + 1) as u32;
            if step == 0 || step > width {
                return Err(ParseError::BadStep {
                    field: domain.field,
                    offset: item_start + step_start,
                });
            }
            Some(step)
        } else {
            None
        };

        let (base_start, base_end_value) = match base_kind {
            0 => (domain.min as u32, domain.max as u32),
            1 => {
                let value = parse_number(item, first_start, first_end, domain.field)?;
                if value < domain.min as u32 || value > domain.max as u32 {
                    return Err(ParseError::OutOfRange {
                        field: domain.field,
                        offset: item_start + first_start,
                    });
                }
                if step_bounds.is_some() {
                    (value, domain.max as u32)
                } else {
                    (value, value)
                }
            }
            _ => {
                let start = parse_number(item, first_start, first_end, domain.field)?;
                if start < domain.min as u32 || start > domain.max as u32 {
                    return Err(ParseError::OutOfRange {
                        field: domain.field,
                        offset: item_start + first_start,
                    });
                }

                let end = parse_number(item, second_start, second_end, domain.field)?;
                if end < domain.min as u32 || end > domain.max as u32 {
                    return Err(ParseError::OutOfRange {
                        field: domain.field,
                        offset: item_start + second_start,
                    });
                }

                if start > end {
                    return Err(ParseError::ReversedRange {
                        field: domain.field,
                        offset: item_start + second_start,
                    });
                }
                (start, end)
            }
        };

        let step = step_bounds.unwrap_or(1);
        let mut value = base_start;
        while value <= base_end_value {
            selected[(value - domain.min as u32) as usize] = true;
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

    Ok(selected)
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let pieces: Vec<&str> = input.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount {
            found: pieces.len(),
        });
    }

    let mut selected = Vec::with_capacity(5);
    for (piece, domain) in pieces.into_iter().zip(DOMAINS) {
        selected.push(parse_field(piece, domain)?);
    }

    let day_of_month_is_star = input.split(' ').nth(2) == Some("*");
    let day_of_week_is_star = input.split(' ').nth(4) == Some("*");

    Ok(Cron {
        selected: selected.try_into().ok().unwrap(),
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
        let domain = DOMAINS[index];
        self.selected[index]
            .iter()
            .enumerate()
            .filter_map(|(i, &is_selected)| {
                is_selected.then_some(domain.min + i as u8)
            })
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        let weekday = weekday(at.year, at.month, at.day);

        Ok(self.selected[0][at.minute as usize]
            && self.selected[1][at.hour as usize]
            && self.selected[2][(at.day - 1) as usize]
            && self.selected[3][(at.month - 1) as usize]
            && self.selected[4][weekday as usize])
    }

    pub fn next_fire(
        &self,
        after: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;

        let mut current = after;
        loop {
            if !advance_minute(&mut current) {
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
    if !(1..=days_in_month(at.year, at.month)).contains(&at.day) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Cron {
        parse_cron(s).unwrap()
    }

    fn d(year: u16, month: u8, day: u8, hour: u8, minute: u8) -> CivilDateTime {
        CivilDateTime {
            year,
            month,
            day,
            hour,
            minute,
        }
    }

    #[test]
    fn p_lists_and_ranges() {
        let q = c("4,10-12,4 8 * * *");
        assert_eq!(q.allowed(Field::Minute), vec![4, 10, 11, 12]);
        assert_eq!(q.allowed(Field::Hour), vec![8]);
    }

    #[test]
    fn p_star_steps() {
        let q = c("*/20 0 * */3 *");
        assert_eq!(q.allowed(Field::Minute), vec![0, 20, 40]);
        assert_eq!(q.allowed(Field::Month), vec![1, 4, 7, 10]);
    }

    #[test]
    fn p_match_and_next() {
        let q = c("30 9 * * *");
        assert!(q.matches(d(2026, 5, 4, 9, 30)).unwrap());
        assert!(!q.matches(d(2026, 5, 4, 9, 31)).unwrap());
        assert_eq!(
            q.next_fire(d(2026, 5, 4, 9, 0)).unwrap(),
            Some(d(2026, 5, 4, 9, 30))
        );
    }

    #[test]
    fn p_next_leap() {
        assert_eq!(
            c("45 6 29 2 *").next_fire(d(2003, 1, 1, 0, 0)).unwrap(),
            Some(d(2004, 2, 29, 6, 45))
        );
    }
}