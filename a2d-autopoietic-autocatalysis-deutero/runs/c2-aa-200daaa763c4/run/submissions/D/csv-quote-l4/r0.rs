use std::collections::HashMap;

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
            DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                header.iter().position(|s| s == name)
            }
        }
    }
}

fn validate_dialect(delimiter: u8, quote: u8) -> Result<(), ConfigPart> {
    if !(0x21..=0x7e).contains(&delimiter) {
        return Err(ConfigPart::Delimiter);
    }
    if !(0x21..=0x7e).contains(&quote) {
        return Err(ConfigPart::Quote);
    }
    if delimiter == quote {
        return Err(ConfigPart::Quote);
    }
    Ok(())
}

fn parse_with(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let start = if input.starts_with('\u{feff}') { 3 } else { 0 };
    if start == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut i = start;
    let mut record_index = 0usize;
    let mut in_quotes = false;
    let mut opening_quote = 0usize;
    let mut after_quote = false;
    let mut field_started = false;
    let mut row_started = false;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul { record: record_index, byte: i });
        }

        if !in_quotes && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr { record: record_index, byte: i });
        }

        if in_quotes {
            if b == quote {
                if bytes.get(i + 1) == Some(&quote) {
                    field.push(quote as char);
                    i += 2;
                } else {
                    in_quotes = false;
                    after_quote = true;
                    i += 1;
                }
            } else {
                let ch = input[i..].chars().next().unwrap();
                field.push(ch);
                i += ch.len_utf8();
            }
            continue;
        }

        if after_quote {
            if b == delimiter {
                row.push(std::mem::take(&mut field));
                after_quote = false;
                field_started = false;
                row_started = true;
                i += 1;
            } else if b == b'\n' || b == b'\r' {
                row.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut row));
                record_index += 1;
                after_quote = false;
                field_started = false;
                row_started = false;
                i += if b == b'\r' { 2 } else { 1 };
            } else {
                return Err(CsvError::TrailingByte { record: record_index, byte: i });
            }
            continue;
        }

        if b == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote { record: record_index, byte: i });
            }
            in_quotes = true;
            opening_quote = i;
            field_started = true;
            row_started = true;
            i += 1;
        } else if b == delimiter {
            row.push(std::mem::take(&mut field));
            field_started = false;
            row_started = true;
            i += 1;
        } else if b == b'\n' || b == b'\r' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            record_index += 1;
            field_started = false;
            row_started = false;
            i += if b == b'\r' { 2 } else { 1 };
        } else {
            let ch = input[i..].chars().next().unwrap();
            field.push(ch);
            field_started = true;
            row_started = true;
            i += ch.len_utf8();
        }
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote { record: record_index, byte: opening_quote });
    }
    if row_started || field_started || !row.is_empty() || after_quote {
        row.push(field);
        records.push(row);
    }
    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with(input, b',', b'"')
}

fn first_duplicate(header: &[String]) -> Option<CsvError> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (second, name) in header.iter().enumerate() {
        if let Some(&first) = seen.get(name.as_str()) {
            return Some(CsvError::DuplicateHeader {
                name: name.clone(),
                first,
                second,
            });
        }
        seen.insert(name.as_str(), second);
    }
    None
}

fn parse_table(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| CsvError::InvalidConfig { component })?;
    let mut records = parse_with(input, config.delimiter, config.quote)?;

    let header = if config.headers && !records.is_empty() {
        Some(records.remove(0))
    } else {
        None
    };

    if let Some(ref h) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            if let Some(err) = first_duplicate(h) {
                return Err(err);
            }
        }
        for (idx, row) in records.iter().enumerate() {
            if row.len() != h.len() {
                return Err(CsvError::HeaderWidth {
                    record: idx + 1,
                    expected: h.len(),
                    actual: row.len(),
                });
            }
        }
    }

    Ok(Table { header, rows: records, duplicates: config.duplicates })
}

pub struct StreamReader {
    config: Config,
    input: String,
    cached: Option<Result<Table, CsvError>>,
}

