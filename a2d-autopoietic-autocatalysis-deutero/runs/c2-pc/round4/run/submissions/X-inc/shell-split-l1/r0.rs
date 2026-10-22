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

    #[derive(Clone, Copy)]
    enum Quote {
        Single(usize),
        Double(usize),
    }

    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut quote: Option<Quote> = None;
    let mut chars = input.char_indices().peekable();

    while let Some((pos, ch)) = chars.next() {
        match quote {
            Some(Quote::Single(_)) => {
                if ch == '\'' {
                    quote = None;
                } else {
                    word.push(ch);
                }
            }
            Some(Quote::Double(_)) => match ch {
                '"' => quote = None,
                '\\' => {
                    let Some(&(_, next)) = chars.peek() else {
                        return Err(SplitError::TrailingEscape { pos });
                    };
                    if matches!(next, '"' | '\\' | '$' | '\n') {
                        chars.next();
                        if next != '\n' {
                            word.push(next);
                        }
                    } else {
                        word.push('\\');
                    }
                }
                _ => word.push(ch),
            },
            None => match ch {
                '\'' => {
                    active = true;
                    quote = Some(Quote::Single(pos));
                }
                '"' => {
                    active = true;
                    quote = Some(Quote::Double(pos));
                }
                '\\' => {
                    let Some(&(_, next)) = chars.peek() else {
                        return Err(SplitError::TrailingEscape { pos });
                    };
                    chars.next();
                    if next != '\n' {
                        active = true;
                        word.push(next);
                    }
                }
                ' ' | '\t' | '\r' | '\n' => {
                    if active {
                        words.push(std::mem::take(&mut word));
                        active = false;
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
        Some(Quote::Single(pos)) => {
            return Err(SplitError::UnterminatedSingle { pos });
        }
        Some(Quote::Double(pos)) => {
            return Err(SplitError::UnterminatedDouble { pos });
        }
        None => {}
    }

    if active {
        words.push(word);
    }
    Ok(words)
}