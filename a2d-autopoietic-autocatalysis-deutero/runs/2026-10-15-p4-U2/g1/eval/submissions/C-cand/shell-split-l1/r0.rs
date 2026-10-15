#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
}

pub fn split(input: &str) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.bytes().position(|byte| byte == 0) {
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
            Quote::None => match ch {
                ' ' | '\t' | '\r' | '\n' => {
                    if active {
                        words.push(std::mem::take(&mut word));
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
                    let Some((_, next)) = chars.next() else {
                        return Err(SplitError::TrailingEscape { pos });
                    };
                    match next {
                        '"' | '\\' | '$' => word.push(next),
                        '\n' => {}
                        _ => {
                            word.push('\\');
                            word.push(next);
                        }
                    }
                }
                _ => word.push(ch),
            },
        }
    }

    match quote {
        Quote::None => {
            if active {
                words.push(word);
            }
            Ok(words)
        }
        Quote::Single(pos) => Err(SplitError::UnterminatedSingle { pos }),
        Quote::Double(pos) => Err(SplitError::UnterminatedDouble { pos }),
    }
}

fn main() {}