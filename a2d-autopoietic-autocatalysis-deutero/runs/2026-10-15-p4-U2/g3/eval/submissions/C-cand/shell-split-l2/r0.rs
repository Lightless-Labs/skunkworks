#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
    UnterminatedExpansion { pos: usize },
    InvalidExpansion { pos: usize },
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&b| b == 0) {
        return Err(SplitError::Nul { pos });
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Quote {
        None,
        Single(usize),
        Double(usize),
    }

    fn name_start(b: u8) -> bool {
        b.is_ascii_alphabetic() || b == b'_'
    }

    fn name_continue(b: u8) -> bool {
        name_start(b) || b.is_ascii_digit()
    }

    fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> &'a str {
        env.iter()
            .find_map(|(key, value)| (*key == name).then_some(*value))
            .unwrap_or("")
    }

    fn decode_default(text: &str) -> String {
        let mut out = String::new();
        let mut i = 0;
        while i < text.len() {
            let ch = text[i..].chars().next().unwrap();
            if ch == '\\' {
                i += 1;
                if i < text.len() {
                    let next = text[i..].chars().next().unwrap();
                    out.push(next);
                    i += next.len_utf8();
                }
            } else {
                out.push(ch);
                i += ch.len_utf8();
            }
        }
        out
    }

    let bytes = input.as_bytes();
    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut quote = Quote::None;
    let mut i = 0;

    while i < bytes.len() {
        match quote {
            Quote::Single(open) => {
                if bytes[i] == b'\'' {
                    quote = Quote::None;
                    i += 1;
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    word.push(ch);
                    i += ch.len_utf8();
                }
                let _ = open;
            }
            Quote::Double(open) => {
                match bytes[i] {
                    b'"' => {
                        quote = Quote::None;
                        i += 1;
                    }
                    b'\\' => {
                        let pos = i;
                        i += 1;
                        if i == bytes.len() {
                            return Err(SplitError::TrailingEscape { pos });
                        }
                        match bytes[i] {
                            b'"' | b'\\' | b'$' => {
                                let ch = input[i..].chars().next().unwrap();
                                word.push(ch);
                                i += ch.len_utf8();
                            }
                            b'\n' => i += 1,
                            _ => word.push('\\'),
                        }
                    }
                    b'$' => {
                        let dollar = i;
                        if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                            let mut j = i + 2;
                            let close = loop {
                                if j >= bytes.len() {
                                    return Err(SplitError::UnterminatedExpansion { pos: dollar });
                                }
                                if bytes[j] == b'\\' {
                                    j += 1;
                                    if j < bytes.len() {
                                        let ch = input[j..].chars().next().unwrap();
                                        j += ch.len_utf8();
                                    }
                                } else if bytes[j] == b'}' {
                                    break j;
                                } else {
                                    let ch = input[j..].chars().next().unwrap();
                                    j += ch.len_utf8();
                                }
                            };
                            let content = &input[i + 2..close];
                            let cb = content.as_bytes();
                            let mut k = 0;
                            if k >= cb.len() || !name_start(cb[k]) {
                                return Err(SplitError::InvalidExpansion { pos: dollar });
                            }
                            k += 1;
                            while k < cb.len() && name_continue(cb[k]) {
                                k += 1;
                            }
                            let name = &content[..k];
                            let selected = if k == cb.len() {
                                lookup(name, env).to_string()
                            } else if k + 2 <= cb.len()
                                && &content[k..k + 2] == ":-"
                            {
                                let value = lookup(name, env);
                                if !value.is_empty() {
                                    value.to_string()
                                } else {
                                    decode_default(&content[k + 2..])
                                }
                            } else {
                                return Err(SplitError::InvalidExpansion { pos: dollar });
                            };
                            word.push_str(&selected);
                            active = true;
                            i = close + 1;
                        } else if i + 1 < bytes.len() && name_start(bytes[i + 1]) {
                            let mut j = i + 2;
                            while j < bytes.len() && name_continue(bytes[j]) {
                                j += 1;
                            }
                            word.push_str(lookup(&input[i + 1..j], env));
                            active = true;
                            i = j;
                        } else {
                            word.push('$');
                            active = true;
                            i += 1;
                        }
                    }
                    _ => {
                        let ch = input[i..].chars().next().unwrap();
                        word.push(ch);
                        i += ch.len_utf8();
                    }
                }
                let _ = open;
            }
            Quote::None => match bytes[i] {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    if active {
                        words.push(std::mem::take(&mut word));
                        active = false;
                    }
                    i += 1;
                }
                b'\'' => {
                    active = true;
                    quote = Quote::Single(i);
                    i += 1;
                }
                b'"' => {
                    active = true;
                    quote = Quote::Double(i);
                    i += 1;
                }
                b'\\' => {
                    let pos = i;
                    i += 1;
                    if i == bytes.len() {
                        return Err(SplitError::TrailingEscape { pos });
                    }
                    if bytes[i] == b'\n' {
                        i += 1;
                    } else {
                        let ch = input[i..].chars().next().unwrap();
                        word.push(ch);
                        active = true;
                        i += ch.len_utf8();
                    }
                }
                b'$' => {
                    let dollar = i;
                    if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                        let mut j = i + 2;
                        let close = loop {
                            if j >= bytes.len() {
                                return Err(SplitError::UnterminatedExpansion { pos: dollar });
                            }
                            if bytes[j] == b'\\' {
                                j += 1;
                                if j < bytes.len() {
                                    let ch = input[j..].chars().next().unwrap();
                                    j += ch.len_utf8();
                                }
                            } else if bytes[j] == b'}' {
                                break j;
                            } else {
                                let ch = input[j..].chars().next().unwrap();
                                j += ch.len_utf8();
                            }
                        };
                        let content = &input[i + 2..close];
                        let cb = content.as_bytes();
                        let mut k = 0;
                        if k >= cb.len() || !name_start(cb[k]) {
                            return Err(SplitError::InvalidExpansion { pos: dollar });
                        }
                        k += 1;
                        while k < cb.len() && name_continue(cb[k]) {
                            k += 1;
                        }
                        let name = &content[..k];
                        let selected = if k == cb.len() {
                            lookup(name, env).to_string()
                        } else if k + 2 <= cb.len() && &content[k..k + 2] == ":-" {
                            let value = lookup(name, env);
                            if !value.is_empty() {
                                value.to_string()
                            } else {
                                decode_default(&content[k + 2..])
                            }
                        } else {
                            return Err(SplitError::InvalidExpansion { pos: dollar });
                        };
                        word.push_str(&selected);
                        active = true;
                        i = close + 1;
                    } else if i + 1 < bytes.len() && name_start(bytes[i + 1]) {
                        let mut j = i + 2;
                        while j < bytes.len() && name_continue(bytes[j]) {
                            j += 1;
                        }
                        word.push_str(lookup(&input[i + 1..j], env));
                        active = true;
                        i = j;
                    } else {
                        word.push('$');
                        active = true;
                        i += 1;
                    }
                }
                _ => {
                    let ch = input[i..].chars().next().unwrap();
                    word.push(ch);
                    active = true;
                    i += ch.len_utf8();
                }
            },
        }
    }

    match quote {
        Quote::Single(pos) => return Err(SplitError::UnterminatedSingle { pos }),
        Quote::Double(pos) => return Err(SplitError::UnterminatedDouble { pos }),
        Quote::None => {}
    }

    if active {
        words.push(word);
    }
    Ok(words)
}

fn main() {}