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
    sets: [u64; 5],
    unrestricted: [bool; 2],
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

const DOMAINS: [(u32, u32); 5] = [(0, 59), (0, 23), (1, 31), (1, 12), (0, 6)];

fn idx(f: Field) -> usize {
    match f {
        Field::Minute => 0,
        Field::Hour => 1,
        Field::DayOfMonth => 2,
        Field::Month => 3,
        Field::DayOfWeek => 4,
    }
}

fn syntax(f: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field: f, offset }
}

fn mapped_name(f: Field, s: &str) -> Option<u32> {
    let table: &[&str] = match f {
        Field::Month => &["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"],
        Field::DayOfWeek => &["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"],
        _ => return None,
    };
    table.iter().position(|n| n.eq_ignore_ascii_case(s)).map(|n| {
        n as u32 + if f == Field::Month { 1 } else { 0 }
    })
}

fn atom_shape(f: Field, s: &str) -> bool {
    !s.is_empty()
        && (s.bytes().all(|b| b.is_ascii_digit()) || mapped_name(f, s).is_some())
}

fn number(s: &str, f: Field, offset: usize, step: bool) -> Result<u32, ParseError> {
    if s.len() > 1 && s.as_bytes()[0] == b'0' {
        return Err(ParseError::LeadingZero { field: f, offset });
    }
    let mut n = 0u32;
    for b in s.bytes() {
        n = n.checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field: f, offset })?;
    }
    if step {
        let (lo, hi) = DOMAINS[idx(f)];
        if n < 1 || n > hi - lo + 1 {
            return Err(ParseError::BadStep { field: f, offset });
        }
    } else {
        let (lo, hi) = DOMAINS[idx(f)];
        if n < lo || n > hi {
            return Err(ParseError::OutOfRange { field: f, offset });
        }
    }
    Ok(n)
}

fn atom(s: &str, f: Field, offset: usize) -> Result<u32, ParseError> {
    if s.bytes().all(|b| b.is_ascii_digit()) {
        number(s, f, offset, false)
    } else {
        mapped_name(f, s).ok_or_else(|| syntax(f, offset))
    }
}

