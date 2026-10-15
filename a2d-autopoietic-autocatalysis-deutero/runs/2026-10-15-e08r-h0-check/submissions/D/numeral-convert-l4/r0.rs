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

const SCALES: [&str; 6] = [
    "thousand", "million", "billion", "trillion", "quadrillion", "quintillion",
];

fn spell_under_thousand(n: u16) -> String {
    if n < 20 {
        return SMALL[n as usize].to_string();
    }
    if n < 100 {
        let tens = TENS[(n / 10) as usize];
        let unit = n % 10;
        return if unit == 0 {
            tens.to_string()
        } else {
            format!("{tens}-{}", SMALL[unit as usize])
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
        return Ok("zero".to_string());
    }

    let mut groups = Vec::new();
    let mut remaining = magnitude;
    while remaining > 0 {
        groups.push((remaining % 1000) as u16);
        remaining /= 1000;
    }

    let mut parts: Vec<(usize, String)> = Vec::new();
    for (index, group) in groups.iter().enumerate().rev() {
        if *group == 0 {
            continue;
        }
        let mut spelling = spell_under_thousand(*group);
        if index > 0 {
            spelling.push(' ');
            spelling.push_str(SCALES[index - 1]);
        }
        parts.push((index, spelling));
    }

    let mut output = String::new();
    for (i, (index, spelling)) in parts.iter().enumerate() {
        if i > 0 {
            let is_final_units_under_100 = *index == 0
                && *groups.first().unwrap_or(&0) < 100;
            output.push_str(if is_final_units_under_100 {
                " and "
            } else {
                " "
            });
        }
        output.push_str(spelling);
    }

    if value < 0 {
        Ok(format!("minus {output}"))
    } else {
        Ok(output)
    }
}

fn dictionary_value(word: &str) -> Option<Word> {
    if word == "zero" {
        return Some(Word::Zero);
    }
    for (i, spelling) in SMALL.iter().enumerate().skip(1) {
        if word == *spelling {
            return Some(Word::Number(i as u128));
        }
    }
    for (i, spelling) in TENS.iter().enumerate().skip(2) {
        if word == *spelling {
            return Some(Word::Number((i * 10) as u128));
        }
    }
    match word {
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
    Zero,
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
        let word = dictionary_value(token).ok_or(WordsError::InvalidWord { index })?;
        if matches!(word, Word::Minus) && index != 0 {
            return Err(WordsError::InvalidWord { index });
        }
        words.push(word);
    }

    let negative = matches!(words.first(), Some(Word::Minus));
    let arithmetic = if negative { &words[1..] } else { &words[..] };

    let mut total: u128 = 0;
    let mut group: u128 = 0;
    for word in arithmetic {
        match *word {
            Word::Zero | Word::And => {}
            Word::Number(value) => group = group.saturating_add(value),
            Word::Hundred => group = group.saturating_mul(100),
            Word::Scale(scale) => {
                total = total.saturating_add(group.saturating_mul(scale));
                group = 0;
            }
            Word::Minus => unreachable!(),
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
        let mismatch = canonical
            .bytes()
            .zip(bytes.iter().copied())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| canonical.len().min(bytes.len()));
        return Err(WordsError::NonCanonical { index: mismatch });
    }

    Ok(value)
}