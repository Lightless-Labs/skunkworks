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
    let mut quote_start = 0usize;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul { record: records.len(), byte: i });
        }

        match state {
            State::Quoted => {
                if b == b'"' {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'"' {
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
            State::FieldStart | State::Unquoted | State::AfterQuote => {
                if b == b'\r' && (i + 1 >= bytes.len() || bytes[i + 1] != b'\n') {
                    return Err(CsvError::BareCr { record: records.len(), byte: i });
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
                            records.push(record);
                            record = Vec::new();
                            state = State::FieldStart;
                            i += 1;
                        }
                        b'\r' => {
                            record.push(String::new());
                            records.push(record);
                            record = Vec::new();
                            state = State::FieldStart;
                            i += 2;
                        }
                        _ => {
                            let ch = input[i..].chars().next().unwrap();
                            field.push(ch);
                            state = State::Unquoted;
                            i += ch.len_utf8();
                        }
                    },
                    State::Unquoted => match b {
                        b'"' => {
                            return Err(CsvError::UnexpectedQuote { record: records.len(), byte: i });
                        }
                        b',' => {
                            record.push(std::mem::take(&mut field));
                            state = State::FieldStart;
                            i += 1;
                        }
                        b'\n' => {
                            record.push(std::mem::take(&mut field));
                            records.push(record);
                            record = Vec::new();
                            state = State::FieldStart;
                            i += 1;
                        }
                        b'\r' => {
                            record.push(std::mem::take(&mut field));
                            records.push(record);
                            record = Vec::new();
                            state = State::FieldStart;
                            i += 2;
                        }
                        _ => {
                            let ch = input[i..].chars().next().unwrap();
                            field.push(ch);
                            i += ch.len_utf8();
                        }
                    },
                    State::AfterQuote => match b {
                        b',' => {
                            record.push(std::mem::take(&mut field));
                            state = State::FieldStart;
                            i += 1;
                        }
                        b'\n' => {
                            record.push(std::mem::take(&mut field));
                            records.push(record);
                            record = Vec::new();
                            state = State::FieldStart;
                            i += 1;
                        }
                        b'\r' => {
                            record.push(std::mem::take(&mut field));
                            records.push(record);
                            record = Vec::new();
                            state = State::FieldStart;
                            i += 2;
                        }
                        _ => {
                            return Err(CsvError::TrailingByte { record: records.len(), byte: i });
                        }
                    },
                    State::Quoted => unreachable!(),
                }
            }
        }
    }

    if state == State::Quoted {
        return Err(CsvError::UnclosedQuote { record: records.len(), byte: quote_start });
    }

    if state != State::FieldStart || !record.is_empty() {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}

fn main() {}