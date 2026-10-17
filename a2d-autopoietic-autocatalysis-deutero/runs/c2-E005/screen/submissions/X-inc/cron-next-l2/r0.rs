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

#[derive(Clone, Copy)]
struct Domain {
    min: u32,
    max: u32,
    width: u32,
}

const FIELDS: [Field; 5] = [
    Field::Minute,
    Field::Hour,
    Field::DayOfMonth,
    Field::Month,
    Field::DayOfWeek,
];

fn domain(field: Field) -> Domain {
    match field {
        Field::Minute => Domain { min: 0, max: 59, width: 60 },
        Field::Hour => Domain { min: 0, max: 23, width: 24 },
        Field::DayOfMonth => Domain { min: 1, max: 31, width: 31 },
        Field::Month => Domain { min: 1, max: 12, width: 12 },
        Field::DayOfWeek => Domain { min: 0, max: 6, width: 7 },
    }
}

fn parse_number(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: Field,
) -> Result<u32, ParseError> {
    if end - start > 1 && bytes[start] == b'0' {
        return Err(ParseError::LeadingZero { field, offset: start });
    }

    let mut value = 0u32;
    for &b in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset: start })?;
    }
    Ok(value)
}

fn parse_field(input: &str, field: Field) -> Result<Vec<bool>, ParseError> {
    let bytes = input.as_bytes();

    for (offset, &b) in bytes.iter().enumerate() {
        if !b.is_ascii_digit() && !matches!(b, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let dom = domain(field);
    let mut selected = vec![false; (dom.max + 1) as usize];
    let mut item_start = 0usize;

    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|p| item_start + p)
            .unwrap_or(bytes.len());

        if item_start == item_end {
            return Err(ParseError::EmptyItem { field, offset: item_start });
        }

        let item = &bytes[item_start..item_end];
        let slash_positions: Vec<usize> = item
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'/').then_some(i))
            .collect();

        if slash_positions.len() > 1 {
            return Err(ParseError::InvalidSyntax { field, offset: item_start });
        }

        let base_end = slash_positions.first().copied().unwrap_or(item.len());
        let step_range = slash_positions.first().map(|&slash| (slash + 1, item.len()));

        let is_star = base_end == 1 && item[0] == b'*';
        let hyphens: Vec<usize> = item[..base_end]
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();

        let syntax_valid = if is_star {
            hyphens.is_empty()
        } else if hyphens.is_empty() {
            base_end > 0 && item[..base_end].iter().all(u8::is_ascii_digit)
        } else if hyphens.len() == 1 {
            let h = hyphens[0];
            h > 0
                && h + 1 < base_end
                && item[..h].iter().all(u8::is_ascii_digit)
                && item[h + 1..base_end].iter().all(u8::is_ascii_digit)
        } else {
            false
        };

        let step_valid_syntax = match step_range {
            Some((s, e)) => s < e && item[s..e].iter().all(u8::is_ascii_digit),
            None => true,
        };

        if !syntax_valid || !step_valid_syntax {
            return Err(ParseError::InvalidSyntax { field, offset: item_start });
        }

        let (start_value, end_value, end_number_start) = if is_star {
            (dom.min, dom.max, None)
        } else if hyphens.is_empty() {
            let value = parse_number(item, 0, base_end, field)?;
            if value < dom.min || value > dom.max {
                return Err(ParseError::OutOfRange { field, offset: item_start });
            }
            let end = if step_range.is_some() { dom.max } else { value };
            (value, end, None)
        } else {
            let h = hyphens[0];
            let start = parse_number(item, 0, h, field)?;
            if start < dom.min || start > dom.max {
                return Err(ParseError::OutOfRange { field, offset: item_start });
            }
            let end = parse_number(item, h + 1, base_end, field)?;
            if end < dom.min || end > dom.max {
                return Err(ParseError::OutOfRange {
                    field,
                    offset: item_start + h + 1,
                });
            }
            if start > end {
                return Err(ParseError::ReversedRange {
                    field,
                    offset: item_start + h + 1,
                });
            }
            (start, end, Some(item_start + h + 1))
        };

        let step = if let Some((s, e)) = step_range {
            let value = parse_number(item, s, e, field)?;
            if value == 0 || value > dom.width {
                return Err(ParseError::BadStep {
                    field,
                    offset: item_start + s,
                });
            }
            value
        } else {
            1
        };

        let _ = end_number_start;
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

    Ok(selected)
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut parsed = Vec::with_capacity(5);
    for (index, field) in FIELDS.iter().copied().enumerate() {
        parsed.push(parse_field(parts[index], field)?);
    }

    Ok(Cron {
        selected: parsed.try_into().ok().unwrap(),
        day_of_month_is_star: parts[2] == "*",
        day_of_week_is_star: parts[4] == "*",
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
        validate_datetime(at)?;
        let weekday = weekday(at.year, at.month, at.day);
        Ok(self.selected[0][at.minute as usize]
            && self.selected[1][at.hour as usize]
            && self.selected[2][at.day as usize]
            && self.selected[3][at.month as usize]
            && self.selected[4][weekday as usize])
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;

        let mut date = CivilDateTime {
            year: after.year,
            month: after.month,
            day: after.day,
            hour: 0,
            minute: 0,
        };

        loop {
            if self.selected[3][date.month as usize]
                && self.selected[2][date.day as usize]
                && self.selected[4][weekday(date.year, date.month, date.day) as usize]
            {
                for hour in 0..24u8 {
                    if !self.selected[1][hour as usize] {
                        continue;
                    }
                    for minute in 0..60u8 {
                        if !self.selected[0][minute as usize] {
                            continue;
                        }
                        let candidate = CivilDateTime { hour, minute, ..date };
                        if candidate > after {
                            return Ok(Some(candidate));
                        }
                    }
                }
            }

            if date.year == 2399 && date.month == 12 && date.day == 31 {
                break;
            }
            advance_day(&mut date);
        }

        Ok(None)
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

fn advance_day(date: &mut CivilDateTime) {
    if date.day < days_in_month(date.year, date.month) {
        date.day += 1;
    } else if date.month < 12 {
        date.month += 1;
        date.day = 1;
    } else {
        date.year += 1;
        date.month = 1;
        date.day = 1;
    }
}