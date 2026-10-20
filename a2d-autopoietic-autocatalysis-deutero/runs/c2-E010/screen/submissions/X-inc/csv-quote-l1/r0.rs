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
    let mut record = Vec::new();
    let mut field = String::new();
    let mut record_index = 0usize;

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
            return Err(CsvError::Nul {
                record: record_index,
                byte: pos,
            });
        }

        let outside_quotes = !matches!(state, State::Quoted { .. });

        if outside_quotes && byte == b'\r'
            && (pos + 1 >= bytes.len() || bytes[pos + 1] != b'\n')
        {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: pos,
            });
        }

        match state {
            State::FieldStart => match byte {
                b'"' => {
                    state = State::Quoted { opening: pos };
                    pos += 1;
                }
                b',' => {
                    record.push(std::mem::take(&mut field));
                    pos += 1;
                }
                b'\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    pos += 1;
                }
                b'\r' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    pos += 2;
                }
                _ => {
                    let ch = input[pos..].chars().next().unwrap();
                    field.push(ch);
                    pos += ch.len_utf8();
                    state = State::Unquoted;
                }
            },
            State::Unquoted => match byte {
                b'"' => {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: pos,
                    });
                }
                b',' => {
                    record.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    pos += 1;
                }
                b'\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = State::FieldStart;
                    pos += 1;
                }
                b'\r' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = State::FieldStart;
                    pos += 2;
                }
                _ => {
                    let ch = input[pos..].chars().next().unwrap();
                    field.push(ch);
                    pos += ch.len_utf8();
                }
            },
            State::Quoted { opening } => {
                if byte == b'"' {
                    if pos + 1 < bytes.len() && bytes[pos + 1] == b'"' {
                        field.push('"');
                        pos += 2;
                    } else {
                        state = State::AfterQuote;
                        pos += 1;
                    }
                } else {
                    let ch = input[pos..].chars().next().unwrap();
                    field.push(ch);
                    pos += ch.len_utf8();
                    state = State::Quoted { opening };
                }
            }
            State::AfterQuote => match byte {
                b',' => {
                    record.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    pos += 1;
                }
                b'\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = State::FieldStart;
                    pos += 1;
                }
                b'\r' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = State::FieldStart;
                    pos += 2;
                }
                _ => {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: pos,
                    });
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

    if !matches!(state, State::FieldStart) || !record.is_empty() {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}