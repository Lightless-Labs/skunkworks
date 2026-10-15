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
    let start = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 };

    if start == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();

    // 0: beginning of field, 1: unquoted field, 2: quoted field, 3: after closing quote
    let mut state = 0u8;
    let mut quote_start = 0usize;
    let mut record_index = 0usize;
    let mut i = start;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if state != 2 && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
        }

        match state {
            0 => match b {
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
                b'"' => {
                    quote_start = i;
                    state = 2;
                    i += 1;
                }
                _ => {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    state = 1;
                    i += ch.len_utf8();
                }
            },
            1 => match b {
                b',' => {
                    record.push(std::mem::take(&mut field));
                    state = 0;
                    i += 1;
                }
                b'\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = 0;
                    i += 1;
                }
                b'\r' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = 0;
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
            2 => {
                if b == b'"' {
                    if bytes.get(i + 1) == Some(&b'"') {
                        field.push('"');
                        i += 2;
                    } else {
                        state = 3;
                        i += 1;
                    }
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    i += ch.len_utf8();
                }
            }
            _ => match b {
                b',' => {
                    record.push(std::mem::take(&mut field));
                    state = 0;
                    i += 1;
                }
                b'\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = 0;
                    i += 1;
                }
                b'\r' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = 0;
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

    if state == 2 {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_start,
        });
    }

    if i > start {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_basic_fields() {
        assert_eq!(
            parse_csv("name,age\nAda,36\n"),
            Ok(vec![
                vec!["name".to_string(), "age".to_string()],
                vec!["Ada".to_string(), "36".to_string()]
            ])
        );
    }

    #[test]
    fn public_escaped_and_quoted() {
        assert_eq!(
            parse_csv("\"red,blue\",\"say \"\"hi\"\"\""),
            Ok(vec![vec![
                "red,blue".to_string(),
                "say \"hi\"".to_string()
            ]])
        );
    }

    #[test]
    fn public_record_endings() {
        assert_eq!(
            parse_csv("u,v\r\n\"two\nlines\",w"),
            Ok(vec![
                vec!["u".to_string(), "v".to_string()],
                vec!["two\nlines".to_string(), "w".to_string()]
            ])
        );
    }

    #[test]
    fn public_unicode_empty() {
        assert_eq!(
            parse_csv("雪,,\"\""),
            Ok(vec![vec![
                "雪".to_string(),
                "".to_string(),
                "".to_string()
            ]])
        );
    }

    #[test]
    fn bom_offsets_and_empty_input() {
        assert_eq!(parse_csv("\u{feff}"), Ok(vec![]));
        assert_eq!(
            parse_csv("\u{feff}雪\n\u{feff}x"),
            Ok(vec![
                vec!["雪".to_string()],
                vec!["\u{feff}x".to_string()]
            ])
        );
        assert_eq!(
            parse_csv("\u{feff}a\""),
            Err(CsvError::UnexpectedQuote { record: 0, byte: 4 })
        );
    }

    #[test]
    fn empty_records_and_trailing_fields() {
        assert_eq!(parse_csv(","), Ok(vec![vec!["".into(), "".into()]]));
        assert_eq!(parse_csv("\n\n"), Ok(vec![vec!["".into()], vec!["".into()]]));
        assert_eq!(parse_csv("a,"), Ok(vec![vec!["a".into(), "".into()]]));
        assert_eq!(parse_csv("a\n"), Ok(vec![vec!["a".into()]]));
        assert_eq!(parse_csv(""), Ok(vec![]));
    }

    #[test]
    fn error_precedence_and_offsets() {
        assert_eq!(
            parse_csv("\"x\" \r"),
            Err(CsvError::BareCr { record: 0, byte: 4 })
        );
        assert_eq!(
            parse_csv("\"x\" \0"),
            Err(CsvError::Nul { record: 0, byte: 4 })
        );
        assert_eq!(
            parse_csv("\"x\"z"),
            Err(CsvError::TrailingByte { record: 0, byte: 3 })
        );
        assert_eq!(
            parse_csv("\"x"),
            Err(CsvError::UnclosedQuote { record: 0, byte: 0 })
        );
        assert_eq!(
            parse_csv("é\""),
            Err(CsvError::UnexpectedQuote { record: 0, byte: 2 })
        );
    }

    #[test]
    fn quote_escape_at_eof_is_unclosed() {
        assert_eq!(
            parse_csv("\"a\"\""),
            Err(CsvError::UnclosedQuote { record: 0, byte: 0 })
        );
    }
}