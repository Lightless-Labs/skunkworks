#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Quote {
    None,
    Single { opening: usize },
    Double { opening: usize },
}

pub fn split(input: &str) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut quote = Quote::None;
    let mut chars = input.char_indices().peekable();

    while let Some((pos, ch)) = chars.next() {
        match quote {
            Quote::Single { .. } => {
                if ch == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(ch);
                }
            }
            Quote::Double { opening } => {
                if ch == '"' {
                    quote = Quote::None;
                } else if ch == '\\' {
                    let Some(&(next_pos, next)) = chars.peek() else {
                        return Err(SplitError::TrailingEscape { pos });
                    };
                    if matches!(next, '"' | '\\' | '$' | '\n') {
                        chars.next();
                        if next != '\n' {
                            word.push(next);
                        }
                    } else {
                        word.push('\\');
                        let _ = next_pos;
                    }
                } else {
                    word.push(ch);
                }
            }
            Quote::None => match ch {
                ' ' | '\t' | '\r' | '\n' => {
                    if active {
                        words.push(std::mem::take(&mut word));
                        active = false;
                    }
                }
                '\'' => {
                    active = true;
                    quote = Quote::Single { opening: pos };
                }
                '"' => {
                    active = true;
                    quote = Quote::Double { opening: pos };
                }
                '\\' => {
                    let Some((_, next)) = chars.next() else {
                        return Err(SplitError::TrailingEscape { pos });
                    };
                    if next != '\n' {
                        active = true;
                        word.push(next);
                    }
                }
                _ => {
                    active = true;
                    word.push(ch);
                }
            },
        }
    }

    match quote {
        Quote::Single { opening } => {
            return Err(SplitError::UnterminatedSingle { pos: opening });
        }
        Quote::Double { opening } => {
            return Err(SplitError::UnterminatedDouble { pos: opening });
        }
        Quote::None => {}
    }

    if active {
        words.push(word);
    }
    Ok(words)
}