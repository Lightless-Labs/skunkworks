#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsvError {
    UnexpectedQuote { record: usize, byte: usize },
    TrailingByte { record: usize, byte: usize },
    BareCr { record: usize, byte: usize },
    UnclosedQuote { record: usize, byte: usize },
    Nul { record: usize, byte: usize },
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    const BOM: &str = "\u{feff}";

    let offset = if input.starts_with(BOM) { BOM.len() } else { 0 };
    let bytes = input.as_bytes();
    if offset == bytes.len() {
        return Ok(Vec::new());
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        StartField,
        Unquoted,
        Quoted,
        AfterQuote,
    }

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut state = State::StartField;
    let mut record_index = 0usize;
    let mut i = offset;
    let mut quote_open = 0usize;

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
            record_index += 1;
            state = State::StartField;
            i += 2;
            continue;
        }

        match state {
            State::StartField => match b {
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
                    let start = i;
                    while i < bytes.len()
                        && bytes[i] != b','
                        && bytes[i] != b'\n'
                        && bytes[i] != b'\r'
                        && bytes[i] != b'"'
                        && bytes[i] != 0
                    {
                        i += 1;
                    }
                    field.push_str(&input[start..i]);
                    state = State::Unquoted;
                }
            },
            State::Unquoted => match b {
                b',' => {
                    record.push(std::mem::take(&mut field));
                    state = State::StartField;
                    i += 1;
                }
                b'\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = State::StartField;
                    i += 1;
                }
                b'"' => {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: i,
                    });
                }
                _ => {
                    let start = i;
                    while i < bytes.len()
                        && bytes[i] != b','
                        && bytes[i] != b'\n'
                        && bytes[i] != b'\r'
                        && bytes[i] != b'"'
                        && bytes[i] != 0
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
                    while i < bytes.len() && bytes[i] != b'"' && bytes[i] != 0 {
                        i += 1;
                    }
                    field.push_str(&input[start..i]);
                }
            }
            State::AfterQuote => match b {
                b',' => {
                    record.push(std::mem::take(&mut field));
                    state = State::StartField;
                    i += 1;
                }
                b'\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = State::StartField;
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

    if i > offset && (state != State::StartField || !record.is_empty() || !field.is_empty()) {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}