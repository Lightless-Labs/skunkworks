#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsvError {
    UnexpectedQuote { record: usize, byte: usize },
    TrailingByte { record: usize, byte: usize },
    BareCr { record: usize, byte: usize },
    UnclosedQuote { record: usize, byte: usize },
    Nul { record: usize, byte: usize },
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    const OUTSIDE: u8 = 0;
    const UNQUOTED: u8 = 1;
    const QUOTED: u8 = 2;
    const CLOSED: u8 = 3;

    let bytes = input.as_bytes();
    let mut i = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    };

    if i == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut state = OUTSIDE;
    let mut quote_start = 0;
    let mut record = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul { record, byte: i });
        }

        if state != QUOTED && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr { record, byte: i });
        }

        match state {
            OUTSIDE => match b {
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
                b'\r' => {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    i += 2;
                }
                b'"' => {
                    quote_start = i;
                    state = QUOTED;
                    i += 1;
                }
                _ => {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    state = UNQUOTED;
                    i += ch.len_utf8();
                }
            },
            UNQUOTED => match b {
                b',' => {
                    fields.push(std::mem::take(&mut field));
                    state = OUTSIDE;
                    i += 1;
                }
                b'\n' => {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    state = OUTSIDE;
                    i += 1;
                }
                b'\r' => {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    state = OUTSIDE;
                    i += 2;
                }
                b'"' => {
                    return Err(CsvError::UnexpectedQuote { record, byte: i });
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
                        state = CLOSED;
                        i += 1;
                    }
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    i += ch.len_utf8();
                }
            }
            CLOSED => match b {
                b',' => {
                    fields.push(std::mem::take(&mut field));
                    state = OUTSIDE;
                    i += 1;
                }
                b'\n' => {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    state = OUTSIDE;
                    i += 1;
                }
                b'\r' => {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    state = OUTSIDE;
                    i += 2;
                }
                _ => return Err(CsvError::TrailingByte { record, byte: i }),
            },
            _ => unreachable!(),
        }
    }

    if state == QUOTED {
        return Err(CsvError::UnclosedQuote {
            record,
            byte: quote_start,
        });
    }

    if state != OUTSIDE || !fields.is_empty() || !field.is_empty() {
        fields.push(field);
        records.push(fields);
    }

    Ok(records)
}