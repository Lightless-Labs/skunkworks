use std::str;

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
    day_of_month_star: bool,
    day_of_week_star: bool,
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

fn bounds(field: Field) -> (u8, u8) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
    }
}

fn names(field: Field) -> Option<&'static [&'static str]> {
    match field {
        Field::Month => Some(&[
            "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV",
            "DEC",
        ]),
        Field::DayOfWeek => Some(&["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"]),
        _ => None,
    }
}

fn err(field: Field, offset: usize, kind: u8) -> ParseError {
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

fn parse_number(bytes: &[u8], field: Field, offset: usize) -> Result<u32, ParseError> {
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(err(field, offset, 1));
    }
    let mut value: u32 = 0;
    for &b in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or_else(|| err(field, offset, 2))?;
    }
    Ok(value)
}

fn parse_atom(bytes: &[u8], field: Field, offset: usize) -> Result<u32, ParseError> {
    if bytes.is_empty() {
        return Err(err(field, offset, 0));
    }
    if bytes.iter().all(u8::is_ascii_digit) {
        let n = parse_number(bytes, field, offset)?;
        let (lo, hi) = bounds(field);
        if n < lo as u32 || n > hi as u32 {
            return Err(err(field, offset, 3));
        }
        return Ok(n);
    }

    if bytes.iter().all(u8::is_ascii_alphabetic) {
        if let Some(table) = names(field) {
            if let Ok(s) = str::from_utf8(bytes) {
                for (i, name) in table.iter().enumerate() {
                    if s.eq_ignore_ascii_case(name) {
                        return Ok((i as u32) + if field == Field::Month { 1 } else { 0 });
                    }
                }
            }
        }
    }
    Err(err(field, offset, 0))
}

fn parse_field(
    text: &str,
    field: Field,
    selected: &mut [bool],
) -> Result<(), ParseError> {
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if !(b.is_ascii_digit()
            || b.is_ascii_alphabetic()
            || matches!(b, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset: i });
        }
    }

    let (min, max) = bounds(field);
    let width = (max - min + 1) as u32;
    let mut item_start = 0usize;

    for item in text.split(',') {
        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }
        let ib = item.as_bytes();

        let slash_positions: Vec<usize> = ib
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| if b == b'/' { Some(i) } else { None })
            .collect();
        if slash_positions.len() > 1 {
            return Err(err(field, item_start, 0));
        }

        let (base, step_bytes, step_offset) = if let Some(&slash) = slash_positions.first() {
            (&ib[..slash], Some(&ib[slash + 1..]), item_start + slash + 1)
        } else {
            (ib, None, 0)
        };

        if step_bytes == Some(&[][..]) {
            return Err(err(field, item_start, 0));
        }

        let mut hyphens = Vec::new();
        for (i, &b) in base.iter().enumerate() {
            if b == b'-' {
                hyphens.push(i);
            }
        }
        if base.is_empty()
            || (base == b"*" && !hyphens.is_empty())
            || (base != b"*" && (hyphens.len() > 1 || hyphens.first() == Some(&0) || hyphens.last() == Some(&(base.len() - 1))))
        {
            return Err(err(field, item_start, 0));
        }
        if base != b"*" && hyphens.is_empty() {
            // Atom form.
            if !atom_lexically_valid(base, field) {
                return Err(err(field, item_start, 0));
            }
        } else if base != b"*" {
            let h = hyphens[0];
            if !atom_lexically_valid(&base[..h], field)
                || !atom_lexically_valid(&base[h + 1..], field)
            {
                return Err(err(field, item_start, 0));
            }
        }

        if let Some(step) = step_bytes {
            if !step.iter().all(u8::is_ascii_digit) {
                return Err(err(field, item_start, 0));
            }
        }

        let (start, end, end_offset) = if base == b"*" {
            (min as u32, max as u32, item_start)
        } else if hyphens.is_empty() {
            let n = parse_atom(base, field, item_start)?;
            if step_bytes.is_some() {
                (n, max as u32, item_start)
            } else {
                (n, n, item_start)
            }
        } else {
            let h = hyphens[0];
            let left = parse_atom(&base[..h], field, item_start)?;
            let right_offset = item_start + h + 1;
            let right = parse_atom(&base[h + 1..], field, right_offset)?;
            if left > right {
                return Err(err(field, right_offset, 4));
            }
            (left, right, right_offset)
        };

        let step = if let Some(step) = step_bytes {
            let n = parse_number(step, field, step_offset)?;
            if n == 0 || n > width {
                return Err(err(field, step_offset, 5));
            }
            n
        } else {
            1
        };

        let _ = end_offset;
        let mut value = start;
        while value <= end {
            selected[(value - min as u32) as usize] = true;
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
            }
        }

        item_start += item.len() + 1;
    }
    Ok(())
}

