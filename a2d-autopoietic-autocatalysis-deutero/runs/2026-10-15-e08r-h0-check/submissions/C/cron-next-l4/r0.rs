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
    day_unrestricted: bool,
    weekday_unrestricted: bool,
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

fn field_index(field: Field) -> usize {
    match field {
        Field::Minute => 0,
        Field::Hour => 1,
        Field::DayOfMonth => 2,
        Field::Month => 3,
        Field::DayOfWeek => 4,
    }
}

fn width(field: Field) -> u32 {
    let (lo, hi) = bounds(field);
    (hi - lo + 1) as u32
}

fn invalid_syntax(field: Field, offset: usize) -> ParseError {
    ParseError::InvalidSyntax { field, offset }
}

fn atom_syntax(text: &str, field: Field) -> bool {
    if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
        return true;
    }
    if !text.bytes().all(|b| b.is_ascii_alphabetic()) {
        return false;
    }
    match field {
        Field::Month => matches!(
            text.to_ascii_uppercase().as_str(),
            "JAN" | "FEB" | "MAR" | "APR" | "MAY" | "JUN" | "JUL" | "AUG" | "SEP"
                | "OCT" | "NOV" | "DEC"
        ),
        Field::DayOfWeek => matches!(
            text.to_ascii_uppercase().as_str(),
            "SUN" | "MON" | "TUE" | "WED" | "THU" | "FRI" | "SAT"
        ),
        _ => false,
    }
}

fn name_value(text: &str, field: Field) -> Option<u8> {
    let upper = text.to_ascii_uppercase();
    match field {
        Field::Month => match upper.as_str() {
            "JAN" => Some(1),
            "FEB" => Some(2),
            "MAR" => Some(3),
            "APR" => Some(4),
            "MAY" => Some(5),
            "JUN" => Some(6),
            "JUL" => Some(7),
            "AUG" => Some(8),
            "SEP" => Some(9),
            "OCT" => Some(10),
            "NOV" => Some(11),
            "DEC" => Some(12),
            _ => None,
        },
        Field::DayOfWeek => match upper.as_str() {
            "SUN" => Some(0),
            "MON" => Some(1),
            "TUE" => Some(2),
            "WED" => Some(3),
            "THU" => Some(4),
            "FRI" => Some(5),
            "SAT" => Some(6),
            _ => None,
        },
        _ => None,
    }
}

fn number(text: &str, field: Field, offset: usize) -> Result<u8, ParseError> {
    if text.len() > 1 && text.as_bytes()[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }
    let mut value = 0u32;
    for b in text.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    let (lo, hi) = bounds(field);
    if value < lo as u32 || value > hi as u32 {
        return Err(ParseError::OutOfRange { field, offset });
    }
    Ok(value as u8)
}

fn parse_atom(text: &str, field: Field, offset: usize) -> Result<u8, ParseError> {
    if text.bytes().all(|b| b.is_ascii_digit()) {
        return number(text, field, offset);
    }
    name_value(text, field).ok_or_else(|| invalid_syntax(field, offset))
}

fn parse_step(text: &str, field: Field, offset: usize) -> Result<u32, ParseError> {
    if text.len() > 1 && text.as_bytes()[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }
    let mut value = 0u32;
    for b in text.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    if value == 0 || value > width(field) {
        return Err(ParseError::BadStep { field, offset });
    }
    Ok(value)
}

