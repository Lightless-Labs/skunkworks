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
    let mut record_index = 0;
    let mut quote_start = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if state != State::Quoted && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
        }

        match state {
            State::FieldStart => match b {
                b'"' => {
                    quote_start = i;
                    state = State::Quoted;
                    i += 1;
                }
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
                b'\r' => {
                    record.push(String::new());
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    i += 2;
                }
                _ => {
                    let start = i;
                    while i < bytes.len()
                        && !matches!(bytes[i], b',' | b'\n' | b'\r' | b'"' | 0)
                    {
                        i += 1;
                    }
                    field.push_str(&input[start..i]);
                    state = State::Unquoted;
                }
            },
            State::Unquoted => match b {
                b'"' => {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: i,
                    });
                }
                b',' => {
                    record.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    i += 1;
                }
                b'\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = State::FieldStart;
                    i += 1;
                }
                b'\r' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = State::FieldStart;
                    i += 2;
                }
                _ => {
                    let start = i;
                    while i < bytes.len()
                        && !matches!(bytes[i], b',' | b'\n' | b'\r' | b'"' | 0)
                    {
                        i += 1;
                    }
                    field.push_str(&input[start..i]);
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
                    let start = i;
                    while i < bytes.len() && !matches!(bytes[i], b'"' | 0) {
                        i += 1;
                    }
                    field.push_str(&input[start..i]);
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
                    record_index += 1;
                    state = State::FieldStart;
                    i += 1;
                }
                b'\r' => {
                    record.push(std::mem::take(&mut field));
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

    if state == State::Quoted {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_start,
        });
    }

    if state != State::FieldStart || !record.is_empty() {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}