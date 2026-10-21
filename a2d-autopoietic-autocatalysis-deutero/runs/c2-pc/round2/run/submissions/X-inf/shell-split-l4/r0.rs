#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
    UnterminatedExpansion { pos: usize },
    InvalidExpansion { pos: usize },
    CommandSubstitution { pos: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub text: String,
    pub glob: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinError {
    Nul { word: usize, pos: usize },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quote {
    Bare,
    Single,
    Double,
}

struct WordBuilder {
    text: String,
    glob: Vec<usize>,
    active: bool,
}

impl WordBuilder {
    fn new() -> Self {
        Self {
            text: String::new(),
            glob: Vec::new(),
            active: false,
        }
    }

    fn put(&mut self, value: &str, wildcards: bool) {
        for ch in value.chars() {
            if wildcards && matches!(ch, '*' | '?' | '[') {
                self.glob.push(self.text.len());
            }
            self.text.push(ch);
        }
    }

    fn push(&mut self, ch: char, wildcards: bool) {
        if wildcards && matches!(ch, '*' | '?' | '[') {
            self.glob.push(self.text.len());
        }
        self.text.push(ch);
    }

    fn flush(&mut self, output: &mut Vec<Word>) {
        if self.active {
            output.push(Word {
                text: std::mem::take(&mut self.text),
                glob: std::mem::take(&mut self.glob),
            });
            self.active = false;
        }
    }
}

fn is_name_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_name_char(ch: char) -> bool {
    is_name_start(ch) || ch.is_ascii_digit()
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    for &(key, value) in env {
        if key == name {
            return Some(value);
        }
    }
    None
}

fn find_brace_end(input: &str, mut i: usize) -> Option<usize> {
    while i < input.len() {
        let ch = input[i..].chars().next()?;
        let n = i + ch.len_utf8();
        if ch == '\\' {
            if n < input.len() {
                let escaped = input[n..].chars().next()?;
                i = n + escaped.len_utf8();
            } else {
                i = n;
            }
        } else if ch == '}' {
            return Some(i);
        } else {
            i = n;
        }
    }
    None
}

fn decode(text: &str) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < text.len() {
        let ch = text[i..].chars().next().unwrap();
        let next = i + ch.len_utf8();
        if ch == '\\' && next < text.len() {
            let escaped = text[next..].chars().next().unwrap();
            result.push(escaped);
            i = next + escaped.len_utf8();
        } else {
            result.push(ch);
            i = next;
        }
    }
    result
}

fn braced_value<'a>(
    input: &'a str,
    dollar: usize,
    start: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
    let end = find_brace_end(input, start)
        .ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[start..end];

    let mut name_end = 0;
    for ch in contents.chars() {
        if (name_end == 0 && is_name_start(ch)) || (name_end != 0 && is_name_char(ch)) {
            name_end += ch.len_utf8();
        } else {
            break;
        }
    }
    if name_end == 0 {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let name = &contents[..name_end];
    let trailing = &contents[name_end..];
    if trailing.is_empty() {
        return Ok((lookup(env, name).unwrap_or("").to_owned(), end + 1));
    }

    let default = trailing
        .strip_prefix(":-")
        .ok_or(SplitError::InvalidExpansion { pos: dollar })?;
    if let Some(value) = lookup(env, name).filter(|value| !value.is_empty()) {
        Ok((value.to_owned(), end + 1))
    } else {
        Ok((decode(default), end + 1))
    }
}

fn variable_end(input: &str, name_start: usize) -> usize {
    let mut i = name_start;
    while i < input.len() {
        let ch = input[i..].chars().next().unwrap();
        if !is_name_char(ch) {
            break;
        }
        i += ch.len_utf8();
    }
    i
}

fn append_expansion(word: &mut WordBuilder, value: &str, quoted: bool) {
    word.active = true;
    word.put(value, !quoted);
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|byte| *byte == 0) {
        return Err(SplitError::Nul { pos });
    }

    let mut result = Vec::new();
    let mut word = WordBuilder::new();
    let mut quote = Quote::Bare;
    let mut opening = 0usize;
    let mut cursor = 0usize;

    while cursor < input.len() {
        let ch = input[cursor..].chars().next().unwrap();
        let after = cursor + ch.len_utf8();

        match quote {
            Quote::Single => {
                if ch == '\'' {
                    quote = Quote::Bare;
                } else {
                    word.push(ch, false);
                }
                cursor = after;
            }
            Quote::Double => {
                if ch == '"' {
                    quote = Quote::Bare;
                    cursor = after;
                } else if ch == '\\' {
                    if after == input.len() {
                        return Err(SplitError::TrailingEscape { pos: cursor });
                    }
                    let escaped = input[after..].chars().next().unwrap();
                    let end = after + escaped.len_utf8();
                    match escaped {
                        '"' | '\\' | '$' => {
                            word.push(escaped, false);
                            cursor = end;
                        }
                        '\n' => cursor = end,
                        _ => {
                            word.push('\\', false);
                            cursor = after;
                        }
                    }
                } else if ch == '`' {
                    return Err(SplitError::CommandSubstitution { pos: cursor });
                } else if ch == '$' {
                    if input[after..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: cursor });
                    }
                    if input[after..].starts_with('{') {
                        let (value, end) = braced_value(input, cursor, after + 1, env)?;
                        append_expansion(&mut word, &value, true);
                        cursor = end;
                    } else if let Some(next) = input[after..].chars().next() {
                        if is_name_start(next) {
                            let end = variable_end(input, after);
                            append_expansion(&mut word, lookup(env, &input[after..end]).unwrap_or(""), true);
                            cursor = end;
                        } else {
                            word.active = true;
                            word.push('$', false);
                            cursor = after;
                        }
                    } else {
                        word.active = true;
                        word.push('$', false);
                        cursor = after;
                    }
                } else {
                    word.push(ch, false);
                    cursor = after;
                }
            }
            Quote::Bare => {
                if matches!(ch, ' ' | '\t' | '\r' | '\n') {
                    word.flush(&mut result);
                    cursor = after;
                } else if ch == '\'' {
                    word.active = true;
                    quote = Quote::Single;
                    opening = cursor;
                    cursor = after;
                } else if ch == '"' {
                    word.active = true;
                    quote = Quote::Double;
                    opening = cursor;
                    cursor = after;
                } else if ch == '\\' {
                    if after == input.len() {
                        return Err(SplitError::TrailingEscape { pos: cursor });
                    }
                    let escaped = input[after..].chars().next().unwrap();
                    let end = after + escaped.len_utf8();
                    if escaped == '\n' {
                        cursor = end;
                    } else {
                        word.active = true;
                        word.push(escaped, false);
                        cursor = end;
                    }
                } else if ch == '`' {
                    return Err(SplitError::CommandSubstitution { pos: cursor });
                } else if ch == '$' {
                    if input[after..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: cursor });
                    }
                    if input[after..].starts_with('{') {
                        let (value, end) = braced_value(input, cursor, after + 1, env)?;
                        append_expansion(&mut word, &value, false);
                        cursor = end;
                    } else if let Some(next) = input[after..].chars().next() {
                        if is_name_start(next) {
                            let end = variable_end(input, after);
                            append_expansion(&mut word, lookup(env, &input[after..end]).unwrap_or(""), false);
                            cursor = end;
                        } else {
                            word.active = true;
                            word.push('$', true);
                            cursor = after;
                        }
                    } else {
                        word.active = true;
                        word.push('$', true);
                        cursor = after;
                    }
                } else {
                    word.active = true;
                    word.push(ch, true);
                    cursor = after;
                }
            }
        }
    }

    match quote {
        Quote::Single => return Err(SplitError::UnterminatedSingle { pos: opening }),
        Quote::Double => return Err(SplitError::UnterminatedDouble { pos: opening }),
        Quote::Bare => {}
    }

    word.flush(&mut result);
    Ok(result)
}

