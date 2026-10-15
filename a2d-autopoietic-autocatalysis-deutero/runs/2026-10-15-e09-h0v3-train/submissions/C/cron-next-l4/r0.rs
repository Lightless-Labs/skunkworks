use std::cmp::Ordering;

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

fn field_index(field: Field) -> usize {
    match field {
        Field::Minute => 0,
        Field::Hour => 1,
        Field::DayOfMonth => 2,
        Field::Month => 3,
        Field::DayOfWeek => 4,
    }
}

fn bounds(field: Field) -> (u8, u8, u8) {
    match field {
        Field::Minute => (0, 59, 60),
        Field::Hour => (0, 23, 24),
        Field::DayOfMonth => (1, 31, 31),
        Field::Month => (1, 12, 12),
        Field::DayOfWeek => (0, 6, 7),
    }
}

fn name_value(field: Field, bytes: &[u8]) -> Option<u8> {
    let names: &[&str] = match field {
        Field::Month => &[
            "JAN", "FEB", "MAR", "APR", "MAY", "JUN",
            "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
        ],
        Field::DayOfWeek => &["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"],
        _ => return None,
    };
    let upper: Vec<u8> = bytes.iter().map(u8::to_ascii_uppercase).collect();
    names
        .iter()
        .position(|name| name.as_bytes() == upper.as_slice())
        .map(|i| if field == Field::Month { i as u8 + 1 } else { i as u8 })
}

fn atom_is_valid(field: Field, atom: &[u8]) -> bool {
    !atom.is_empty()
        && (atom.iter().all(u8::is_ascii_digit) || name_value(field, atom).is_some())
}

fn parse_number(field: Field, bytes: &[u8], offset: usize, is_step: bool) -> Result<u32, ParseError> {
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset });
    }
    let mut value = 0u32;
    for &b in bytes {
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset })?;
    }
    let (min, max, width) = bounds(field);
    if is_step {
        if value < 1 || value > width as u32 {
            return Err(ParseError::BadStep { field, offset });
        }
    } else if value < min as u32 || value > max as u32 {
        return Err(ParseError::OutOfRange { field, offset });
    }
    Ok(value)
}

fn parse_endpoint(field: Field, atom: &[u8], offset: usize) -> Result<u8, ParseError> {
    if atom.iter().all(u8::is_ascii_digit) {
        Ok(parse_number(field, atom, offset, false)? as u8)
    } else {
        name_value(field, atom).ok_or(ParseError::InvalidSyntax { field, offset })
    }
}

