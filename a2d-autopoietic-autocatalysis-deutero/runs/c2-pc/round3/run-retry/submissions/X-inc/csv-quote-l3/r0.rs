#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsvError {
    UnexpectedQuote { record: usize, byte: usize },
    TrailingByte { record: usize, byte: usize },
    BareCr { record: usize, byte: usize },
    UnclosedQuote { record: usize, byte: usize },
    Nul { record: usize, byte: usize },
    InvalidConfig { component: ConfigPart },
    DuplicateHeader { name: String, first: usize, second: usize },
    HeaderWidth { record: usize, expected: usize, actual: usize },
    ReaderFinished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigPart {
    Delimiter,
    Quote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicatePolicy {
    Reject,
    KeepFirst,
    KeepLast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub delimiter: u8,
    pub quote: u8,
    pub headers: bool,
    pub duplicates: DuplicatePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub header: Option<Vec<String>>,
    pub rows: Vec<Vec<String>>,
    pub duplicates: DuplicatePolicy,
}

impl Table {
    pub fn column_index(&self, name: &str) -> Option<usize> {
        let header = self.header.as_ref()?;
        match self.duplicates {
            DuplicatePolicy::KeepLast => header.iter().rposition(|s| s == name),
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|s| s == name)
            }
        }
    }
}

pub struct StreamReader {
    config: Config,
    input: String,
    result: Option<Result<Table, CsvError>>,
}

fn validate_config(config: Config) -> Result<(), CsvError> {
    if !(0x21..=0x7e).contains(&config.delimiter) {
        return Err(CsvError::InvalidConfig {
            component: ConfigPart::Delimiter,
        });
    }
    if !(0x21..=0x7e).contains(&config.quote) {
        return Err(CsvError::InvalidConfig {
            component: ConfigPart::Quote,
        });
    }
    if config.delimiter == config.quote {
        return Err(CsvError::InvalidConfig {
            component: ConfigPart::Quote,
        });
    }
    Ok(())
}

fn parse_with(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut pos = if input.starts_with('\u{feff}') { 3 } else { 0 };
    if pos == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut record_index = 0usize;

    let mut in_quotes = false;
    let mut after_quote = false;
    let mut opening_quote = 0usize;
    let mut at_field_start = true;

    while pos < bytes.len() {
        let b = bytes[pos];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: pos,
            });
        }

        if !in_quotes && b == b'\r' && bytes.get(pos + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: pos,
            });
        }

        if in_quotes {
            if b == quote {
                if bytes.get(pos + 1) == Some(&quote) {
                    field.push(quote as char);
                    pos += 2;
                } else {
                    in_quotes = false;
                    after_quote = true;
                    pos += 1;
                }
            } else {
                let ch = input[pos..].chars().next().unwrap();
                field.push(ch);
                pos += ch.len_utf8();
            }
            continue;
        }

        if after_quote {
            if b == delimiter {
                fields.push(std::mem::take(&mut field));
                at_field_start = true;
                after_quote = false;
                pos += 1;
                continue;
            }
            if b == b'\n' || b == b'\r' {
                fields.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut fields));
                record_index += 1;
                at_field_start = true;
                after_quote = false;
                pos += if b == b'\r' { 2 } else { 1 };
                continue;
            }
            return Err(CsvError::TrailingByte {
                record: record_index,
                byte: pos,
            });
        }

        if b == delimiter {
            fields.push(std::mem::take(&mut field));
            at_field_start = true;
            pos += 1;
        } else if b == b'\n' || b == b'\r' {
            fields.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut fields));
            record_index += 1;
            at_field_start = true;
            pos += if b == b'\r' { 2 } else { 1 };
        } else if b == quote {
            if at_field_start {
                in_quotes = true;
                opening_quote = pos;
                at_field_start = false;
                pos += 1;
            } else {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: pos,
                });
            }
        } else {
            let ch = input[pos..].chars().next().unwrap();
            field.push(ch);
            at_field_start = false;
            pos += ch.len_utf8();
        }
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: opening_quote,
        });
    }

    if after_quote || !at_field_start || !fields.is_empty() {
        fields.push(field);
        records.push(fields);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with(input, b',', b'"')
}

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let (header, rows) = if config.headers {
        if records.is_empty() {
            (None, Vec::new())
        } else {
            let mut iter = records.into_iter();
            (iter.next(), iter.collect::<Vec<_>>())
        }
    } else {
        (None, records)
    };

    if let Some(ref header_fields) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            let mut seen: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            for (second, name) in header_fields.iter().enumerate() {
                if let Some(&first) = seen.get(name.as_str()) {
                    return Err(CsvError::DuplicateHeader {
                        name: name.clone(),
                        first,
                        second,
                    });
                }
                seen.insert(name.as_str(), second);
            }
        }

        for (i, row) in rows.iter().enumerate() {
            if row.len() != header_fields.len() {
                return Err(CsvError::HeaderWidth {
                    record: i + 1,
                    expected: header_fields.len(),
                    actual: row.len(),
                });
            }
        }
    }

    Ok(Table {
        header,
        rows,
        duplicates: config.duplicates,
    })
}

impl StreamReader {
    pub fn new(config: Config) -> Result<Self, CsvError> {
        validate_config(config)?;
        Ok(Self {
            config,
            input: String::new(),
            result: None,
        })
    }

    pub fn feed(&mut self, chunk: &str) -> Result<(), CsvError> {
        if self.result.is_some() {
            return Err(CsvError::ReaderFinished);
        }
        self.input.push_str(chunk);
        Ok(())
    }

    pub fn finish(&mut self) -> Result<Table, CsvError> {
        if let Some(result) = &self.result {
            return result.clone();
        }

        let result = parse_with(&self.input, self.config.delimiter, self.config.quote)
            .and_then(|records| build_table(records, self.config));
        self.result = Some(result.clone());
        result
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.result = None;
    }
}

