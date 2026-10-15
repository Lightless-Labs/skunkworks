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
    let _ = (s, f);
    None
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
            if !(b.is_ascii_digit() || b"*-,/".contains(&b)) {
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
