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
            DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
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

fn parse_with_dialect(
    input: &str,
    delimiter: u8,
    quote: u8,
) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut pos = if input.starts_with('\u{feff}') { 3 } else { 0 };
    if pos == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut state = 0u8; // 0: field start, 1: unquoted, 2: quoted, 3: after quote
    let mut quote_start = 0usize;
    let mut record_index = 0usize;
    let mut record_started = false;

    while pos < bytes.len() {
        let b = bytes[pos];
        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: pos,
            });
        }

        if state != 2 && b == b'\r' {
            if bytes.get(pos + 1) != Some(&b'\n') {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: pos,
                });
            }
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            pos += 2;
            record_index += 1;
            state = 0;
            record_started = false;
            continue;
        }

        if state != 2 && b == b'\n' {
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            pos += 1;
            record_index += 1;
            state = 0;
            record_started = false;
            continue;
        }

        match state {
            0 => {
                if b == delimiter {
                    record.push(std::mem::take(&mut field));
                    record_started = true;
                    pos += 1;
                } else if b == quote {
                    state = 2;
                    quote_start = pos;
                    record_started = true;
                    pos += 1;
                } else {
                    let start = pos;
                    while pos < bytes.len()
                        && bytes[pos] != 0
                        && bytes[pos] != delimiter
                        && bytes[pos] != quote
                        && bytes[pos] != b'\r'
                        && bytes[pos] != b'\n'
                    {
                        pos += 1;
                    }
                    field.push_str(&input[start..pos]);
                    state = 1;
                    record_started = true;
                }
            }
            1 => {
                if b == delimiter {
                    record.push(std::mem::take(&mut field));
                    state = 0;
                    record_started = true;
                    pos += 1;
                } else if b == quote {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: pos,
                    });
                } else {
                    let start = pos;
                    while pos < bytes.len()
                        && bytes[pos] != 0
                        && bytes[pos] != delimiter
                        && bytes[pos] != quote
                        && bytes[pos] != b'\r'
                        && bytes[pos] != b'\n'
                    {
                        pos += 1;
                    }
                    field.push_str(&input[start..pos]);
                }
            }
            2 => {
                if b == quote {
                    if bytes.get(pos + 1) == Some(&quote) {
                        field.push(quote as char);
                        pos += 2;
                    } else {
                        state = 3;
                        pos += 1;
                    }
                } else {
                    let start = pos;
                    while pos < bytes.len() && bytes[pos] != 0 && bytes[pos] != quote {
                        pos += 1;
                    }
                    field.push_str(&input[start..pos]);
                }
            }
            3 => {
                if b == delimiter {
                    record.push(std::mem::take(&mut field));
                    state = 0;
                    record_started = true;
                    pos += 1;
                } else {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: pos,
                    });
                }
            }
            _ => unreachable!(),
        }
    }

    if state == 2 {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_start,
        });
    }

    if record_started || !record.is_empty() || !field.is_empty() || state == 3 {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn duplicate_header(header: &[String]) -> Option<CsvError> {
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

fn table_from_records(
    records: Vec<Vec<String>>,
    config: Config,
) -> Result<Table, CsvError> {
    if !config.headers {
        return Ok(Table {
            header: None,
            rows: records,
            duplicates: config.duplicates,
        });
    }

    let mut iter = records.into_iter();
    let Some(header) = iter.next() else {
        return Ok(Table {
            header: None,
            rows: Vec::new(),
            duplicates: config.duplicates,
        });
    };

    if config.duplicates == DuplicatePolicy::Reject {
        if let Some(error) = duplicate_header(&header) {
            return Err(error);
        }
    }

    let rows: Vec<Vec<String>> = iter.collect();
    for (index, row) in rows.iter().enumerate() {
        if row.len() != header.len() {
            return Err(CsvError::HeaderWidth {
                record: index + 1,
                expected: header.len(),
                actual: row.len(),
            });
        }
    }

    Ok(Table {
        header: Some(header),
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
        validate_dialect(config.delimiter, config.quote).map_err(|component| {
            CsvError::InvalidConfig { component }
        })?;
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
        let result = parse_with_dialect(&self.input, self.config.delimiter, self.config.quote)
            .and_then(|records| table_from_records(records, self.config));
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

fn valid_integer_syntax(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let digits = if bytes[0] == b'-' { &bytes[1..] } else { bytes };
    if digits.is_empty() {
        return false;
    }
    if digits == b"0" {
        return bytes[0] != b'-';
    }
    (b'1'..=b'9').contains(&digits[0]) && digits[1..].iter().all(u8::is_ascii_digit)
}

fn valid_decimal_syntax(s: &str) -> bool {
    let bytes = s.as_bytes();
    let digits = if bytes.first() == Some(&b'-') {
        &bytes[1..]
    } else {
        bytes
    };
    let Some(dot) = digits.iter().position(|&b| b == b'.') else {
        return false;
    };
    let whole = &digits[..dot];
    let frac = &digits[dot + 1..];
    if frac.len() != 2 || !frac.iter().all(u8::is_ascii_digit) || whole.is_empty() {
        return false;
    }
    if whole == b"0" {
        return bytes.first() != Some(&b'-') || frac != b"00";
    }
    (b'1'..=b'9').contains(&whole[0]) && whole[1..].iter().all(u8::is_ascii_digit)
}

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    if !valid_integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>().map_err(|_| CellErrorKind::IntegerOverflow)
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !valid_decimal_syntax(s) {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let negative = s.starts_with('-');
    let unsigned = if negative { &s[1..] } else { s };
    let (whole, fraction) = unsigned.split_once('.').unwrap();
    let magnitude = whole
        .parse::<i128>()
        .unwrap()
        .checked_mul(100)
        .and_then(|n| n.checked_add(fraction.parse::<i128>().unwrap()))
        .unwrap();
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
    let mut stopped = false;

    for (row_index, row) in table.rows.iter().enumerate() {
        if stopped {
            break;
        }
        let mut out = vec![None; columns.len()];
        let mut row_failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };
            let (column, cell) = match resolved {
                None => {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: None,
                        kind: CellErrorKind::UnknownColumn,
                    });
                    row_failed = true;
                    if policy == ErrorPolicy::Stop {
                        stopped = true;
                    }
                    if policy != ErrorPolicy::Collect {
                        break;
                    }
                    continue;
                }
                Some(index) => match row.get(index) {
                    None => {
                        result.errors.push(CellError {
                            row: row_index,
                            spec: spec_index,
                            column: Some(index),
                            kind: CellErrorKind::MissingCell,
                        });
                        row_failed = true;
                        if policy == ErrorPolicy::Stop {
                            stopped = true;
                        }
                        if policy != ErrorPolicy::Collect {
                            break;
                        }
                        continue;
                    }
                    Some(cell) => (index, cell),
                },
            };

            let value = if cell.is_empty() && spec.nullable {
                Ok(Value::Null)
            } else if cell.is_empty() && spec.kind != ColumnType::Text {
                Err(CellErrorKind::Empty)
            } else {
                match spec.kind {
                    ColumnType::Text => Ok(Value::Text(cell.clone())),
                    ColumnType::Integer => parse_integer(cell).map(Value::Integer),
                    ColumnType::Decimal => parse_decimal(cell).map(Value::Decimal),
                    ColumnType::Boolean => match cell.as_str() {
                        "yes" | "1" => Ok(Value::Boolean(true)),
                        "no" | "0" => Ok(Value::Boolean(false)),
                        _ => Err(CellErrorKind::InvalidBoolean),
                    },
                }
            };

            match value {
                Ok(value) => out[spec_index] = Some(value),
                Err(kind) => {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column),
                        kind,
                    });
                    row_failed = true;
                    if policy == ErrorPolicy::Stop {
                        stopped = true;
                    }
                    if policy != ErrorPolicy::Collect {
                        break;
                    }
                }
            }
        }

        result.rows.push(out);
        if policy == ErrorPolicy::FirstPerRow && row_failed {
            continue;
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

fn write_records(table: &Table) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    if let Some(header) = &table.header {
        records.push(header.clone());
    }
    records.extend(table.rows.iter().cloned());
    records
}

