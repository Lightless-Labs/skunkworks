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
                    }
                    let _ = next_pos;
                } else {
                    word.push(ch);
                }
            }
            None => match ch {
                '\'' | '"' => {
                    active = true;
                    quote = Some((ch, pos));
                }
                '\\' => {
                    let Some((_, next)) = chars.next() else {
                        return Err(SplitError::TrailingEscape { pos });
                    };
                    if next == '\n' {
                        continue;
                    }
                    active = true;
                    word.push(next);
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
            _ => unreachable!(),
        }
    }

    if let Some((quote_char, pos)) = quote {
        return Err(if quote_char == '\'' {
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

fn main() {}