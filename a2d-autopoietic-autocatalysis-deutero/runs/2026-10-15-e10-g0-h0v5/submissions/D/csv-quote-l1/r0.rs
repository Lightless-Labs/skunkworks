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
    let mut index = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    };

    if index == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut record_index = 0usize;
    let mut state = State::FieldStart;
    let mut opening_quote = 0usize;

    while index < bytes.len() {
        let byte = bytes[index];

        if byte == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: index,
            });
        }

        if !matches!(state, State::Quoted)
            && byte == b'\r'
            && bytes.get(index + 1) != Some(&b'\n')
        {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: index,
            });
        }

        match state {
            State::FieldStart => match byte {
                b',' => {
                    record.push(String::new());
                    index += 1;
                }
                b'\n' => {
                    record.push(String::new());
                    records.push(record);
                    record = Vec::new();
                    record_index += 1;
                    index += 1;
                }
                b'\r' => {
                    record.push(String::new());
                    records.push(record);
                    record = Vec::new();
                    record_index += 1;
                    index += 2;
                }
                b'"' => {
                    opening_quote = index;
                    state = State::Quoted;
                    index += 1;
                }
                _ => {
                    let start = index;
                    while index < bytes.len()
                        && !matches!(bytes[index], b',' | b'\n' | b'\r' | b'"' | 0)
                    {
                        index += 1;
                    }
                    field.push_str(&input[start..index]);
                    state = State::Unquoted;
                }
            },
            State::Unquoted => match byte {
                b',' => {
                    record.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    index += 1;
                }
                b'\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(record);
                    record = Vec::new();
                    record_index += 1;
                    state = State::FieldStart;
                    index += 1;
                }
                b'\r' => {
                    record.push(std::mem::take(&mut field));
                    records.push(record);
                    record = Vec::new();
                    record_index += 1;
                    state = State::FieldStart;
                    index += 2;
                }
                b'"' => {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: index,
                    });
                }
                _ => {
                    let start = index;
                    while index < bytes.len()
                        && !matches!(bytes[index], b',' | b'\n' | b'\r' | b'"' | 0)
                    {
                        index += 1;
                    }
                    field.push_str(&input[start..index]);
                }
            },
            State::Quoted => {
                if byte == b'"' {
                    if bytes.get(index + 1) == Some(&b'"') {
                        field.push('"');
                        index += 2;
                    } else {
                        state = State::AfterQuote;
                        index += 1;
                    }
                } else {
                    let start = index;
                    while index < bytes.len() && !matches!(bytes[index], b'"' | 0) {
                        index += 1;
                    }
                    field.push_str(&input[start..index]);
                }
            }
            State::AfterQuote => match byte {
                b',' => {
                    record.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    index += 1;
                }
                b'\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(record);
                    record = Vec::new();
                    record_index += 1;
                    state = State::FieldStart;
                    index += 1;
                }
                b'\r' => {
                    record.push(std::mem::take(&mut field));
                    records.push(record);
                    record = Vec::new();
                    record_index += 1;
                    state = State::FieldStart;
                    index += 2;
                }
                _ => {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: index,
                    });
                }
            },
        }
    }

    if state == State::Quoted {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: opening_quote,
        });
    }

    if state != State::FieldStart || !record.is_empty() {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    FieldStart,
    Unquoted,
    Quoted,
    AfterQuote,
}