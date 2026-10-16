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
            DuplicatePolicy::KeepLast => header.iter().rposition(|value| value == name),
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|value| value == name)
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

fn parse_dialect(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut i = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };
    if i == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut quote_start = 0usize;
    let mut record_index = 0usize;
    let mut field_started = false;

    while i < bytes.len() {
        let b = bytes[i];

        // NUL has precedence in every parser state.
        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if in_quotes {
            if b == quote {
                // An adjacent pair is always an escape, including at EOF.
                if i + 1 < bytes.len() && bytes[i + 1] == quote {
                    field.push(quote as char);
                    i += 2;
                } else {
                    in_quotes = false;
                    after_quote = true;
                    i += 1;
                }
            } else {
                // Copy complete UTF-8 characters, preserving their bytes.
                let ch = input[i..].chars().next().unwrap();
                field.push(ch);
                i += ch.len_utf8();
            }
            continue;
        }

        // Outside quotes, validate CR before checking post-quote syntax.
        if b == b'\r' && (i + 1 >= bytes.len() || bytes[i + 1] != b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
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
            return Err(CsvError::TrailingByte {
                record: record_index,
                byte: i,
            });
        }

        if b == delimiter {
            row.push(std::mem::take(&mut field));
            field_started = false;
            i += 1;
        } else if b == b'\n' || b == b'\r' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            record_index += 1;
            field_started = false;
            i += if b == b'\r' { 2 } else { 1 };
        } else if b == quote {
            if !field_started {
                in_quotes = true;
                quote_start = i;
                field_started = true;
                i += 1;
            } else {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: i,
                });
            }
        } else {
            let ch = input[i..].chars().next().unwrap();
            field.push(ch);
            field_started = true;
            i += ch.len_utf8();
        }
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_start,
        });
    }

    if field_started || !row.is_empty() || !field.is_empty() || after_quote {
        row.push(field);
        records.push(row);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_dialect(input, b',', b'"')
}

fn duplicate_header_error(names: &[String]) -> Option<CsvError> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (second, name) in names.iter().enumerate() {
        if let Some(&first) = seen.get(name.as_str()) {
            return Some(CsvError::DuplicateHeader {
                name: name.clone(),
                first,
                second,
            });
        }
        seen.insert(name, second);
    }
    None
}

fn parse_table(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| CsvError::InvalidConfig { component })?;

    let mut records = parse_dialect(input, config.delimiter, config.quote)?;
    let header = if config.headers && !records.is_empty() {
        Some(records.remove(0))
    } else {
        None
    };

    if let Some(names) = &header {
        if config.duplicates == DuplicatePolicy::Reject {
            if let Some(error) = duplicate_header_error(names) {
                return Err(error);
            }
        }
        for (index, row) in records.iter().enumerate() {
            if row.len() != names.len() {
                return Err(CsvError::HeaderWidth {
                    record: index + 1,
                    expected: names.len(),
                    actual: row.len(),
                });
            }
        }
    }

    Ok(Table {
        header,
        rows: records,
        duplicates: config.duplicates,
    })
}

pub struct StreamReader {
    config: Config,
    input: String,
    result: Option<Result<Table, CsvError>>,
}

impl StreamReader {
    pub fn new(config: Config) -> Result<Self, CsvError> {
        validate_dialect(config.delimiter, config.quote)
            .map_err(|component| CsvError::InvalidConfig { component })?;
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
        let result = parse_table(&self.input, self.config);
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
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let start = usize::from(bytes[0] == b'-');
    if start == bytes.len() {
        return false;
    }
    if bytes[start] == b'0' {
        return start == 0 && bytes.len() == 1;
    }
    (b'1'..=b'9').contains(&bytes[start])
        && bytes[start + 1..].iter().all(u8::is_ascii_digit)
}

fn decimal_syntax(s: &str) -> bool {
    let bytes = s.as_bytes();
    let start = usize::from(bytes.first() == Some(&b'-'));
    if start >= bytes.len() {
        return false;
    }

    let dot = match bytes[start..].iter().position(|&b| b == b'.') {
        Some(position) => start + position,
        None => return false,
    };
    if dot + 3 != bytes.len() || !bytes[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }

    let integral = &bytes[start..dot];
    if integral == b"0" {
        return start == 0;
    }
    !integral.is_empty()
        && (b'1'..=b'9').contains(&integral[0])
        && integral[1..].iter().all(u8::is_ascii_digit)
}

fn evaluate(value: &str, spec: &ColumnSpec) -> Result<Value, CellErrorKind> {
    if value.is_empty() {
        if spec.nullable {
            return Ok(Value::Null);
        }
        if spec.kind != ColumnType::Text {
            return Err(CellErrorKind::Empty);
        }
    }

    match spec.kind {
        ColumnType::Text => Ok(Value::Text(value.to_string())),
        ColumnType::Integer => {
            if !integer_syntax(value) {
                return Err(CellErrorKind::InvalidInteger);
            }
            value
                .parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| CellErrorKind::IntegerOverflow)
        }
        ColumnType::Decimal => {
            if !decimal_syntax(value) {
                return Err(CellErrorKind::InvalidDecimal);
            }
            let negative = value.starts_with('-');
            let unsigned = if negative { &value[1..] } else { value };
            let (whole, fraction) = unsigned.split_once('.').unwrap();
            let whole = whole.parse::<i128>().unwrap();
            let fraction = fraction.parse::<i128>().unwrap();
            let magnitude = whole * 100 + fraction;
            let signed = if negative { -magnitude } else { magnitude };
            i64::try_from(signed)
                .map(Value::Decimal)
                .map_err(|_| CellErrorKind::DecimalOverflow)
        }
        ColumnType::Boolean => match value {
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
    let mut result = TypedResult {
        rows: Vec::new(),
        errors: Vec::new(),
    };

    for (row_index, row) in table.rows.iter().enumerate() {
        let mut output = vec![None; columns.len()];
        let mut row_failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if row_failed {
                break;
            }

            let column = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let (resolved_column, value) = match column {
                None => (None, Err(CellErrorKind::UnknownColumn)),
                Some(index) => match row.get(index) {
                    None => (Some(index), Err(CellErrorKind::MissingCell)),
                    Some(cell) => (Some(index), evaluate(cell, spec)),
                },
            };

            match value {
                Ok(value) => output[spec_index] = Some(value),
                Err(kind) => {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: resolved_column,
                        kind,
                    });
                    if policy != ErrorPolicy::Collect {
                        row_failed = true;
                    }
                }
            }
        }

