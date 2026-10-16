#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WordsError {
    Empty,
    InvalidByte { index: usize },
    Separator { index: usize },
    InvalidWord { index: usize },
    Range,
    NonCanonical { index: usize },
}

const LIMIT: u128 = 1_000_000_000_000_000_000;

const ONES: [&str; 20] = [
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
    "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen",
    "seventeen", "eighteen", "nineteen",
];

const TENS: [&str; 8] = [
    "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

const SCALES: [&str; 6] = [
    "thousand", "million", "billion", "trillion", "quadrillion", "quintillion",
];

fn append_group(value: u16, out: &mut String) {
    let mut first = true;

    if value >= 100 {
        out.push_str(ONES[(value / 100) as usize]);
        out.push_str(" hundred");
        first = false;

        let remainder = value % 100;
        if remainder != 0 {
            out.push_str(" and ");
            append_group(remainder, out);
        }
        return;
    }

    if value >= 20 {
        out.push_str(TENS[(value / 10 - 2) as usize]);
        let units = value % 10;
        if units != 0 {
            out.push('-');
            out.push_str(ONES[units as usize]);
        }
    } else if value != 0 {
        out.push_str(ONES[value as usize]);
    }

    let _ = first;
}

pub fn words_encode(value: i128) -> Result<String, WordsError> {
    let magnitude = value.unsigned_abs();
    if magnitude > LIMIT {
        return Err(WordsError::Range);
    }

    if magnitude == 0 {
        return Ok("zero".to_string());
    }

    let mut groups = [0u16; 7];
    let mut remaining = magnitude;
    for group in &mut groups {
        *group = (remaining % 1000) as u16;
        remaining /= 1000;
    }

    let mut out = String::new();
    if value < 0 {
        out.push_str("minus ");
    }

    let mut higher_present = false;
    for index in (1..groups.len()).rev() {
        let group = groups[index];
        if group == 0 {
            continue;
        }
        if !out.is_empty() && !(value < 0 && out == "minus ") {
            out.push(' ');
        }
        append_group(group, &mut out);
        out.push(' ');
        out.push_str(SCALES[index - 1]);
        higher_present = true;
    }

    let units = groups[0];
    if units != 0 {
        if higher_present {
            out.push_str(if units < 100 { " and " } else { " " });
        }
        append_group(units, &mut out);
    }

    Ok(out)
}

fn dictionary_value(word: &str) -> Option<u128> {
    if word == "zero" {
        return Some(0);
    }
    for (value, spelling) in ONES.iter().enumerate().skip(1) {
        if word == *spelling {
            return Some(value as u128);
        }
    }
    for (index, spelling) in TENS.iter().enumerate() {
        if word == *spelling {
            return Some((20 + index * 10) as u128);
        }
    }

    match word {
        "hundred" => Some(100),
        "thousand" => Some(1_000),
        "million" => Some(1_000_000),
        "billion" => Some(1_000_000_000),
        "trillion" => Some(1_000_000_000_000),
        "quadrillion" => Some(1_000_000_000_000_000),
        "quintillion" => Some(1_000_000_000_000_000_000),
        "and" | "minus" => Some(0),
        _ => None,
    }
}

pub fn words_decode(input: &str) -> Result<i128, WordsError> {
    if input.is_empty() {
        return Err(WordsError::Empty);
    }

    for (index, byte) in input.bytes().enumerate() {
        if !(byte.is_ascii_lowercase() || byte == b' ' || byte == b'-') {
            return Err(WordsError::InvalidByte { index });
        }
    }

    let bytes = input.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b' ' || byte == b'-' {
            if index == 0
                || index + 1 == bytes.len()
                || !bytes[index - 1].is_ascii_lowercase()
                || !bytes[index + 1].is_ascii_lowercase()
            {
                return Err(WordsError::Separator { index });
            }
        }
    }

    let mut tokens: Vec<(&str, usize)> = Vec::new();
    let mut start = 0;
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b' ' || byte == b'-' {
            tokens.push((&input[start..index], start));
            start = index + 1;
        }
    }
    tokens.push((&input[start..], start));

    for (position, &(token, index)) in tokens.iter().enumerate() {
        if token == "minus" {
            if position != 0 {
                return Err(WordsError::InvalidWord { index });
            }
        } else if dictionary_value(token).is_none() {
            return Err(WordsError::InvalidWord { index });
        }
    }

    let negative = tokens.first().is_some_and(|(word, _)| *word == "minus");
    let arithmetic = if negative { &tokens[1..] } else { &tokens[..] };

    let mut total: u128 = 0;
    let mut group: u128 = 0;

    for &(word, _) in arithmetic {
        if word == "and" || word == "zero" {
            // No arithmetic effect.
        } else if word == "hundred" {
            group = group.checked_mul(100).ok_or(WordsError::Range)?;
        } else if let Some(scale) = scale_value(word) {
            total = total
                .checked_add(group.checked_mul(scale).ok_or(WordsError::Range)?)
                .ok_or(WordsError::Range)?;
            group = 0;
        } else if let Some(value) = dictionary_value(word) {
            group = group.checked_add(value).ok_or(WordsError::Range)?;
        }

        if total.checked_add(group).ok_or(WordsError::Range)? > LIMIT {
            return Err(WordsError::Range);
        }
    }

    let magnitude = total.checked_add(group).ok_or(WordsError::Range)?;
    let signed = if negative && magnitude != 0 {
        -(magnitude as i128)
    } else {
        magnitude as i128
    };

    let canonical = words_encode(signed)?;
    if canonical != input {
        let mismatch = canonical
            .bytes()
            .zip(input.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| canonical.len().min(input.len()));
        return Err(WordsError::NonCanonical { index: mismatch });
    }

    Ok(signed)
}

fn scale_value(word: &str) -> Option<u128> {
    match word {
        "thousand" => Some(1_000),
        "million" => Some(1_000_000),
        "billion" => Some(1_000_000_000),
        "trillion" => Some(1_000_000_000_000),
        "quadrillion" => Some(1_000_000_000_000_000),
        "quintillion" => Some(1_000_000_000_000_000_000),
        _ => None,
    }
}