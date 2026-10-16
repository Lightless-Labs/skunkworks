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

pub fn words_encode(value: i128) -> Result<String, WordsError> {
    let magnitude = value.unsigned_abs();
    if magnitude > LIMIT {
        return Err(WordsError::Range);
    }
    if magnitude == 0 {
        return Ok("zero".to_string());
    }

    let mut groups = [0u16; 7];
    let mut n = magnitude;
    for group in &mut groups {
        *group = (n % 1000) as u16;
        n /= 1000;
    }

    let mut parts: Vec<String> = Vec::new();
    for i in (0..groups.len()).rev() {
        let group = groups[i];
        if group == 0 {
            continue;
        }
        let mut part = spell_group(group);
        if i > 0 {
            part.push(' ');
            part.push_str(SCALES[i - 1]);
        }
        parts.push(part);
    }

    let units = groups[0];
    let use_cross_and = units > 0
        && units < 100
        && parts.len() > 1;

    let mut result = String::new();
    if value < 0 {
        result.push_str("minus ");
    }

    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            if use_cross_and && i + 1 == parts.len() {
                result.push_str(" and ");
            } else {
                result.push(' ');
            }
        }
        result.push_str(part);
    }
    Ok(result)
}

fn spell_group(n: u16) -> String {
    let mut result = String::new();
    let hundreds = n / 100;
    let remainder = n % 100;

    if hundreds > 0 {
        result.push_str(SMALL[hundreds as usize]);
        result.push_str(" hundred");
        if remainder > 0 {
            result.push_str(" and ");
        }
    }

    if remainder > 0 {
        if remainder < 20 {
            result.push_str(SMALL[remainder as usize]);
        } else {
            result.push_str(TENS[(remainder / 10 - 2) as usize]);
            if remainder % 10 > 0 {
                result.push('-');
                result.push_str(SMALL[(remainder % 10) as usize]);
            }
        }
    }
    result
}

pub fn words_decode(input: &str) -> Result<i128, WordsError> {
    if input.is_empty() {
        return Err(WordsError::Empty);
    }

    let bytes = input.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        if !(byte.is_ascii_lowercase() || byte == b' ' || byte == b'-') {
            return Err(WordsError::InvalidByte { index });
        }
    }

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

    for (i, &(word, index)) in tokens.iter().enumerate() {
        if word == "minus" {
            if i != 0 {
                return Err(WordsError::InvalidWord { index });
            }
        } else if !is_dictionary_word(word) {
            return Err(WordsError::InvalidWord { index });
        }
    }

    let negative = tokens.first().is_some_and(|(word, _)| *word == "minus");
    let arithmetic = if negative { &tokens[1..] } else { &tokens[..] };

    let mut total: u128 = 0;
    let mut group: u128 = 0;

    for &(word, _) in arithmetic {
        match word {
            "zero" => {}
            "and" => {}
            "hundred" => group *= 100,
            "thousand" => {
                total += group * 1_000;
                group = 0;
            }
            "million" => {
                total += group * 1_000_000;
                group = 0;
            }
            "billion" => {
                total += group * 1_000_000_000;
                group = 0;
            }
            "trillion" => {
                total += group * 1_000_000_000_000;
                group = 0;
            }
            "quadrillion" => {
                total += group * 1_000_000_000_000_000;
                group = 0;
            }
            "quintillion" => {
                total += group * 1_000_000_000_000_000_000;
                group = 0;
            }
            _ => {
                group += word_value(word).expect("dictionary words have numeric values");
            }
        }

        if total + group > LIMIT {
            return Err(WordsError::Range);
        }
    }

    let magnitude = total + group;
    let value = if negative && magnitude != 0 {
        -(magnitude as i128)
    } else {
        magnitude as i128
    };

    let canonical = words_encode(value)?;
    if canonical.as_bytes() != bytes {
        let first_difference = canonical
            .as_bytes()
            .iter()
            .zip(bytes.iter())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| canonical.len().min(bytes.len()));
        return Err(WordsError::NonCanonical {
            index: first_difference,
        });
    }
    Ok(value)
}

fn is_dictionary_word(word: &str) -> bool {
    matches!(
        word,
        "zero"
            | "hundred"
            | "thousand"
            | "million"
            | "billion"
            | "trillion"
            | "quadrillion"
            | "quintillion"
            | "and"
            | "one"
            | "two"
            | "three"
            | "four"
            | "five"
            | "six"
            | "seven"
            | "eight"
            | "nine"
            | "ten"
            | "eleven"
            | "twelve"
            | "thirteen"
            | "fourteen"
            | "fifteen"
            | "sixteen"
            | "seventeen"
            | "eighteen"
            | "nineteen"
            | "twenty"
            | "thirty"
            | "forty"
            | "fifty"
            | "sixty"
            | "seventy"
            | "eighty"
            | "ninety"
            | "minus"
    )
}

fn word_value(word: &str) -> Option<u128> {
    for (value, candidate) in SMALL.iter().enumerate().skip(1) {
        if word == *candidate {
            return Some(value as u128);
        }
    }
    for (index, candidate) in TENS.iter().enumerate() {
        if word == *candidate {
            return Some((20 + index * 10) as u128);
        }
    }
    None
}