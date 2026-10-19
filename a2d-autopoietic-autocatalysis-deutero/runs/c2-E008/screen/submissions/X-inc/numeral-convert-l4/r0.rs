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

fn encode_group(n: u16) -> String {
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

    if remainder >= 20 {
        out.push_str(TENS[(remainder / 10 - 2) as usize]);
        if remainder % 10 != 0 {
            out.push('-');
            out.push_str(SMALL[(remainder % 10) as usize]);
        }
    } else if remainder != 0 {
        out.push_str(SMALL[remainder as usize]);
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

    let mut groups = [0u16; 7];
    let mut rest = magnitude;
    for group in groups.iter_mut().rev() {
        *group = (rest % 1000) as u16;
        rest /= 1000;
    }

    let scales = [
        "quintillion",
        "quadrillion",
        "trillion",
        "billion",
        "million",
        "thousand",
        "",
    ];
    let mut parts: Vec<(usize, String)> = Vec::new();

    for (i, &group) in groups.iter().enumerate() {
        if group == 0 {
            continue;
        }
        let mut part = encode_group(group);
        if !scales[i].is_empty() {
            part.push(' ');
            part.push_str(scales[i]);
        }
        parts.push((i, part));
    }

    let mut out = String::new();
    if value < 0 {
        out.push_str("minus ");
    }

    for (j, (i, part)) in parts.iter().enumerate() {
        if j != 0 {
            let is_final_units_and = *i == 6
                && groups[6] < 100
                && groups[6] != 0
                && parts.iter().any(|(higher, _)| *higher < 6);
            out.push_str(if is_final_units_and { " and " } else { " " });
        }
        out.push_str(part);
    }

    Ok(out)
}

fn dictionary_value(word: &str) -> Option<Word> {
    if word == "zero" {
        return Some(Word::Number(0));
    }
    for (i, candidate) in SMALL.iter().enumerate().skip(1) {
        if word == *candidate {
            return Some(Word::Number(i as u128));
        }
    }
    for (i, candidate) in TENS.iter().enumerate() {
        if word == *candidate {
            return Some(Word::Number((i as u128 + 2) * 10));
        }
    }
    if word == "hundred" {
        return Some(Word::Hundred);
    }
    if word == "and" {
        return Some(Word::And);
    }
    for (name, value) in SCALES {
        if word == name {
            return Some(Word::Scale(value));
        }
    }
    None
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
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b' ' || *byte == b'-' {
            tokens.push((&input[start..index], start));
            start = index + 1;
        }
    }
    tokens.push((&input[start..], start));

    let mut words = Vec::with_capacity(tokens.len());
    for &(token, index) in &tokens {
        if token == "minus" {
            if index != 0 {
                return Err(WordsError::InvalidWord { index });
            }
            words.push(Word::And); // Placeholder; the leading sign is handled below.
        } else {
            match dictionary_value(token) {
                Some(word) => words.push(word),
                None => return Err(WordsError::InvalidWord { index }),
            }
        }
    }

    let negative = tokens.first().is_some_and(|(token, _)| *token == "minus");
    let arithmetic_start = usize::from(negative);
    let mut total: u128 = 0;
    let mut group: u128 = 0;

    for word in words.into_iter().skip(arithmetic_start) {
        match word {
            Word::Number(value) => group += value,
            Word::Hundred => group *= 100,
            Word::Scale(scale) => {
                total += group * scale;
                group = 0;
            }
            Word::And => {}
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
        let differing = canonical
            .bytes()
            .zip(input.bytes())
            .position(|(left, right)| left != right)
            .unwrap_or_else(|| canonical.len().min(input.len()));
        return Err(WordsError::NonCanonical { index: differing });
    }
    Ok(value)
}