        result.rows.push(output);
        if policy == ErrorPolicy::Stop && !result.errors.is_empty() {
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

fn first_nul(row: &[String]) -> Option<(usize, usize)> {
    for (column, field) in row.iter().enumerate() {
        if let Some(byte) = field.as_bytes().iter().position(|&b| b == 0) {
            return Some((column, byte));
        }
    }
    None
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

fn encode_field(
    field: &str,
    delimiter: u8,
    quote: u8,
    first_output_field: bool,
    sole_empty: bool,
) -> String {
    let bytes = field.as_bytes();
    let needs_quotes = bytes.contains(&delimiter)
        || bytes.contains(&quote)
        || bytes.contains(&b'\r')
        || bytes.contains(&b'\n')
        || bytes.first().is_some_and(|&b| b == b' ' || b == b'\t')
        || bytes.last().is_some_and(|&b| b == b' ' || b == b'\t')
        || (first_output_field && field.starts_with('\u{feff}'))
        || (sole_empty && field.is_empty());

    if !needs_quotes {
        return field.to_string();
    }

    let mut output = String::new();
    output.push(quote as char);
    for ch in field.chars() {
        if ch as u32 == quote as u32 {
            output.push(quote as char);
        }
        output.push(ch);
    }
    output.push(quote as char);
    output
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let mut original_records: Vec<&[String]> = Vec::new();
    if let Some(header) = &table.header {
        original_records.push(header);
    }
    original_records.extend(table.rows.iter().map(Vec::as_slice));

    for (record, row) in original_records.iter().enumerate() {
        if let Some((column, byte)) = first_nul(row) {
            return Err(WriteError::Nul {
                record,
                column,
                byte,
            });
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

    for (record, row) in original_records.iter().enumerate() {
        if row.is_empty() {
            return Err(WriteError::ZeroColumns { record });
        }
    }

    if config.mode == WriteMode::Canonical {
        if let Some(header) = &table.header {
            if table.duplicates == DuplicatePolicy::Reject {
                if let Some(CsvError::DuplicateHeader {
                    name,
                    first,
                    second,
                }) = duplicate_header_error(header)
                {
                    return Err(WriteError::DuplicateHeader {
                        name,
                        first,
                        second,
                    });
                }
            }
        }
    }

    let (rows, canonical) = match config.mode {
        WriteMode::Preserve => {
            let mut rows = Vec::new();
            if let Some(header) = &table.header {
                rows.push(header.clone());
            }
            rows.extend(table.rows.iter().cloned());
            (rows, false)
        }
        WriteMode::Canonical => {
            let mut rows = Vec::new();
            if let Some(header) = &table.header {
                let mut selected: HashMap<&str, usize> = HashMap::new();
                for (index, name) in header.iter().enumerate() {
                    match table.duplicates {
                        DuplicatePolicy::KeepLast => {
                            selected.insert(name, index);
                        }
                        DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                            selected.entry(name).or_insert(index);
                        }
                    }
                }

                let mut columns: Vec<(&str, usize)> = selected.into_iter().collect();
                columns.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));

                rows.push(
                    columns
                        .iter()
                        .map(|(name, _)| (*name).to_string())
                        .collect(),
                );
                for row in &table.rows {
                    rows.push(
                        columns
                            .iter()
                            .map(|(_, index)| row[*index].clone())
                            .collect(),
                    );
                }
            } else {
                rows = table.rows.clone();
            }

            let data_start = usize::from(table.header.is_some());
            rows[data_start..].sort_by(|a, b| compare_rows(a, b));
            (rows, true)
        }
    };

    if rows.is_empty() {
        return Ok(String::new());
    }

    let ending = if canonical {
        "\n"
    } else {
        match config.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    };
    let final_terminator = canonical || config.final_terminator;
    let mut output = String::new();

    for (record_index, row) in rows.iter().enumerate() {
        if record_index > 0 {
            output.push_str(ending);
        }
        for (column_index, field) in row.iter().enumerate() {
            if column_index > 0 {
                output.push(config.delimiter as char);
            }
            let first = record_index == 0 && column_index == 0;
            output.push_str(&encode_field(
                field,
                config.delimiter,
                config.quote,
                first,
                row.len() == 1,
            ));
        }
    }

    if final_terminator {
        output.push_str(ending);
    }
    Ok(output)
}