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
    UnknownSpecial,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    sets: [Vec<u8>; 5],
    bare: [bool; 2],
}
const FIELDS: [Field; 5] = [
    Field::Minute,
    Field::Hour,
    Field::DayOfMonth,
    Field::Month,
    Field::DayOfWeek,
];
const LOW: [u8; 5] = [0, 0, 1, 1, 0];
const HIGH: [u8; 5] = [59, 23, 31, 12, 6];
fn index(field: Field) -> usize {
    FIELDS.iter().position(|&x| x == field).unwrap()
}
fn name(s: &str, f: usize) -> Option<u8> {
    let names: &[&str] = match f {
        3 => &[
            "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
        ],
        4 => &["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"],
        _ => &[],
    };
    names
        .iter()
        .position(|n| s.eq_ignore_ascii_case(n))
        .map(|n| n as u8 + LOW[f])
}
fn atom(s: &str, f: usize) -> bool {
    !s.is_empty() && (s.bytes().all(|b| b.is_ascii_digit()) || name(s, f).is_some())
}
fn number(s: &str, f: usize, offset: usize, step: bool) -> Result<u32, ParseError> {
    let field = FIELDS[f];
    if let Some(n) = name(s, f) {
        return Ok(n as u32);
    }
    if s.len() > 1 && s.starts_with('0') {
        return Err(ParseError::LeadingZero { field, offset });
    }
    let mut n = 0u32;
    for b in s.bytes() {
        n = n
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    if step {
        if n == 0 || n > (HIGH[f] - LOW[f] + 1) as u32 {
            return Err(ParseError::BadStep { field, offset });
        }
    } else if n < LOW[f] as u32 || n > HIGH[f] as u32 {
        return Err(ParseError::OutOfRange { field, offset });
    }
    Ok(n)
}
pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    if input.starts_with('@') {
        let expanded = match input {
            "@hourly" => "0 * * * *",
            "@daily" => "0 0 * * *",
            "@weekly" => "0 0 * * 0",
            "@monthly" => "0 0 1 * *",
            "@yearly" => "0 0 1 1 *",
            _ => return Err(ParseError::UnknownSpecial),
        };
        return parse_cron(expanded);
    }
    let fields: Vec<&str> = input.split(' ').collect();
    if fields.len() != 5 {
        return Err(ParseError::FieldCount {
            found: fields.len(),
        });
    }
    let mut cron = Cron {
        sets: std::array::from_fn(|_| Vec::new()),
        bare: [fields[2] == "*", fields[4] == "*"],
    };
    for f in 0..5 {
        let field = FIELDS[f];
        let s = fields[f];
        for (offset, b) in s.bytes().enumerate() {
            if !(b.is_ascii_digit() || b"*-,/".contains(&b) || b.is_ascii_alphabetic()) {
                return Err(ParseError::InvalidCharacter { field, offset });
            }
        }
        let mut selected = [false; 60];
        let mut offset = 0;
        for item in s.split(',') {
            if item.is_empty() {
                return Err(ParseError::EmptyItem { field, offset });
            }
            let slash: Vec<&str> = item.split('/').collect();
            let base = slash[0];
            let ends: Vec<&str> = base.split('-').collect();
            let valid_base = base == "*"
                || (ends.len() == 1 && atom(ends[0], f))
                || (ends.len() == 2 && atom(ends[0], f) && atom(ends[1], f));
            let valid_step = slash.len() == 1
                || (slash.len() == 2
                    && !slash[1].is_empty()
                    && slash[1].bytes().all(|b| b.is_ascii_digit()));
            if !valid_base || !valid_step || slash.len() > 2 {
                return Err(ParseError::InvalidSyntax { field, offset });
            }
            let stepped = slash.len() == 2;
            let (start, end) = if base == "*" {
                (LOW[f] as u32, HIGH[f] as u32)
            } else {
                let a = number(ends[0], f, offset, false)?;
                let b = if ends.len() == 2 {
                    number(ends[1], f, offset + ends[0].len() + 1, false)?
                } else if stepped {
                    HIGH[f] as u32
                } else {
                    a
                };
                if a > b {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: offset + ends[0].len() + 1,
                    });
                }
                (a, b)
            };
            let stride = if stepped {
                number(slash[1], f, offset + base.len() + 1, true)?
            } else {
                1
            };
            let mut n = start;
            while n <= end {
                selected[n as usize] = true;
                n += stride;
            }
            offset += item.len() + 1;
        }
        cron.sets[f] = (LOW[f]..=HIGH[f])
            .filter(|&v| selected[v as usize])
            .collect();
    }
    Ok(cron)
}
impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.sets[index(field)].clone()
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
fn days(y: u16, m: u8) -> u8 {
    match m {
        2 => {
            if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}
fn validate(t: CivilDateTime) -> Result<(), DateError> {
    let component = if !(2000..=2399).contains(&t.year) {
        Some(DatePart::Year)
    } else if !(1..=12).contains(&t.month) {
        Some(DatePart::Month)
    } else if t.day == 0 || t.day > days(t.year, t.month) {
        Some(DatePart::Day)
    } else if t.hour > 23 {
        Some(DatePart::Hour)
    } else if t.minute > 59 {
        Some(DatePart::Minute)
    } else {
        None
    };
    match component {
        Some(component) => Err(DateError::InvalidDateTime { component }),
        None => Ok(()),
    }
}
fn ordinal(t: CivilDateTime) -> u32 {
    let mut n = 0;
    for y in 2000..t.year {
        n += if days(y, 2) == 29 { 366 } else { 365 };
    }
    for m in 1..t.month {
        n += days(t.year, m) as u32;
    }
    n + t.day as u32 - 1
}
fn shift_day(mut t: CivilDateTime, forward: bool) -> Option<CivilDateTime> {
    if forward {
        if t.day < days(t.year, t.month) {
            t.day += 1;
        } else {
            t.day = 1;
            if t.month < 12 {
                t.month += 1;
            } else {
                if t.year == 2399 {
                    return None;
                }
                t.year += 1;
                t.month = 1;
            }
        }
    } else if t.day > 1 {
        t.day -= 1;
    } else {
        if t.month > 1 {
            t.month -= 1;
        } else {
            if t.year == 2000 {
                return None;
            }
            t.year -= 1;
            t.month = 12;
        }
        t.day = days(t.year, t.month);
    }
    Some(t)
}
impl Cron {
    fn date_match(&self, t: CivilDateTime, weekday: u8) -> bool {
        if !self.sets[3].contains(&t.month) {
            return false;
        }
        let dom = self.sets[2].contains(&t.day);
        let dow = self.sets[4].contains(&weekday);
        match self.bare {
            [true, true] => true,
            [true, false] => dow,
            [false, true] => dom,
            [false, false] => {
                if t.day == days(t.year, t.month) {
                    dom && dow
                } else {
                    dom || dow
                }
            }
        }
    }
    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate(at)?;
        Ok(self.date_match(at, ((ordinal(at) + 6) % 7) as u8)
            && self.sets[0].contains(&at.minute)
            && self.sets[1].contains(&at.hour))
    }
    fn seek(&self, bound: CivilDateTime, forward: bool, inclusive: bool) -> Option<CivilDateTime> {
        let mut date = bound;
        let mut weekday = ((ordinal(bound) + 6) % 7) as u8;
        loop {
            if self.date_match(date, weekday) {
                if forward {
                    for &hour in &self.sets[1] {
                        for &minute in &self.sets[0] {
                            let t = CivilDateTime {
                                hour,
                                minute,
                                ..date
                            };
                            if t > bound || (inclusive && t == bound) {
                                return Some(t);
                            }
                        }
                    }
                } else {
                    for &hour in self.sets[1].iter().rev() {
                        for &minute in self.sets[0].iter().rev() {
                            let t = CivilDateTime {
                                hour,
                                minute,
                                ..date
                            };
                            if t < bound || (inclusive && t == bound) {
                                return Some(t);
                            }
                        }
                    }
                }
            }
            date = shift_day(date, forward)?;
            weekday = if forward {
                (weekday + 1) % 7
            } else {
                (weekday + 6) % 7
            };
        }
    }
    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate(after)?;
        Ok(self.seek(after, true, false))
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowError {
    InvalidStart { component: DatePart },
    InvalidEnd { component: DatePart },
    ReversedWindow,
    ZeroCap,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FireWindow {
    pub fires: Vec<CivilDateTime>,
    pub truncated: bool,
}
impl Cron {
    pub fn canonical(&self) -> String {
        (0..5)
            .map(|f| {
                let full = self.sets[f].len() == (HIGH[f] - LOW[f] + 1) as usize;
                if full {
                    if (f == 2 && !self.bare[0]) || (f == 4 && !self.bare[1]) {
                        "*".to_string()
                    } else {
                        "*".to_string()
                    }
                } else {
                    self.sets[f]
                        .iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
    pub fn previous_fire(&self, before: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate(before)?;
        Ok(self.seek(before, false, false))
    }
    pub fn fires_between(
        &self,
        start: CivilDateTime,
        end: CivilDateTime,
        cap: usize,
    ) -> Result<FireWindow, WindowError> {
        validate(start).map_err(|DateError::InvalidDateTime { component }| {
            WindowError::InvalidStart { component }
        })?;
        validate(end).map_err(|DateError::InvalidDateTime { component }| {
            WindowError::InvalidEnd { component }
        })?;
        if start > end {
            return Err(WindowError::ReversedWindow);
        }
        if cap == 0 {
            return Err(WindowError::ZeroCap);
        }
        let mut fires = Vec::new();
        let mut found = self.seek(start, true, true);
        while let Some(t) = found {
            if t >= end {
                break;
            }
            if fires.len() == cap {
                return Ok(FireWindow {
                    fires,
                    truncated: true,
                });
            }
            fires.push(t);
            found = self.seek(t, true, false);
        }
        Ok(FireWindow {
            fires,
            truncated: false,
        })
    }
}
