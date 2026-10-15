#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsvError {
    UnexpectedQuote { record: usize, byte: usize },
    TrailingByte { record: usize, byte: usize },
    BareCr { record: usize, byte: usize },
    UnclosedQuote { record: usize, byte: usize },
    Nul { record: usize, byte: usize },
    
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_dialect(input, b',', b'"')
}

fn parse_dialect(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = Vec::new();
    // 0 = field start, 1 = unquoted, 2 = quoted, 3 = closed quote.
    let mut state = 0;
    let mut opening = 0;
    let mut active = false;
    while i < bytes.len() {
        let b = bytes[i];
        let record = records.len();
        if b == 0 {
            return Err(CsvError::Nul { record, byte: i });
        }
        if state == 2 {
            if b == quote {
                if i + 1 < bytes.len() && bytes[i + 1] == quote {
                    field.push(quote);
                    i += 2;
                } else {
                    state = 3;
                    i += 1;
                }
            } else {
                field.push(b);
                i += 1;
            }
            continue;
        }
        if b == b'\r' && (i + 1 == bytes.len() || bytes[i + 1] != b'\n') {
            return Err(CsvError::BareCr { record, byte: i });
        }
        if b == delimiter {
            row.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
            state = 0;
            active = true;
            i += 1;
        } else if b == b'\n' || b == b'\r' {
            row.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
            records.push(std::mem::take(&mut row));
            state = 0;
            active = false;
            i += if b == b'\r' { 2 } else { 1 };
        } else if state == 3 {
            return Err(CsvError::TrailingByte { record, byte: i });
        } else if b == quote {
            if state == 1 {
                return Err(CsvError::UnexpectedQuote { record, byte: i });
            }
            state = 2;
            opening = i;
            active = true;
            i += 1;
        } else {
            field.push(b);
            state = 1;
            active = true;
            i += 1;
        }
    }
    if state == 2 {
        return Err(CsvError::UnclosedQuote { record: records.len(), byte: opening });
    }
    if active {
        row.push(String::from_utf8(field).unwrap());
        records.push(row);
    }
    Ok(records)
}
