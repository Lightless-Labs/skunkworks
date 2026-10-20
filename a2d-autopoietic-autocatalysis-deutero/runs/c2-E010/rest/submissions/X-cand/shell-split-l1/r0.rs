#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
}

pub fn split(input: &str) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&b| b == 0) {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut current = String::new();
    let mut active = false;
    let mut quote: Option<(char, usize)> = None;
    let mut chars = input.char_indices().peekable();

    while let Some((pos, ch)) = chars.next() {
        if let Some((delimiter, _)) = quote {
            if ch == delimiter {
                quote = None;
                continue;
            }

            if delimiter == '"' && ch == '\\' {
                let Some((_, next)) = chars.peek().copied() else {
                    return Err(SplitError::TrailingEscape { pos });
                };
                if matches!(next, '"' | '\\' | '$') {
                    chars.next();
                    current.push(next);
                } else if next == '\n' {
                    chars.next();
                } else {
                    current.push('\\');
                }
                continue;
            }

            current.push(ch);
            continue;
        }

        match ch {
            '\'' | '"' => {
                active = true;
                quote = Some((ch, pos));
            }
            '\\' => match chars.peek().copied() {
                None => return Err(SplitError::TrailingEscape { pos }),
                Some((_, '\n')) => {
                    chars.next();
                }
                Some((_, escaped)) => {
                    chars.next();
                    active = true;
                    current.push(escaped);
                }
            },
            ' ' | '\t' | '\r' | '\n' => {
                if active {
                    words.push(std::mem::take(&mut current));
                    active = false;
                }
            }
            _ => {
                active = true;
                current.push(ch);
            }
        }
    }

    if let Some((delimiter, pos)) = quote {
        return Err(if delimiter == '\'' {
            SplitError::UnterminatedSingle { pos }
        } else {
            SplitError::UnterminatedDouble { pos }
        });
    }

    if active {
        words.push(current);
    }
    Ok(words)
}
