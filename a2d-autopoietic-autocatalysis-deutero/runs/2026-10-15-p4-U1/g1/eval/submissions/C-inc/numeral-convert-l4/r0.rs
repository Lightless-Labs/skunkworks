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

fn append_under_hundred(n: u16, out: &mut String) {
    if n < 20 {
        out.push_str(SMALL[n as usize]);
    } else {
        let tens = TENS[(n / 10 - 2) as usize];
        out.push_str(tens);
        if n % 10 != 0 {
            out.push('-');
            out.push_str(SMALL[(n % 10) as usize]);
        }
    }
}

fn append_group(n: u16, out: &mut String) {
    if n >= 100 {
        out.push_str(SMALL[(n / 100) as usize]);
        out.push_str(" hundred");
        let rem = n % 100;
        if rem != 0 {
            out.push_str(" and ");
            append_under_hundred(rem, out);
        }
    } else {
        append_under_hundred(n, out);
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

    let mut remainder = magnitude;
    let mut groups: Vec<(u16, Option<&str>)> = Vec::new();
    for &(scale, name) in &SCALES {
        let group = (remainder / scale) as u16;
        remainder %= scale;
        if group != 0 {
            groups.push((group, Some(name)));
        }
    }
    if remainder != 0 {
        groups.push((remainder as u16, None));
    }

    let units_is_small = groups
        .last()
        .is_some_and(|(n, scale)| scale.is_none() && *n < 100);
    let last_index = groups.len().saturating_sub(1);

    for (i, &(group, scale)) in groups.iter().enumerate() {
        if i != 0 {
            if i == last_index && units_is_small {
                out.push_str(" and ");
            } else {
                out.push(' ');
            }
        }
        append_group(group, &mut out);
        if let Some(name) = scale {
            out.push(' ');
            out.push_str(name);
        }
    }
    Ok(out)
}

fn dictionary_value(word: &str) -> Option<Word> {
    if word == "zero" {
        return Some(Word::Zero);
    }
    for (i, candidate) in SMALL.iter().enumerate().skip(1) {
        if word == *candidate {
            return Some(Word::Number(i as u128));
        }
    }
    for (i, candidate) in TENS.iter().enumerate() {
        if word == *candidate {
            return Some(Word::Number(((i + 2) * 10) as u128));
        }
    }
    if word == "hundred" {
        return Some(Word::Hundred);
    }
    if word == "and" {
        return Some(Word::And);
    }
    for &(scale, name) in &SCALES {
        if word == name {
            return Some(Word::Scale(scale));
        }
    }
    if word == "minus" {
        return Some(Word::Minus);
    }
    None
}

#[derive(Clone, Copy)]
enum Word {
    Zero,
    Number(u128),
    Hundred,
    Scale(u128),
    And,
    Minus,
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
            let before_letter = index > 0 && bytes[index - 1].is_ascii_lowercase();
            let after_letter = index + 1 < bytes.len() && bytes[index + 1].is_ascii_lowercase();
            if !before_letter || !after_letter {
                return Err(WordsError::Separator { index });
            }
        }
    }

    let mut tokens: Vec<(usize, &str)> = Vec::new();
    let mut start = None;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b' ' || *byte == b'-' {
            if let Some(s) = start.take() {
                tokens.push((s, &input[s..index]));
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(s) = start {
        tokens.push((s, &input[s..]));
    }

    let mut words = Vec::with_capacity(tokens.len());
    for &(index, token) in &tokens {
        match dictionary_value(token) {
            Some(Word::Minus) if index == tokens[0].0 => words.push(Word::Minus),
            Some(Word::Minus) | None => return Err(WordsError::InvalidWord { index }),
            Some(word) => words.push(word),
        }
    }

    let negative = matches!(words.first(), Some(Word::Minus));
    let arithmetic = if negative { &words[1..] } else { &words[..] };

    let mut total: u128 = 0;
    let mut group: u128 = 0;
    for word in arithmetic {
        match *word {
            Word::Zero | Word::And => {}
            Word::Number(n) => group += n,
            Word::Hundred => group *= 100,
            Word::Scale(scale) => {
                total += group * scale;
                group = 0;
            }
            Word::Minus => unreachable!(),
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
    if canonical != input {
        let mismatch = canonical
            .bytes()
            .zip(input.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| canonical.len().min(input.len()));
        return Err(WordsError::NonCanonical { index: mismatch });
    }
    Ok(value)
}

fn main() {}