fn compare_rows(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (left, right) in a.iter().zip(b.iter()) {
        let order = left.as_bytes().cmp(right.as_bytes());
        if order != std::cmp::Ordering::Equal {
            return order;
        }
    }
    a.len().cmp(&b.len())
}

fn quote_field(field: &str, first_field_first_record: bool, sole_empty: bool, delimiter: u8, quote: u8) -> String {
    let bytes = field.as_bytes();
    let needs_quote = bytes.contains(&delimiter)
        || bytes.contains(&quote)
        || bytes.contains(&b'\r')
        || bytes.contains(&b'\n')
        || bytes.first().is_some_and(|b| *b == b' ' || *b == b'\t')
        || bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t')
        || (first_field_first_record && field.starts_with('\u{feff}'))
        || (sole_empty && field.is_empty());

    if !needs_quote {
        return field.to_string();
    }

    let mut out = String::with_capacity(field.len() + 2);
    out.push(quote as char);
    for ch in field.chars() {
        if ch as u32 == quote as u32 {
            out.push(quote as char);
        }
        out.push(ch);
    }
    out.push(quote as char);
    out
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let originals = write_records(table);
    for (record_index, record) in originals.iter().enumerate() {
        for (column, field) in record.iter().enumerate() {
            if let Some(byte) = field.as_bytes().iter().position(|&b| b == 0) {
                return Err(WriteError::Nul {
                    record: record_index,
                    column,
                    byte,
                });
            }
        }
    }

    if let Some(header) = &table.header {
        for (row_index, row) in table.rows.iter().enumerate() {
            if row.len() != header.len() {
                return Err(WriteError::Width {
                    record: row_index + 1,
                    expected: header.len(),
                    actual: row.len(),
                });
            }
        }
    }

    for (record_index, record) in originals.iter().enumerate() {
        if record.is_empty() {
            return Err(WriteError::ZeroColumns {
                record: record_index,
            });
        }
    }

    if config.mode == WriteMode::Canonical
        && table.header.is_some()
        && table.duplicates == DuplicatePolicy::Reject
    {
        if let Some(CsvError::DuplicateHeader {
            name,
            first,
            second,
        }) = duplicate_header(table.header.as_ref().unwrap())
        {
            return Err(WriteError::DuplicateHeader {
                name,
                first,
                second,
            });
        }
    }

    let mut output_records: Vec<Vec<String>>;
    if config.mode == WriteMode::Preserve {
        output_records = originals;
    } else if let Some(header) = &table.header {
        let mut retained: Vec<usize> = Vec::new();
        match table.duplicates {
            DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                let mut seen: HashMap<&str, usize> = HashMap::new();
                for (i, name) in header.iter().enumerate() {
                    if !seen.contains_key(name.as_str()) {
                        seen.insert(name.as_str(), i);
                        retained.push(i);
                    }
                }
            }
            DuplicatePolicy::KeepLast => {
                let mut last: HashMap<&str, usize> = HashMap::new();
                for (i, name) in header.iter().enumerate() {
                    last.insert(name.as_str(), i);
                }
                retained = last.values().copied().collect();
            }
        }
        retained.sort_by(|&a, &b| header[a].as_bytes().cmp(header[b].as_bytes()));

        let projected_header: Vec<String> =
            retained.iter().map(|&i| header[i].clone()).collect();
        let mut projected_rows: Vec<Vec<String>> = table
            .rows
            .iter()
            .map(|row| retained.iter().map(|&i| row[i].clone()).collect())
            .collect();
        projected_rows.sort_by(|a, b| compare_rows(a, b));
        output_records = vec![projected_header];
        output_records.extend(projected_rows);
    } else {
        let mut rows = table.rows.clone();
        rows.sort_by(|a, b| compare_rows(a, b));
        output_records = rows;
    }

    if output_records.is_empty() {
        return Ok(String::new());
    }

    let canonical = config.mode == WriteMode::Canonical;
    let terminator = if canonical || config.line_ending == LineEnding::Lf {
        "\n"
    } else {
        "\r\n"
    };
    let mut output = String::new();
    for (record_index, record) in output_records.iter().enumerate() {
        if record_index > 0 {
            output.push_str(terminator);
        }
        let sole_empty = record.len() == 1 && record[0].is_empty();
        for (column_index, field) in record.iter().enumerate() {
            if column_index > 0 {
                output.push(config.delimiter as char);
            }
            output.push_str(&quote_field(
                field,
                record_index == 0 && column_index == 0,
                sole_empty,
                config.delimiter,
                config.quote,
            ));
        }
    }
    if canonical || config.final_terminator {
        output.push_str(terminator);
    }
    Ok(output)
}