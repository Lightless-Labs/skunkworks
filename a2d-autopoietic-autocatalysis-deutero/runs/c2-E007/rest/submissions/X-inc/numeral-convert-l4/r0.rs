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
        let ten = n / 10;
        let unit = n % 10;
        out.push_str(TENS[(ten - 2) as usize]);
        if unit != 0 {
            out.push('-');
            out.push_str(SMALL[unit as usize]);
        }
    }
}

fn append_group(n: u16, out: &mut String) {
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

    let mut groups: Vec<(u16, Option<&'static str>)> = Vec::new();
    let mut remaining = magnitude;
    for &(scale, name) in &SCALES {
        let group = (remaining / scale) as u16;
        remaining %= scale;
        if group != 0 {
            groups.push((group, Some(name)));
        }
    }
    if remaining != 0 {
        groups.push((remaining as u16, None));
    }

    let units_is_small = groups
        .last()
        .is_some_and(|(group, scale)| scale.is_none() && *group < 100);

    for (i, &(group, scale)) in groups.iter().enumerate() {
        if i != 0 {
            if units_is_small && i + 1 == groups.len() {
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

fn dictionary_value(word: &str) -> Option<DictWord> {
    for (i, &name) in SMALL.iter().enumerate() {
        if word == name {
            return Some(DictWord::Number(i as u128));
        }
    }
    for (i, &name) in TENS.iter().enumerate() {
        if word == name {
            return Some(DictWord::Number(((i + 2) * 10) as u128));
        }
    }
    match word {
        "hundred" => Some(DictWord::Hundred),
        "thousand" => Some(DictWord::Scale(1_000)),
        "million" => Some(DictWord::Scale(1_000_000)),
        "billion" => Some(DictWord::Scale(1_000_000_000)),
        "trillion" => Some(DictWord::Scale(1_000_000_000_000)),
        "quadrillion" => Some(DictWord::Scale(1_000_000_000_000_000)),
        "quintillion" => Some(DictWord::Scale(1_000_000_000_000_000_000)),
        "and" => Some(DictWord::And),
        "minus" => Some(DictWord::Minus),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum DictWord {
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

    let bytes = input.as_bytes();

    for (index, &byte) in bytes.iter().enumerate() {
        if !(byte.is_ascii_lowercase() || byte == b' ' || byte == b'-') {
            return Err(WordsError::InvalidByte { index });
        }
    }

    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b' ' || byte == b'-' {
            let before_is_letter = index > 0 && bytes[index - 1].is_ascii_lowercase();
            let after_is_letter =
                index + 1 < bytes.len() && bytes[index + 1].is_ascii_lowercase();
            if !before_is_letter || !after_is_letter {
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
        if matches!(word, DictWord::Minus) && index != tokens[0].1 {
            return Err(WordsError::InvalidWord { index });
        }
        words.push(word);
    }

    let negative = matches!(words.first(), Some(DictWord::Minus));
    let arithmetic = if negative { &words[1..] } else { &words[..] };

    let mut total: u128 = 0;
    let mut group: u128 = 0;

    for word in arithmetic {
        match *word {
            DictWord::Number(value) => {
                group = group.checked_add(value).ok_or(WordsError::Range)?;
            }
            DictWord::Hundred => {
                group = group.checked_mul(100).ok_or(WordsError::Range)?;
            }
            DictWord::Scale(scale) => {
                let scaled = group.checked_mul(scale).ok_or(WordsError::Range)?;
                total = total.checked_add(scaled).ok_or(WordsError::Range)?;
                group = 0;
            }
            DictWord::And => {}
            DictWord::Minus => unreachable!(),
        }
        if total.checked_add(group).ok_or(WordsError::Range)? > LIMIT {
            return Err(WordsError::Range);
        }
    }

    let magnitude = total.checked_add(group).ok_or(WordsError::Range)?;
    if magnitude > LIMIT {
        return Err(WordsError::Range);
    }

    let value = if negative && magnitude != 0 {
        -(magnitude as i128)
    } else {
        magnitude as i128
    };

    let canonical = words_encode(value)?;
    if canonical.as_bytes() != bytes {
        let common_len = canonical.len().min(bytes.len());
        let differing_index = (0..common_len)
            .find(|&index| canonical.as_bytes()[index] != bytes[index])
            .unwrap_or(common_len);
        return Err(WordsError::NonCanonical {
            index: differing_index,
        });
    }

    Ok(value)
}