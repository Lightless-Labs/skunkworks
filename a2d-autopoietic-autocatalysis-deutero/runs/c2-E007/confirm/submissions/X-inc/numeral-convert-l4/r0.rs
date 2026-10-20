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

const SCALES: [(&str, u128); 6] = [
    ("thousand", 1_000),
    ("million", 1_000_000),
    ("billion", 1_000_000_000),
    ("trillion", 1_000_000_000_000),
    ("quadrillion", 1_000_000_000_000_000),
    ("quintillion", 1_000_000_000_000_000_000),
];

fn encode_under_thousand(n: u16) -> String {
    if n < 20 {
        return SMALL[n as usize].to_owned();
    }
    if n < 100 {
        let tens = TENS[(n / 10 - 2) as usize];
        let units = n % 10;
        if units == 0 {
            return tens.to_owned();
        }
        return format!("{}-{}", tens, SMALL[units as usize]);
    }

    let hundreds = n / 100;
    let remainder = n % 100;
    if remainder == 0 {
        format!("{} hundred", SMALL[hundreds as usize])
    } else {
        format!(
            "{} hundred and {}",
            SMALL[hundreds as usize],
            encode_under_thousand(remainder)
        )
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

    let mut remaining = magnitude;
    let mut groups: Vec<(u16, Option<&'static str>)> = Vec::new();

    for &(scale, value) in SCALES.iter().rev() {
        let group = remaining / value;
        if group != 0 {
            groups.push((group as u16, Some(scale)));
            remaining %= value;
        }
    }
    if remaining != 0 {
        groups.push((remaining as u16, None));
    }

    let units_and = groups
        .last()
        .is_some_and(|(group, scale)| scale.is_none() && *group < 100)
        && groups.len() > 1;

    let mut parts = Vec::with_capacity(groups.len());
    for (i, (group, scale)) in groups.iter().enumerate() {
        let mut part = encode_under_thousand(*group);
        if let Some(scale) = scale {
            part.push(' ');
            part.push_str(scale);
        }
        if units_and && i + 1 == groups.len() {
            parts.push(format!("and {}", part));
        } else {
            parts.push(part);
        }
    }

    let mut result = parts.join(" ");
    if value < 0 {
        result.insert_str(0, "minus ");
    }
    Ok(result)
}

fn dictionary_value(word: &str) -> Option<DictionaryWord> {
    if word == "minus" {
        return Some(DictionaryWord::Minus);
    }
    if word == "and" {
        return Some(DictionaryWord::And);
    }
    if word == "hundred" {
        return Some(DictionaryWord::Hundred);
    }
    for (i, candidate) in SMALL.iter().enumerate() {
        if word == *candidate {
            return Some(DictionaryWord::Number(i as u128));
        }
    }
    for (i, candidate) in TENS.iter().enumerate() {
        if word == *candidate {
            return Some(DictionaryWord::Number((i as u128 + 2) * 10));
        }
    }
    for &(scale, value) in &SCALES {
        if word == scale {
            return Some(DictionaryWord::Scale(value));
        }
    }
    None
}

#[derive(Clone, Copy)]
enum DictionaryWord {
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

    let mut words = Vec::with_capacity(tokens.len());
    for &(token, index) in &tokens {
        match dictionary_value(token) {
            Some(DictionaryWord::Minus) if index == 0 => words.push(DictionaryWord::Minus),
            Some(DictionaryWord::Minus) => {
                return Err(WordsError::InvalidWord { index });
            }
            Some(word) => words.push(word),
            None => return Err(WordsError::InvalidWord { index }),
        }
    }

    let negative = matches!(words.first(), Some(DictionaryWord::Minus));
    let arithmetic = if negative { &words[1..] } else { &words[..] };

    let mut total: u128 = 0;
    let mut group: u128 = 0;
    for word in arithmetic {
        match *word {
            DictionaryWord::Minus => unreachable!(),
            DictionaryWord::And => {}
            DictionaryWord::Number(value) => group += value,
            DictionaryWord::Hundred => group *= 100,
            DictionaryWord::Scale(scale) => {
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
        let differing = canonical
            .as_bytes()
            .iter()
            .zip(bytes.iter())
            .position(|(left, right)| left != right)
            .unwrap_or_else(|| canonical.len().min(bytes.len()));
        return Err(WordsError::NonCanonical { index: differing });
    }
    Ok(value)
}