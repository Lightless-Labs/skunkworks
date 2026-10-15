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
    "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen",
    "eighteen", "nineteen",
];

const TENS: [&str; 8] = [
    "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

const SCALES: [(&str, u128); 6] = [
    ("thousand", 1_000),
    ("million", 1_000_000),
    ("billion", 1_000_000_000),
    ("trillion", 1_000_000_000_000),
    ("quadrillion", 1_000_000_000_000_000),
    ("quintillion", 1_000_000_000_000_000_000),
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
        out.push_str(TENS[(n / 10 - 2) as usize]);
        let unit = n % 10;
        if unit != 0 {
            out.push('-');
            out.push_str(SMALL[unit as usize]);
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

    let mut out = String::new();
    if value < 0 {
        out.push_str("minus ");
    }

    let mut groups = [0u16; 7];
    let mut remaining = magnitude;
    for group in &mut groups {
        *group = (remaining % 1000) as u16;
        remaining /= 1000;
    }

    let highest = (0..groups.len()).rev().find(|&i| groups[i] != 0).unwrap();
    let mut higher_written = false;

    for i in (0..groups.len()).rev() {
        let group = groups[i];
        if group == 0 {
            continue;
        }

        if higher_written {
            out.push(' ');
            if i == 0 && group < 100 {
                out.push_str("and ");
            }
        }

        append_under_thousand(group, &mut out);
        if i > 0 {
            out.push(' ');
            out.push_str(SCALES[i - 1].0);
        }
        higher_written = true;

        if i == highest {
            continue;
        }
    }

    Ok(out)
}

#[derive(Clone, Copy)]
enum TokenKind {
    Number(u128),
    Hundred,
    Scale(u128),
    And,
    Minus,
}

fn dictionary_word(word: &str) -> Option<TokenKind> {
    if word == "zero" {
        return Some(TokenKind::Number(0));
    }
    for (i, spelling) in SMALL.iter().enumerate().skip(1) {
        if word == *spelling {
            return Some(TokenKind::Number(i as u128));
        }
    }
    for (i, spelling) in TENS.iter().enumerate() {
        if word == *spelling {
            return Some(TokenKind::Number(((i + 2) * 10) as u128));
        }
    }
    if word == "hundred" {
        return Some(TokenKind::Hundred);
    }
    for &(spelling, value) in &SCALES {
        if word == spelling {
            return Some(TokenKind::Scale(value));
        }
    }
    match word {
        "and" => Some(TokenKind::And),
        "minus" => Some(TokenKind::Minus),
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
            let before_is_letter = index > 0 && bytes[index - 1].is_ascii_lowercase();
            let after_is_letter = index + 1 < bytes.len() && bytes[index + 1].is_ascii_lowercase();
            if !before_is_letter || !after_is_letter {
                return Err(WordsError::Separator { index });
            }
        }
    }

    let mut words: Vec<(&str, usize)> = Vec::new();
    let mut start = 0;
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b' ' || byte == b'-' {
            words.push((&input[start..index], start));
            start = index + 1;
        }
    }
    words.push((&input[start..], start));

    let mut kinds = Vec::with_capacity(words.len());
    for (word, index) in &words {
        let kind = dictionary_word(word).ok_or(WordsError::InvalidWord { index: *index })?;
        if matches!(kind, TokenKind::Minus) && *index != words[0].1 {
            return Err(WordsError::InvalidWord { index: *index });
        }
        kinds.push(kind);
    }

    let negative = matches!(kinds.first(), Some(TokenKind::Minus));
    let arithmetic = if negative { &kinds[1..] } else { &kinds[..] };

    let mut total = 0u128;
    let mut group = 0u128;
    for kind in arithmetic {
        match *kind {
            TokenKind::Number(value) => group += value,
            TokenKind::Hundred => group *= 100,
            TokenKind::Scale(scale) => {
                total += group * scale;
                group = 0;
            }
            TokenKind::And => {}
            TokenKind::Minus => unreachable!(),
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
    if canonical != input {
        let common = canonical
            .as_bytes()
            .iter()
            .zip(bytes.iter())
            .take_while(|(a, b)| a == b)
            .count();
        return Err(WordsError::NonCanonical { index: common });
    }

    Ok(value)
}

fn main() {}