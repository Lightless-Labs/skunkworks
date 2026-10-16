#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsvError {
    UnexpectedQuote { record: usize, byte: usize },
    TrailingByte { record: usize, byte: usize },
    BareCr { record: usize, byte: usize },
    UnclosedQuote { record: usize, byte: usize },
    Nul { record: usize, byte: usize },
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut i = if input.starts_with('\u{FEFF}') { 3 } else { 0 };

    if i == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();

    #[derive(Clone, Copy)]
    enum State {
        Start,
        Unquoted,
        Quoted { opening: usize },
        AfterQuote,
    }

    let mut state = State::Start;
    let mut record_has_content = false;
    let mut record_index = 0;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if !matches!(state, State::Quoted { .. }) && byte == b'\r' {
            if bytes.get(i + 1) != Some(&b'\n') {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: i,
                });
            }
        }

        match state {
            State::Quoted { opening } => {
                if byte == b'"' {
                    if bytes.get(i + 1) == Some(&b'"') {
                        field.push('"');
                        i += 2;
                    } else {
                        state = State::AfterQuote;
                        i += 1;
                    }
                } else {
                    let ch = input[i..].chars().next().expect("valid UTF-8 input");
                    field.push(ch);
                    i += ch.len_utf8();
                }
                let _ = opening;
            }
            State::AfterQuote => match byte {
                b',' => {
                    record.push(std::mem::take(&mut field));
                    state = State::Start;
                    record_has_content = true;
                    i += 1;
                }
                b'\n' | b'\r' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    record_has_content = false;
                    state = State::Start;
                    i += if byte == b'\r' { 2 } else { 1 };
                }
                _ => {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: i,
                    });
                }
            },
            State::Start => match byte {
                b'"' => {
                    state = State::Quoted { opening: i };
                    record_has_content = true;
                    i += 1;
                }
                b',' => {
                    record.push(String::new());
                    record_has_content = true;
                    i += 1;
                }
                b'\n' | b'\r' => {
                    record.push(String::new());
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    record_has_content = false;
                    i += if byte == b'\r' { 2 } else { 1 };
                }
                _ => {
                    let ch = input[i..].chars().next().expect("valid UTF-8 input");
                    field.push(ch);
                    state = State::Unquoted;
                    record_has_content = true;
                    i += ch.len_utf8();
                }
            },
            State::Unquoted => match byte {
                b'"' => {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: i,
                    });
                }
                b',' => {
                    record.push(std::mem::take(&mut field));
                    state = State::Start;
                    record_has_content = true;
                    i += 1;
                }
                b'\n' | b'\r' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    record_has_content = false;
                    state = State::Start;
                    i += if byte == b'\r' { 2 } else { 1 };
                }
                _ => {
                    let ch = input[i..].chars().next().expect("valid UTF-8 input");
                    field.push(ch);
                    i += ch.len_utf8();
                }
            },
        }
    }

    if let State::Quoted { opening } = state {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: opening,
        });
    }

    if record_has_content || !record.is_empty() || !field.is_empty() {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}