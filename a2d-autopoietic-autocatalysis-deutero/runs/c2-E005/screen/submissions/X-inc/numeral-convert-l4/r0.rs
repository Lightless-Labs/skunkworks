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

const SCALES: [(&str, u128); 7] = [
    ("quintillion", 1_000_000_000_000_000_000),
    ("quadrillion", 1_000_000_000_000_000),
    ("trillion", 1_000_000_000_000),
    ("billion", 1_000_000_000),
    ("million", 1_000_000),
    ("thousand", 1_000),
    ("", 1),
];

fn spell_under_thousand(n: u16) -> String {
    if n < 20 {
        return SMALL[n as usize].to_owned();
    }

    if n < 100 {
        let tens = TENS[(n / 10 - 2) as usize];
        let units = n % 10;
        if units == 0 {
            tens.to_owned()
        } else {
            format!("{}-{}", tens, SMALL[units as usize])
        }
    } else {
        let hundreds = format!("{} hundred", SMALL[(n / 100) as usize]);
        let remainder = n % 100;
        if remainder == 0 {
            hundreds
        } else {
            format!("{} and {}", hundreds, spell_under_thousand(remainder))
        }
    }
}

pub fn words_encode(value: i128) -> Result<String, WordsError> {
    let magnitude = value.unsigned_abs();
    if magnitude > LIMIT {
        return Err(WordsError::Range);
    }
    if magnitude == 0 {
        return Ok("zero".to_owned());
    }

    let mut groups = Vec::new();
    for (scale, factor) in SCALES {
        let group = (magnitude / factor) % 1000;
        if group != 0 {
            let mut phrase = spell_under_thousand(group as u16);
            if !scale.is_empty() {
                phrase.push(' ');
                phrase.push_str(scale);
            }
            groups.push((phrase, scale.is_empty(), group));
        }
    }

    let mut result = String::new();
    for (i, (phrase, is_units, group)) in groups.iter().enumerate() {
        if i != 0 {
            result.push(' ');
            if *is_units && *group < 100 {
                result.push_str("and ");
            }
        }
        result.push_str(phrase);
    }

    if value < 0 {
        Ok(format!("minus {}", result))
    } else {
        Ok(result)
    }
}

fn word_value(word: &str) -> Option<(u128, TokenKind)> {
    if word == "zero" {
        return Some((0, TokenKind::Number));
    }
    for (i, spelling) in SMALL.iter().enumerate().skip(1) {
        if word == *spelling {
            return Some((i as u128, TokenKind::Number));
        }
    }
    for (i, spelling) in TENS.iter().enumerate() {
        if word == *spelling {
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
            tokens.push((start, &input[start..index]));
            start = index + 1;
        }
    }
    tokens.push((start, &input[start..]));

    for (i, &(index, word)) in tokens.iter().enumerate() {
        if word == "minus" {
            if i != 0 {
                return Err(WordsError::InvalidWord { index });
            }
        } else if word_value(word).is_none() {
            return Err(WordsError::InvalidWord { index });
        }
    }

    let negative = tokens[0].1 == "minus";
    let arithmetic_tokens = if negative { &tokens[1..] } else { &tokens[..] };

    let mut total: u128 = 0;
    let mut group: u128 = 0;

    for &(_, word) in arithmetic_tokens {
        if word == "minus" {
            return Err(WordsError::InvalidWord {
                index: tokens[0].0,
            });
        }
        let (value, kind) = word_value(word).expect("dictionary scan validated token");
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
    let signed = if negative && magnitude != 0 {
        -(magnitude as i128)
    } else {
        magnitude as i128
    };

    let canonical = words_encode(signed)?;
    if canonical != input {
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