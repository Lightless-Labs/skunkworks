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
            DuplicatePolicy::KeepLast => header.iter().rposition(|item| item == name),
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|item| item == name)
            }
        }
    }
}

fn valid_config(delimiter: u8, quote: u8) -> Result<(), ConfigPart> {
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

fn parse_dialect(
    input: &str,
    delimiter: u8,
    quote: u8,
) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut i = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) { 3 } else { 0 };
    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut record = 0usize;
    let mut field_started = false;
    let mut quoted = false;
    let mut after_quote = false;
    let mut quote_open = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        if b == 0 {
            return Err(CsvError::Nul { record, byte: i });
        }

        if quoted {
            if b == quote {
                if i + 1 < bytes.len() && bytes[i + 1] == quote {
                    field.push(quote as char);
                    i += 2;
                } else {
                    quoted = false;
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

        if b == b'\r' && (i + 1 >= bytes.len() || bytes[i + 1] != b'\n') {
            return Err(CsvError::BareCr { record, byte: i });
        }

        if after_quote {
            if b == delimiter {
                row.push(std::mem::take(&mut field));
                field_started = false;
                after_quote = false;
                i += 1;
            } else if b == b'\n' || b == b'\r' {
                row.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut row));
                field_started = false;
                after_quote = false;
                record += 1;
                i += if b == b'\r' { 2 } else { 1 };
            } else {
                return Err(CsvError::TrailingByte { record, byte: i });
            }
            continue;
        }

        if b == delimiter {
            row.push(std::mem::take(&mut field));
            field_started = false;
            i += 1;
        } else if b == b'\n' || b == b'\r' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            field_started = false;
            record += 1;
            i += if b == b'\r' { 2 } else { 1 };
        } else if b == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote { record, byte: i });
            }
            quoted = true;
            field_started = true;
            quote_open = i;
            i += 1;
        } else {
            let ch = input[i..].chars().next().unwrap();
            field.push(ch);
            field_started = true;
            i += ch.len_utf8();
        }
    }

    if quoted {
        return Err(CsvError::UnclosedQuote {
            record,
            byte: quote_open,
        });
    }

    if !row.is_empty() || !field.is_empty() || field_started || after_quote {
        row.push(field);
        records.push(row);
    }
    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_dialect(input, b',', b'"')
}

