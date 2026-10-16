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

fn bounds(field: Field) -> (u32, u32) {
    match field {
        Field::Minute => (0, 59),
        Field::Hour => (0, 23),
        Field::DayOfMonth => (1, 31),
        Field::Month => (1, 12),
        Field::DayOfWeek => (0, 6),
    }
}

fn parse_number(bytes: &[u8], start: usize, field: Field) -> Result<u32, ParseError> {
    if bytes.is_empty() {
        return Err(ParseError::InvalidSyntax { field, offset: start });
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(ParseError::LeadingZero { field, offset: start });
    }
    let mut value = 0u32;
    for &b in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or(ParseError::NumberOverflow { field, offset: start })?;
    }
    Ok(value)
}

fn parse_field(
    text: &str,
    field: Field,
    selected: &mut [bool],
) -> Result<(), ParseError> {
    let bytes = text.as_bytes();

    for (offset, &b) in bytes.iter().enumerate() {
        if !b.is_ascii_digit() && !matches!(b, b'*' | b'-' | b'/' | b',') {
            return Err(ParseError::InvalidCharacter { field, offset });
        }
    }

    let (minimum, maximum) = bounds(field);

    let mut item_start = 0;
    loop {
        let item_end = bytes[item_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|p| item_start + p)
            .unwrap_or(bytes.len());
        let item = &bytes[item_start..item_end];

        if item.is_empty() {
            return Err(ParseError::EmptyItem {
                field,
                offset: item_start,
            });
        }

        let slash_positions: Vec<usize> = item
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| if b == b'/' { Some(i) } else { None })
            .collect();

        if slash_positions.len() > 1 {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        let (base, step_bytes, step_start) = if let Some(&slash) = slash_positions.first() {
            (&item[..slash], Some(&item[slash + 1..]), Some(item_start + slash + 1))
        } else {
            (item, None, None)
        };

        let (kind, first_bytes, second_bytes, first_start, second_start) = if base == b"*" {
            (0u8, &[][..], &[][..], item_start, item_start)
        } else {
            let hyphens: Vec<usize> = base
                .iter()
                .enumerate()
                .filter_map(|(i, &b)| if b == b'-' { Some(i) } else { None })
                .collect();

            if hyphens.len() > 1 {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }

            if let Some(&hyphen) = hyphens.first() {
                if base[..hyphen].is_empty() || base[hyphen + 1..].is_empty() {
                    return Err(ParseError::InvalidSyntax {
                        field,
                        offset: item_start,
                    });
                }
                (
                    2u8,
                    &base[..hyphen],
                    &base[hyphen + 1..],
                    item_start,
                    item_start + hyphen + 1,
                )
            } else {
                (1u8, base, &[][..], item_start, item_start)
            }
        };

        if kind != 0
            && (first_bytes.is_empty()
                || !first_bytes.iter().all(u8::is_ascii_digit)
                || (kind == 2
                    && !second_bytes.iter().all(u8::is_ascii_digit)))
        {
            return Err(ParseError::InvalidSyntax {
                field,
                offset: item_start,
            });
        }

        if let Some(step) = step_bytes {
            if step.is_empty() || !step.iter().all(u8::is_ascii_digit) {
                return Err(ParseError::InvalidSyntax {
                    field,
                    offset: item_start,
                });
            }
        }

        let mut start_value = minimum;
        let mut end_value = maximum;

        if kind != 0 {
            let first = parse_number(first_bytes, first_start, field)?;
            if first < minimum || first > maximum {
                return Err(ParseError::OutOfRange {
                    field,
                    offset: first_start,
                });
            }
            start_value = first;
            end_value = first;

            if kind == 2 {
                let second = parse_number(second_bytes, second_start, field)?;
                if second < minimum || second > maximum {
                    return Err(ParseError::OutOfRange {
                        field,
                        offset: second_start,
                    });
                }
                if first > second {
                    return Err(ParseError::ReversedRange {
                        field,
                        offset: second_start,
                    });
                }
                end_value = second;
            } else if step_bytes.is_some() {
                end_value = maximum;
            }
        }

        let step = if let Some(step) = step_bytes {
            let step_start = step_start.unwrap();
            let value = parse_number(step, step_start, field)?;
            let width = maximum - minimum + 1;
            if value < 1 || value > width {
                return Err(ParseError::BadStep {
                    field,
                    offset: step_start,
                });
            }
            value
        } else {
            1
        };

        let mut value = start_value;
        while value <= end_value {
            selected[(value - minimum) as usize] = true;
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

    Ok(())
}

pub fn parse_cron(input: &str) -> Result<Cron, ParseError> {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.len() != 5 {
        return Err(ParseError::FieldCount { found: parts.len() });
    }

    let mut selected = [
        vec![false; 60],
        vec![false; 24],
        vec![false; 31],
        vec![false; 12],
        vec![false; 7],
    ];

    let mut day_of_month_star = false;
    let mut day_of_week_star = false;

    for i in 0..5 {
        if i == 2 {
            day_of_month_star = parts[i] == "*";
        } else if i == 4 {
            day_of_week_star = parts[i] == "*";
        }
        parse_field(parts[i], FIELDS[i], &mut selected[i])?;
    }

    Ok(Cron {
        selected,
        day_of_month_star,
        day_of_week_star,
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
        let minimum = bounds(field).0;
        self.selected[index]
            .iter()
            .enumerate()
            .filter_map(|(i, &yes)| {
                if yes {
                    Some((minimum + i as u32) as u8)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn matches(&self, at: CivilDateTime) -> Result<bool, DateError> {
        validate_datetime(at)?;
        let weekday = weekday(at.year, at.month, at.day) as usize;
        Ok(self.selected[0][at.minute as usize]
            && self.selected[1][at.hour as usize]
            && self.selected[2][(at.day - 1) as usize]
            && self.selected[3][(at.month - 1) as usize]
            && self.selected[4][weekday])
    }

    pub fn next_fire(
        &self,
        after: CivilDateTime,
    ) -> Result<Option<CivilDateTime>, DateError> {
        validate_datetime(after)?;
        let mut current = after;

        loop {
            if current.year == 2399
                && current.month == 12
                && current.day == 31
                && current.hour == 23
                && current.minute == 59
            {
                return Ok(None);
            }

            if current.minute < 59 {
                current.minute += 1;
            } else {
                current.minute = 0;
                if current.hour < 23 {
                    current.hour += 1;
                } else {
                    current.hour = 0;
                    let days = days_in_month(current.year, current.month);
                    if current.day < days {
                        current.day += 1;
                    } else {
                        current.day = 1;
                        if current.month < 12 {
                            current.month += 1;
                        } else {
                            current.month = 1;
                            current.year += 1;
                        }
                    }
                }
            }

            if self.matches(current)? {
                return Ok(Some(current));
            }
        }
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
    if at.day < 1 || at.day > days_in_month(at.year, at.month) {
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