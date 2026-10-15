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

fn append_under_thousand(value: u16, out: &mut String) {
    if value >= 100 {
        out.push_str(SMALL[(value / 100) as usize]);
        out.push_str(" hundred");
        let remainder = value % 100;
        if remainder != 0 {
            out.push_str(" and ");
            append_under_hundred(remainder, out);
        }
    } else {
        append_under_hundred(value, out);
    }
}

fn append_under_hundred(value: u16, out: &mut String) {
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

    let mut parts: Vec<String> = Vec::new();
    for index in (0..7).rev() {
        let group = groups[index];
        if group == 0 {
            continue;
        }

        let mut part = String::new();
        append_under_thousand(group, &mut part);
        if index > 0 {
            part.push(' ');
            part.push_str(SCALES[index - 1]);
        }
        parts.push(part);
    }

    let units = groups[0];
    let use_cross_and = units > 0
        && units < 100
        && groups[1..].iter().any(|&group| group != 0);

    let mut result = String::new();
    if value < 0 {
        result.push_str("minus ");
    }
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            if use_cross_and && i == parts.len() - 1 {
                result.push_str(" and ");
            } else {
                result.push(' ');
            }
        }
        result.push_str(part);
    }
    Ok(result)
}

fn dictionary_value(token: &str) -> Option<TokenKind> {
    if token == "minus" {
        return Some(TokenKind::Minus);
    }
    if token == "and" {
        return Some(TokenKind::And);
    }
    if token == "hundred" {
        return Some(TokenKind::Hundred);
    }

    for (value, word) in SMALL.iter().enumerate() {
        if token == *word {
            return Some(TokenKind::Number(value as u128));
        }
    }
    for (i, word) in TENS.iter().enumerate() {
        if token == *word {
            return Some(TokenKind::Number((i as u128 + 2) * 10));
        }
    }
    for (i, word) in SCALES.iter().enumerate() {
        if token == *word {
            return Some(TokenKind::Scale(1000u128.pow(i as u32 + 1)));
        }
    }
    None
}

#[derive(Clone, Copy)]
enum TokenKind {
    Minus,
    And,
    Hundred,
    Number(u128),
    Scale(u128),
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

    let mut kinds = Vec::with_capacity(tokens.len());
    for &(token, position) in &tokens {
        let kind = dictionary_value(token).ok_or(WordsError::InvalidWord { index: position })?;
        if matches!(kind, TokenKind::Minus) && position != tokens[0].1 {
            return Err(WordsError::InvalidWord { index: position });
        }
        kinds.push(kind);
    }

    let negative = matches!(kinds.first(), Some(TokenKind::Minus));
    let arithmetic = if negative { &kinds[1..] } else { &kinds[..] };

    let mut total = 0u128;
    let mut group = 0u128;

    for kind in arithmetic {
        match *kind {
            TokenKind::Minus => return Err(WordsError::InvalidWord { index: 0 }),
            TokenKind::And => {}
            TokenKind::Number(value) => {
                group = group.checked_add(value).ok_or(WordsError::Range)?;
            }
            TokenKind::Hundred => {
                group = group.checked_mul(100).ok_or(WordsError::Range)?;
            }
            TokenKind::Scale(scale) => {
                let scaled = group.checked_mul(scale).ok_or(WordsError::Range)?;
                total = total.checked_add(scaled).ok_or(WordsError::Range)?;
                group = 0;
            }
        }

        if total.checked_add(group).ok_or(WordsError::Range)? > LIMIT {
            return Err(WordsError::Range);
        }
    }

    let magnitude = total.checked_add(group).ok_or(WordsError::Range)?;
    if magnitude > LIMIT {
        return Err(WordsError::Range);
    }

    let value = if negative && magnitude != 0 {
        -(magnitude as i128)
    } else {
        magnitude as i128
    };

    let canonical = words_encode(value)?;
    if canonical.as_bytes() != input.as_bytes() {
        let differing = canonical
            .bytes()
            .zip(input.bytes())
            .position(|(left, right)| left != right)
            .unwrap_or_else(|| canonical.len().min(input.len()));
        return Err(WordsError::NonCanonical { index: differing });
    }
    Ok(value)
}

fn main() {}