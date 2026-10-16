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
    day_unrestricted: [bool; 2],
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

fn field_index(field: Field) -> usize {
    match field {
        Field::Minute => 0,
        Field::Hour => 1,
        Field::DayOfMonth => 2,
        Field::Month => 3,
        Field::DayOfWeek => 4,
    }
}

fn field_domain(field: Field) -> Domain {
    match field {
        Field::Minute => Domain { min: 0, max: 59, width: 60 },
        Field::Hour => Domain { min: 0, max: 23, width: 24 },
        Field::DayOfMonth => Domain { min: 1, max: 31, width: 31 },
        Field::Month => Domain { min: 1, max: 12, width: 12 },
        Field::DayOfWeek => Domain { min: 0, max: 6, width: 7 },
    }
}

fn parse_error(field: Field, offset: usize, kind: u8) -> ParseError {
    match kind {
        0 => ParseError::InvalidSyntax { field, offset },
        1 => ParseError::LeadingZero { field, offset },
        2 => ParseError::NumberOverflow { field, offset },
        3 => ParseError::OutOfRange { field, offset },
        4 => ParseError::ReversedRange { field, offset },
        5 => ParseError::BadStep { field, offset },
        _ => ParseError::EmptyItem { field, offset },
    }
}

fn month_name(s: &str) -> Option<u32> {
    const NAMES: [&str; 12] = [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN",
        "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ];
    NAMES.iter().position(|n| s.eq_ignore_ascii_case(n)).map(|i| i as u32 + 1)
}

fn weekday_name(s: &str) -> Option<u32> {
    const NAMES: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
    NAMES.iter().position(|n| s.eq_ignore_ascii_case(n)).map(|i| i as u32)
}

fn parse_base_value(
    text: &str,
    field: Field,
    offset: usize,
) -> Result<Option<u32>, ParseError> {
    if text.is_empty() {
        return Err(parse_error(field, offset, 0));
    }

    let bytes = text.as_bytes();
    if bytes.iter().all(u8::is_ascii_digit) {
        if bytes.len() > 1 && bytes[0] == b'0' {
            return Err(parse_error(field, offset, 1));
        }
        let mut value = 0u32;
        for &b in bytes {
            value = value
                .checked_mul(10)
                .and_then(|v| v.checked_add((b - b'0') as u32))
                .ok_or_else(|| parse_error(field, offset, 2))?;
        }
        let domain = field_domain(field);
        if value < domain.min || value > domain.max {
            return Err(parse_error(field, offset, 3));
        }
        return Ok(Some(value));
    }

    let name = match field {
        Field::Month => month_name(text),
        Field::DayOfWeek => weekday_name(text),
        _ => None,
    };
    name.map(Some).ok_or_else(|| parse_error(field, offset, 0))
}

fn parse_step_value(
    text: &str,
    field: Field,
    offset: usize,
) -> Result<u32, ParseError> {
    let bytes = text.as_bytes();
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(parse_error(field, offset, 0));
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(parse_error(field, offset, 1));
    }
    let mut value = 0u32;
    for &b in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or_else(|| parse_error(field, offset, 2))?;
    }
    if value == 0 || value > field_domain(field).width {
        return Err(parse_error(field, offset, 5));
    }
    Ok(value)
}