fn bare_candidate(word: &str) -> Option<String> {
    if word.is_empty() || word.contains('\n') {
        return None;
    }
    let mut encoded = String::new();
    for ch in word.chars() {
        if ch.is_ascii_alphanumeric() || "_-./:=,+@%".contains(ch) {
            encoded.push(ch);
        } else {
            encoded.push('\\');
            encoded.push(ch);
        }
    }
    Some(encoded)
}

fn single_candidate(word: &str) -> Option<String> {
    if word.contains('\'') {
        None
    } else {
        Some(format!("'{}'", word))
    }
}

fn double_candidate(word: &str) -> Option<String> {
    if word.contains('`') {
        None
    } else {
        let mut encoded = String::from("\"");
        for ch in word.chars() {
            if matches!(ch, '\\' | '"' | '$') {
                encoded.push('\\');
            }
            encoded.push(ch);
        }
        encoded.push('"');
        Some(encoded)
    }
}

fn encode_word(word: &str) -> String {
    let candidates = [
        bare_candidate(word).map(|s| (0u8, s)),
        single_candidate(word).map(|s| (1u8, s)),
        double_candidate(word).map(|s| (2u8, s)),
    ];

    if let Some((_, selected)) = candidates
        .into_iter()
        .flatten()
        .min_by_key(|(priority, candidate)| (candidate.len(), *priority))
    {
        return selected;
    }

    let mut fallback = String::from("'");
    for ch in word.chars() {
        if ch == '\'' {
            fallback.push('\'');
            fallback.push('\\');
            fallback.push('\'');
            fallback.push('\'');
        } else {
            fallback.push(ch);
        }
    }
    fallback.push('\'');
    fallback
}

pub fn join(words: &[String]) -> Result<String, JoinError> {
    for (word_index, word) in words.iter().enumerate() {
        if let Some(pos) = word.as_bytes().iter().position(|byte| *byte == 0) {
            return Err(JoinError::Nul {
                word: word_index,
                pos,
            });
        }
    }

    let mut output = String::new();
    for (index, word) in words.iter().enumerate() {
        if index != 0 {
            output.push(' ');
        }
        output.push_str(&encode_word(word));
    }
    Ok(output)
}