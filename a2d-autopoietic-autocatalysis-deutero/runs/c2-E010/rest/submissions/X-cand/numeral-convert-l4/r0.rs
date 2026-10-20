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
    (1_000_000_000_000_000, "quadrillion"),
    (1_000_000_000_000, "trillion"),
    (1_000_000_000, "billion"),
    (1_000_000, "million"),
    (1_000, "thousand"),
    (1, ""),
];

fn encode_under_thousand(n: u16) -> String {
    if n < 20 {
        return SMALL[n as usize].to_owned();
    }
    if n < 100 {
        let ten = TENS[(n / 10 - 2) as usize];
        let unit = n % 10;
        return if unit == 0 {
            ten.to_owned()
        } else {
            format!("{ten}-{}", SMALL[unit as usize])
        };
    }
    let hundreds = n / 100;
    let rem = n % 100;
    if rem == 0 {
        format!("{} hundred", SMALL[hundreds as usize])
    } else {
        format!("{} hundred and {}", SMALL[hundreds as usize], encode_under_thousand(rem))
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

    let scales: [(u128, &str); 7] = [
        (1_000_000_000_000_000_000, "quintillion"),
        (1_000_000_000_000_000, "quadrillion"),
        (1_000_000_000_000, "trillion"),
        (1_000_000_000, "billion"),
        (1_000_000, "million"),
        (1_000, "thousand"),
        (1, ""),
    ];

    let mut remaining = magnitude;
    let mut parts: Vec<(String, bool, u16)> = Vec::new();
    for (scale, name) in scales {
        let group = (remaining / scale) as u16;
        remaining %= scale;
        if group != 0 {
            let mut part = encode_under_thousand(group);
            if !name.is_empty() {
                part.push(' ');
                part.push_str(name);
            }
            parts.push((part, name.is_empty(), group));
        }
    }

    let mut out = String::new();
    for i in 0..parts.len() {
        if i > 0 {
            let last_is_units_under_100 = i == parts.len() - 1
                && parts[i].1
                && parts[i].2 < 100;
            out.push_str(if last_is_units_under_100 { " and " } else { " " });
        }
        out.push_str(&parts[i].0);
    }

    if value < 0 {
        Ok(format!("minus {out}"))
    } else {
        Ok(out)
    }
}

fn word_value(word: &str) -> Option<u128> {
    match word {
        "zero" => Some(0),
        "one" => Some(1),
        "two" => Some(2),
        "three" => Some(3),
        "four" => Some(4),
        "five" => Some(5),
        "six" => Some(6),
        "seven" => Some(7),
        "eight" => Some(8),
        "nine" => Some(9),
        "ten" => Some(10),
        "eleven" => Some(11),
        "twelve" => Some(12),
        "thirteen" => Some(13),
        "fourteen" => Some(14),
        "fifteen" => Some(15),
        "sixteen" => Some(16),
        "seventeen" => Some(17),
        "eighteen" => Some(18),
        "nineteen" => Some(19),
        "twenty" => Some(20),
        "thirty" => Some(30),
        "forty" => Some(40),
        "fifty" => Some(50),
        "sixty" => Some(60),
        "seventy" => Some(70),
        "eighty" => Some(80),
        "ninety" => Some(90),
        "hundred" | "and" => Some(0),
        "thousand" => Some(1_000),
        "million" => Some(1_000_000),
        "billion" => Some(1_000_000_000),
        "trillion" => Some(1_000_000_000_000),
        "quadrillion" => Some(1_000_000_000_000_000),
        "quintillion" => Some(1_000_000_000_000_000_000),
        _ => None,
    }
}

fn is_scale(word: &str) -> Option<u128> {
    match word {
        "thousand" => Some(1_000),
        "million" => Some(1_000_000),
        "billion" => Some(1_000_000_000),
        "trillion" => Some(1_000_000_000_000),
        "quadrillion" => Some(1_000_000_000_000_000),
        "quintillion" => Some(1_000_000_000_000_000_000),
        _ => None,
    }
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

    let tokens: Vec<(usize, &str)> = input
        .split(|c| c == ' ' || c == '-')
        .scan(0usize, |offset, token| {
            let start = *offset;
            *offset += token.len() + 1;
            Some((start, token))
        })
        .collect();

    for (i, (index, token)) in tokens.iter().enumerate() {
        if *token == "minus" {
            if i != 0 {
                return Err(WordsError::InvalidWord { index: *index });
            }
        } else if word_value(token).is_none() {
            return Err(WordsError::InvalidWord { index: *index });
        }
    }

    let negative = tokens.first().is_some_and(|(_, word)| *word == "minus");
    let arithmetic_tokens = if negative { &tokens[1..] } else { &tokens[..] };

    let mut total: u128 = 0;
    let mut group: u128 = 0;
    for (_, token) in arithmetic_tokens {
        match *token {
            "zero" => {}
            "hundred" => group *= 100,
            "and" => {}
            scale_word => {
                if let Some(scale) = is_scale(scale_word) {
                    total += group * scale;
                    group = 0;
                } else {
                    group += word_value(scale_word).unwrap();
                }
            }
        }
        if total + group > LIMIT {
            return Err(WordsError::Range);
        }
    }

    let magnitude = total + group;
    let value = if negative {
        -(magnitude as i128)
    } else {
        magnitude as i128
    };
    let canonical = words_encode(value)?;
    if canonical.as_bytes() != input.as_bytes() {
        let a = canonical.as_bytes();
        let b = input.as_bytes();
        let common = a.len().min(b.len());
        let index = (0..common).find(|&i| a[i] != b[i]).unwrap_or(common);
        return Err(WordsError::NonCanonical { index });
    }
    Ok(value)
}