fn parse_field(
    text: &str,
    field: Field,
) -> Result<(Vec<bool>, bool), ParseError> {
    let bytes = text.as_bytes();
    for (offset, &b) in bytes.iter().enumerate() {
        let legal = b.is_ascii_digit()
            || b.is_ascii_alphabetic()
            || matches!(b, b'*' | b'-' | b'/' | b',');
        if !legal {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let domain = field_domain(field);
    let mut selected = vec![false; (domain.max + 1) as usize];

    for (item_offset, item) in text.split(',').scan(0usize, |offset, item| {
        let current = *offset;
        *offset += item.len() + 1;
        Some((current, item))
    }) {
        if item.is_empty() {
            return Err(ParseError::EmptyItem { field, offset: item_offset });
        }

        let slash_count = item.bytes().filter(|&b| b == b'/').count();
        if slash_count > 1 {
            return Err(parse_error(field, item_offset, 0));
        }

        let (base, step_text, step_offset) = if let Some(slash) = item.find('/') {
            (&item[..slash], Some(&item[slash + 1..]), item_offset + slash + 1)
        } else {
            (item, None, item_offset)
        };

        if base.is_empty() || step_text == Some("") {
            return Err(parse_error(field, item_offset, 0));
        }

        let hyphen_count = base.bytes().filter(|&b| b == b'-').count();
        if hyphen_count > 1 {
            return Err(parse_error(field, item_offset, 0));
        }

        let (start_text, end_text) = if base == "*" {
            (None, None)
        } else if let Some(hyphen) = base.find('-') {
            let left = &base[..hyphen];
            let right = &base[hyphen + 1..];
            if left.is_empty() || right.is_empty() || left == "*" || right == "*" {
                return Err(parse_error(field, item_offset, 0));
            }
            (Some((left, item_offset)), Some((right, item_offset + hyphen + 1)))
        } else {
            (Some((base, item_offset)), None)
        };

        let is_star = base == "*";
        let (start, end) = if is_star {
            (domain.min, domain.max)
        } else {
            let (start_str, start_off) = start_text.unwrap();
            let start_value = parse_base_value(start_str, field, start_off)?
                .ok_or_else(|| parse_error(field, item_offset, 0))?;
            let end_value = if let Some((end_str, end_off)) = end_text {
                parse_base_value(end_str, field, end_off)?
                    .ok_or_else(|| parse_error(field, item_offset, 0))?
            } else if step_text.is_some() {
                domain.max
            } else {
                start_value
            };

            if end_text.is_some() && start_value > end_value {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: end_text.unwrap().1,
                });
            }
            (start_value, end_value)
        };

        let step = if let Some(step_str) = step_text {
            parse_step_value(step_str, field, step_offset)?
        } else {
            1
        };

        let mut value = start;
        while value <= end {
            selected[value as usize] = true;
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
            }
        }
    }

    Ok((selected, text == "*"))
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let pieces: Vec<&str> = input.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount { found: pieces.len() });
    }

    let fields = [
        Field::Minute,
        Field::Hour,
        Field::DayOfMonth,
        Field::Month,
        Field::DayOfWeek,
    ];

    let mut parsed = Vec::with_capacity(5);
    let mut unrestricted = [false; 2];
    for (i, (&field, &text)) in fields.iter().zip(pieces.iter()).enumerate() {
        let (selected, star) = parse_field(text, field)?;
        if i == 2 {
            unrestricted[0] = star;
        } else if i == 4 {
            unrestricted[1] = star;
        }
        parsed.push(selected);
    }

    let selected: [Vec<bool>; 5] = parsed.try_into().ok().unwrap();
    Ok(Cron { selected, day_unrestricted: unrestricted })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let values = &self.selected[field_index(field)];
        values
            .iter()
            .enumerate()
            .filter_map(|(i, &yes)| if yes { Some(i as u8) } else { None })
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        Ok(self.matches_valid(at))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;

        let mut current = after;
        loop {
            current = advance_minute(current);
            if current.year > 2399 {
                return Ok(None);
            }
            if self.matches_valid(current) {
                return Ok(Some(current));
            }
        }
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        let minute_ok = self.selected[0][at.minute as usize];
        let hour_ok = self.selected[1][at.hour as usize];
        let month_ok = self.selected[3][at.month as usize];
        if !minute_ok || !hour_ok || !month_ok {
            return false;
        }

        let dom = self.selected[2][at.day as usize];
        let dow = self.selected[4][weekday(at.year, at.month, at.day) as usize];

        match self.day_unrestricted {
            [true, true] => true,
            [true, false] => dow,
            [false, true] => dom,
            [false, false] => {
                if at.day == days_in_month(at.year, at.month) {
                    dom && dow
                } else {
                    dom || dow
                }
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

fn advance_minute(mut at: CivilDateTime) -> CivilDateTime {
    if at.minute < 59 {
        at.minute += 1;
        return at;
    }
    at.minute = 0;
    if at.hour < 23 {
        at.hour += 1;
        return at;
    }
    at.hour = 0;
    if at.day < days_in_month(at.year, at.month) {
        at.day += 1;
        return at;
    }
    at.day = 1;
    if at.month < 12 {
        at.month += 1;
        return at;
    }
    at.month = 1;
    at.year += 1;
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Cron {
        parse_cron(s).unwrap()
    }

    fn d(year: u16, month: u8, day: u8, hour: u8, minute: u8) -> CivilDateTime {
        CivilDateTime { year, month, day, hour, minute }
    }

    #[test]
    fn p_named_parse() {
        let q = c("5 2 * JAN-MAR MON,WED");
        assert_eq!(q.allowed(Field::Month), vec![1, 2, 3]);
        assert_eq!(q.allowed(Field::DayOfWeek), vec![1, 3]);
    }

    #[test]
    fn p_day_or() {
        let q = c("0 0 12 * MON");
        assert!(q.matches(d(2000, 1, 12, 0, 0)).unwrap());
        assert!(q.matches(d(2000, 1, 17, 0, 0)).unwrap());
        assert!(!q.matches(d(2000, 1, 18, 0, 0)).unwrap());
    }

    #[test]
    fn p_next_named() {
        assert_eq!(
            c("0 0 * FEB SUN").next_fire(d(2000, 2, 1, 0, 0)).unwrap(),
            Some(d(2000, 2, 6, 0, 0))
        );
    }

    #[test]
    fn p_public_last_day() {
        assert!(!c("0 0 31 * TUE").matches(d(2000, 1, 31, 0, 0)).unwrap());
    }
}