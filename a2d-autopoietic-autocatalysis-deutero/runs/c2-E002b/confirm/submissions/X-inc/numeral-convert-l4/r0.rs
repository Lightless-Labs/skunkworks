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
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty",
    "ninety",
];

const SCALES: [&str; 6] = [
    "thousand", "million", "billion", "trillion", "quadrillion", "quintillion",
];

fn spell_under_thousand(n: u16) -> String {
    if n < 20 {
        return SMALL[n as usize].to_owned();
    }
    if n < 100 {
        let tens = TENS[(n / 10) as usize];
        let units = n % 10;
        return if units == 0 {
            tens.to_owned()
        } else {
            format!("{tens}-{}", SMALL[units as usize])
        };
    }

    let hundreds = n / 100;
    let remainder = n % 100;
    if remainder == 0 {
        format!("{} hundred", SMALL[hundreds as usize])
    } else {
        format!(
            "{} hundred and {}",
            SMALL[hundreds as usize],
            spell_under_thousand(remainder)
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

    let mut groups = Vec::new();
    let mut remaining = magnitude;
    for index in 0..=6 {
        let group = (remaining % 1000) as u16;
        remaining /= 1000;
        groups.push(group);
        if remaining == 0 && index == 6 {
            break;
        }
    }

    let mut parts = Vec::new();
    for index in (0..groups.len()).rev() {
        let group = groups[index];
        if group == 0 {
            continue;
        }

        let mut part = spell_under_thousand(group);
        if index > 0 {
            part.push(' ');
            part.push_str(SCALES[index - 1]);
        }
        parts.push((part, index, group));
    }

    let mut output = String::new();
    if value < 0 {
        output.push_str("minus ");
    }
    for (position, (part, index, group)) in parts.iter().enumerate() {
        if position > 0 {
            let is_final_small_units = *index == 0 && *group < 100;
            output.push_str(if is_final_small_units { " and " } else { " " });
        }
        output.push_str(part);
    }
    Ok(output)
}

fn dictionary_value(word: &str) -> Option<Word> {
    match word {
        "zero" => Some(Word::Number(0)),
        "one" => Some(Word::Number(1)),
        "two" => Some(Word::Number(2)),
        "three" => Some(Word::Number(3)),
        "four" => Some(Word::Number(4)),
        "five" => Some(Word::Number(5)),
        "six" => Some(Word::Number(6)),
        "seven" => Some(Word::Number(7)),
        "eight" => Some(Word::Number(8)),
        "nine" => Some(Word::Number(9)),
        "ten" => Some(Word::Number(10)),
        "eleven" => Some(Word::Number(11)),
        "twelve" => Some(Word::Number(12)),
        "thirteen" => Some(Word::Number(13)),
        "fourteen" => Some(Word::Number(14)),
        "fifteen" => Some(Word::Number(15)),
        "sixteen" => Some(Word::Number(16)),
        "seventeen" => Some(Word::Number(17)),
        "eighteen" => Some(Word::Number(18)),
        "nineteen" => Some(Word::Number(19)),
        "twenty" => Some(Word::Number(20)),
        "thirty" => Some(Word::Number(30)),
        "forty" => Some(Word::Number(40)),
        "fifty" => Some(Word::Number(50)),
        "sixty" => Some(Word::Number(60)),
        "seventy" => Some(Word::Number(70)),
        "eighty" => Some(Word::Number(80)),
        "ninety" => Some(Word::Number(90)),
        "hundred" => Some(Word::Hundred),
        "thousand" => Some(Word::Scale(1_000)),
        "million" => Some(Word::Scale(1_000_000)),
        "billion" => Some(Word::Scale(1_000_000_000)),
        "trillion" => Some(Word::Scale(1_000_000_000_000)),
        "quadrillion" => Some(Word::Scale(1_000_000_000_000_000)),
        "quintillion" => Some(Word::Scale(1_000_000_000_000_000_000)),
        "and" => Some(Word::And),
        "minus" => Some(Word::Minus),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum Word {
    Number(u128),
    Hundred,
    Scale(u128),
    And,
    Minus,
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

    for index in 0..bytes.len() {
        if bytes[index] == b' ' || bytes[index] == b'-' {
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
    for index in 0..=bytes.len() {
        if index == bytes.len() || bytes[index] == b' ' || bytes[index] == b'-' {
            tokens.push((start, &input[start..index]));
            start = index + 1;
        }
    }

    let mut words = Vec::with_capacity(tokens.len());
    for (index, (position, token)) in tokens.iter().enumerate() {
        let word = dictionary_value(token)
            .ok_or(WordsError::InvalidWord { index: *position })?;
        if matches!(word, Word::Minus) && index != 0 {
            return Err(WordsError::InvalidWord { index: *position });
        }
        words.push(word);
    }

    let negative = matches!(words.first(), Some(Word::Minus));
    let mut total = 0u128;
    let mut group = 0u128;

    for word in words.into_iter().skip(if negative { 1 } else { 0 }) {
        match word {
            Word::Number(value) => group += value,
            Word::Hundred => group *= 100,
            Word::Scale(scale) => {
                total += group * scale;
                group = 0;
            }
            Word::And | Word::Minus => {}
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
            .position(|(left, right)| left != right)
            .unwrap_or_else(|| canonical.len().min(bytes.len()));
        return Err(WordsError::NonCanonical { index: mismatch });
    }
    Ok(value)
}