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
    let start = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    };

    if start == bytes.len() {
        return Ok(Vec::new());
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        FieldStart,
        Unquoted,
        Quoted,
        AfterQuote,
    }

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut state = State::FieldStart;
    let mut quote_open = 0;
    let mut record_index = 0;
    let mut i = start;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if state != State::Quoted && b == b'\r' {
            if bytes.get(i + 1) != Some(&b'\n') {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: i,
                });
            }

            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            state = State::FieldStart;
            record_index += 1;
            i += 2;
            continue;
        }

        match state {
            State::FieldStart => match b {
                b',' => {
                    record.push(String::new());
                    i += 1;
                }
                b'\n' => {
                    record.push(String::new());
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    i += 1;
                }
                b'"' => {
                    state = State::Quoted;
                    quote_open = i;
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
                    record.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    i += 1;
                }
                b'\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    state = State::FieldStart;
                    record_index += 1;
                    i += 1;
                }
                b'"' => {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: i,
                    });
                }
                _ => {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    i += ch.len_utf8();
                }
            },
            State::Quoted => {
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
                }
            }
            State::AfterQuote => match b {
                b',' => {
                    record.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    i += 1;
                }
                b'\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    state = State::FieldStart;
                    record_index += 1;
                    i += 1;
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

    if state == State::Quoted {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_open,
        });
    }

    if !bytes[start..].ends_with(b"\n") {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}