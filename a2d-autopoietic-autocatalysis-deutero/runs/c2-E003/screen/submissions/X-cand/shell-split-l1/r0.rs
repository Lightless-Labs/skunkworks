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
    let mut chars = input.char_indices().peekable();

    while let Some((pos, ch)) = chars.next() {
        match ch {
            ' ' | '\t' | '\r' | '\n' => {
                if active {
                    words.push(std::mem::take(&mut word));
                    active = false;
                }
            }
            '\'' => {
                active = true;
                loop {
                    match chars.next() {
                        Some((_, '\'')) => break,
                        Some((_, c)) => word.push(c),
                        None => return Err(SplitError::UnterminatedSingle { pos }),
                    }
                }
            }
            '"' => {
                active = true;
                loop {
                    match chars.next() {
                        Some((_, '"')) => break,
                        Some((escape_pos, '\\')) => match chars.next() {
                            None => return Err(SplitError::TrailingEscape { pos: escape_pos }),
                            Some((_, '"')) => word.push('"'),
                            Some((_, '\\')) => word.push('\\'),
                            Some((_, '$')) => word.push('$'),
                            Some((_, '\n')) => {}
                            Some((_, c)) => {
                                word.push('\\');
                                word.push(c);
                            }
                        },
                        Some((_, c)) => word.push(c),
                        None => return Err(SplitError::UnterminatedDouble { pos }),
                    }
                }
            }
            '\\' => match chars.next() {
                None => return Err(SplitError::TrailingEscape { pos }),
                Some((_, '\n')) => {}
                Some((_, c)) => {
                    active = true;
                    word.push(c);
                }
            },
            c => {
                active = true;
                word.push(c);
            }
        }
    }

    if active {
        words.push(word);
    }

    Ok(words)
}