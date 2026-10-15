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

const SMALL: [&str; 20] = [
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

fn under_thousand(n: u16) -> String {
    if n < 20 {
        return SMALL[n as usize].to_string();
    }

    if n < 100 {
        let ten = TENS[(n / 10 - 2) as usize];
        let unit = n % 10;
        if unit == 0 {
            ten.to_string()
        } else {
            format!("{}-{}", ten, SMALL[unit as usize])
        }
    } else {
        let hundreds = format!("{} hundred", SMALL[(n / 100) as usize]);
        let remainder = n % 100;
        if remainder == 0 {
            hundreds
        } else {
            format!("{} and {}", hundreds, under_thousand(remainder))
        }
    }
}

pub fn words_encode(value: i128) -> Result<String, WordsError> {
    let magnitude = value.unsigned_abs();
    if magnitude > LIMIT {
        return Err(WordsError::Range);
    }
    if magnitude == 0 {
        return Ok("zero".to_string());
    }

    let mut groups = Vec::new();
    let mut remaining = magnitude;
    for index in (0..=6).rev() {
        let group = ((remaining / 1000u128.pow(index as u32)) % 1000) as u16;
        if group != 0 {
            groups.push((index, group));
        }
    }

    let units_present = groups.iter().any(|(index, _)| *index == 0);
    let units_value = groups
        .iter()
        .find(|(index, _)| *index == 0)
        .map(|(_, group)| *group)
        .unwrap_or(0);
    let has_higher = groups.iter().any(|(index, _)| *index > 0);

    let mut parts = Vec::new();
    for (index, group) in groups {
        let mut part = under_thousand(group);
        if index > 0 {
            part.push(' ');
            part.push_str(SCALES[index - 1]);
        }
        parts.push(part);
    }

    let mut result = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            let is_last_units = units_present
                && has_higher
                && i == parts.len() - 1
                && units_value < 100;
            result.push_str(if is_last_units { " and " } else { " " });
        }
        result.push_str(part);
    }

    if value < 0 {
        Ok(format!("minus {}", result))
    } else {
        Ok(result)
    }
}

fn word_value(word: &str) -> Option<u128> {
    if word == "zero" {
        return Some(0);
    }
    for (i, candidate) in SMALL.iter().enumerate().skip(1) {
        if word == *candidate {
            return Some(i as u128);
        }
    }
    for (i, candidate) in TENS.iter().enumerate() {
        if word == *candidate {
            return Some(((i + 2) * 10) as u128);
        }
    }
    None
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

fn known_word(word: &str, is_first: bool) -> bool {
    word == "minus" && is_first
        || word == "hundred"
        || word == "and"
        || scale_value(word).is_some()
        || word_value(word).is_some()
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

    let tokens: Vec<(usize, &str)> = input
        .split(|c| c == ' ' || c == '-')
        .scan(0usize, |offset, word| {
            let start = *offset;
            *offset += word.len() + 1;
            Some((start, word))
        })
        .collect();

    for (token_index, (position, word)) in tokens.iter().enumerate() {
        if !known_word(word, token_index == 0) {
            return Err(WordsError::InvalidWord { index: *position });
        }
    }

    let negative = tokens.first().map(|(_, word)| *word) == Some("minus");
    let arithmetic_tokens = if negative { &tokens[1..] } else { &tokens[..] };

    let mut total = 0u128;
    let mut group = 0u128;
    for (_, word) in arithmetic_tokens {
        if let Some(value) = word_value(word) {
            group += value;
        } else if *word == "hundred" {
            group *= 100;
        } else if *word == "and" {
            // No arithmetic effect.
        } else if let Some(scale) = scale_value(word) {
            total += group * scale;
            group = 0;
        }

        if total + group > LIMIT {
            return Err(WordsError::Range);
        }
    }

    let magnitude = total + group;
    let signed_value = if negative && magnitude != 0 {
        -(magnitude as i128)
    } else {
        magnitude as i128
    };

    let canonical = words_encode(signed_value)?;
    if canonical != input {
        let differing_index = canonical
            .bytes()
            .zip(input.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| canonical.len().min(input.len()));
        return Err(WordsError::NonCanonical {
            index: differing_index,
        });
    }

    Ok(signed_value)
}

fn main() {}