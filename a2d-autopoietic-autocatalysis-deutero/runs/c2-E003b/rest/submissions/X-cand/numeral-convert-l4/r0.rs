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
    for index in (0..7).rev() {
        let group = groups[index];
        if group == 0 {
            continue;
        }

        let mut spelling = spell_group(group);
        if index > 0 {
            spelling.push(' ');
            spelling.push_str(SCALES[index - 1]);
        }
        parts.push(spelling);
    }

    let units = groups[0];
    let and_before_units = units > 0
        && units < 100
        && groups[1..].iter().any(|&group| group != 0);

    let mut result = String::new();
    if value < 0 {
        result.push_str("minus ");
    }

    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            if and_before_units && index + 1 == parts.len() {
                result.push_str(" and ");
            } else {
                result.push(' ');
            }
        }
        result.push_str(part);
    }

    Ok(result)
}

fn spell_group(value: u16) -> String {
    let mut result = String::new();
    let hundreds = value / 100;
    let remainder = value % 100;

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
        if !byte.is_ascii_lowercase() && byte != b' ' && byte != b'-' {
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

    for (token_index, &(word, position)) in tokens.iter().enumerate() {
        if word == "minus" {
            if token_index != 0 {
                return Err(WordsError::InvalidWord { index: position });
            }
        } else if !is_dictionary_word(word) {
            return Err(WordsError::InvalidWord { index: position });
        }
    }

    let negative = tokens.first().is_some_and(|(word, _)| *word == "minus");
    let arithmetic_tokens = if negative { &tokens[1..] } else { &tokens[..] };

    let mut total: u128 = 0;
    let mut group: u128 = 0;

    for &(word, _) in arithmetic_tokens {
        match word {
            "zero" | "and" => {}
            "one" => group += 1,
            "two" => group += 2,
            "three" => group += 3,
            "four" => group += 4,
            "five" => group += 5,
            "six" => group += 6,
            "seven" => group += 7,
            "eight" => group += 8,
            "nine" => group += 9,
            "ten" => group += 10,
            "eleven" => group += 11,
            "twelve" => group += 12,
            "thirteen" => group += 13,
            "fourteen" => group += 14,
            "fifteen" => group += 15,
            "sixteen" => group += 16,
            "seventeen" => group += 17,
            "eighteen" => group += 18,
            "nineteen" => group += 19,
            "twenty" => group += 20,
            "thirty" => group += 30,
            "forty" => group += 40,
            "fifty" => group += 50,
            "sixty" => group += 60,
            "seventy" => group += 70,
            "eighty" => group += 80,
            "ninety" => group += 90,
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
            _ => unreachable!(),
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
        let mismatch = canonical
            .as_bytes()
            .iter()
            .zip(bytes.iter())
            .position(|(left, right)| left != right)
            .unwrap_or_else(|| canonical.len().min(bytes.len()));
        return Err(WordsError::NonCanonical { index: mismatch });
    }

    Ok(value)
}

fn is_dictionary_word(word: &str) -> bool {
    word == "zero"
        || word == "hundred"
        || word == "and"
        || SCALES.contains(&word)
        || SMALL[1..].contains(&word)
        || TENS.contains(&word)
}