pub fn read_csv(input: &str, config: Config) -> Result<Table, CsvError> {
    let mut reader = StreamReader::new(config)?;
    reader.feed(input)?;
    reader.finish()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnRef {
    Index(usize),
    Name(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType {
    Text,
    Integer,
    Decimal,
    Boolean,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnSpec {
    pub column: ColumnRef,
    pub kind: ColumnType,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Null,
    Text(String),
    Integer(i64),
    Decimal(i64),
    Boolean(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellErrorKind {
    UnknownColumn,
    MissingCell,
    Empty,
    InvalidInteger,
    IntegerOverflow,
    InvalidDecimal,
    DecimalOverflow,
    InvalidBoolean,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellError {
    pub row: usize,
    pub spec: usize,
    pub column: Option<usize>,
    pub kind: CellErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorPolicy {
    Collect,
    FirstPerRow,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedResult {
    pub rows: Vec<Vec<Option<Value>>>,
    pub errors: Vec<CellError>,
}

fn integer_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    let start = usize::from(b[0] == b'-');
    if start == b.len() {
        return false;
    }
    if b[start] == b'0' {
        return start == 0 && b.len() == 1;
    }
    if !(b'1'..=b'9').contains(&b[start]) {
        return false;
    }
    b[start + 1..].iter().all(u8::is_ascii_digit)
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    let start = usize::from(b[0] == b'-');
    if start >= b.len() {
        return false;
    }
    let dot = match b[start..].iter().position(|&x| x == b'.') {
        Some(p) => start + p,
        None => return false,
    };
    if b.len() != dot + 3 {
        return false;
    }
    let integral = &b[start..dot];
    if integral.is_empty()
        || !(integral == b"0"
            || ((b'1'..=b'9').contains(&integral[0])
                && integral[1..].iter().all(u8::is_ascii_digit)))
        || !b[dot + 1..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    !(start == 1 && integral == b"0" && b[dot + 1] == b'0' && b[dot + 2] == b'0')
}

fn evaluate_cell(
    text: &str,
    spec: &ColumnSpec,
    _row: usize,
    _spec_index: usize,
    _column: usize,
) -> Result<Value, CellErrorKind> {
    if spec.nullable && text.is_empty() {
        return Ok(Value::Null);
    }
    if text.is_empty() && spec.kind != ColumnType::Text {
        return Err(CellErrorKind::Empty);
    }

    match spec.kind {
        ColumnType::Text => Ok(Value::Text(text.to_string())),
        ColumnType::Integer => {
            if !integer_syntax(text) {
                return Err(CellErrorKind::InvalidInteger);
            }
            text.parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| CellErrorKind::IntegerOverflow)
        }
        ColumnType::Decimal => {
            if !decimal_syntax(text) {
                return Err(CellErrorKind::InvalidDecimal);
            }

            let negative = text.starts_with('-');
            let unsigned = if negative { &text[1..] } else { text };
            let (integral, fractional) = unsigned.split_once('.').unwrap();

            // Accumulate only as far as needed to distinguish an i64 result
            // from overflow, so arbitrarily long valid spellings never panic.
            let mut magnitude = 0i128;
            let mut overflow = false;
            for byte in integral.bytes().chain(fractional.bytes()) {
                let digit = (byte - b'0') as i128;
                match magnitude.checked_mul(10).and_then(|n| n.checked_add(digit)) {
                    Some(n) => magnitude = n,
                    None => {
                        overflow = true;
                        break;
                    }
                }
            }

            if overflow {
                return Err(CellErrorKind::DecimalOverflow);
            }

            let signed = if negative { -magnitude } else { magnitude };
            i64::try_from(signed)
                .map(Value::Decimal)
                .map_err(|_| CellErrorKind::DecimalOverflow)
        }
        ColumnType::Boolean => match text {
            "yes" | "1" => Ok(Value::Boolean(true)),
            "no" | "0" => Ok(Value::Boolean(false)),
            _ => Err(CellErrorKind::InvalidBoolean),
        },
    }
}

pub fn extract_columns(
    table: &Table,
    columns: &[ColumnSpec],
    policy: ErrorPolicy,
) -> TypedResult {
    let mut output = Vec::new();
    let mut errors = Vec::new();

    'rows: for (row_index, row) in table.rows.iter().enumerate() {
        let mut out_row = vec![None; columns.len()];
        for (spec_index, spec) in columns.iter().enumerate() {
            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let column = match resolved {
                Some(index) => index,
                None => {
                    errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: None,
                        kind: CellErrorKind::UnknownColumn,
                    });
                    if policy == ErrorPolicy::Stop {
                        output.push(out_row);
                        break 'rows;
                    }
                    if policy == ErrorPolicy::FirstPerRow {
                        break;
                    }
                    continue;
                }
            };

            let text = match row.get(column) {
                Some(text) => text,
                None => {
                    errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column),
                        kind: CellErrorKind::MissingCell,
                    });
                    if policy == ErrorPolicy::Stop {
                        output.push(out_row);
                        break 'rows;
                    }
                    if policy == ErrorPolicy::FirstPerRow {
                        break;
                    }
                    continue;
                }
            };

            match evaluate_cell(text, spec, row_index, spec_index, column) {
                Ok(value) => out_row[spec_index] = Some(value),
                Err(kind) => {
                    errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column),
                        kind,
                    });
                    if policy == ErrorPolicy::Stop {
                        output.push(out_row);
                        break 'rows;
                    }
                    if policy == ErrorPolicy::FirstPerRow {
                        break;
                    }
                }
            }
        }
        output.push(out_row);
    }

    TypedResult {
        rows: output,
        errors,
    }
}