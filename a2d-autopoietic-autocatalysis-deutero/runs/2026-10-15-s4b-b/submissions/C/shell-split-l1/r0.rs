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

    #[derive(Clone, Copy)]
    enum State {
        Outside,
        Single { opening: usize },
        Double { opening: usize },
    }

    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut state = State::Outside;
    let mut iter = input.char_indices().peekable();

    while let Some((pos, ch)) = iter.next() {
        match state {
            State::Outside => match ch {
                ' ' | '\t' | '\r' | '\n' => {
                    if active {
                        words.push(std::mem::take(&mut word));
                        active = false;
                    }
                }
                '\'' => {
                    active = true;
                    state = State::Single { opening: pos };
                }
                '"' => {
                    active = true;
                    state = State::Double { opening: pos };
                }
                '\\' => {
                    let Some(&(next_pos, next_ch)) = iter.peek() else {
                        return Err(SplitError::TrailingEscape { pos });
                    };
                    iter.next();
                    if next_ch != '\n' {
                        active = true;
                        word.push(next_ch);
                    }
                    let _ = next_pos;
                }
                _ => {
                    active = true;
                    word.push(ch);
                }
            },
            State::Single { .. } => {
                if ch == '\'' {
                    state = State::Outside;
                } else {
                    word.push(ch);
                }
            }
            State::Double { .. } => match ch {
                '"' => state = State::Outside,
                '\\' => {
                    let Some(&(_, next_ch)) = iter.peek() else {
                        return Err(SplitError::TrailingEscape { pos });
                    };
                    if matches!(next_ch, '"' | '\\' | '$' | '\n') {
                        iter.next();
                        if next_ch != '\n' {
                            word.push(next_ch);
                        }
                    } else {
                        word.push('\\');
                    }
                }
                _ => word.push(ch),
            },
        }
    }

    match state {
        State::Single { opening } => {
            return Err(SplitError::UnterminatedSingle { pos: opening });
        }
        State::Double { opening } => {
            return Err(SplitError::UnterminatedDouble { pos: opening });
        }
        State::Outside => {}
    }

    if active {
        words.push(word);
    }
    Ok(words)
}

fn main() {}