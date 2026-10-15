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

const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

const SCALES: [(u128, &str); 6] = [
    (1_000_000_000_000_000, "quadrillion"),
    (1_000_000_000_000, "trillion"),
    (1_000_000_000, "billion"),
    (1_000_000, "million"),
    (1_000, "thousand"),
    (1, ""),
];

fn append_under_thousand(n: u16, out: &mut String) {
    if n >= 100 {
        out.push_str(SMALL[(n / 100) as usize]);
        out.push_str(" hundred");
        let remainder = n % 100;
        if remainder != 0 {
            out.push_str(" and ");
            append_under_hundred(remainder, out);
        }
    } else {
        append_under_hundred(n, out);
    }
}

fn append_under_hundred(n: u16, out: &mut String) {
    if n < 20 {
        out.push_str(SMALL[n as usize]);
    } else {
        out.push_str(TENS[(n / 10) as usize]);
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

    let mut result = String::new();
    if value < 0 {
        result.push_str("minus ");
    }

    let mut remaining = magnitude;
    let mut groups: Vec<(u16, &'static str)> = Vec::new();
    for &(scale, name) in &SCALES {
        let group = (remaining / scale) as u16;
        remaining %= scale;
        if group != 0 {
            groups.push((group, name));
        }
    }

    for (index, &(group, scale)) in groups.iter().enumerate() {
        if index > 0 {
            let is_final_units_and = scale.is_empty()
                && group < 100
                && groups[..index].iter().any(|(_, s)| !s.is_empty());
            result.push_str(if is_final_units_and { " and " } else { " " });
        }
        append_under_thousand(group, &mut result);
        if !scale.is_empty() {
            result.push(' ');
            result.push_str(scale);
        }
    }
    Ok(result)
}

#[derive(Clone, Copy)]
enum TokenKind {
    Number(u128),
    And,
    Hundred,
    Scale(u128),
    Minus,
}

fn token_kind(word: &str) -> Option<TokenKind> {
    if word == "minus" {
        return Some(TokenKind::Minus);
    }
    if word == "and" {
        return Some(TokenKind::And);
    }
    if word == "hundred" {
        return Some(TokenKind::Hundred);
    }
    for (i, spelling) in SMALL.iter().enumerate() {
        if word == *spelling {
            return Some(TokenKind::Number(i as u128));
        }
    }
    for (i, spelling) in TENS.iter().enumerate().skip(2) {
        if word == *spelling {
            return Some(TokenKind::Number((i * 10) as u128));
        }
    }
    match word {
        "thousand" => Some(TokenKind::Scale(1_000)),
        "million" => Some(TokenKind::Scale(1_000_000)),
        "billion" => Some(TokenKind::Scale(1_000_000_000)),
        "trillion" => Some(TokenKind::Scale(1_000_000_000_000)),
        "quadrillion" => Some(TokenKind::Scale(1_000_000_000_000_000)),
        "quintillion" => Some(TokenKind::Scale(1_000_000_000_000_000_000)),
        _ => None,
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

    let mut tokens: Vec<(usize, &str, TokenKind)> = Vec::new();
    let mut start = 0;
    for index in 0..=bytes.len() {
        if index == bytes.len() || bytes[index] == b' ' || bytes[index] == b'-' {
            let word = &input[start..index];
            let kind = token_kind(word).ok_or(WordsError::InvalidWord { index: start })?;
            tokens.push((start, word, kind));
            start = index + 1;
        }
    }

    let negative = matches!(tokens.first().map(|t| t.2), Some(TokenKind::Minus));
    for (i, (position, _, kind)) in tokens.iter().enumerate() {
        if matches!(kind, TokenKind::Minus) && i != 0 {
            return Err(WordsError::InvalidWord { index: *position });
        }
    }

    let mut total: u128 = 0;
    let mut group: u128 = 0;
    for (_, _, kind) in tokens.iter().skip(usize::from(negative)) {
        match *kind {
            TokenKind::Minus => {}
            TokenKind::Number(value) => group += value,
            TokenKind::And => {}
            TokenKind::Hundred => group *= 100,
            TokenKind::Scale(scale) => {
                total += group * scale;
                group = 0;
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

fn main() {}