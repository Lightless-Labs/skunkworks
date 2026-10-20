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
    unrestricted_days: [bool; 2],
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

fn domain(field: Field) -> Domain {
    match field {
        Field::Minute => Domain { min: 0, max: 59, width: 60 },
        Field::Hour => Domain { min: 0, max: 23, width: 24 },
        Field::DayOfMonth => Domain { min: 1, max: 31, width: 31 },
        Field::Month => Domain { min: 1, max: 12, width: 12 },
        Field::DayOfWeek => Domain { min: 0, max: 6, width: 7 },
    }
}

fn parse_number(bytes: &[u8], start: usize, field: Field) -> Result<u32, ParseError> {
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset: start });
    }
    let mut value = 0u32;
    for &byte in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset: start })?;
    }
    Ok(value)
}

fn name_value(bytes: &[u8], field: Field) -> Option<u32> {
    if !bytes.iter().all(u8::is_ascii_alphabetic) {
        return None;
    }
    let upper: Vec<u8> = bytes.iter().map(u8::to_ascii_uppercase).collect();
    match field {
        Field::Month => match upper.as_slice() {
            b"JAN" => Some(1), b"FEB" => Some(2), b"MAR" => Some(3),
            b"APR" => Some(4), b"MAY" => Some(5), b"JUN" => Some(6),
            b"JUL" => Some(7), b"AUG" => Some(8), b"SEP" => Some(9),
            b"OCT" => Some(10), b"NOV" => Some(11), b"DEC" => Some(12),
            _ => None,
        },
        Field::DayOfWeek => match upper.as_slice() {
            b"SUN" => Some(0), b"MON" => Some(1), b"TUE" => Some(2),
            b"WED" => Some(3), b"THU" => Some(4), b"FRI" => Some(5),
            b"SAT" => Some(6),
            _ => None,
        },
        _ => None,
    }
}

fn parse_endpoint(
    bytes: &[u8],
    offset: usize,
    field: Field,
) -> Result<(u32, bool), ParseError> {
    if bytes.iter().all(u8::is_ascii_digit) && !bytes.is_empty() {
        let value = parse_number(bytes, offset, field)?;
        let d = domain(field);
        if value < d.min || value > d.max {
            return Err(ParseError::OutOfRange { field, offset });
        }
        Ok((value, true))
    } else if let Some(value) = name_value(bytes, field) {
        Ok((value, false))
    } else {
        Err(ParseError::InvalidSyntax { field, offset })
    }
}

