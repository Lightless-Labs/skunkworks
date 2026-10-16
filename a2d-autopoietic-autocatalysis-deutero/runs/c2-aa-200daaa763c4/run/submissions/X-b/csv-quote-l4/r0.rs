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
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|s| s == name)
            }
        }
    }
}

pub struct StreamReader {
    config: Config,
    buffer: String,
    result: Option<Result<Table, CsvError>>,
}

impl StreamReader {
    pub fn new(config: Config) -> Result<Self, CsvError> {
        validate_config(config.delimiter, config.quote)
            .map_err(|component| CsvError::InvalidConfig { component })?;
        Ok(Self {
            config,
            buffer: String::new(),
            result: None,
        })
    }

    pub fn feed(&mut self, chunk: &str) -> Result<(), CsvError> {
        if self.result.is_some() {
            return Err(CsvError::ReaderFinished);
        }
        self.buffer.push_str(chunk);
        Ok(())
    }

    pub fn finish(&mut self) -> Result<Table, CsvError> {
        if let Some(result) = &self.result {
            return result.clone();
        }
        let result = parse_dialect(&self.buffer, self.config.delimiter, self.config.quote)
            .and_then(|records| make_table(records, self.config));
        self.result = Some(result.clone());
        result
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.result = None;
    }
}

pub fn read_csv(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_config(config.delimiter, config.quote)
        .map_err(|component| CsvError::InvalidConfig { component })?;
    let mut reader = StreamReader {
        config,
        buffer: input.to_string(),
        result: None,
    };
    reader.finish()
}

fn validate_config(delimiter: u8, quote: u8) -> Result<(), ConfigPart> {
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
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    if i == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut state = 0u8; // 0: field start, 1: unquoted, 2: quoted, 3: after quote
    let mut opening = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        let record_index = records.len();

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if state != 2 && b == b'\r' {
            if i + 1 >= bytes.len() || bytes[i + 1] != b'\n' {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: i,
                });
            }
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            state = 0;
            i += 2;
            continue;
        }

        match state {
            0 => {
                if b == delimiter {
                    record.push(std::mem::take(&mut field));
                } else if b == quote {
                    opening = i;
                    state = 2;
                } else if b == b'\n' {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                } else {
                    let n = utf8_width(b);
                    field.push_str(std::str::from_utf8(&bytes[i..i + n]).unwrap());
                    state = 1;
                    i += n;
                    continue;
                }
            }
            1 => {
                if b == delimiter {
                    record.push(std::mem::take(&mut field));
                    state = 0;
                } else if b == b'\n' {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    state = 0;
                } else if b == quote {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: i,
                    });
                } else {
                    let n = utf8_width(b);
                    field.push_str(std::str::from_utf8(&bytes[i..i + n]).unwrap());
                    i += n;
                    continue;
                }
            }
            2 => {
                if b == quote {
                    if i + 1 < bytes.len() && bytes[i + 1] == quote {
                        field.push(quote as char);
                        i += 2;
                        continue;
                    }
                    state = 3;
                } else {
                    let n = utf8_width(b);
                    field.push_str(std::str::from_utf8(&bytes[i..i + n]).unwrap());
                    i += n;
                    continue;
                }
            }
            _ => {
                if b == delimiter {
                    record.push(std::mem::take(&mut field));
                    state = 0;
                } else if b == b'\n' {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    state = 0;
                } else {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: i,
                    });
                }
            }
        }
        i += 1;
    }

    if state == 2 {
        return Err(CsvError::UnclosedQuote {
            record: records.len(),
            byte: opening,
        });
    }

    if !record.is_empty() || !field.is_empty() || state != 0 {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}

fn utf8_width(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b < 0xe0 {
        2
    } else if b < 0xf0 {
        3
    } else {
        4
    }
}

fn duplicate_header(header: &[String]) -> Option<CsvError> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (i, name) in header.iter().enumerate() {
        if let Some(&first) = seen.get(name.as_str()) {
            return Some(CsvError::DuplicateHeader {
                name: name.clone(),
                first,
                second: i,
            });
        }
        seen.insert(name, i);
    }
    None
}

