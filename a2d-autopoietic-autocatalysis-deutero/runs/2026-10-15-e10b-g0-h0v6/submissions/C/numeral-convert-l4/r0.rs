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

const SCALES: [(u128, &str); 6] = [
    (1_000_000_000_000_000_000, "quintillion"),
    (1_000_000_000_000_000, "quadrillion"),
    (1_000_000_000_000, "trillion"),
    (1_000_000_000, "billion"),
    (1_000_000, "million"),
    (1_000, "thousand"),
];

fn spell_under_thousand(n: u16) -> String {
    if n < 20 {
        return SMALL[n as usize].to_string();
    }
    if n < 100 {
        let tens = TENS[(n / 10 - 2) as usize];
        let units = n % 10;
        return if units == 0 {
            tens.to_string()
        } else {
            format!("{tens}-{}", SMALL[units as usize])
        };
    }

    let hundreds = SMALL[(n / 100) as usize];
    let remainder = n % 100;
    if remainder == 0 {
        format!("{hundreds} hundred")
    } else {
        format!("{hundreds} hundred and {}", spell_under_thousand(remainder))
    }
}

fn encode_magnitude(mut magnitude: u128) -> String {
    if magnitude == 0 {
        return "zero".to_string();
    }

    let mut parts = Vec::new();
    for &(scale, name) in &SCALES {
        let group = magnitude / scale;
        if group != 0 {
            parts.push((spell_under_thousand(group as u16), Some(name)));
            magnitude %= scale;
        }
    }
    if magnitude != 0 {
        parts.push((spell_under_thousand(magnitude as u16), None));
    }

    let units_and = parts.len() > 1
        && parts.last().is_some_and(|(spelling, scale)| {
            scale.is_none()
                && spelling
                    .split([' ', '-'])
                    .count()
                    <= 2
                && magnitude < 100
        });

    let mut output = String::new();
    for (index, (spelling, scale)) in parts.iter().enumerate() {
        if index != 0 {
            if units_and && index + 1 == parts.len() {
                output.push_str(" and ");
            } else {
                output.push(' ');
            }
        }
        output.push_str(spelling);
        if let Some(scale) = scale {
            output.push(' ');
            output.push_str(scale);
        }
    }
    output
}

pub fn words_encode(value: i128) -> Result<String, WordsError> {
    let magnitude = value.unsigned_abs();
    if magnitude > LIMIT {
        return Err(WordsError::Range);
    }
    let spelling = encode_magnitude(magnitude);
    if value < 0 {
        Ok(format!("minus {spelling}"))
    } else {
        Ok(spelling)
    }
}

fn dictionary_value(word: &str) -> Option<u128> {
    if word == "zero" {
        return Some(0);
    }
    for (i, spelling) in SMALL.iter().enumerate().skip(1) {
        if word == *spelling {
            return Some(i as u128);
        }
    }
    for (i, spelling) in TENS.iter().enumerate() {
        if word == *spelling {
            return Some(((i + 2) * 10) as u128);
        }
    }
    match word {
        "hundred" => Some(100),
        "thousand" => Some(1_000),
        "million" => Some(1_000_000),
        "billion" => Some(1_000_000_000),
        "trillion" => Some(1_000_000_000_000),
        "quadrillion" => Some(1_000_000_000_000_000),
        "quintillion" => Some(LIMIT),
        "and" | "minus" => Some(0),
        _ => None,
    }
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
        .split(|ch| ch == ' ' || ch == '-')
        .scan(0usize, |offset, token| {
            let start = *offset;
            *offset += token.len() + 1;
            Some((start, token))
        })
        .collect();

    for (position, (index, token)) in tokens.iter().enumerate() {
        if *token == "minus" {
            if position != 0 {
                return Err(WordsError::InvalidWord { index: *index });
            }
        } else if dictionary_value(token).is_none() || *token == "zero" && position == 0 && tokens.len() > 1 {
            // "zero" is a dictionary word; the additional condition is intentionally
            // not used for validation and is handled by canonical comparison.
            if dictionary_value(token).is_none() {
                return Err(WordsError::InvalidWord { index: *index });
            }
        }
    }

    let negative = tokens.first().is_some_and(|(_, word)| *word == "minus");
    let arithmetic_tokens = if negative { &tokens[1..] } else { &tokens[..] };

    let mut total = 0u128;
    let mut group = 0u128;

    for (_, token) in arithmetic_tokens {
        match *token {
            "and" => {}
            "hundred" => group = group.saturating_mul(100),
            "thousand" => {
                total = total.saturating_add(group.saturating_mul(1_000));
                group = 0;
            }
            "million" => {
                total = total.saturating_add(group.saturating_mul(1_000_000));
                group = 0;
            }
            "billion" => {
                total = total.saturating_add(group.saturating_mul(1_000_000_000));
                group = 0;
            }
            "trillion" => {
                total = total.saturating_add(group.saturating_mul(1_000_000_000_000));
                group = 0;
            }
            "quadrillion" => {
                total = total.saturating_add(group.saturating_mul(1_000_000_000_000_000));
                group = 0;
            }
            "quintillion" => {
                total = total.saturating_add(group.saturating_mul(LIMIT));
                group = 0;
            }
            word => group = group.saturating_add(dictionary_value(word).unwrap_or(0)),
        }

        if total.saturating_add(group) > LIMIT {
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
        let differing_index = canonical
            .bytes()
            .zip(input.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| canonical.len().min(input.len()));
        return Err(WordsError::NonCanonical {
            index: differing_index,
        });
    }
    Ok(value)
}

fn main() {}