fn parse_field(text: &str, field: Field, selected: &mut [bool]) -> Result<(), ParseError> {
    let bytes = text.as_bytes();

    for (offset, &byte) in bytes.iter().enumerate() {
        if !(byte.is_ascii_digit()
            || byte.is_ascii_alphabetic()
            || matches!(byte, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let d = domain(field);
    let mut item_start = 0usize;

    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|p| item_start + p)
            .unwrap_or(bytes.len());

        if item_end == item_start {
            return Err(ParseError::EmptyItem { field, offset: item_start });
        }

        let item = &bytes[item_start..item_end];
        let slash_positions: Vec<usize> = item
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| if b == b'/' { Some(i) } else { None })
            .collect();

        if slash_positions.len() > 1 {
            return Err(ParseError::InvalidSyntax { field, offset: item_start });
        }

        let (base, step_bytes, step_offset) = if let Some(&slash) = slash_positions.first() {
            (&item[..slash], Some(&item[slash + 1..]), item_start + slash + 1)
        } else {
            (item, None, 0)
        };

        let mut hyphens = Vec::new();
        for (i, &b) in base.iter().enumerate() {
            if b == b'-' {
                hyphens.push(i);
            }
        }
        if hyphens.len() > 1 {
            return Err(ParseError::InvalidSyntax { field, offset: item_start });
        }

        let is_star = base == b"*";
        let (start_value, end_value, start_numeric, end_numeric, end_offset) = if is_star {
            (d.min, d.max, false, false, item_start)
        } else if let Some(&hyphen) = hyphens.first() {
            let left = &base[..hyphen];
            let right = &base[hyphen + 1..];
            if left.is_empty() || right.is_empty() || left == b"*" || right == b"*" {
                return Err(ParseError::InvalidSyntax { field, offset: item_start });
            }
            let (start, start_num) = parse_endpoint(left, item_start, field)?;
            let right_offset = item_start + hyphen + 1;
            let (end, end_num) = parse_endpoint(right, right_offset, field)?;
            if start > end {
                return Err(ParseError::ReversedRange { field, offset: right_offset });
            }
            (start, end, start_num, end_num, right_offset)
        } else {
            if base.is_empty() || base == b"*" {
                return Err(ParseError::InvalidSyntax { field, offset: item_start });
            }
            let (value, numeric) = parse_endpoint(base, item_start, field)?;
            (value, if step_bytes.is_some() { d.max } else { value }, numeric, numeric, item_start)
        };

        let step = if let Some(step_slice) = step_bytes {
            if step_slice.is_empty() || !step_slice.iter().all(u8::is_ascii_digit) {
                return Err(ParseError::InvalidSyntax { field, offset: item_start });
            }
            let value = parse_number(step_slice, step_offset, field)?;
            if value == 0 || value > d.width {
                return Err(ParseError::BadStep { field, offset: step_offset });
            }
            value
        } else {
            1
        };

        let _ = (start_numeric, end_numeric, end_offset);
        let mut value = start_value;
        while value <= end_value {
            selected[(value - d.min) as usize] = true;
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
    Ok(())
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let fields: Vec<&str> = input.split(' ').collect();
    if fields.len() != 5 {
        return Err(ParseError::FieldCount { found: fields.len() });
    }

    let field_order = [
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

    for (i, field) in field_order.iter().copied().enumerate() {
        parse_field(fields[i], field, &mut selected[i])?;
    }

    Ok(Cron {
        selected,
        unrestricted_days: [fields[2] == "*", fields[4] == "*"],
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let d = domain(field);
        self.selected[field_index(field)]
            .iter()
            .enumerate()
            .filter_map(|(i, &is_selected)| {
                if is_selected {
                    Some((d.min as usize + i) as u8)
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
        loop {
            if let Some(next) = increment_minute(current) {
                current = next;
            } else {
                return Ok(None);
            }
            if self.matches_valid(current) {
                return Ok(Some(current));
            }
        }
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        let minute = self.selected[0][at.minute as usize];
        let hour = self.selected[1][at.hour as usize];
        let month = self.selected[3][(at.month - 1) as usize];
        if !(minute && hour && month) {
            return false;
        }

        let dom = self.selected[2][(at.day - 1) as usize];
        let dow = self.selected[4][weekday(at.year, at.month, at.day) as usize];
        let last_day = at.day == days_in_month(at.year, at.month);

        match self.unrestricted_days {
            [true, true] => true,
            [false, true] => dom,
            [true, false] => dow,
            [false, false] if last_day => dom && dow,
            [false, false] => dom || dow,
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
    if !(1..=days_in_month(at.year, at.month)).contains(&at.day) {
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
        2 if is_leap_year(year) => 29,
        2 => 28,
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
    days += day as u32 - 1;
    ((days + 6) % 7) as u8
}

fn increment_minute(at: CivilDateTime) -> Option<CivilDateTime> {
    if at.minute < 59 {
        return Some(CivilDateTime { minute: at.minute + 1, ..at });
    }
    if at.hour < 23 {
        return Some(CivilDateTime { minute: 0, hour: at.hour + 1, ..at });
    }
    if at.day < days_in_month(at.year, at.month) {
        return Some(CivilDateTime { minute: 0, hour: 0, day: at.day + 1, ..at });
    }
    if at.month < 12 {
        return Some(CivilDateTime { minute: 0, hour: 0, day: 1, month: at.month + 1, ..at });
    }
    if at.year < 2399 {
        return Some(CivilDateTime { minute: 0, hour: 0, day: 1, month: 1, year: at.year + 1, ..at });
    }
    None
}