fn make_table(mut records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let header = if config.headers && !records.is_empty() {
        Some(records.remove(0))
    } else {
        None
    };

    if let Some(h) = &header {
        if config.duplicates == DuplicatePolicy::Reject {
            if let Some(err) = duplicate_header(h) {
                return Err(err);
            }
        }
        for (i, row) in records.iter().enumerate() {
            if row.len() != h.len() {
                return Err(CsvError::HeaderWidth {
                    record: i + 1,
                    expected: h.len(),
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

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_dialect(input, b',', b'"')
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

pub fn extract_columns(
    table: &Table,
    columns: &[ColumnSpec],
    policy: ErrorPolicy,
) -> TypedResult {
    let mut result = TypedResult {
        rows: Vec::new(),
        errors: Vec::new(),
    };

    'rows: for (ri, row) in table.rows.iter().enumerate() {
        let mut out = vec![None; columns.len()];
        for (si, spec) in columns.iter().enumerate() {
            let index = match &spec.column {
                ColumnRef::Index(i) => Some(*i),
                ColumnRef::Name(n) => table.column_index(n),
            };

            let (value, error) = match index {
                None => (
                    None,
                    Some(CellError {
                        row: ri,
                        spec: si,
                        column: None,
                        kind: CellErrorKind::UnknownColumn,
                    }),
                ),
                Some(ci) => match row.get(ci) {
                    None => (
                        None,
                        Some(CellError {
                            row: ri,
                            spec: si,
                            column: Some(ci),
                            kind: CellErrorKind::MissingCell,
                        }),
                    ),
                    Some(cell) => convert_cell(cell, spec, ri, si, ci),
                },
            };

            out[si] = value;
            if let Some(e) = error {
                result.errors.push(e);
                match policy {
                    ErrorPolicy::Collect => {}
                    ErrorPolicy::FirstPerRow => break,
                    ErrorPolicy::Stop => {
                        result.rows.push(out);
                        break 'rows;
                    }
                }
            }
        }
        result.rows.push(out);
    }

    result
}

fn convert_cell(
    cell: &str,
    spec: &ColumnSpec,
    row: usize,
    si: usize,
    ci: usize,
) -> (Option<Value>, Option<CellError>) {
    let error = |kind| {
        Some(CellError {
            row,
            spec: si,
            column: Some(ci),
            kind,
        })
    };

    if cell.is_empty() {
        if spec.nullable {
            return (Some(Value::Null), None);
        }
        if spec.kind != ColumnType::Text {
            return (None, error(CellErrorKind::Empty));
        }
    }

    match spec.kind {
        ColumnType::Text => (Some(Value::Text(cell.to_string())), None),
        ColumnType::Boolean => match cell {
            "yes" | "1" => (Some(Value::Boolean(true)), None),
            "no" | "0" => (Some(Value::Boolean(false)), None),
            _ => (None, error(CellErrorKind::InvalidBoolean)),
        },
        ColumnType::Integer => match parse_integer(cell) {
            Ok(v) => (Some(Value::Integer(v)), None),
            Err(kind) => (None, error(kind)),
        },
        ColumnType::Decimal => match parse_decimal(cell) {
            Ok(v) => (Some(Value::Decimal(v)), None),
            Err(kind) => (None, error(kind)),
        },
    }
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
        return start == b.len() - 1;
    }
    if !(b'1'..=b'9').contains(&b[start]) {
        return false;
    }
    b[start + 1..].iter().all(|c| c.is_ascii_digit())
}

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    if !integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse().map_err(|_| CellErrorKind::IntegerOverflow)
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    let start = usize::from(b.first() == Some(&b'-'));
    let dot = match b.iter().position(|&x| x == b'.') {
        Some(d) => d,
        None => return false,
    };
    if dot < start || b.len() != dot + 3 {
        return false;
    }
    let int = &b[start..dot];
    if int.is_empty() || !b[dot + 1..].iter().all(|c| c.is_ascii_digit()) {
        return false;
    }
    if int == b"0" {
        return start == 0 || &b[dot + 1..] != b"00";
    }
    (b'1'..=b'9').contains(&int[0]) && int[1..].iter().all(|c| c.is_ascii_digit())
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !decimal_syntax(s) {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let neg = s.starts_with('-');
    let digits = s.trim_start_matches('-').replace('.', "");
    let magnitude: i128 = digits
        .parse()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let signed = if neg { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
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
    ZeroColumns {
        record: usize,
    },
    DuplicateHeader {
        name: String,
        first: usize,
        second: usize,
    },
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_config(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let mut original: Vec<&Vec<String>> = Vec::new();
    if let Some(header) = &table.header {
        original.push(header);
    }
    original.extend(table.rows.iter());

    for (ri, rec) in original.iter().enumerate() {
        for (ci, cell) in rec.iter().enumerate() {
            if let Some(byte) = cell.as_bytes().iter().position(|&b| b == 0) {
                return Err(WriteError::Nul {
                    record: ri,
                    column: ci,
                    byte,
                });
            }
        }
    }

    if let Some(header) = &table.header {
        for (i, row) in table.rows.iter().enumerate() {
            if row.len() != header.len() {
                return Err(WriteError::Width {
                    record: i + 1,
                    expected: header.len(),
                    actual: row.len(),
                });
            }
        }
    }

    for (ri, rec) in original.iter().enumerate() {
        if rec.is_empty() {
            return Err(WriteError::ZeroColumns { record: ri });
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

    let records: Vec<Vec<String>> = if config.mode == WriteMode::Preserve {
        original.iter().map(|r| (*r).clone()).collect()
    } else if let Some(header) = &table.header {
        let mut keep: HashMap<&str, usize> = HashMap::new();
        for (i, name) in header.iter().enumerate() {
            match table.duplicates {
                DuplicatePolicy::KeepLast => {
                    keep.insert(name, i);
                }
                DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                    keep.entry(name).or_insert(i);
                }
            }
        }

        let mut indexes: Vec<usize> = keep.values().copied().collect();
        indexes.sort_by(|&a, &b| header[a].as_bytes().cmp(header[b].as_bytes()));

        let mut projected: Vec<Vec<String>> = table
            .rows
            .iter()
            .map(|row| indexes.iter().map(|&i| row[i].clone()).collect())
            .collect();
        projected.sort_by(|a, b| cmp_rows(a, b));

        let mut records = vec![indexes.iter().map(|&i| header[i].clone()).collect()];
        records.extend(projected);
        records
    } else {
        let mut rows = table.rows.clone();
        rows.sort_by(|a, b| cmp_rows(a, b));
        rows
    };

    if records.is_empty() {
        return Ok(String::new());
    }

    let canonical = config.mode == WriteMode::Canonical;
    let ending: &[u8] = if canonical || config.line_ending == LineEnding::Lf {
        b"\n"
    } else {
        b"\r\n"
    };
    let final_term = canonical || config.final_terminator;

    let mut out = String::new();
    for (ri, rec) in records.iter().enumerate() {
        if ri > 0 {
            out.push_str(std::str::from_utf8(ending).unwrap());
        }

        for (ci, field) in rec.iter().enumerate() {
            if ci > 0 {
                out.push(config.delimiter as char);
            }

            let bytes = field.as_bytes();
            let first_bom = ri == 0 && ci == 0 && field.starts_with('\u{feff}');
            let sole_empty = rec.len() == 1 && field.is_empty();
            let quote = first_bom
                || sole_empty
                || bytes.first().is_some_and(|b| *b == b' ' || *b == b'\t')
                || bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t')
                || bytes.iter().any(|&b| {
                    b == config.delimiter || b == config.quote || b == b'\r' || b == b'\n'
                });

            if quote {
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

        if final_term && ri + 1 == records.len() {
            out.push_str(std::str::from_utf8(ending).unwrap());
        }
    }

    Ok(out)
}

fn cmp_rows(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        let order = x.as_bytes().cmp(y.as_bytes());
        if order != std::cmp::Ordering::Equal {
            return order;
        }
    }
    a.len().cmp(&b.len())
}