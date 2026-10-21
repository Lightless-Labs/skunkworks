#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsvError {
    UnexpectedQuote { record: usize, byte: usize },
    TrailingByte { record: usize, byte: usize },
    BareCr { record: usize, byte: usize },
    UnclosedQuote { record: usize, byte: usize },
    Nul { record: usize, byte: usize },
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    const FIELD_START: u8 = 0;
    const UNQUOTED: u8 = 1;
    const QUOTED: u8 = 2;
    const AFTER_QUOTE: u8 = 3;

    let bytes = input.as_bytes();
    let mut i = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    };

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut state = FIELD_START;
    let mut quote_open = 0;
    let mut record_index = 0;
    let mut record_started = false;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if state != QUOTED && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
        }

        match state {
            FIELD_START => match b {
                b',' => {
                    record.push(String::new());
                    record_started = true;
                    i += 1;
                }
                b'\n' => {
                    record.push(String::new());
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    record_started = false;
                    i += 1;
                }
                b'\r' => {
                    record.push(String::new());
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    record_started = false;
                    i += 2;
                }
                b'"' => {
                    state = QUOTED;
                    quote_open = i;
                    record_started = true;
                    i += 1;
                }
                _ => {
                    state = UNQUOTED;
                    record_started = true;
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    i += ch.len_utf8();
                }
            },
            UNQUOTED => match b {
                b',' => {
                    record.push(std::mem::take(&mut field));
                    state = FIELD_START;
                    record_started = true;
                    i += 1;
                }
                b'\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = FIELD_START;
                    record_started = false;
                    i += 1;
                }
                b'\r' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = FIELD_START;
                    record_started = false;
                    i += 2;
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
            QUOTED => {
                if b == b'"' {
                    if bytes.get(i + 1) == Some(&b'"') {
                        field.push('"');
                        i += 2;
                    } else {
                        state = AFTER_QUOTE;
                        i += 1;
                    }
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    i += ch.len_utf8();
                }
            }
            AFTER_QUOTE => match b {
                b',' => {
                    record.push(std::mem::take(&mut field));
                    state = FIELD_START;
                    record_started = true;
                    i += 1;
                }
                b'\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = FIELD_START;
                    record_started = false;
                    i += 1;
                }
                b'\r' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = FIELD_START;
                    record_started = false;
                    i += 2;
                }
                _ => {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: i,
                    });
                }
            },
            _ => unreachable!(),
        }
    }

    if state == QUOTED {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_open,
        });
    }

    if record_started || !record.is_empty() || state == AFTER_QUOTE || state == UNQUOTED {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}