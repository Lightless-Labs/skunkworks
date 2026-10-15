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

fn encode_group(value: u16, out: &mut String) {
    if value >= 100 {
        out.push_str(SMALL[(value / 100) as usize]);
        out.push_str(" hundred");
        let remainder = value % 100;
        if remainder != 0 {
            out.push_str(" and ");
            encode_group(remainder, out);
        }
    } else if value < 20 {
        out.push_str(SMALL[value as usize]);
    } else {
        out.push_str(TENS[(value / 10 - 2) as usize]);
        let units = value % 10;
        if units != 0 {
            out.push('-');
            out.push_str(SMALL[units as usize]);
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

    let mut result = String::new();
    if value < 0 {
        result.push_str("minus ");
    }

    let mut groups = [0u16; 7];
    let mut remaining = magnitude;
    for group in &mut groups {
        *group = (remaining % 1000) as u16;
        remaining /= 1000;
    }

    let mut emitted_higher = false;
    for index in (1..groups.len()).rev() {
        let group = groups[index];
        if group == 0 {
            continue;
        }
        if emitted_higher {
            result.push(' ');
        }
        encode_group(group, &mut result);
        result.push(' ');
        result.push_str(SCALES[index - 1]);
        emitted_higher = true;
    }

    let units = groups[0];
    if units != 0 {
        if emitted_higher {
            result.push(' ');
            if units < 100 {
                result.push_str("and ");
            }
        }
        encode_group(units, &mut result);
    }

    Ok(result)
}

fn dictionary_value(word: &str) -> Option<Word> {
    match word {
        "zero" => Some(Word::Number(0)),
        "one" => Some(Word::Number(1)),
        "two" => Some(Word::Number(2)),
        "three" => Some(Word::Number(3)),
        "four" => Some(Word::Number(4)),
        "five" => Some(Word::Number(5)),
        "six" => Some(Word::Number(6)),
        "seven" => Some(Word::Number(7)),
        "eight" => Some(Word::Number(8)),
        "nine" => Some(Word::Number(9)),
        "ten" => Some(Word::Number(10)),
        "eleven" => Some(Word::Number(11)),
        "twelve" => Some(Word::Number(12)),
        "thirteen" => Some(Word::Number(13)),
        "fourteen" => Some(Word::Number(14)),
        "fifteen" => Some(Word::Number(15)),
        "sixteen" => Some(Word::Number(16)),
        "seventeen" => Some(Word::Number(17)),
        "eighteen" => Some(Word::Number(18)),
        "nineteen" => Some(Word::Number(19)),
        "twenty" => Some(Word::Number(20)),
        "thirty" => Some(Word::Number(30)),
        "forty" => Some(Word::Number(40)),
        "fifty" => Some(Word::Number(50)),
        "sixty" => Some(Word::Number(60)),
        "seventy" => Some(Word::Number(70)),
        "eighty" => Some(Word::Number(80)),
        "ninety" => Some(Word::Number(90)),
        "hundred" => Some(Word::Hundred),
        "thousand" => Some(Word::Scale(1_000)),
        "million" => Some(Word::Scale(1_000_000)),
        "billion" => Some(Word::Scale(1_000_000_000)),
        "trillion" => Some(Word::Scale(1_000_000_000_000)),
        "quadrillion" => Some(Word::Scale(1_000_000_000_000_000)),
        "quintillion" => Some(Word::Scale(1_000_000_000_000_000_000)),
        "and" => Some(Word::And),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum Word {
    Number(u128),
    Hundred,
    Scale(u128),
    And,
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

    let mut tokens: Vec<(&str, usize)> = Vec::new();
    let mut start = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b' ' || *byte == b'-' {
            tokens.push((&input[start..index], start));
            start = index + 1;
        }
    }
    tokens.push((&input[start..], start));

    let mut words = Vec::with_capacity(tokens.len());
    for &(token, position) in &tokens {
        if token == "minus" {
            if position != 0 {
                return Err(WordsError::InvalidWord { index: position });
            }
            words.push(None);
        } else if let Some(word) = dictionary_value(token) {
            words.push(Some(word));
        } else {
            return Err(WordsError::InvalidWord { index: position });
        }
    }

    let negative = tokens.first().is_some_and(|(token, _)| *token == "minus");
    let first_arithmetic_token = usize::from(negative);

    let mut total = 0u128;
    let mut group = 0u128;

    for word in words.iter().skip(first_arithmetic_token) {
        match word {
            Some(Word::Number(value)) => group += value,
            Some(Word::Hundred) => group *= 100,
            Some(Word::Scale(scale)) => {
                total += group * scale;
                group = 0;
            }
            Some(Word::And) => {}
            None => {}
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
    if canonical.as_bytes() != input.as_bytes() {
        let mismatch = canonical
            .bytes()
            .zip(input.bytes())
            .position(|(left, right)| left != right)
            .unwrap_or_else(|| canonical.len().min(input.len()));
        return Err(WordsError::NonCanonical { index: mismatch });
    }

    Ok(value)
}

fn main() {}