fn field_set(text: &str, f: Field) -> Result<(u64, bool), ParseError> {
    for (i, b) in text.bytes().enumerate() {
        if !(b.is_ascii_digit() || b.is_ascii_alphabetic() || b"*-/, ".contains(&b) && b != b' ') {
            return Err(ParseError::InvalidCharacter { field: f, offset: i });
        }
    }

    let (lo, hi) = DOMAINS[idx(f)];
    let mut bits = 0u64;
    let unrestricted = (f == Field::DayOfMonth || f == Field::DayOfWeek) && text == "*";
    let mut base_offset = 0usize;

    for item in text.split(',') {
        if item.is_empty() {
            return Err(ParseError::EmptyItem { field: f, offset: base_offset });
        }
        let slash: Vec<usize> = item.bytes().enumerate().filter_map(|(i, b)| (b == b'/').then_some(i)).collect();
        if slash.len() > 1 {
            return Err(syntax(f, base_offset));
        }
        let (base, step_text, step_offset) = match slash.first() {
            Some(&p) => (&item[..p], Some(&item[p + 1..]), base_offset + p + 1),
            None => (item, None, 0),
        };

        if let Some(st) = step_text {
            if st.is_empty() || !st.bytes().all(|b| b.is_ascii_digit()) {
                return Err(syntax(f, base_offset));
            }
        }

        let hyphen: Vec<usize> = base.bytes().enumerate().filter_map(|(i, b)| (b == b'-').then_some(i)).collect();
        let (kind, atext, btext, ao, bo) = if base == "*" {
            (0, "", "", 0, 0)
        } else if hyphen.len() == 1 {
            let p = hyphen[0];
            let a = &base[..p];
            let b = &base[p + 1..];
            if !atom_shape(f, a) || !atom_shape(f, b) {
                return Err(syntax(f, base_offset));
            }
            (2, a, b, base_offset, base_offset + p + 1)
        } else if hyphen.is_empty() && atom_shape(f, base) {
            (1, base, "", base_offset, 0)
        } else {
            return Err(syntax(f, base_offset));
        };

        let (mut start, mut end) = match kind {
            0 => (lo, hi),
            1 => {
                let v = atom(atext, f, ao)?;
                (v, if step_text.is_some() { hi } else { v })
            }
            _ => {
                let a = atom(atext, f, ao)?;
                let b = atom(btext, f, bo)?;
                if a > b {
                    return Err(ParseError::ReversedRange { field: f, offset: bo });
                }
                (a, b)
            }
        };
        let step = match step_text {
            Some(st) => number(st, f, step_offset, true)?,
            None if kind == 1 => 0,
            None => 1,
        };
        if step == 0 {
            bits |= 1u64 << start;
        } else {
            while start <= end {
                bits |= 1u64 << start;
                match start.checked_add(step) {
                    Some(n) => start = n,
                    None => break,
                }
            }
        }
        base_offset += item.len() + 1;
        let _ = &mut end;
    }
    Ok((bits, unrestricted))
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }
    let fields = [Field::Minute, Field::Hour, Field::DayOfMonth, Field::Month, Field::DayOfWeek];
    let mut sets = [0u64; 5];
    let mut unrestricted = [false; 2];
    for i in 0..5 {
        let (set, u) = field_set(parts[i], fields[i])?;
        sets[i] = set;
        if i == 2 { unrestricted[0] = u; }
        if i == 4 { unrestricted[1] = u; }
    }
    Ok(Cron { sets, unrestricted })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        let (lo, hi) = DOMAINS[idx(field)];
        (lo..=hi)
            .filter(|&n| self.sets[idx(field)] & (1u64 << n) != 0)
            .map(|n| n as u8)
            .collect()
    }

    fn has(&self, f: Field, n: u8) -> bool {
        self.sets[idx(f)] & (1u64 << n) != 0
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate(at)?;
        Ok(self.matches_valid(at))
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        if !self.has(Field::Minute, at.minute)
            || !self.has(Field::Hour, at.hour)
            || !self.has(Field::Month, at.month)
        {
            return false;
        }
        let dom = self.has(Field::DayOfMonth, at.day);
        let dow = self.has(Field::DayOfWeek, weekday(at.year, at.month, at.day));
        match self.unrestricted {
            [true, true] => true,
            [true, false] => dow,
            [false, true] => dom,
            [false, false] if at.day == month_days(at.year, at.month) => dom && dow,
            [false, false] => dom || dow,
        }
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate(after)?;
        let mut x = after;
        loop {
            if x.minute < 59 {
                x.minute += 1;
            } else {
                x.minute = 0;
                if x.hour < 23 {
                    x.hour += 1;
                } else {
                    x.hour = 0;
                    if x.day < month_days(x.year, x.month) {
                        x.day += 1;
                    } else {
                        x.day = 1;
                        if x.month < 12 {
                            x.month += 1;
                        } else {
                            x.month = 1;
                            if x.year == 2399 {
                                return Ok(None);
                            }
                            x.year += 1;
                        }
                    }
                }
            }
            if self.matches_valid(x) {
                return Ok(Some(x));
            }
        }
    }
}

fn leap(y: u16) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

fn month_days(y: u16, m: u8) -> u8 {
    match m {
        2 => if leap(y) { 29 } else { 28 },
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn validate(x: CivilDateTime) -> Result<(), DateError> {
    if !(2000..=2399).contains(&x.year) {
        return Err(DateError::InvalidDateTime { component: DatePart::Year });
    }
    if !(1..=12).contains(&x.month) {
        return Err(DateError::InvalidDateTime { component: DatePart::Month });
    }
    if x.day == 0 || x.day > month_days(x.year, x.month) {
        return Err(DateError::InvalidDateTime { component: DatePart::Day });
    }
    if x.hour > 23 {
        return Err(DateError::InvalidDateTime { component: DatePart::Hour });
    }
    if x.minute > 59 {
        return Err(DateError::InvalidDateTime { component: DatePart::Minute });
    }
    Ok(())
}

fn weekday(y: u16, m: u8, d: u8) -> u8 {
    let mut n = 0u32;
    for year in 2000..y {
        n += if leap(year) { 366 } else { 365 };
    }
    for month in 1..m {
        n += month_days(y, month) as u32;
    }
    ((6 + n + d as u32 - 1) % 7) as u8
}
