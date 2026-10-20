#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsvError {
    UnexpectedQuote { record: usize, byte: usize },
    TrailingByte { record: usize, byte: usize },
    BareCr { record: usize, byte: usize },
    UnclosedQuote { record: usize, byte: usize },
    Nul { record: usize, byte: usize },
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    #[derive(Clone, Copy)]
    enum State {
        FieldStart,
        Unquoted,
        Quoted { opening: usize },
        AfterQuote,
    }

    let bytes = input.as_bytes();
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    if i == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = Vec::new();
    let mut state = State::FieldStart;
    let mut record_index = 0usize;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        let outside_quotes = !matches!(state, State::Quoted { .. });
        if outside_quotes && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
        }

        match state {
            State::FieldStart => match b {
                b',' => {
                    record.push(String::new());
                    i += 1;
                }
                b'"' => {
                    state = State::Quoted { opening: i };
                    i += 1;
                }
                b'\n' => {
                    record.push(String::new());
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    i += 1;
                }
                b'\r' => {
                    record.push(String::new());
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    i += 2;
                }
                _ => {
                    field.push(b);
                    state = State::Unquoted;
                    i += 1;
                }
            },
            State::Unquoted => match b {
                b',' => {
                    record.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    state = State::FieldStart;
                    i += 1;
                }
                b'\n' => {
                    record.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = State::FieldStart;
                    i += 1;
                }
                b'\r' => {
                    record.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = State::FieldStart;
                    i += 2;
                }
                b'"' => {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: i,
                    });
                }
                _ => {
                    field.push(b);
                    i += 1;
                }
            },
            State::Quoted { opening } => {
                if b == b'"' {
                    if bytes.get(i + 1) == Some(&b'"') {
                        field.push(b'"');
                        i += 2;
                    } else {
                        state = State::AfterQuote;
                        i += 1;
                    }
                } else {
                    field.push(b);
                    i += 1;
                }
                let _ = opening;
            }
            State::AfterQuote => match b {
                b',' => {
                    record.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    state = State::FieldStart;
                    i += 1;
                }
                b'\n' => {
                    record.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = State::FieldStart;
                    i += 1;
                }
                b'\r' => {
                    record.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = State::FieldStart;
                    i += 2;
                }
                _ => {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: i,
                    });
                }
            },
        }
    }

    match state {
        State::Quoted { opening } => {
            return Err(CsvError::UnclosedQuote {
                record: record_index,
                byte: opening,
            });
        }
        State::Unquoted | State::AfterQuote => {
            record.push(String::from_utf8(field).unwrap());
            records.push(record);
        }
        State::FieldStart => {
            if !record.is_empty() {
                record.push(String::new());
                records.push(record);
            }
        }
    }

    Ok(records)
}
