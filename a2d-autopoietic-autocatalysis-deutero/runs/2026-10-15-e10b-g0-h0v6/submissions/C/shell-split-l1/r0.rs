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
    let mut current = String::new();
    let mut active = false;
    let mut quote = Quote::None;
    let mut chars = input.char_indices().peekable();

    while let Some((pos, ch)) = chars.next() {
        match quote {
            Quote::Single(_) => {
                if ch == '\'' {
                    quote = Quote::None;
                } else {
                    current.push(ch);
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
                            current.push(next);
                        }
                    } else {
                        current.push('\\');
                        let _ = next_pos;
                    }
                }
                _ => current.push(ch),
            },
            Quote::None => match ch {
                ' ' | '\t' | '\r' | '\n' => {
                    if active {
                        words.push(std::mem::take(&mut current));
                        active = false;
                    }
                }
                '\'' => {
                    active = true;
                    quote = Quote::Single(pos);
                }
                '"' => {
                    active = true;
                    quote = Quote::Double(pos);
                }
                '\\' => {
                    let Some(&(_, next)) = chars.peek() else {
                        return Err(SplitError::TrailingEscape { pos });
                    };
                    chars.next();
                    if next != '\n' {
                        current.push(next);
                        active = true;
                    }
                }
                _ => {
                    current.push(ch);
                    active = true;
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
        words.push(current);
    }

    Ok(words)
}

fn main() {}