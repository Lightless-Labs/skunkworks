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

    let mut groups = Vec::new();
    let mut remaining = magnitude;
    while remaining > 0 {
        groups.push((remaining % 1000) as u16);
        remaining /= 1000;
    }
    groups.reverse();

    let mut nonzero = Vec::new();
    let group_count = groups.len();
    for (i, group) in groups.into_iter().enumerate() {
        if group == 0 {
            continue;
        }
        let scale_index = group_count - 1 - i;
        let mut spelling = spell_group(group);
        if scale_index > 0 {
            spelling.push(' ');
            spelling.push_str(SCALES[scale_index - 1]);
        }
        nonzero.push((spelling, group, scale_index));
    }

    let mut output = String::new();
    if value < 0 {
        output.push_str("minus ");
    }

    for (i, (spelling, group, scale_index)) in nonzero.iter().enumerate() {
        if i > 0 {
            let is_units_group = *scale_index == 0;
            let use_cross_and = is_units_group && *group < 100;
            output.push_str(if use_cross_and { " and " } else { " " });
        }
        output.push_str(spelling);
    }

    Ok(output)
}

fn spell_group(value: u16) -> String {
    let mut output = String::new();
    let hundreds = value / 100;
    let remainder = value % 100;

    if hundreds > 0 {
        output.push_str(SMALL[hundreds as usize]);
        output.push_str(" hundred");
        if remainder > 0 {
            output.push_str(" and ");
        }
    }

    if remainder > 0 {
        if remainder < 20 {
            output.push_str(SMALL[remainder as usize]);
        } else {
            let tens = remainder / 10;
            let units = remainder % 10;
            output.push_str(TENS[(tens - 2) as usize]);
            if units > 0 {
                output.push('-');
                output.push_str(SMALL[units as usize]);
            }
        }
    }

    output
}

pub fn words_decode(input: &str) -> Result<i128, WordsError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(WordsError::Empty);
    }

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

    let mut tokens = Vec::new();
    let mut start = 0;
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b' ' || byte == b'-' {
            tokens.push((&input[start..index], start));
            start = index + 1;
        }
    }
    tokens.push((&input[start..], start));

    for (i, &(token, position)) in tokens.iter().enumerate() {
        if token == "minus" {
            if i != 0 {
                return Err(WordsError::InvalidWord { index: position });
            }
        } else if !is_dictionary_word(token) {
            return Err(WordsError::InvalidWord { index: position });
        }
    }

    let negative = tokens.first().is_some_and(|(word, _)| *word == "minus");
    let arithmetic_tokens = if negative { &tokens[1..] } else { &tokens[..] };

    let mut total: u128 = 0;
    let mut group: u128 = 0;

    for &(token, _) in arithmetic_tokens {
        if let Some(value) = small_value(token) {
            group += value;
        } else if let Some(value) = tens_value(token) {
            group += value;
        } else if token == "hundred" {
            group *= 100;
        } else if let Some(scale) = scale_value(token) {
            total += group * scale;
            group = 0;
        } else if token == "and" || token == "zero" {
            // No arithmetic effect.
        }

        if total + group > LIMIT {
            return Err(WordsError::Range);
        }
    }

    let magnitude = total + group;
    let signed = magnitude as i128;
    let value = if negative && magnitude != 0 {
        -signed
    } else {
        signed
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
        || small_value(word).is_some()
        || tens_value(word).is_some()
        || word == "hundred"
        || scale_value(word).is_some()
        || word == "and"
}

fn small_value(word: &str) -> Option<u128> {
    SMALL
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(value, spelling)| (*spelling == word).then_some(value as u128))
}

fn tens_value(word: &str) -> Option<u128> {
    TENS.iter()
        .enumerate()
        .find_map(|(index, spelling)| (*spelling == word).then_some((index as u128 + 2) * 10))
}

fn scale_value(word: &str) -> Option<u128> {
    SCALES
        .iter()
        .enumerate()
        .find_map(|(index, spelling)| (*spelling == word).then_some(1000u128.pow(index as u32 + 1)))
}

fn main() {}