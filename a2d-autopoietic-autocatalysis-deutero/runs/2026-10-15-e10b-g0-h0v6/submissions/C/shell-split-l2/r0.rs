#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
    UnterminatedExpansion { pos: usize },
    InvalidExpansion { pos: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Quote {
    None,
    Single { start: usize },
    Double { start: usize },
}

struct Parser<'a> {
    input: &'a str,
    env: &'a [(&'a str, &'a str)],
    pos: usize,
    words: Vec<String>,
    word: String,
    active: bool,
    quote: Quote,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str, env: &'a [(&'a str, &'a str)]) -> Self {
        Self {
            input,
            env,
            pos: 0,
            words: Vec::new(),
            word: String::new(),
            active: false,
            quote: Quote::None,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn next_char(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn push_word(&mut self) {
        if self.active {
            self.words.push(std::mem::take(&mut self.word));
            self.active = false;
        }
    }

    fn lookup(&self, name: &str) -> &str {
        self.env
            .iter()
            .find_map(|(key, value)| (*key == name).then_some(*value))
            .unwrap_or("")
    }

    fn decode_default(text: &str) -> String {
        let mut result = String::new();
        let mut chars = text.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    result.push(next);
                } else {
                    result.push('\\');
                }
            } else {
                result.push(ch);
            }
        }
        result
    }

    fn append_expansion(&mut self, dollar: usize) -> Result<(), SplitError> {
        self.active = true;
        self.pos += 1; // $
        match self.peek() {
            Some('{') => {
                self.pos += 1;
                let content_start = self.pos;
                let mut cursor = self.pos;
                let mut close = None;

                while cursor < self.input.len() {
                    let ch = self.input[cursor..].chars().next().unwrap();
                    if ch == '\\' {
                        cursor += ch.len_utf8();
                        if cursor < self.input.len() {
                            let next = self.input[cursor..].chars().next().unwrap();
                            cursor += next.len_utf8();
                        }
                    } else if ch == '}' {
                        close = Some(cursor);
                        break;
                    } else {
                        cursor += ch.len_utf8();
                    }
                }

                let close_pos =
                    close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
                let contents = &self.input[content_start..close_pos];
                let bytes = contents.as_bytes();

                let mut name_end = 0;
                if bytes
                    .first()
                    .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
                {
                    name_end = 1;
                    while name_end < bytes.len()
                        && (bytes[name_end].is_ascii_alphanumeric() || bytes[name_end] == b'_')
                    {
                        name_end += 1;
                    }
                }

                if name_end == 0 {
                    return Err(SplitError::InvalidExpansion { pos: dollar });
                }

                let name = &contents[..name_end];
                let default = if name_end == bytes.len() {
                    None
                } else if contents[name_end..].starts_with(":-") {
                    Some(&contents[name_end + 2..])
                } else {
                    return Err(SplitError::InvalidExpansion { pos: dollar });
                };

                let value = self.lookup(name).to_string();
                let addition = match default {
                    Some(default_text) if value.is_empty() => {
                        Self::decode_default(default_text)
                    }
                    _ => value,
                };
                self.word.push_str(&addition);
                self.pos = close_pos + 1;
            }
            Some(ch) if ch.is_ascii_alphabetic() || ch == '_' => {
                let start = self.pos;
                self.pos += ch.len_utf8();
                while let Some(next) = self.peek() {
                    if next.is_ascii_alphanumeric() || next == '_' {
                        self.pos += next.len_utf8();
                    } else {
                        break;
                    }
                }
                let name = self.input[start..self.pos].to_string();
                let value = self.lookup(&name).to_string();
                self.word.push_str(&value);
            }
            _ => self.word.push('$'),
        }
        Ok(())
    }

    fn parse(mut self) -> Result<Vec<String>, SplitError> {
        while let Some(ch) = self.peek() {
            match self.quote.clone() {
                Quote::Single { .. } => {
                    self.next_char();
                    if ch == '\'' {
                        self.quote = Quote::None;
                    } else {
                        self.word.push(ch);
                    }
                }
                Quote::Double { .. } => {
                    if ch == '"' {
                        self.next_char();
                        self.quote = Quote::None;
                    } else if ch == '\\' {
                        let slash = self.pos;
                        self.next_char();
                        match self.peek() {
                            None => return Err(SplitError::TrailingEscape { pos: slash }),
                            Some('"') | Some('\\') | Some('$') => {
                                let escaped = self.next_char().unwrap();
                                self.word.push(escaped);
                            }
                            Some('\n') => {
                                self.next_char();
                            }
                            Some(_) => self.word.push('\\'),
                        }
                    } else if ch == '$' {
                        let dollar = self.pos;
                        self.append_expansion(dollar)?;
                    } else {
                        self.next_char();
                        self.word.push(ch);
                    }
                }
                Quote::None => {
                    if matches!(ch, ' ' | '\t' | '\r' | '\n') {
                        self.next_char();
                        self.push_word();
                    } else if ch == '\'' {
                        let start = self.pos;
                        self.next_char();
                        self.active = true;
                        self.quote = Quote::Single { start };
                    } else if ch == '"' {
                        let start = self.pos;
                        self.next_char();
                        self.active = true;
                        self.quote = Quote::Double { start };
                    } else if ch == '\\' {
                        let slash = self.pos;
                        self.next_char();
                        match self.peek() {
                            None => return Err(SplitError::TrailingEscape { pos: slash }),
                            Some('\n') => {
                                self.next_char();
                            }
                            Some(next) => {
                                self.next_char();
                                self.active = true;
                                self.word.push(next);
                            }
                        }
                    } else if ch == '$' {
                        let dollar = self.pos;
                        self.append_expansion(dollar)?;
                    } else {
                        self.next_char();
                        self.active = true;
                        self.word.push(ch);
                    }
                }
            }
        }

        match self.quote {
            Quote::Single { start } => {
                return Err(SplitError::UnterminatedSingle { pos: start });
            }
            Quote::Double { start } => {
                return Err(SplitError::UnterminatedDouble { pos: start });
            }
            Quote::None => {}
        }

        self.push_word();
        Ok(self.words)
    }
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&byte| byte == 0) {
        return Err(SplitError::Nul { pos });
    }

    Parser::new(input, env).parse()
}

fn main() {}