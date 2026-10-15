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
    let mut pos = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    };

    if pos == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut record = 0usize;

    #[derive(Clone, Copy)]
    enum State {
        FieldStart,
        Unquoted,
        Quoted { opening: usize },
        AfterQuote,
    }

    let mut state = State::FieldStart;

    while pos < bytes.len() {
        let byte = bytes[pos];

        if byte == 0 {
            return Err(CsvError::Nul { record, byte: pos });
        }

        if !matches!(state, State::Quoted { .. }) && byte == b'\r' {
            if bytes.get(pos + 1) != Some(&b'\n') {
                return Err(CsvError::BareCr { record, byte: pos });
            }
        }

        match state {
            State::FieldStart => match byte {
                b',' => {
                    fields.push(std::mem::take(&mut field));
                    pos += 1;
                }
                b'\n' => {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    pos += 1;
                }
                b'\r' => {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    pos += 2;
                }
                b'"' => {
                    state = State::Quoted { opening: pos };
                    pos += 1;
                }
                _ => {
                    let ch = input[pos..].chars().next().unwrap();
                    field.push(ch);
                    state = State::Unquoted;
                    pos += ch.len_utf8();
                }
            },
            State::Unquoted => match byte {
                b',' => {
                    fields.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    pos += 1;
                }
                b'\n' => {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    state = State::FieldStart;
                    pos += 1;
                }
                b'\r' => {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    state = State::FieldStart;
                    pos += 2;
                }
                b'"' => {
                    return Err(CsvError::UnexpectedQuote { record, byte: pos });
                }
                _ => {
                    let ch = input[pos..].chars().next().unwrap();
                    field.push(ch);
                    pos += ch.len_utf8();
                }
            },
            State::Quoted { opening } => match byte {
                b'"' if bytes.get(pos + 1) == Some(&b'"') => {
                    field.push('"');
                    pos += 2;
                }
                b'"' => {
                    state = State::AfterQuote;
                    pos += 1;
                }
                _ => {
                    let ch = input[pos..].chars().next().unwrap();
                    field.push(ch);
                    pos += ch.len_utf8();
                    let _ = opening;
                }
            },
            State::AfterQuote => match byte {
                b',' => {
                    fields.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    pos += 1;
                }
                b'\n' => {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    state = State::FieldStart;
                    pos += 1;
                }
                b'\r' => {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    state = State::FieldStart;
                    pos += 2;
                }
                _ => {
                    return Err(CsvError::TrailingByte { record, byte: pos });
                }
            },
        }
    }

    match state {
        State::Quoted { opening } => {
            return Err(CsvError::UnclosedQuote {
                record,
                byte: opening,
            });
        }
        State::AfterQuote | State::Unquoted | State::FieldStart => {
            if pos > 0 && (bytes[pos - 1] == b'\n') {
                // A final record terminator does not create another record.
            } else {
                fields.push(field);
                records.push(fields);
            }
        }
    }

    Ok(records)
}