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
pub enum ConfigPart { Delimiter, Quote }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicatePolicy { Reject, KeepFirst, KeepLast }

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
            DuplicatePolicy::KeepLast => header.iter().rposition(|v| v == name),
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst =>
                header.iter().position(|v| v == name),
        }
    }
}

pub struct StreamReader {
    config: Config,
    input: String,
    cached: Option<Result<Table, CsvError>>,
}

fn validate_config(config: Config) -> Result<(), CsvError> {
    if !(0x21..=0x7e).contains(&config.delimiter) {
        return Err(CsvError::InvalidConfig { component: ConfigPart::Delimiter });
    }
    if !(0x21..=0x7e).contains(&config.quote) {
        return Err(CsvError::InvalidConfig { component: ConfigPart::Quote });
    }
    if config.delimiter == config.quote {
        return Err(CsvError::InvalidConfig { component: ConfigPart::Quote });
    }
    Ok(())
}

fn parse_with(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    if i == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut field_started = false;
    let mut quote_start = 0usize;
    let mut record_index = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        if b == 0 {
            return Err(CsvError::Nul { record: record_index, byte: i });
        }

        if in_quotes {
            if b == quote {
                if i + 1 < bytes.len() && bytes[i + 1] == quote {
                    field.push(quote as char);
                    i += 2;
                    continue;
                }
                in_quotes = false;
                after_quote = true;
                i += 1;
                continue;
            }
            let ch = input[i..].chars().next().unwrap();
            field.push(ch);
            i += ch.len_utf8();
            continue;
        }

        if b == b'\r' && (i + 1 >= bytes.len() || bytes[i + 1] != b'\n') {
            return Err(CsvError::BareCr { record: record_index, byte: i });
        }

        if after_quote {
            if b == delimiter {
                row.push(std::mem::take(&mut field));
                after_quote = false;
                field_started = false;
                i += 1;
                continue;
            }
            if b == b'\n' || b == b'\r' {
                row.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut row));
                record_index += 1;
                after_quote = false;
                field_started = false;
                i += if b == b'\r' { 2 } else { 1 };
                continue;
            }
            return Err(CsvError::TrailingByte { record: record_index, byte: i });
        }

        if b == delimiter {
            row.push(std::mem::take(&mut field));
            field_started = false;
            i += 1;
            continue;
        }
        if b == b'\n' || b == b'\r' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            record_index += 1;
            field_started = false;
            i += if b == b'\r' { 2 } else { 1 };
            continue;
        }
        if b == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote { record: record_index, byte: i });
            }
            in_quotes = true;
            quote_start = i;
            field_started = true;
            i += 1;
            continue;
        }

        let ch = input[i..].chars().next().unwrap();
        field.push(ch);
        field_started = true;
        i += ch.len_utf8();
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote { record: record_index, byte: quote_start });
    }

    if !row.is_empty() || !field.is_empty() || field_started || after_quote {
        row.push(field);
        records.push(row);
    }
    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with(input, b',', b'"')
}

fn build_table(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_config(config)?;
    let records = parse_with(input, config.delimiter, config.quote)?;

    let (header, rows, header_record_index) = if config.headers {
        if records.is_empty() {
            (None, Vec::new(), None)
        } else {
            (Some(records[0].clone()), records[1..].to_vec(), Some(1usize))
        }
    } else {
        (None, records, None)
    };

    if let (Some(h), DuplicatePolicy::Reject) = (&header, config.duplicates) {
        let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for (i, name) in h.iter().enumerate() {
            if let Some(&first) = seen.get(name.as_str()) {
                return Err(CsvError::DuplicateHeader {
                    name: name.clone(), first, second: i,
                });
            }
            seen.insert(name.as_str(), i);
        }
    }

    if let Some(h) = &header {
        for (i, row) in rows.iter().enumerate() {
            if row.len() != h.len() {
                return Err(CsvError::HeaderWidth {
                    record: header_record_index.unwrap_or(0) + i,
                    expected: h.len(),
                    actual: row.len(),
                });
            }
        }
    }

    Ok(Table { header, rows, duplicates: config.duplicates })
}

impl StreamReader {
    pub fn new(config: Config) -> Result<Self, CsvError> {
        validate_config(config)?;
        Ok(Self { config, input: String::new(), cached: None })
    }

    pub fn feed(&mut self, chunk: &str) -> Result<(), CsvError> {
        if self.cached.is_some() {
            return Err(CsvError::ReaderFinished);
        }
        self.input.push_str(chunk);
        Ok(())
    }

    pub fn finish(&mut self) -> Result<Table, CsvError> {
        if let Some(result) = &self.cached {
            return result.clone();
        }
        let result = build_table(&self.input, self.config);
        self.cached = Some(result.clone());
        result
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.cached = None;
    }
}