impl StreamReader {
    pub fn new(config: Config) -> Result<Self, CsvError> {
        validate_dialect(config.delimiter, config.quote)
            .map_err(|component| CsvError::InvalidConfig { component })?;
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
        let result = parse_table(&self.input, self.config);
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
    if b.is_empty() { return false; }
    let digits = if b[0] == b'-' { &b[1..] } else { b };
    if digits.is_empty() { return false; }
    if digits == b"0" { return b[0] != b'-'; }
    digits[0].is_ascii_digit() && digits[0] != b'0' && digits.iter().all(u8::is_ascii_digit)
}

fn decimal_value(s: &str) -> Result<i64, CellErrorKind> {
    let b = s.as_bytes();
    let negative = b.first() == Some(&b'-');
    let start = usize::from(negative);
    if b.len() < start + 4 || b.get(b.len() - 3) != Some(&b'.') {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let int = &b[start..b.len() - 3];
    let frac = &b[b.len() - 2..];
    let valid_int = if int == b"0" {
        true
    } else {
        !int.is_empty()
            && int[0].is_ascii_digit()
            && int[0] != b'0'
            && int.iter().all(u8::is_ascii_digit)
    };
    if !valid_int || !frac.iter().all(u8::is_ascii_digit) {
        return Err(CellErrorKind::InvalidDecimal);
    }
    if negative && int == b"0" && frac == b"00" {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let mut value: i128 = 0;
    for &digit in int.iter().chain(frac.iter()) {
        if digit == b'.' { continue; }
        value = value * 10 + (digit - b'0') as i128;
    }
    if negative { value = -value; }
    i64::try_from(value).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn evaluate(s: &str, spec: &ColumnSpec) -> Result<Value, CellErrorKind> {
    if s.is_empty() {
        if spec.nullable { return Ok(Value::Null); }
        if spec.kind != ColumnType::Text { return Err(CellErrorKind::Empty); }
    }
    match spec.kind {
        ColumnType::Text => Ok(Value::Text(s.to_string())),
        ColumnType::Integer => {
            if !integer_syntax(s) { return Err(CellErrorKind::InvalidInteger); }
            s.parse::<i64>().map(Value::Integer).map_err(|_| CellErrorKind::IntegerOverflow)
        }
        ColumnType::Decimal => decimal_value(s).map(Value::Decimal),
        ColumnType::Boolean => match s {
            "yes" | "1" => Ok(Value::Boolean(true)),
            "no" | "0" => Ok(Value::Boolean(false)),
            _ => Err(CellErrorKind::InvalidBoolean),
        },
    }
}

pub fn extract_columns(table: &Table, columns: &[ColumnSpec], policy: ErrorPolicy) -> TypedResult {
    let mut result = TypedResult { rows: Vec::new(), errors: Vec::new() };
    'rows: for (row_idx, row) in table.rows.iter().enumerate() {
        let mut out = Vec::with_capacity(columns.len());
        for (spec_idx, spec) in columns.iter().enumerate() {
            let resolved = match &spec.column {
                ColumnRef::Index(i) => Some(*i),
                ColumnRef::Name(name) => table.column_index(name),
            };
            let (column, value, error) = match resolved {
                None => (None, None, Some(CellErrorKind::UnknownColumn)),
                Some(index) => match row.get(index) {
                    None => (Some(index), None, Some(CellErrorKind::MissingCell)),
                    Some(cell) => match evaluate(cell, spec) {
                        Ok(v) => (Some(index), Some(v), None),
                        Err(e) => (Some(index), None, Some(e)),
                    },
                },
            };
            if let Some(kind) = error {
                result.errors.push(CellError { row: row_idx, spec: spec_idx, column, kind });
                out.push(None);
                match policy {
                    ErrorPolicy::Collect => {}
                    ErrorPolicy::FirstPerRow => {
                        out.resize(columns.len(), None);
                        break;
                    }
                    ErrorPolicy::Stop => {
                        out.resize(columns.len(), None);
                        result.rows.push(out);
                        break 'rows;
                    }
                }
            } else {
                out.push(value);
            }
        }
        result.rows.push(out);
    }
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    Preserve,
    Canonical,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteConfig {
    pub delimiter: u8,
    pub quote: u8,
    pub line_ending: LineEnding,
    pub final_terminator: bool,
    pub mode: WriteMode,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    InvalidConfig { component: ConfigPart },
    Nul { record: usize, column: usize, byte: usize },
    Width { record: usize, expected: usize, actual: usize },
    ZeroColumns { record: usize },
    DuplicateHeader { name: String, first: usize, second: usize },
}

fn cmp_rows(a: &Vec<String>, b: &Vec<String>) -> std::cmp::Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        let c = x.as_bytes().cmp(y.as_bytes());
        if c != std::cmp::Ordering::Equal { return c; }
    }
    a.len().cmp(&b.len())
}

fn needs_quote(field: &str, delimiter: u8, quote: u8, first_field_first_record: bool, sole_empty: bool) -> bool {
    let bytes = field.as_bytes();
    bytes.contains(&delimiter)
        || bytes.contains(&quote)
        || bytes.contains(&b'\r')
        || bytes.contains(&b'\n')
        || bytes.first().is_some_and(|b| *b == b' ' || *b == b'\t')
        || bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t')
        || (first_field_first_record && field.starts_with('\u{feff}'))
        || (sole_empty && field.is_empty())
}

fn encode_field(field: &str, delimiter: u8, quote: u8, first_field_first_record: bool, sole_empty: bool) -> String {
    if !needs_quote(field, delimiter, quote, first_field_first_record, sole_empty) {
        return field.to_string();
    }
    let q = quote as char;
    let mut out = String::new();
    out.push(q);
    for ch in field.chars() {
        if ch as u32 == quote as u32 { out.push(q); }
        out.push(ch);
    }
    out.push(q);
    out
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let mut originals: Vec<&Vec<String>> = Vec::new();
    if let Some(h) = &table.header { originals.push(h); }
    originals.extend(table.rows.iter());

    let mut record_base = if table.header.is_some() { 1usize } else { 0usize };
    for (record, row) in originals.iter().enumerate() {
        for (column, field) in row.iter().enumerate() {
            if let Some(byte) = field.as_bytes().iter().position(|b| *b == 0) {
                return Err(WriteError::Nul { record, column, byte });
            }
        }
    }

    if let Some(header) = &table.header {
        for (idx, row) in table.rows.iter().enumerate() {
            if row.len() != header.len() {
                return Err(WriteError::Width {
                    record: idx + 1,
                    expected: header.len(),
                    actual: row.len(),
                });
            }
        }
    }

    for (record, row) in originals.iter().enumerate() {
        if row.is_empty() {
            return Err(WriteError::ZeroColumns { record });
        }
    }

    if config.mode == WriteMode::Canonical
        && table.header.is_some()
        && table.duplicates == DuplicatePolicy::Reject
    {
        if let Some(CsvError::DuplicateHeader { name, first, second }) =
            first_duplicate(table.header.as_ref().unwrap())
        {
            return Err(WriteError::DuplicateHeader { name, first, second });
        }
    }

    let mut output_records: Vec<Vec<String>> = Vec::new();
    match config.mode {
        WriteMode::Preserve => {
            if let Some(header) = &table.header { output_records.push(header.clone()); }
            output_records.extend(table.rows.clone());
        }
        WriteMode::Canonical => {
            if let Some(header) = &table.header {
                let mut chosen: HashMap<&str, usize> = HashMap::new();
                for (i, name) in header.iter().enumerate() {
                    match table.duplicates {
                        DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                            chosen.entry(name.as_str()).or_insert(i);
                        }
                        DuplicatePolicy::KeepLast => {
                            chosen.insert(name.as_str(), i);
                        }
                    }
                }
                let mut cols: Vec<(String, usize)> = chosen
                    .into_iter()
                    .map(|(name, idx)| (name.to_string(), idx))
                    .collect();
                cols.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
                output_records.push(cols.iter().map(|(name, _)| name.clone()).collect());
                let indexes: Vec<usize> = cols.iter().map(|(_, i)| *i).collect();
                let mut rows: Vec<Vec<String>> = table.rows.iter()
                    .map(|row| indexes.iter().map(|&i| row[i].clone()).collect())
                    .collect();
                rows.sort_by(cmp_rows);
                output_records.extend(rows);
            } else {
                let mut rows = table.rows.clone();
                rows.sort_by(cmp_rows);
                output_records.extend(rows);
            }
        }
    }

    if output_records.is_empty() { return Ok(String::new()); }

    let sep = match config.mode {
        WriteMode::Canonical => "\n",
        WriteMode::Preserve => match config.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        },
    };

    let mut encoded_records = Vec::with_capacity(output_records.len());
    for (ri, row) in output_records.iter().enumerate() {
        let sole_empty = row.len() == 1 && row[0].is_empty();
        let mut fields = Vec::with_capacity(row.len());
        for (ci, field) in row.iter().enumerate() {
            fields.push(encode_field(
                field,
                config.delimiter,
                config.quote,
                ri == 0 && ci == 0,
                sole_empty,
            ));
        }
        encoded_records.push(fields.join(&(config.delimiter as char).to_string()));
    }

    let mut out = encoded_records.join(sep);
    if config.mode == WriteMode::Canonical || config.final_terminator {
        out.push_str(sep);
    }
    let _ = record_base;
    Ok(out)
}