fn parse_field(field: Field, text: &str) -> Result<Vec<u8>, ParseError> {
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if !(b.is_ascii_digit()
            || b.is_ascii_alphabetic()
            || matches!(b, b'*' | b'-' | b'/' | b','))
        {
            return Err(ParseError::InvalidCharacter { field, offset: i });
        }
    }

    let (min, max, _) = bounds(field);
    let mut selected = vec![false; 60];
    let mut item_start = 0usize;

    for item in text.split(',') {
        let item_bytes = item.as_bytes();
        if item_bytes.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let slash_positions: Vec<usize> = item_bytes
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'/').then_some(i))
            .collect();

        if slash_positions.len() > 1 {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let (base, step_part) = if let Some(&slash) = slash_positions.first() {
            (&item_bytes[..slash], Some((slash + 1, &item_bytes[slash + 1..])))
        } else {
            (item_bytes, None)
        };

        if base.is_empty() || step_part.is_some_and(|(_, s)| s.is_empty()) {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let hyphens: Vec<usize> = base
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'-').then_some(i))
            .collect();

        let is_star = base == b"*";
        let (start_atom, end_atom) = match hyphens.as_slice() {
            [] if is_star => (None, None),
            [] if atom_is_valid(field, base) => (Some((0, base)), None),
            [h] if *h > 0 && *h + 1 < base.len() => {
                let left = &base[..*h];
                let right = &base[*h + 1..];
                if atom_is_valid(field, left) && atom_is_valid(field, right) {
                    (Some((0, left)), Some((*h + 1, right)))
                } else {
                    return Err(ParseError::InvalidSyntax { field, offset: item_start });
                }
            }
            _ => return Err(ParseError::InvalidSyntax { field, offset: item_start }),
        };

        let parsed_start = if let Some((rel, atom)) = start_atom {
            Some(parse_endpoint(field, atom, item_start + rel)?)
        } else {
            None
        };
        let parsed_end = if let Some((rel, atom)) = end_atom {
            Some(parse_endpoint(field, atom, item_start + rel)?)
        } else {
            None
        };

        if let (Some(start), Some(end)) = (parsed_start, parsed_end) {
            if start > end {
                let rel = end_atom.unwrap().0;
                return Err(ParseError::ReversedRange {
                    field,
                    offset: item_start + rel,
                });
            }
        }

        let step = if let Some((step_offset, step_bytes)) = step_part {
            if !step_bytes.iter().all(u8::is_ascii_digit) {
                return Err(ParseError::InvalidSyntax { field, offset: item_start });
            }
            Some(parse_number(field, step_bytes, item_start + step_offset, true)? as u8)
        } else {
            None
        };

        let (range_start, range_end) = match (parsed_start, parsed_end, is_star, step) {
            (None, None, true, _) => (min, max),
            (Some(s), None, false, Some(_)) => (s, max),
            (Some(s), None, false, None) => (s, s),
            (Some(s), Some(e), false, _) => (s, e),
            _ => return Err(ParseError::InvalidSyntax { field, offset: item_start }),
        };

        if let Some(step) = step {
            let mut v = range_start as u16;
            while v <= range_end as u16 {
                selected[v as usize] = true;
                v += step as u16;
            }
        } else {
            for v in range_start..=range_end {
                selected[v as usize] = true;
            }
        }

        item_start += item.len() + 1;
    }

    Ok((min..=max).filter(|&v| selected[v as usize]).collect())
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    if input.as_bytes().first() == Some(&b'@') {
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
    let mut sets: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());

    for (i, (&field, text)) in fields.iter().zip(parts.iter()).enumerate() {
        sets[i] = parse_field(field, text)?;
    }

    Ok(Cron {
        sets,
        unrestricted_days: [parts[2] == "*", parts[4] == "*"],
    })
}

impl Cron {
    pub fn allowed(&self, field: Field) -> Vec<u8> {
        self.sets[field_index(field)].clone()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        Ok(self.matches_valid(at))
    }

