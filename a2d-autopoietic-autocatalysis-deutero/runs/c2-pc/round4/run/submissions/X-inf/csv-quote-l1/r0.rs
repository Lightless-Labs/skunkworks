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
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    if i == bytes.len() {
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

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul { record, byte: i });
        }

        if !matches!(state, State::Quoted { .. }) && b == b'\r' {
            if bytes.get(i + 1) == Some(&b'\n') {
                fields.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut fields));
                record += 1;
                i += 2;
                state = State::FieldStart;
                continue;
            }
            return Err(CsvError::BareCr { record, byte: i });
        }

        match state {
            State::FieldStart => match b {
                b',' => {
                    fields.push(std::mem::take(&mut field));
                    i += 1;
                }
                b'\n' => {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    i += 1;
                }
                b'"' => {
                    state = State::Quoted { opening: i };
                    i += 1;
                }
                _ => {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    state = State::Unquoted;
                    i += ch.len_utf8();
                }
            },
            State::Unquoted => match b {
                b',' => {
                    fields.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    i += 1;
                }
                b'\n' => {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    state = State::FieldStart;
                    i += 1;
                }
                b'"' => return Err(CsvError::UnexpectedQuote { record, byte: i }),
                _ => {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    i += ch.len_utf8();
                }
            },
            State::Quoted { opening } => {
                if b == b'"' {
                    if bytes.get(i + 1) == Some(&b'"') {
                        field.push('"');
                        i += 2;
                    } else {
                        state = State::AfterQuote;
                        i += 1;
                    }
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    i += ch.len_utf8();
                    let _ = opening;
                }
            }
            State::AfterQuote => match b {
                b',' => {
                    fields.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    i += 1;
                }
                b'\n' => {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    state = State::FieldStart;
                    i += 1;
                }
                _ => return Err(CsvError::TrailingByte { record, byte: i }),
            },
        }
    }

    if let State::Quoted { opening } = state {
        return Err(CsvError::UnclosedQuote { record, byte: opening });
    }

    if !matches!(state, State::FieldStart) || !fields.is_empty() || !field.is_empty() {
        fields.push(field);
        records.push(fields);
    }

    Ok(records)
}