fn build_table(
    records: Vec<Vec<String>>,
    config: Config,
) -> Result<Table, CsvError> {
    let (header, rows) = if config.headers {
        if records.is_empty() {
            (None, Vec::new())
        } else {
            let mut iter = records.into_iter();
            (iter.next(), iter.collect())
        }
    } else {
        (None, records)
    };

    if let Some(header) = &header {
        if config.duplicates == DuplicatePolicy::Reject {
            let mut seen: HashMap<&str, usize> = HashMap::new();
            for (column, name) in header.iter().enumerate() {
                if let Some(&first) = seen.get(name.as_str()) {
                    return Err(CsvError::DuplicateHeader {
                        name: name.clone(),
                        first,
                        second: column,
                    });
                }
                seen.insert(name, column);
            }
        }
        for (index, row) in rows.iter().enumerate() {
            if row.len() != header.len() {
                return Err(CsvError::HeaderWidth {
                    record: index + 1,
                    expected: header.len(),
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

pub struct StreamReader {
    config: Config,
    input: String,
    cached: Option<Result<Table, CsvError>>,
}

impl StreamReader {
    pub fn new(config: Config) -> Result<Self, CsvError> {
        valid_config(config.delimiter, config.quote)
            .map_err(|component| CsvError::InvalidConfig { component })?;
        Ok(Self {
            config,
            input: String::new(),
            cached: None,
        })
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
        let result = parse_dialect(&self.input, self.config.delimiter, self.config.quote)
            .and_then(|records| build_table(records, self.config));
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
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let start = usize::from(bytes[0] == b'-');
    if start == bytes.len() {
        return false;
    }
    if bytes[start] == b'0' {
        return start + 1 == bytes.len();
    }
    (b'1'..=b'9').contains(&bytes[start])
        && bytes[start + 1..].iter().all(u8::is_ascii_digit)
}

fn decimal_syntax(s: &str) -> bool {
    let bytes = s.as_bytes();
    let start = usize::from(bytes.first() == Some(&b'-'));
    let Some(dot) = bytes.iter().position(|&b| b == b'.') else {
        return false;
    };
    if dot < start || bytes.len() != dot + 3 {
        return false;
    }
    let integral = &bytes[start..dot];
    let integral_ok = if integral == b"0" {
        true
    } else {
        !integral.is_empty()
            && (b'1'..=b'9').contains(&integral[0])
            && integral[1..].iter().all(u8::is_ascii_digit)
    };
    integral_ok && bytes[dot + 1..].iter().all(u8::is_ascii_digit)
}

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    if !integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>().map_err(|_| CellErrorKind::IntegerOverflow)
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !decimal_syntax(s) || s == "-0.00" {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let negative = s.starts_with('-');
    let unsigned = if negative { &s[1..] } else { s };
    let dot = unsigned.find('.').unwrap();
    let whole = unsigned[..dot]
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let frac = unsigned[dot + 1..].parse::<i128>().unwrap();
    let magnitude = whole * 100 + frac;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

pub fn extract_columns(
    table: &Table,
    columns: &[ColumnSpec],
    policy: ErrorPolicy,
) -> TypedResult {
    let mut result = TypedResult {
        rows: Vec::new(),
        errors: Vec::new(),
    };

    for (row_index, row) in table.rows.iter().enumerate() {
        let mut output = Vec::with_capacity(columns.len());
        let mut failed = false;
        for (spec_index, spec) in columns.iter().enumerate() {
            if failed {
                output.push(None);
                continue;
            }

            let column = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let Some(column_index) = column else {
                output.push(None);
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: None,
                    kind: CellErrorKind::UnknownColumn,
                });
                failed = true;
                continue;
            };

            let Some(cell) = row.get(column_index) else {
                output.push(None);
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: Some(column_index),
                    kind: CellErrorKind::MissingCell,
                });
                failed = true;
                continue;
            };

            if cell.is_empty() {
                if spec.nullable {
                    output.push(Some(Value::Null));
                    continue;
                }
                if spec.kind != ColumnType::Text {
                    output.push(None);
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column_index),
                        kind: CellErrorKind::Empty,
                    });
                    failed = true;
                    continue;
                }
            }

            let value = match spec.kind {
                ColumnType::Text => Ok(Value::Text(cell.clone())),
                ColumnType::Integer => parse_integer(cell).map(Value::Integer),
                ColumnType::Decimal => parse_decimal(cell).map(Value::Decimal),
                ColumnType::Boolean => match cell.as_str() {
                    "yes" | "1" => Ok(Value::Boolean(true)),
                    "no" | "0" => Ok(Value::Boolean(false)),
                    _ => Err(CellErrorKind::InvalidBoolean),
                },
            };

            match value {
                Ok(value) => output.push(Some(value)),
                Err(kind) => {
                    output.push(None);
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column_index),
                        kind,
                    });
                    failed = true;
                }
            }

            if failed && policy != ErrorPolicy::Collect {
                for _ in spec_index + 1..columns.len() {
                    output.push(None);
                }
                break;
            }
        }

        result.rows.push(output);
        if failed && policy == ErrorPolicy::Stop {
            break;
        }
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
    Nul {
        record: usize,
        column: usize,
        byte: usize,
    },
    Width {
        record: usize,
        expected: usize,
        actual: usize,
    },
    ZeroColumns { record: usize },
    DuplicateHeader {
        name: String,
        first: usize,
        second: usize,
    },
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    valid_config(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let mut records: Vec<&Vec<String>> = Vec::new();
    if let Some(header) = &table.header {
        records.push(header);
    }
    records.extend(table.rows.iter());

    for (record, row) in records.iter().enumerate() {
        for (column, field) in row.iter().enumerate() {
            if let Some(byte) = field.as_bytes().iter().position(|&b| b == 0) {
                return Err(WriteError::Nul {
                    record,
                    column,
                    byte,
                });
            }
        }
    }

    if let Some(header) = &table.header {
        for (index, row) in table.rows.iter().enumerate() {
            if row.len() != header.len() {
                return Err(WriteError::Width {
                    record: index + 1,
                    expected: header.len(),
                    actual: row.len(),
                });
            }
        }
    }

    for (record, row) in records.iter().enumerate() {
        if row.is_empty() {
            return Err(WriteError::ZeroColumns { record });
        }
    }

    let mut owned_records: Vec<Vec<String>> = records.iter().map(|r| (*r).clone()).collect();

    if config.mode == WriteMode::Canonical {
        if let Some(header) = &table.header {
            if table.duplicates == DuplicatePolicy::Reject {
                let mut seen: HashMap<&str, usize> = HashMap::new();
                for (column, name) in header.iter().enumerate() {
                    if let Some(&first) = seen.get(name.as_str()) {
                        return Err(WriteError::DuplicateHeader {
                            name: name.clone(),
                            first,
                            second: column,
                        });
                    }
                    seen.insert(name, column);
                }
            }

            let mut chosen: HashMap<&str, usize> = HashMap::new();
            for (index, name) in header.iter().enumerate() {
                match table.duplicates {
                    DuplicatePolicy::KeepLast => {
                        chosen.insert(name, index);
                    }
                    DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                        chosen.entry(name).or_insert(index);
                    }
                }
            }
            let mut columns: Vec<(&str, usize)> =
                chosen.into_iter().map(|(name, index)| (name, index)).collect();
            columns.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

            let canonical_header: Vec<String> =
                columns.iter().map(|(name, _)| (*name).to_string()).collect();
            let mut projected_rows = Vec::with_capacity(table.rows.len());
            for row in &table.rows {
                projected_rows.push(
                    columns
                        .iter()
                        .map(|(_, index)| row[*index].clone())
                        .collect::<Vec<_>>(),
                );
            }
            projected_rows.sort_by(|a, b| compare_rows(a, b));
            owned_records.clear();
            owned_records.push(canonical_header);
            owned_records.extend(projected_rows);
        } else {
            let mut sorted = table.rows.clone();
            sorted.sort_by(|a, b| compare_rows(a, b));
            owned_records = sorted;
        }
    }

    if owned_records.is_empty() {
        return Ok(String::new());
    }

    let mut out = String::new();
    let separator = match (config.mode, config.line_ending) {
        (WriteMode::Canonical, _) | (_, LineEnding::Lf) => "\n",
        (_, LineEnding::CrLf) => "\r\n",
    };

    for (record_index, row) in owned_records.iter().enumerate() {
        if record_index > 0 {
            out.push_str(separator);
        }
        for (column_index, field) in row.iter().enumerate() {
            if column_index > 0 {
                out.push(config.delimiter as char);
            }
            let first_bom = record_index == 0
                && column_index == 0
                && field.starts_with('\u{feff}');
            let quote_field = field.contains(config.delimiter as char)
                || field.contains(config.quote as char)
                || field.contains('\r')
                || field.contains('\n')
                || field.starts_with(' ')
                || field.starts_with('\t')
                || field.ends_with(' ')
                || field.ends_with('\t')
                || first_bom
                || (row.len() == 1 && field.is_empty());

            if quote_field {
                out.push(config.quote as char);
                for ch in field.chars() {
                    if ch as u32 == config.quote as u32 {
                        out.push(config.quote as char);
                    }
                    out.push(ch);
                }
                out.push(config.quote as char);
            } else {
                out.push_str(field);
            }
        }
    }

    let append_final = match config.mode {
        WriteMode::Canonical => true,
        WriteMode::Preserve => config.final_terminator,
    };
    if append_final {
        out.push_str(separator);
    }
    Ok(out)
}

fn compare_rows(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (left, right) in a.iter().zip(b.iter()) {
        let ordering = left.as_bytes().cmp(right.as_bytes());
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }
    a.len().cmp(&b.len())
}