pub fn read_csv(input: &str, config: Config) -> Result<Table, CsvError> {
    let mut reader = StreamReader::new(config)?;
    reader.feed(input)?;
    reader.finish()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnRef { Index(usize), Name(String) }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType { Text, Integer, Decimal, Boolean }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnSpec {
    pub column: ColumnRef,
    pub kind: ColumnType,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value { Null, Text(String), Integer(i64), Decimal(i64), Boolean(bool) }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellErrorKind {
    UnknownColumn, MissingCell, Empty, InvalidInteger, IntegerOverflow,
    InvalidDecimal, DecimalOverflow, InvalidBoolean,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellError {
    pub row: usize,
    pub spec: usize,
    pub column: Option<usize>,
    pub kind: CellErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorPolicy { Collect, FirstPerRow, Stop }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedResult {
    pub rows: Vec<Vec<Option<Value>>>,
    pub errors: Vec<CellError>,
}

fn integer_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() { return false; }
    let start = usize::from(b[0] == b'-');
    if start == b.len() { return false; }
    if b[start] == b'0' {
        return start == 0 && b.len() == 1;
    }
    if !(b'1'..=b'9').contains(&b[start]) { return false; }
    b[start + 1..].iter().all(u8::is_ascii_digit)
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() { return false; }
    let start = usize::from(b[0] == b'-');
    if start == b.len() { return false; }
    let dot = match b[start..].iter().position(|&x| x == b'.') {
        Some(p) => start + p,
        None => return false,
    };
    if b.len() != dot + 3 { return false; }
    let integral = &b[start..dot];
    if integral.is_empty() { return false; }
    if integral == b"0" {
        // allowed unless the full spelling is negative zero
    } else if !(b'1'..=b'9').contains(&integral[0])
        || !integral[1..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    b[dot + 1..].iter().all(u8::is_ascii_digit)
        && !(start == 1 && integral == b"0" && b[dot + 1..] == *b"00")
}

fn eval_cell(s: &str, kind: ColumnType, nullable: bool) -> Result<Value, CellErrorKind> {
    if nullable && s.is_empty() {
        return Ok(Value::Null);
    }
    if s.is_empty() && kind != ColumnType::Text {
        return Err(CellErrorKind::Empty);
    }
    match kind {
        ColumnType::Text => Ok(Value::Text(s.to_string())),
        ColumnType::Integer => {
            if !integer_syntax(s) { return Err(CellErrorKind::InvalidInteger); }
            s.parse::<i64>().map(Value::Integer).map_err(|_| CellErrorKind::IntegerOverflow)
        }
        ColumnType::Decimal => {
            if !decimal_syntax(s) { return Err(CellErrorKind::InvalidDecimal); }
            let negative = s.starts_with('-');
            let body = if negative { &s[1..] } else { s };
            let mut parts = body.split('.');
            let whole = parts.next().unwrap();
            let frac = parts.next().unwrap();
            let magnitude_text = format!("{}{}", whole, frac);
            let magnitude = magnitude_text.parse::<i128>().map_err(|_| CellErrorKind::DecimalOverflow)?;
            let signed = if negative { -magnitude } else { magnitude };
            if signed < i64::MIN as i128 || signed > i64::MAX as i128 {
                return Err(CellErrorKind::DecimalOverflow);
            }
            Ok(Value::Decimal(signed as i64))
        }
        ColumnType::Boolean => match s {
            "yes" | "1" => Ok(Value::Boolean(true)),
            "no" | "0" => Ok(Value::Boolean(false)),
            _ => Err(CellErrorKind::InvalidBoolean),
        },
    }
}

pub fn extract_columns(table: &Table, columns: &[ColumnSpec], policy: ErrorPolicy) -> TypedResult {
    let mut result = TypedResult { rows: Vec::new(), errors: Vec::new() };
    'rows: for (row_index, row) in table.rows.iter().enumerate() {
        let mut out = Vec::with_capacity(columns.len());
        for (spec_index, spec) in columns.iter().enumerate() {
            let column = match &spec.column {
                ColumnRef::Index(i) => Some(*i),
                ColumnRef::Name(name) => table.column_index(name),
            };
            let failure = if column.is_none() {
                Some((None, CellErrorKind::UnknownColumn))
            } else {
                let index = column.unwrap();
                match row.get(index) {
                    None => Some((Some(index), CellErrorKind::MissingCell)),
                    Some(cell) => match eval_cell(cell, spec.kind, spec.nullable) {
                        Ok(value) => {
                            out.push(Some(value));
                            continue;
                        }
                        Err(kind) => Some((Some(index), kind)),
                    }
                }
            };
            let (error_column, kind) = failure.unwrap();
            result.errors.push(CellError {
                row: row_index, spec: spec_index, column: error_column, kind,
            });
            out.push(None);
            match policy {
                ErrorPolicy::Collect => {}
                ErrorPolicy::FirstPerRow => {
                    out.extend((spec_index + 1..columns.len()).map(|_| None));
                    break;
                }
                ErrorPolicy::Stop => {
                    out.extend((spec_index + 1..columns.len()).map(|_| None));
                    result.rows.push(out);
                    break 'rows;
                }
            }
        }
        result.rows.push(out);
    }
    result
}