fn parse_field(text: &str, field: Field) -> Result<Vec<u8>, ParseError> {
    for (offset, b) in text.bytes().enumerate() {
        if !(b.is_ascii_digit()
            || b.is_ascii_alphabetic()
            || matches!(b, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (lo, hi) = bounds(field);
    let mut selected = vec![false; 256];
    let mut item_offset = 0usize;

    for item in text.split(',') {
        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_offset,
            });
        }

        let slashes = item.bytes().filter(|&b| b == b'/').count();
        let (base, step_text, step_offset) = if slashes == 0 {
            (item, None, 0)
        } else if slashes == 1 {
            let pos = item.find('/').unwrap();
            (&item[..pos], Some(&item[pos + 1..]), pos + 1)
        } else {
            return Err(invalid_syntax(field, item_offset));
        };

        if base.is_empty() || step_text == Some("") {
            return Err(invalid_syntax(field, item_offset));
        }

        let hyphens = base.bytes().filter(|&b| b == b'-').count();
        let star = base == "*";
        if hyphens > 1 || (star && hyphens != 0) {
            return Err(invalid_syntax(field, item_offset));
        }

        let (left_text, right_text, left_off, right_off) = if hyphens == 1 {
            let pos = base.find('-').unwrap();
            let left = &base[..pos];
            let right = &base[pos + 1..];
            if left.is_empty() || right.is_empty() || left == "*" || right == "*" {
                return Err(invalid_syntax(field, item_offset));
            }
            (left, Some(right), item_offset, item_offset + pos + 1)
        } else if star {
            ("*", None, item_offset, item_offset)
        } else {
            (base, None, item_offset, item_offset)
        };

        // Reject malformed atom shapes before reading any numeric values.
        if star {
            if base != "*" {
                return Err(invalid_syntax(field, item_offset));
            }
        } else if !atom_syntax(left_text, field)
            || right_text.is_some_and(|r| !atom_syntax(r, field))
        {
            return Err(invalid_syntax(field, item_offset));
        }

        let (start, end) = if star {
            (lo, hi)
        } else {
            let start = parse_atom(left_text, field, left_off)?;
            if let Some(right) = right_text {
                let end = parse_atom(right, field, right_off)?;
                if start > end {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: right_off,
                    });
                }
                (start, end)
            } else if step_text.is_some() {
                (start, hi)
            } else {
                (start, start)
            }
        };

        let step = if let Some(s) = step_text {
            if !s.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid_syntax(field, item_offset));
            }
            parse_step(s, field, item_offset + step_offset)?
        } else {
            1
        };

        let mut value = start as u32;
        while value <= end as u32 {
            selected[value as usize] = true;
            value += step;
        }

        item_offset += item.len() + 1;
    }

    Ok((lo..=hi).filter(|v| selected[*v as usize]).collect())
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    if input.starts_with('@') {
        let expansion = match input {
            "@hourly" => "0 * * * *",
            "@daily" => "0 0 * * *",
            "@weekly" => "0 0 * * 0",
            "@monthly" => "0 0 1 * *",
            "@yearly" => "0 0 1 1 *",
            _ => return Err(ParseError::UnknownSpecial),
        };
        return parse_cron(expansion);
    }

    let pieces: Vec<&str> = input.split(' ').collect();
    if pieces.len() != 5 {
        return Err(ParseError::FieldCount { found: pieces.len() });
    }

    let mut sets: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());
    for i in 0..5 {
        sets[i] = parse_field(pieces[i], FIELDS[i])?;
    }

    Ok(Cron {
        sets,
        day_unrestricted: pieces[2] == "*",
        weekday_unrestricted: pieces[4] == "*",
    })
}