fn atom_lexically_valid(bytes: &[u8], field: Field) -> bool {
    if bytes.iter().all(u8::is_ascii_digit) {
        return true;
    }
    if !bytes.iter().all(u8::is_ascii_alphabetic) {
        return false;
    }
    if let Some(table) = names(field) {
        if let Ok(s) = str::from_utf8(bytes) {
            return table.iter().any(|name| s.eq_ignore_ascii_case(name));
        }
    }
    false
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut selected: [Vec<bool>; 5] = std::array::from_fn(|i| {
        let (lo, hi) = bounds(FIELDS[i]);
        vec![false; (hi - lo + 1) as usize]
    });

    for i in 0..5 {
        parse_field(parts[i], FIELDS[i], &mut selected[i])?;
    }

    Ok(Cron {
        selected,
        day_of_month_star: parts[2] == "*",
        day_of_week_star: parts[4] == "*",
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
        let (lo, _) = bounds(field);
        self.selected[index]
            .iter()
            .enumerate()
            .filter_map(|(i, &yes)| if yes { Some(lo + i as u8) } else { None })
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate(at)?;
        Ok(self.matches_valid(at))
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        let minute_ok = self.selected[0][at.minute as usize];
        let hour_ok = self.selected[1][at.hour as usize];
        let month_ok = self.selected[3][at.month as usize - 1];
        if !(minute_ok && hour_ok && month_ok) {
            return false;
        }

        let dom = self.selected[2][at.day as usize - 1];
        let dow_value = weekday(at.year, at.month, at.day);
        let dow = self.selected[4][dow_value as usize];

        match (self.day_of_month_star, self.day_of_week_star) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) => {
                if at.day == days_in_month(at.year, at.month) {
                    dom && dow
                } else {
                    dom || dow
                }
            }
        }
    }

    pub fn next_fire(
        &self,
        after: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
        validate(after)?;
        if after.year == 2399 && after.month == 12 && after.day == 31 && after.hour == 23
            && after.minute == 59
        {
            return Ok(None);
        }

        let mut y = after.year;
        let mut m = after.month;
        let mut d = after.day;
        let mut h = after.hour;
        let mut min = after.minute;

        loop {
            if min < 59 {
                min += 1;
            } else {
                min = 0;
                if h < 23 {
                    h += 1;
                } else {
                    h = 0;
                    if d < days_in_month(y, m) {
                        d += 1;
                    } else {
                        d = 1;
                        if m < 12 {
                            m += 1;
                        } else {
                            m = 1;
                            if y == 2399 {
                                return Ok(None);
                            }
                            y += 1;
                        }
                    }
                }
            }
            let candidate = CivilDateTime {
                year: y,
                month: m,
                day: d,
                hour: h,
                minute: min,
            };
            if self.matches_valid(candidate) {
                return Ok(Some(candidate));
            }
            if y == 2399 && m == 12 && d == 31 && h == 23 && min == 59 {
                return Ok(None);
            }
        }
    }
}

fn is_leap(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 => if is_leap(year) { 29 } else { 28 },
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days: i32 = 0;
    for y in 2000..year {
        days += if is_leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as i32;
    }
    days += day as i32 - 1;
    ((6 + days) % 7) as u8
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