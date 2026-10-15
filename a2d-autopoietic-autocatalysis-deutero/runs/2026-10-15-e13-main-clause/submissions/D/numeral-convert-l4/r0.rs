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

const SCALES: [(u128, &str); 6] = [
    (1_000_000_000_000_000_000, "quintillion"),
    (1_000_000_000_000_000, "quadrillion"),
    (1_000_000_000_000, "trillion"),
    (1_000_000_000, "billion"),
    (1_000_000, "million"),
    (1_000, "thousand"),
];

fn encode_under_thousand(n: u16) -> String {
    let mut out = String::new();
    let hundreds = n / 100;
    let remainder = n % 100;

    if hundreds != 0 {
        out.push_str(SMALL[hundreds as usize]);
        out.push_str(" hundred");
        if remainder != 0 {
            out.push_str(" and ");
        }
    }

    if remainder != 0 {
        if remainder < 20 {
            out.push_str(SMALL[remainder as usize]);
        } else {
            out.push_str(TENS[(remainder / 10 - 2) as usize]);
            if remainder % 10 != 0 {
                out.push('-');
                out.push_str(SMALL[(remainder % 10) as usize]);
            }
        }
    }

    out
}

pub fn words_encode(value: i128) -> Result<String, WordsError> {
    let magnitude = value.unsigned_abs();
    if magnitude > LIMIT {
        return Err(WordsError::Range);
    }

    if magnitude == 0 {
        return Ok("zero".to_string());
    }

    let mut remaining = magnitude;
    let mut parts: Vec<(String, bool, u16)> = Vec::new();

    for &(scale, name) in &SCALES {
        let group = (remaining / scale) as u16;
        remaining %= scale;
        if group != 0 {
            parts.push((format!("{} {}", encode_under_thousand(group), name), true, group));
        }
    }

    if remaining != 0 {
        let group = remaining as u16;
        parts.push((encode_under_thousand(group), false, group));
    }

    let mut result = String::new();
    if value < 0 {
        result.push_str("minus ");
    }

    for (i, (part, scaled, group)) in parts.iter().enumerate() {
        if i != 0 {
            let use_and = !scaled && *group < 100;
            result.push_str(if use_and { " and " } else { " " });
        }
        result.push_str(part);
    }

    Ok(result)
}

fn dictionary_value(word: &str) -> Option<(u128, TokenKind)> {
    if word == "zero" {
        return Some((0, TokenKind::Number));
    }
    for (i, candidate) in SMALL.iter().enumerate().skip(1) {
        if word == *candidate {
            return Some((i as u128, TokenKind::Number));
        }
    }
    for (i, candidate) in TENS.iter().enumerate() {
        if word == *candidate {
            return Some(((i as u128 + 2) * 10, TokenKind::Number));
        }
    }

    match word {
        "hundred" => Some((100, TokenKind::Hundred)),
        "thousand" => Some((1_000, TokenKind::Scale)),
        "million" => Some((1_000_000, TokenKind::Scale)),
        "billion" => Some((1_000_000_000, TokenKind::Scale)),
        "trillion" => Some((1_000_000_000_000, TokenKind::Scale)),
        "quadrillion" => Some((1_000_000_000_000_000, TokenKind::Scale)),
        "quintillion" => Some((1_000_000_000_000_000_000, TokenKind::Scale)),
        "and" => Some((0, TokenKind::And)),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum TokenKind {
    Number,
    Hundred,
    Scale,
    And,
}

#[derive(Clone, Copy)]
struct Token<'a> {
    word: &'a str,
    index: usize,
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

    let mut tokens = Vec::new();
    let mut start = 0;
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b' ' || byte == b'-' {
            tokens.push(Token {
                word: &input[start..index],
                index: start,
            });
            start = index + 1;
        }
    }
    tokens.push(Token {
        word: &input[start..],
        index: start,
    });

    for (i, token) in tokens.iter().enumerate() {
        if token.word == "minus" {
            if i != 0 {
                return Err(WordsError::InvalidWord { index: token.index });
            }
        } else if dictionary_value(token.word).is_none() {
            return Err(WordsError::InvalidWord { index: token.index });
        }
    }

    let negative = tokens.first().is_some_and(|token| token.word == "minus");
    let arithmetic_tokens = if negative { &tokens[1..] } else { &tokens[..] };

    let mut total: u128 = 0;
    let mut group: u128 = 0;

    for token in arithmetic_tokens {
        let (value, kind) = dictionary_value(token.word).expect("dictionary scan completed");
        match kind {
            TokenKind::Number => group += value,
            TokenKind::Hundred => group *= 100,
            TokenKind::Scale => {
                total += group * value;
                group = 0;
            }
            TokenKind::And => {}
        }

        if total + group > LIMIT {
            return Err(WordsError::Range);
        }
    }

    let magnitude = total + group;
    let signed = if negative {
        -(magnitude as i128)
    } else {
        magnitude as i128
    };

    let canonical = words_encode(signed)?;
    if canonical.as_bytes() != bytes {
        let common = canonical
            .as_bytes()
            .iter()
            .zip(bytes.iter())
            .take_while(|(a, b)| a == b)
            .count();
        return Err(WordsError::NonCanonical { index: common });
    }

    Ok(signed)
}