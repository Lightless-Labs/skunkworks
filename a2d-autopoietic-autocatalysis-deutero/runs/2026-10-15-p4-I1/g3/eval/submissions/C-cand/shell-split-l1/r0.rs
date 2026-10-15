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
        None,
        Single(usize),
        Double(usize),
    }

    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut quote = Quote::None;
    let mut chars = input.char_indices().peekable();

    while let Some((pos, ch)) = chars.next() {
        match quote {
            Quote::Single(_) => {
                if ch == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(ch);
                }
            }
            Quote::Double(_) => match ch {
                '"' => quote = Quote::None,
                '\\' => {
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
                }
                _ => word.push(ch),
            },
            Quote::None => match ch {
                '\'' => {
                    active = true;
                    quote = Quote::Single(pos);
                }
                '"' => {
                    active = true;
                    quote = Quote::Double(pos);
                }
                '\\' => {
                    let Some((_, next)) = chars.next() else {
                        return Err(SplitError::TrailingEscape { pos });
                    };
                    if next == '\n' {
                        // A line continuation contributes nothing and activates no word.
                    } else {
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