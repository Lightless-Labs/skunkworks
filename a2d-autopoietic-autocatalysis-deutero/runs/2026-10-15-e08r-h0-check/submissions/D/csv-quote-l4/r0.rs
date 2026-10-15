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
    let mut pos = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) { 3 } else { 0 };
    let mut records = Vec::new();

    if pos == bytes.len() {
        return Ok(records);
    }

    let mut record_index = 0usize;
    let mut row = Vec::new();
    let mut field = String::new();
    let mut state = 0u8; // 0: field start, 1: unquoted, 2: quoted, 3: after quote
    let mut opening_quote = 0usize;

    while pos < bytes.len() {
        let b = bytes[pos];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: pos,
            });
        }

        if state != 2 && b == b'\r' && bytes.get(pos + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: pos,
            });
        }

        if state == 2 {
            if b == quote {
                if bytes.get(pos + 1) == Some(&quote) {
                    field.push(quote as char);
                    pos += 2;
                } else {
                    state = 3;
                    pos += 1;
                }
            } else {
                let ch = input[pos..].chars().next().unwrap();
                field.push(ch);
                pos += ch.len_utf8();
            }
            continue;
        }

        if state == 3 {
            if b == delimiter {
                row.push(std::mem::take(&mut field));
                state = 0;
                pos += 1;
            } else if b == b'\n' {
                row.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut row));
                record_index += 1;
                state = 0;
                pos += 1;
            } else if b == b'\r' {
                row.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut row));
                record_index += 1;
                state = 0;
                pos += 2;
            } else {
                return Err(CsvError::TrailingByte {
                    record: record_index,
                    byte: pos,
                });
            }
            continue;
        }

        if b == delimiter {
            row.push(std::mem::take(&mut field));
            state = 0;
            pos += 1;
        } else if b == b'\n' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            record_index += 1;
            state = 0;
            pos += 1;
        } else if b == b'\r' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            record_index += 1;
            state = 0;
            pos += 2;
        } else if b == quote {
            if state == 0 {
                opening_quote = pos;
                state = 2;
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
            state = 1;
            pos += ch.len_utf8();
        }
    }

    if state == 2 {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: opening_quote,
        });
    }

    if !row.is_empty() || !field.is_empty() || state == 3 || state == 1 {
        row.push(field);
        records.push(row);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn duplicate_header_error(header: &[String]) -> Option<CsvError> {
    let mut seen = std::collections::HashMap::<&str, usize>::new();
    for (second, name) in header.iter().enumerate() {
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

fn table_from_records(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    if !config.headers {
        return Ok(Table {
            header: None,
            rows: records,
            duplicates: config.duplicates,
        });
    }

    let mut records = records.into_iter();
    let Some(header) = records.next() else {
        return Ok(Table {
            header: None,
            rows: Vec::new(),
            duplicates: config.duplicates,
        });
    };

    if config.duplicates == DuplicatePolicy::Reject {
        if let Some(error) = duplicate_header_error(&header) {
            return Err(error);
        }
    }

    let rows: Vec<Vec<String>> = records.collect();
    for (offset, row) in rows.iter().enumerate() {
        if row.len() != header.len() {
            return Err(CsvError::HeaderWidth {
                record: offset + 1,
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
        validate_dialect(config.delimiter, config.quote)
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
        return start + 1 == b.len();
    }
    if !(b'1'..=b'9').contains(&b[start]) {
        return false;
    }
    b[start + 1..].iter().all(u8::is_ascii_digit)
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    let start = usize::from(b.first() == Some(&b'-'));
    if start >= b.len() {
        return false;
    }
    let dot = match b[start..].iter().position(|&b| b == b'.') {
        Some(offset) => start + offset,
        None => return false,
    };
    if dot + 3 != b.len() || !b[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    let integral = &b[start..dot];
    if integral.is_empty() {
        return false;
    }
    if integral == b"0" {
        return true;
    }
    (b'1'..=b'9').contains(&integral[0])
        && integral[1..].iter().all(u8::is_ascii_digit)
}

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    if !integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>()
        .map_err(|_| CellErrorKind::IntegerOverflow)
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !decimal_syntax(s) {
        return Err(CellErrorKind::InvalidDecimal);
    }
    if s == "-0.00" {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let negative = s.starts_with('-');
    let body = if negative { &s[1..] } else { s };
    let dot = body.find('.').unwrap();
    let whole = body[..dot]
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let fraction = body[dot + 1..]
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let magnitude = whole
        .checked_mul(100)
        .and_then(|value| value.checked_add(fraction))
        .ok_or(CellErrorKind::DecimalOverflow)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn typed_value(cell: &str, spec: &ColumnSpec) -> Result<Value, CellErrorKind> {
    if cell.is_empty() {
        if spec.nullable {
            return Ok(Value::Null);
        }
        if spec.kind != ColumnType::Text {
            return Err(CellErrorKind::Empty);
        }
    }

    match spec.kind {
        ColumnType::Text => Ok(Value::Text(cell.to_string())),
        ColumnType::Integer => parse_integer(cell).map(Value::Integer),
        ColumnType::Decimal => parse_decimal(cell).map(Value::Decimal),
        ColumnType::Boolean => match cell {
            "yes" | "1" => Ok(Value::Boolean(true)),
            "no" | "0" => Ok(Value::Boolean(false)),
            _ => Err(CellErrorKind::InvalidBoolean),
        },
    }
}

pub fn extract_columns(table: &Table, columns: &[ColumnSpec], policy: ErrorPolicy) -> TypedResult {
    let mut result = TypedResult {
        rows: Vec::new(),
        errors: Vec::new(),
    };

    for (row_index, row) in table.rows.iter().enumerate() {
        let mut output = Vec::with_capacity(columns.len());
        let mut row_failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if row_failed {
                output.push(None);
                continue;
            }

            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let Some(column) = resolved else {
                output.push(None);
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: None,
                    kind: CellErrorKind::UnknownColumn,
                });
                row_failed = true;
                if policy == ErrorPolicy::Stop {
                    break;
                }
                continue;
            };

            let Some(cell) = row.get(column) else {
                output.push(None);
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: Some(column),
                    kind: CellErrorKind::MissingCell,
                });
                row_failed = true;
                if policy == ErrorPolicy::Stop {
                    break;
                }
                continue;
            };

            match typed_value(cell, spec) {
                Ok(value) => output.push(Some(value)),
                Err(kind) => {
                    output.push(None);
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column),
                        kind,
                    });
                    row_failed = true;
                    if policy == ErrorPolicy::Stop {
                        break;
                    }
                }
            }

            if row_failed && policy == ErrorPolicy::FirstPerRow {
                for _ in output.len()..columns.len() {
                    output.push(None);
                }
                break;
            }
        }

        result.rows.push(output);

        if row_failed && policy == ErrorPolicy::Stop {
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
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let mut records: Vec<&[String]> = Vec::new();
    if let Some(header) = &table.header {
        records.push(header);
    }
    records.extend(table.rows.iter().map(Vec::as_slice));

    for (record_index, record) in records.iter().enumerate() {
        for (column_index, cell) in record.iter().enumerate() {
            if let Some(byte) = cell.as_bytes().iter().position(|&b| b == 0) {
                return Err(WriteError::Nul {
                    record: record_index,
                    column: column_index,
                    byte,
                });
            }
        }
    }

    if let Some(header) = &table.header {
        for (offset, row) in table.rows.iter().enumerate() {
            if row.len() != header.len() {
                return Err(WriteError::Width {
                    record: offset + 1,
                    expected: header.len(),
                    actual: row.len(),
                });
            }
        }
    }

    for (record_index, record) in records.iter().enumerate() {
        if record.is_empty() {
            return Err(WriteError::ZeroColumns {
                record: record_index,
            });
        }
    }

    let mut output_records: Vec<Vec<String>> = match config.mode {
        WriteMode::Preserve => records.into_iter().map(|record| record.to_vec()).collect(),
        WriteMode::Canonical => {
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

                let mut retained = Vec::<(String, usize)>::new();
                for (index, name) in header.iter().enumerate() {
                    match table.duplicates {
                        DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                            if !retained.iter().any(|(seen, _)| seen == name) {
                                retained.push((name.clone(), index));
                            }
                        }
                        DuplicatePolicy::KeepLast => {
                            if !header[index + 1..].iter().any(|later| later == name) {
                                retained.push((name.clone(), index));
                            }
                        }
                    }
                }
                retained.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
                let indexes: Vec<usize> = retained.iter().map(|(_, index)| *index).collect();
                let sorted_header: Vec<String> =
                    retained.into_iter().map(|(name, _)| name).collect();

                let mut rows: Vec<Vec<String>> = table
                    .rows
                    .iter()
                    .map(|row| indexes.iter().map(|&index| row[index].clone()).collect())
                    .collect();
                rows.sort_by(|a, b| compare_rows(a, b));

                let mut all = Vec::with_capacity(rows.len() + 1);
                all.push(sorted_header);
                all.extend(rows);
                all
            } else {
                let mut rows = table.rows.clone();
                rows.sort_by(|a, b| compare_rows(a, b));
                rows
            }
        }
    };

    if output_records.is_empty() {
        return Ok(String::new());
    }

    let canonical = config.mode == WriteMode::Canonical;
    let line_ending: &[u8] = if canonical || config.line_ending == LineEnding::Lf {
        b"\n"
    } else {
        b"\r\n"
    };

    let mut out = Vec::new();
    for (record_index, record) in output_records.iter().enumerate() {
        if record_index > 0 {
            out.extend_from_slice(line_ending);
        }
        for (column_index, field) in record.iter().enumerate() {
            if column_index > 0 {
                out.push(config.delimiter);
            }

            let bytes = field.as_bytes();
            let sole_empty = record.len() == 1 && field.is_empty();
            let boundary_space = bytes.first().is_some_and(|b| *b == b' ' || *b == b'\t')
                || bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t');
            let initial_bom = record_index == 0
                && column_index == 0
                && field.starts_with('\u{feff}');
            let needs_quote = sole_empty
                || boundary_space
                || initial_bom
                || bytes.iter().any(|&b| {
                    b == config.delimiter || b == config.quote || b == b'\r' || b == b'\n'
                });

            if needs_quote {
                out.push(config.quote);
                for &b in bytes {
                    out.push(b);
                    if b == config.quote {
                        out.push(config.quote);
                    }
                }
                out.push(config.quote);
            } else {
                out.extend_from_slice(bytes);
            }
        }
    }

    if canonical || config.final_terminator {
        out.extend_from_slice(line_ending);
    }

    output_records.clear();
    Ok(String::from_utf8(out).unwrap())
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