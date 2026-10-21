#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
}

pub fn split(input: &str) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&byte| byte == 0) {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut current = String::new();
    let mut word_is_active = false;
    let mut quote: Option<(char, usize)> = None;
    let mut i = 0;

    while i < input.len() {
        let ch = input[i..].chars().next().unwrap();
        let next_i = i + ch.len_utf8();

        if let Some((delimiter, _)) = quote {
            if delimiter == '\'' {
                if ch == '\'' {
                    quote = None;
                } else {
                    current.push(ch);
                }
                i = next_i;
                continue;
            }

            if ch == '"' {
                quote = None;
                i = next_i;
                continue;
            }
            if ch == '\\' {
                if next_i == input.len() {
                    return Err(SplitError::TrailingEscape { pos: i });
                }
                let next = input[next_i..].chars().next().unwrap();
                if matches!(next, '"' | '\\' | '$' | '\n') {
                    if next != '\n' {
                        current.push(next);
                    }
                    i = next_i + next.len_utf8();
                } else {
                    current.push('\\');
                    i = next_i;
                }
                continue;
            }
            current.push(ch);
            i = next_i;
            continue;
        }

        match ch {
            ' ' | '\t' | '\r' | '\n' => {
                if word_is_active {
                    words.push(std::mem::take(&mut current));
                    word_is_active = false;
                }
            }
            '\'' | '"' => {
                quote = Some((ch, i));
                word_is_active = true;
            }
            '\\' => {
                if next_i == input.len() {
                    return Err(SplitError::TrailingEscape { pos: i });
                }
                let next = input[next_i..].chars().next().unwrap();
                if next != '\n' {
                    current.push(next);
                    word_is_active = true;
                }
                i = next_i + next.len_utf8();
                continue;
            }
            _ => {
                current.push(ch);
                word_is_active = true;
            }
        }
        i = next_i;
    }

    if let Some((delimiter, pos)) = quote {
        return Err(if delimiter == '\'' {
            SplitError::UnterminatedSingle { pos }
        } else {
            SplitError::UnterminatedDouble { pos }
        });
    }

    if word_is_active {
        words.push(current);
    }
    Ok(words)
}