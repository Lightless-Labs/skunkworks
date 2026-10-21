#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
}

pub fn split(input: &str) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut quote: Option<(char, usize)> = None;
    let mut chars = input.char_indices().peekable();

    while let Some((pos, ch)) = chars.next() {
        match quote {
            Some(('\'', _)) => {
                if ch == '\'' {
                    quote = None;
                } else {
                    word.push(ch);
                }
            }
            Some(('"', _)) => {
                if ch == '"' {
                    quote = None;
                } else if ch == '\\' {
                    match chars.peek().copied() {
                        None => return Err(SplitError::TrailingEscape { pos }),
                        Some((_, next)) if next == '"' || next == '\\' || next == '$' => {
                            chars.next();
                            word.push(next);
                        }
                        Some((_, '\n')) => {
                            chars.next();
                        }
                        Some(_) => word.push('\\'),
                    }
                } else {
                    word.push(ch);
                }
            }
            None => match ch {
                ' ' | '\t' | '\r' | '\n' => {
                    if active {
                        words.push(std::mem::take(&mut word));
                        active = false;
                    }
                }
                '\'' | '"' => {
                    active = true;
                    quote = Some((ch, pos));
                }
                '\\' => match chars.peek().copied() {
                    None => return Err(SplitError::TrailingEscape { pos }),
                    Some((_, '\n')) => {
                        chars.next();
                    }
                    Some((_, next)) => {
                        chars.next();
                        word.push(next);
                        active = true;
                    }
                },
                _ => {
                    word.push(ch);
                    active = true;
                }
            },
            _ => unreachable!(),
        }
    }

    if let Some((q, pos)) = quote {
        return Err(if q == '\'' {
            SplitError::UnterminatedSingle { pos }
        } else {
            SplitError::UnterminatedDouble { pos }
        });
    }

    if active {
        words.push(word);
    }

    Ok(words)
}