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

fn append_under_thousand(n: u16, out: &mut String) {
    if n >= 100 {
        out.push_str(SMALL[(n / 100) as usize]);
        out.push_str(" hundred");
        let rem = n % 100;
        if rem != 0 {
            out.push_str(" and ");
            append_under_hundred(rem, out);
        }
    } else {
        append_under_hundred(n, out);
    }
}

fn append_under_hundred(n: u16, out: &mut String) {
    if n < 20 {
        out.push_str(SMALL[n as usize]);
    } else {
        out.push_str(TENS[(n / 10 - 2) as usize]);
        if n % 10 != 0 {
            out.push('-');
            out.push_str(SMALL[(n % 10) as usize]);
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
    while remaining != 0 {
        groups.push((remaining % 1000) as u16);
        remaining /= 1000;
    }

    let mut parts: Vec<(String, bool)> = Vec::new();
    for (index, &group) in groups.iter().enumerate().rev() {
        if group == 0 {
            continue;
        }
        let mut part = String::new();
        append_under_thousand(group, &mut part);
        if index > 0 {
            part.push(' ');
            part.push_str(SCALES[index - 1]);
        }
        parts.push((part, index == 0 && group < 100));
    }

    let mut result = String::new();
    if value < 0 {
        result.push_str("minus ");
    }
    for (i, (part, is_and_join)) in parts.iter().enumerate() {
        if i > 0 {
            if *is_and_join {
                result.push_str(" and ");
            } else {
                result.push(' ');
            }
        }
        result.push_str(part);
    }
    Ok(result)
}

fn word_value(word: &str) -> Option<u128> {
    if word == "zero" {
        return Some(0);
    }
    for (i, w) in SMALL.iter().enumerate().skip(1) {
        if word == *w {
            return Some(i as u128);
        }
    }
    for (i, w) in TENS.iter().enumerate() {
        if word == *w {
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

fn is_dictionary_word(word: &str) -> bool {
    word == "minus"
        || word == "hundred"
        || word == "and"
        || word_value(word).is_some()
        || scale_value(word).is_some()
}

pub fn words_decode(input: &str) -> Result<i128, WordsError> {
    if input.is_empty() {
        return Err(WordsError::Empty);
    }

    for (index, byte) in input.bytes().enumerate() {
        if !byte.is_ascii_lowercase() && byte != b' ' && byte != b'-' {
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

    for (i, (position, word)) in tokens.iter().enumerate() {
        if !is_dictionary_word(word) || (*word == "minus" && i != 0) {
            return Err(WordsError::InvalidWord { index: *position });
        }
    }

    let negative = tokens.first().map(|(_, word)| *word == "minus").unwrap_or(false);
    let arithmetic_tokens = if negative { &tokens[1..] } else { &tokens[..] };

    let mut total: u128 = 0;
    let mut group: u128 = 0;

    for (_, word) in arithmetic_tokens {
        if let Some(value) = word_value(word) {
            group += value;
        } else if *word == "hundred" {
            group *= 100;
        } else if let Some(scale) = scale_value(word) {
            total += group * scale;
            group = 0;
        } else if *word == "and" {
        }

        if total + group > LIMIT {
            return Err(WordsError::Range);
        }
    }

    let magnitude = total + group;
    let signed = if negative && magnitude != 0 {
        -(magnitude as i128)
    } else {
        magnitude as i128
    };

    let canonical = words_encode(signed)?;
    if canonical.as_bytes() != input.as_bytes() {
        let mismatch = canonical
            .bytes()
            .zip(input.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| canonical.len().min(input.len()));
        return Err(WordsError::NonCanonical { index: mismatch });
    }
    Ok(signed)
}

fn main() {}