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

    let mut result = String::new();
    if value < 0 {
        result.push_str("minus ");
    }
    if magnitude == 0 {
        result.push_str("zero");
        return Ok(result);
    }

    let mut groups = [0u16; 7];
    let mut n = magnitude;
    for group in &mut groups {
        *group = (n % 1000) as u16;
        n /= 1000;
    }

    let nonzero: Vec<usize> = (0..7).rev().filter(|&i| groups[i] != 0).collect();
    for (position, &i) in nonzero.iter().enumerate() {
        if position > 0 {
            result.push(' ');
        }
        if i == 0 && groups[i] < 100 && !result.is_empty() {
            result.push_str("and ");
        }

        append_group(&mut result, groups[i]);

        if i > 0 {
            result.push(' ');
            result.push_str(SCALES[i - 1]);
        }
    }
    Ok(result)
}

fn append_group(out: &mut String, value: u16) {
    if value >= 100 {
        out.push_str(SMALL[(value / 100) as usize]);
        out.push_str(" hundred");
        let remainder = value % 100;
        if remainder != 0 {
            out.push_str(" and ");
            append_under_hundred(out, remainder);
        }
    } else {
        append_under_hundred(out, value);
    }
}

fn append_under_hundred(out: &mut String, value: u16) {
    if value < 20 {
        out.push_str(SMALL[value as usize]);
    } else {
        out.push_str(TENS[(value / 10 - 2) as usize]);
        if value % 10 != 0 {
            out.push('-');
            out.push_str(SMALL[(value % 10) as usize]);
        }
    }
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
                || !(bytes[index - 1].is_ascii_lowercase()
                    && bytes[index + 1].is_ascii_lowercase())
            {
                return Err(WordsError::Separator { index });
            }
        }
    }

    let tokens: Vec<(usize, &str)> = input
        .split(|c| c == ' ' || c == '-')
        .scan(0usize, |offset, token| {
            let start = *offset;
            *offset += token.len() + 1;
            Some((start, token))
        })
        .collect();

    for (i, &(index, token)) in tokens.iter().enumerate() {
        if !is_dictionary_word(token) || (token == "minus" && i != 0) {
            return Err(WordsError::InvalidWord { index });
        }
    }

    let negative = tokens.first().is_some_and(|(_, token)| *token == "minus");
    let arithmetic_start = usize::from(negative);
    let mut total: u128 = 0;
    let mut group: u128 = 0;

    for &(_, token) in &tokens[arithmetic_start..] {
        match token {
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
            "thousand" => apply_scale(&mut total, &mut group, 1_000),
            "million" => apply_scale(&mut total, &mut group, 1_000_000),
            "billion" => apply_scale(&mut total, &mut group, 1_000_000_000),
            "trillion" => apply_scale(&mut total, &mut group, 1_000_000_000_000),
            "quadrillion" => apply_scale(&mut total, &mut group, 1_000_000_000_000_000),
            "quintillion" => apply_scale(&mut total, &mut group, 1_000_000_000_000_000_000),
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
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| canonical.len().min(bytes.len()));
        return Err(WordsError::NonCanonical { index: mismatch });
    }
    Ok(value)
}

fn apply_scale(total: &mut u128, group: &mut u128, scale: u128) {
    *total += *group * scale;
    *group = 0;
}

fn is_dictionary_word(word: &str) -> bool {
    word == "minus"
        || word == "and"
        || word == "hundred"
        || SMALL.contains(&word)
        || TENS.contains(&word)
        || SCALES.contains(&word)
}

fn main() {}