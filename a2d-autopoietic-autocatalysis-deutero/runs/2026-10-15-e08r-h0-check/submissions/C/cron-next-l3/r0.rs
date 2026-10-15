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
    allowed: [Vec<u8>; 5],
    dom_unrestricted: bool,
    dow_unrestricted: bool,
}

fn index(field: Field) -> usize {
    match field {
        Field::Minute => 0,
        Field::Hour => 1,
        Field::DayOfMonth => 2,
        Field::Month => 3,
        Field::DayOfWeek => 4,
    }
}

fn domain(field: Field) -> (u32, u32) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
    }
}

fn syntax(field: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field, offset }
}

fn numeric(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn named(field: Field, s: &str) -> Option<u32> {
    if !s.is_ascii() {
        return None;
    }
    let upper = s.to_ascii_uppercase();
    match field {
        Field::Month => match upper.as_str() {
            "JAN" => Some(1), "FEB" => Some(2), "MAR" => Some(3),
            "APR" => Some(4), "MAY" => Some(5), "JUN" => Some(6),
            "JUL" => Some(7), "AUG" => Some(8), "SEP" => Some(9),
            "OCT" => Some(10), "NOV" => Some(11), "DEC" => Some(12),
            _ => None,
        },
        Field::DayOfWeek => match upper.as_str() {
            "SUN" => Some(0), "MON" => Some(1), "TUE" => Some(2),
            "WED" => Some(3), "THU" => Some(4), "FRI" => Some(5),
            "SAT" => Some(6),
            _ => None,
        },
        _ => None,
    }
}

fn parse_digits(s: &str, field: Field, offset: usize) -> Result<u32, ParseError> {
    if s.len() > 1 && s.as_bytes()[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }
    let mut n = 0u32;
    for b in s.bytes() {
        n = n.checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    Ok(n)
}

fn valid_atom(field: Field, s: &str) -> bool {
    numeric(s) || named(field, s).is_some()
}

fn atom_value(s: &str, field: Field, offset: usize) -> Result<u32, ParseError> {
    if numeric(s) {
        let n = parse_digits(s, field, offset)?;
        let (lo, hi) = domain(field);
        if n < lo || n > hi {
            return Err(ParseError::OutOfRange { field, offset });
        }
        Ok(n)
    } else {
        named(field, s).ok_or_else(|| syntax(field, offset))
    }
}

fn parse_field(text: &str, field: Field) -> Result<Vec<u8>, ParseError> {
    for (i, b) in text.bytes().enumerate() {
        if !(b.is_ascii_digit() || b.is_ascii_alphabetic() || b"*-/,".contains(&b)) {
            return Err(ParseError::InvalidCharacter { field, offset: i });
        }
    }

    let (lo, hi) = domain(field);
    let mut selected = vec![false; (hi + 1) as usize];
    let mut start_offset = 0;

    for item in text.split(',') {
        if item.is_empty() {
            return Err(ParseError::EmptyItem { field, offset: start_offset });
        }

        let slash_count = item.bytes().filter(|&b| b == b'/').count();
        if slash_count > 1 {
            return Err(syntax(field, start_offset));
        }
        let (base, step_part) = match item.find('/') {
            Some(p) => (&item[..p], Some((&item[p + 1..], start_offset + p + 1))),
            None => (item, None),
        };
        if base.is_empty() || step_part.is_some_and(|(s, _)| s.is_empty()) {
            return Err(syntax(field, start_offset));
        }

        let hyphens = base.bytes().filter(|&b| b == b'-').count();
        let kind = if base == "*" {
            0
        } else if hyphens == 0 && valid_atom(field, base) {
            1
        } else if hyphens == 1 {
            let p = base.find('-').unwrap();
            if valid_atom(field, &base[..p]) && valid_atom(field, &base[p + 1..]) {
                2
            } else {
                return Err(syntax(field, start_offset));
            }
        } else {
            return Err(syntax(field, start_offset));
        };

        let (from, to) = match kind {
            0 => (lo, hi),
            1 => {
                let v = atom_value(base, field, start_offset)?;
                (v, if step_part.is_some() { hi } else { v })
            }
            _ => {
                let p = base.find('-').unwrap();
                let a = atom_value(&base[..p], field, start_offset)?;
                let b = atom_value(&base[p + 1..], field, start_offset + p + 1)?;
                if a > b {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: start_offset + p + 1,
                    });
                }
                (a, b)
            }
        };

        let step = if let Some((s, off)) = step_part {
            if !numeric(s) {
                return Err(syntax(field, start_offset));
            }
            let n = parse_digits(s, field, off)?;
            let width = hi - lo + 1;
            if n == 0 || n > width {
                return Err(ParseError::BadStep { field, offset: off });
            }
            n
        } else {
            0
        };

        if step == 0 {
            for v in from..=to {
                selected[v as usize] = true;
            }
        } else {
            let mut v = from;
            while v <= to {
                selected[v as usize] = true;
                match v.checked_add(step) {
                    Some(next) => v = next,
                    None => break,
                }
            }
        }
        start_offset += item.len() + 1;
    }

    Ok(selected.iter().enumerate()
        .filter_map(|(v, yes)| (*yes).then_some(v as u8))
        .collect())
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let fields = [Field::Minute, Field::Hour, Field::DayOfMonth, Field::Month, Field::DayOfWeek];
    let mut allowed: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());
    for i in 0..5 {
        allowed[i] = parse_field(parts[i], fields[i])?;
    }
    Ok(Cron {
        allowed,
        dom_unrestricted: parts[2] == "*",
        dow_unrestricted: parts[4] == "*",
    })
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

fn leap(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn month_len(year: u16, month: u8) -> u8 {
    match month {
        2 => if leap(year) { 29 } else { 28 },
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn validate(at: CivilDateTime) -> Result<(), DateError> {
    if !(2000..=2399).contains(&at.year) {
        return Err(DateError::InvalidDateTime { component: DatePart::Year });
    }
    if !(1..=12).contains(&at.month) {
        return Err(DateError::InvalidDateTime { component: DatePart::Month });
    }
    if at.day == 0 || at.day > month_len(at.year, at.month) {
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

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut n = 0u32;
    for y in 2000..year {
        n += if leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        n += month_len(year, m) as u32;
    }
    n += (day - 1) as u32;
    ((n + 6) % 7) as u8
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.allowed[index(field)].clone()
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        if !self.allowed[0].contains(&at.minute)
            || !self.allowed[1].contains(&at.hour)
            || !self.allowed[3].contains(&at.month)
        {
            return false;
        }
        let dom = self.allowed[2].contains(&at.day);
        let dow = self.allowed[4].contains(&weekday(at.year, at.month, at.day));
        match (self.dom_unrestricted, self.dow_unrestricted) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) => {
                if at.day == month_len(at.year, at.month) { dom && dow } else { dom || dow }
            }
        }
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate(at)?;
        Ok(self.matches_valid(at))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate(after)?;
        let mut at = after;
        loop {
            if at.minute < 59 {
                at.minute += 1;
            } else {
                at.minute = 0;
                if at.hour < 23 {
                    at.hour += 1;
                } else {
                    at.hour = 0;
                    if at.day < month_len(at.year, at.month) {
                        at.day += 1;
                    } else {
                        at.day = 1;
                        if at.month < 12 {
                            at.month += 1;
                        } else {
                            at.month = 1;
                            if at.year == 2399 {
                                return Ok(None);
                            }
                            at.year += 1;
                        }
                    }
                }
            }
            if self.matches_valid(at) {
                return Ok(Some(at));
            }
        }
    }
}

fn main() {}