fn leap(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn month_days(year: u16, month: u8) -> u8 {
    match month {
        2 if leap(year) => 29,
        2 => 28,
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
    if at.day < 1 || at.day > month_days(at.year, at.month) {
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

fn date_ordinal(year: u16, month: u8, day: u8) -> i32 {
    let mut count = 0;
    for y in 2000..year {
        count += if leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        count += month_days(year, m) as i32;
    }
    count + day as i32 - 1
}

fn from_ordinal(mut ordinal: i32) -> (u16, u8, u8) {
    let mut year = 2000u16;
    loop {
        let days = if leap(year) { 366 } else { 365 };
        if ordinal < days {
            break;
        }
        ordinal -= days;
        year += 1;
    }
    let mut month = 1u8;
    loop {
        let days = month_days(year, month) as i32;
        if ordinal < days {
            break;
        }
        ordinal -= days;
        month += 1;
    }
    (year, month, ordinal as u8 + 1)
}

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    ((date_ordinal(year, month, day) + 6) % 7) as u8
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.sets[field_index(field)].clone()
    }

    fn day_matches(&self, at: CivilDateTime) -> bool {
        let dom = self.sets[2].contains(&at.day);
        let dow = self.sets[4].contains(&weekday(at.year, at.month, at.day));
        match (self.day_unrestricted, self.weekday_unrestricted) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) if at.day == month_days(at.year, at.month) => dom && dow,
            (false, false) => dom || dow,
        }
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate(at)?;
        Ok(self.sets[0].contains(&at.minute)
            && self.sets[1].contains(&at.hour)
            && self.sets[3].contains(&at.month)
            && self.day_matches(at))
    }

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate(after)?;
        let mut ordinal = date_ordinal(after.year, after.month, after.day);
        let last = date_ordinal(2399, 12, 31);
        while ordinal <= last {
            let (year, month, day) = from_ordinal(ordinal);
            let date = CivilDateTime { year, month, day, hour: 0, minute: 0 };
            if self.sets[3].contains(&month) && self.day_matches(date) {
                for &hour in &self.sets[1] {
                    for &minute in &self.sets[0] {
                        let candidate = CivilDateTime { year, month, day, hour, minute };
                        if candidate > after {
                            return Ok(Some(candidate));
                        }
                    }
                }
            }
            ordinal += 1;
        }
        Ok(None)
    }

    pub fn previous_fire(
        &self,
        before: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
        validate(before)?;
        let mut ordinal = date_ordinal(before.year, before.month, before.day);
        let first = date_ordinal(2000, 1, 1);
        loop {
            let (year, month, day) = from_ordinal(ordinal);
            let date = CivilDateTime { year, month, day, hour: 0, minute: 0 };
            if self.sets[3].contains(&month) && self.day_matches(date) {
                for &hour in self.sets[1].iter().rev() {
                    for &minute in self.sets[0].iter().rev() {
                        let candidate = CivilDateTime { year, month, day, hour, minute };
                        if candidate < before {
                            return Ok(Some(candidate));
                        }
                    }
                }
            }
            if ordinal == first {
                break;
            }
            ordinal -= 1;
        }
        Ok(None)
    }

    pub fn fires_between(
        &self,
        start: CivilDateTime,
        end: CivilDateTime,
        cap: usize,
    ) -> Result<FireWindow, WindowError> {
        validate(start).map_err(|e| match e {
            DateError::InvalidDateTime { component } => WindowError::InvalidStart { component },
        })?;
        validate(end).map_err(|e| match e {
            DateError::InvalidDateTime { component } => WindowError::InvalidEnd { component },
        })?;
        if start > end {
            return Err(WindowError::ReversedWindow);
        }
        if cap == 0 {
            return Err(WindowError::ZeroCap);
        }

        let mut fires = Vec::new();
        let mut cursor = start;
        loop {
            let Some(candidate) = self.next_fire(cursor).unwrap() else {
                break;
            };
            if candidate >= end {
                break;
            }
            if fires.len() == cap {
                return Ok(FireWindow { fires, truncated: true });
            }
            fires.push(candidate);
            cursor = candidate;
        }
        Ok(FireWindow { fires, truncated: false })
    }

    pub fn canonical(&self) -> String {
        (0..5)
            .map(|i| {
                let (lo, hi) = bounds(FIELDS[i]);
                if (i == 2 && self.day_unrestricted) || (i == 4 && self.weekday_unrestricted) {
                    "*".to_string()
                } else if self.sets[i].len() == (hi - lo + 1) as usize {
                    if i == 2 || i == 4 { "*/1".to_string() } else { "*".to_string() }
                } else {
                    self.sets[i].iter().map(u8::to_string).collect::<Vec<_>>().join(",")
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn main() {}