    fn matches_valid(&self, at: CivilDateTime) -> bool {
        if !self.sets[0].contains(&at.minute)
            || !self.sets[1].contains(&at.hour)
            || !self.sets[3].contains(&at.month)
        {
            return false;
        }

        let dom = self.sets[2].contains(&at.day);
        let dow = self.sets[4].contains(&weekday(at.year, at.month, at.day));

        match (self.unrestricted_days[0], self.unrestricted_days[1]) {
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

    pub fn next_fire(&self, after: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;
        let mut date = (after.year, after.month, after.day);
        loop {
            let (year, month, day) = date;
            if !self.sets[3].contains(&month) || !self.date_matches(year, month, day) {
                date = match next_date(year, month, day) {
                    Some(d) => d,
                    None => return Ok(None),
                };
                continue;
            }
            for &hour in &self.sets[1] {
                for &minute in &self.sets[0] {
                    let candidate = CivilDateTime { year, month, day, hour, minute };
                    if candidate > after {
                        return Ok(Some(candidate));
                    }
                }
            }
            date = match next_date(year, month, day) {
                Some(d) => d,
                None => return Ok(None),
            };
        }
    }

    pub fn previous_fire(&self, before: CivilDateTime) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(before)?;
        let mut date = (before.year, before.month, before.day);
        loop {
            let (year, month, day) = date;
            if self.sets[3].contains(&month) && self.date_matches(year, month, day) {
                for &hour in self.sets[1].iter().rev() {
                    for &minute in self.sets[0].iter().rev() {
                        let candidate = CivilDateTime { year, month, day, hour, minute };
                        if candidate < before {
                            return Ok(Some(candidate));
                        }
                    }
                }
            }
            date = match previous_date(year, month, day) {
                Some(d) => d,
                None => return Ok(None),
            };
        }
    }

    pub fn fires_between(
        &self,
        start: CivilDateTime,
        end: CivilDateTime,
        cap: usize,
    ) -> Result<FireWindow, WindowError> {
        validate_datetime(start).map_err(|e| match e {
            DateError::InvalidDateTime { component } => WindowError::InvalidStart { component },
        })?;
        validate_datetime(end).map_err(|e| match e {
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
        while let Some(fire) = self.next_or_at(cursor) {
            if fire >= end {
                break;
            }
            if fires.len() == cap {
                return Ok(FireWindow { fires, truncated: true });
            }
            fires.push(fire);
            cursor = match increment_minute(fire) {
                Some(v) => v,
                None => break,
            };
        }
        Ok(FireWindow { fires, truncated: false })
    }

    fn next_or_at(&self, at: CivilDateTime) -> Option<CivilDateTime> {
        if self.matches_valid(at) {
            Some(at)
        } else {
            self.next_fire(at).ok().flatten()
        }
    }

    fn date_matches(&self, year: u16, month: u8, day: u8) -> bool {
        let dt = CivilDateTime { year, month, day, hour: 0, minute: 0 };
        let dom = self.sets[2].contains(&day);
        let dow = self.sets[4].contains(&weekday(year, month, day));
        match (self.unrestricted_days[0], self.unrestricted_days[1]) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) => {
                if day == days_in_month(year, month) { dom && dow } else { dom || dow }
            }
        }
    }

    pub fn canonical(&self) -> String {
        let fields = [
            self.format_field(0, Field::Minute),
            self.format_field(1, Field::Hour),
            self.format_field(2, Field::DayOfMonth),
            self.format_field(3, Field::Month),
            self.format_field(4, Field::DayOfWeek),
        ];
        fields.join(" ")
    }

    fn format_field(&self, index: usize, field: Field) -> String {
        let (min, max, _) = bounds(field);
        let values = &self.sets[index];
        if index == 2 && self.unrestricted_days[0] {
            return "*".to_string();
        }
        if index == 4 && self.unrestricted_days[1] {
            return "*".to_string();
        }
        if values.len() == (max - min + 1) as usize
            && values.first() == Some(&min)
            && values.last() == Some(&max)
        {
            if index == 2 || index == 4 {
                return "*/1".to_string();
            }
            return "*".to_string();
        }
        values.iter().map(u8::to_string).collect::<Vec<_>>().join(",")
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

fn leap(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 => if leap(year) { 29 } else { 28 },
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn weekday(year: u16, month: u8, day: u8) -> u8 {
    let mut days = 0u32;
    for y in 2000..year {
        days += if leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(year, m) as u32;
    }
    days += day as u32 - 1;
    ((days + 6) % 7) as u8
}

fn next_date(year: u16, month: u8, day: u8) -> Option<(u16, u8, u8)> {
    if day < days_in_month(year, month) {
        Some((year, month, day + 1))
    } else if month < 12 {
        Some((year, month + 1, 1))
    } else if year < 2399 {
        Some((year + 1, 1, 1))
    } else {
        None
    }
}

fn previous_date(year: u16, month: u8, day: u8) -> Option<(u16, u8, u8)> {
    if day > 1 {
        Some((year, month, day - 1))
    } else if month > 1 {
        let m = month - 1;
        Some((year, m, days_in_month(year, m)))
    } else if year > 2000 {
        Some((year - 1, 12, 31))
    } else {
        None
    }
}

fn increment_minute(at: CivilDateTime) -> Option<CivilDateTime> {
    if at.minute < 59 {
        Some(CivilDateTime { minute: at.minute + 1, ..at })
    } else if at.hour < 23 {
        Some(CivilDateTime { hour: at.hour + 1, minute: 0, ..at })
    } else {
        next_date(at.year, at.month, at.day).map(|(year, month, day)| CivilDateTime {
            year, month, day, hour: 0, minute: 0,
        })
    }
}

fn main() {}