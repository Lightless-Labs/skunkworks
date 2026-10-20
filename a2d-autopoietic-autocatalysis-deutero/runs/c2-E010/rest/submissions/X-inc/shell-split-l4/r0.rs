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
    None,
    Single,
    Double,
}

struct Builder {
    text: String,
    glob: Vec<usize>,
    active: bool,
}

impl Builder {
    fn new() -> Self {
        Self {
            text: String::new(),
            glob: Vec::new(),
            active: false,
        }
    }

    fn push(&mut self, ch: char, wildcard_active: bool) {
        if wildcard_active && matches!(ch, '*' | '?' | '[') {
            self.glob.push(self.text.len());
        }
        self.text.push(ch);
        self.active = true;
    }

    fn push_str(&mut self, value: &str, wildcard_active: bool) {
        for ch in value.chars() {
            self.push(ch, wildcard_active);
        }
        self.active = true;
    }

    fn finish(&mut self, words: &mut Vec<Word>) {
        if self.active {
            words.push(Word {
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

fn is_name_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn decode_default(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.char_indices().peekable();
    while let Some((_, ch)) = chars.next() {
        if ch == '\\' {
            if let Some((_, next)) = chars.next() {
                out.push(next);
            } else {
                out.push('\\');
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn parse_expansion(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String), SplitError> {
    let bytes = input.as_bytes();

    if bytes.get(dollar + 1) != Some(&b'{') {
        let mut end = dollar + 1;
        let first = input[end..].chars().next().unwrap();
        if !is_name_start(first) {
            return Ok((dollar + 1, "$".to_string()));
        }
        end += first.len_utf8();
        while end < input.len() {
            let ch = input[end..].chars().next().unwrap();
            if !is_name_continue(ch) {
                break;
            }
            end += ch.len_utf8();
        }
        let name = &input[dollar + 1..end];
        return Ok((end, lookup(name, env).unwrap_or("").to_string()));
    }

    let content_start = dollar + 2;
    let mut cursor = content_start;
    let mut close = None;
    while cursor < input.len() {
        let ch = input[cursor..].chars().next().unwrap();
        if ch == '\\' {
            cursor += ch.len_utf8();
            if cursor < input.len() {
                cursor += input[cursor..].chars().next().unwrap().len_utf8();
            }
        } else if ch == '}' {
            close = Some(cursor);
            break;
        } else {
            cursor += ch.len_utf8();
        }
    }

    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[content_start..close];

    let name_end = contents
        .char_indices()
        .find_map(|(i, ch)| (!is_name_continue(ch)).then_some(i))
        .unwrap_or(contents.len());

    let name = &contents[..name_end];
    let mut valid_name = !name.is_empty();
    if valid_name {
        let mut chars = name.chars();
        valid_name = chars.next().map(is_name_start).unwrap_or(false)
            && chars.all(is_name_continue);
    }

    let result = if name_end == contents.len() {
        if !valid_name {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }
        lookup(name, env).unwrap_or("").to_string()
    } else if contents[name_end..].starts_with(":-") {
        if !valid_name {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }
        let default_text = &contents[name_end + 2..];
        match lookup(name, env) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => decode_default(default_text),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((close + 1, result))
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut word = Builder::new();
    let mut quote = Quote::None;
    let mut quote_start = 0usize;
    let mut i = 0usize;

    while i < input.len() {
        let ch = input[i..].chars().next().unwrap();
        let next_i = i + ch.len_utf8();

        match quote {
            Quote::Single => {
                if ch == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(ch, false);
                }
                i = next_i;
            }
            Quote::Double => {
                if ch == '"' {
                    quote = Quote::None;
                    i = next_i;
                } else if ch == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let next = input[next_i..].chars().next().unwrap();
                    if matches!(next, '"' | '\\' | '$' | '\n') {
                        if next != '\n' {
                            word.push(next, false);
                        }
                        i = next_i + next.len_utf8();
                    } else {
                        word.push('\\', false);
                        i = next_i;
                    }
                } else if ch == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if ch == '$' {
                    if input.as_bytes().get(next_i) == Some(&b'(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if next_i < input.len()
                        && (input.as_bytes()[next_i] == b'{' ||
                            is_name_start(input[next_i..].chars().next().unwrap()))
                    {
                        let (end, value) = parse_expansion(input, i, env)?;
                        word.push_str(&value, false);
                        i = end;
                    } else {
                        word.push('$', false);
                        i = next_i;
                    }
                } else {
                    word.push(ch, false);
                    i = next_i;
                }
            }
            Quote::None => {
                if matches!(ch, ' ' | '\t' | '\r' | '\n') {
                    word.finish(&mut words);
                    i = next_i;
                } else if ch == '\'' {
                    word.active = true;
                    quote = Quote::Single;
                    quote_start = i;
                    i = next_i;
                } else if ch == '"' {
                    word.active = true;
                    quote = Quote::Double;
                    quote_start = i;
                    i = next_i;
                } else if ch == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let next = input[next_i..].chars().next().unwrap();
                    if next == '\n' {
                        i = next_i + next.len_utf8();
                    } else {
                        word.push(next, false);
                        i = next_i + next.len_utf8();
                    }
                } else if ch == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if ch == '$' {
                    if input.as_bytes().get(next_i) == Some(&b'(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if next_i < input.len()
                        && (input.as_bytes()[next_i] == b'{' ||
                            is_name_start(input[next_i..].chars().next().unwrap()))
                    {
                        let (end, value) = parse_expansion(input, i, env)?;
                        word.push_str(&value, true);
                        i = end;
                    } else {
                        word.push('$', true);
                        i = next_i;
                    }
                } else {
                    word.push(ch, true);
                    i = next_i;
                }
            }
        }
    }

    match quote {
        Quote::Single => return Err(SplitError::UnterminatedSingle { pos: quote_start }),
        Quote::Double => return Err(SplitError::UnterminatedDouble { pos: quote_start }),
        Quote::None => {}
    }

    word.finish(&mut words);
    Ok(words)
}

fn bare_candidate(word: &str) -> Option<String> {
    if word.is_empty() || word.contains('\n') {
        return None;
    }
    let mut out = String::new();
    for ch in word.chars() {
        if ch.is_ascii_alphanumeric() || "_-./:=,+@%".contains(ch) {
            out.push(ch);
        } else {
            out.push('\\');
            out.push(ch);
        }
    }
    Some(out)
}

fn single_candidate(word: &str) -> Option<String> {
    if word.contains('\'') {
        None
    } else {
        Some(format!("'{word}'"))
    }
}

fn double_candidate(word: &str) -> Option<String> {
    if word.contains('`') {
        return None;
    }
    let mut out = String::from("\"");
    for ch in word.chars() {
        if matches!(ch, '\\' | '"' | '$') {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('"');
    Some(out)
}

fn fallback_candidate(word: &str) -> String {
    let mut out = String::from("'");
    for ch in word.chars() {
        if ch == '\'' {
            out.push('\'');
            out.push('\\');
            out.push('\'');
            out.push('\'');
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

pub fn join(words: &[String]) -> Result<String, JoinError> {
    for (word_index, word) in words.iter().enumerate() {
        if let Some(pos) = word.find('\0') {
            return Err(JoinError::Nul {
                word: word_index,
                pos,
            });
        }
    }

    let mut encoded = Vec::with_capacity(words.len());
    for word in words {
        let candidates = [
            bare_candidate(word),
            single_candidate(word),
            double_candidate(word),
        ];
        let mut best: Option<(usize, usize, String)> = None;
        for (priority, candidate) in candidates.into_iter().enumerate() {
            if let Some(candidate) = candidate {
                let length = candidate.len();
                if best
                    .as_ref()
                    .map(|(best_len, best_priority, _)| {
                        length < *best_len || (length == *best_len && priority < *best_priority)
                    })
                    .unwrap_or(true)
                {
                    best = Some((length, priority, candidate));
                }
            }
        }

        let result = best
            .map(|(_, _, candidate)| candidate)
            .unwrap_or_else(|| fallback_candidate(word));
        encoded.push(result);
    }

    Ok(encoded.join(" "))
}