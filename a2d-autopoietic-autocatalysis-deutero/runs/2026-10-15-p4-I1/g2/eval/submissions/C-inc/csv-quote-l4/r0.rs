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

pub struct StreamReader {
    config: Config,
    input: String,
    result: Option<Result<Table, CsvError>>,
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
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    if i == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut field_started = false;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut opening_quote = 0usize;
    let mut record_index = 0usize;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if !in_quotes && b == b'\r' {
            if i + 1 >= bytes.len() || bytes[i + 1] != b'\n' {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: i,
                });
            }
        }

        if in_quotes {
            if b == quote {
                if i + 1 < bytes.len() && bytes[i + 1] == quote {
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
                field_started = false;
                after_quote = false;
                i += 1;
            } else if b == b'\n' || b == b'\r' {
                row.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut row));
                field_started = false;
                after_quote = false;
                record_index += 1;
                i += if b == b'\r' { 2 } else { 1 };
            } else {
                return Err(CsvError::TrailingByte {
                    record: record_index,
                    byte: i,
                });
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
            record_index += 1;
            i += if b == b'\r' { 2 } else { 1 };
        } else if b == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: i,
                });
            }
            in_quotes = true;
            opening_quote = i;
            field_started = true;
            i += 1;
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
            byte: opening_quote,
        });
    }

    if !row.is_empty() || !field.is_empty() || field_started || after_quote {
        row.push(field);
        records.push(row);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn duplicate_header_error(header: &[String]) -> Option<CsvError> {
    for second in 0..header.len() {
        if let Some(first) = header[..second].iter().position(|s| s == &header[second]) {
            return Some(CsvError::DuplicateHeader {
                name: header[second].clone(),
                first,
                second,
            });
        }
    }
    None
}

fn read_validated(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| CsvError::InvalidConfig { component })?;

    let records = parse_with_dialect(input, config.delimiter, config.quote)?;
    let (header, rows) = if config.headers && !records.is_empty() {
        let mut iter = records.into_iter();
        (iter.next(), iter.collect::<Vec<_>>())
    } else {
        (None, records)
    };

    if let Some(ref h) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            if let Some(e) = duplicate_header_error(h) {
                return Err(e);
            }
        }
        for (idx, row) in rows.iter().enumerate() {
            if row.len() != h.len() {
                return Err(CsvError::HeaderWidth {
                    record: idx + 1,
                    expected: h.len(),
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
        if self.result.is_none() {
            self.result = Some(read_validated(&self.input, self.config));
        }
        self.result.as_ref().unwrap().clone()
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.result = None;
    }
}

pub fn read_csv(input: &str, config: Config) -> Result<Table, CsvError> {
    read_validated(input, config)
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
    let start = usize::from(b.first() == Some(&b'-'));
    if b.len() < start + 4 {
        return false;
    }
    let dot = b.len() - 3;
    if b[dot] != b'.' || !b[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    if dot == start + 1 && b[start] == b'0' {
        return false;
    }
    if dot == start {
        return false;
    }
    if b[start] == b'0' {
        dot == start + 1
    } else {
        (b'1'..=b'9').contains(&b[start]) && b[start + 1..dot].iter().all(u8::is_ascii_digit)
    }
}

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    if !integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>().map_err(|_| CellErrorKind::IntegerOverflow)
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !decimal_syntax(s) {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let negative = s.starts_with('-');
    let body = if negative { &s[1..] } else { s };
    let dot = body.len() - 3;
    let whole = body[..dot]
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let frac = body[dot + 1..]
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let magnitude = whole
        .checked_mul(100)
        .and_then(|v| v.checked_add(frac))
        .ok_or(CellErrorKind::DecimalOverflow)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn evaluate_cell(
    text: &str,
    kind: ColumnType,
    nullable: bool,
) -> Result<Value, CellErrorKind> {
    if text.is_empty() {
        if nullable {
            return Ok(Value::Null);
        }
        if kind != ColumnType::Text {
            return Err(CellErrorKind::Empty);
        }
    }

    match kind {
        ColumnType::Text => Ok(Value::Text(text.to_string())),
        ColumnType::Integer => parse_integer(text).map(Value::Integer),
        ColumnType::Decimal => parse_decimal(text).map(Value::Decimal),
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
    let mut result = TypedResult {
        rows: Vec::new(),
        errors: Vec::new(),
    };
    let mut stopped = false;

    for (row_idx, row) in table.rows.iter().enumerate() {
        if stopped {
            break;
        }
        let mut out = Vec::with_capacity(columns.len());
        let mut row_failed = false;

        for (spec_idx, spec) in columns.iter().enumerate() {
            if row_failed {
                out.push(None);
                continue;
            }

            let resolved = match &spec.column {
                ColumnRef::Index(k) => Some(*k),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let (column, evaluated) = match resolved {
                None => (None, Err(CellErrorKind::UnknownColumn)),
                Some(index) => match row.get(index) {
                    None => (Some(index), Err(CellErrorKind::MissingCell)),
                    Some(cell) => (Some(index), evaluate_cell(cell, spec.kind, spec.nullable)),
                },
            };

            match evaluated {
                Ok(value) => out.push(Some(value)),
                Err(kind) => {
                    out.push(None);
                    result.errors.push(CellError {
                        row: row_idx,
                        spec: spec_idx,
                        column,
                        kind,
                    });
                    match policy {
                        ErrorPolicy::Collect => {}
                        ErrorPolicy::FirstPerRow => row_failed = true,
                        ErrorPolicy::Stop => {
                            row_failed = true;
                            stopped = true;
                        }
                    }
                }
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

fn rows_have_nul(rows: &[Vec<String>], starting_record: usize) -> Option<WriteError> {
    for (i, row) in rows.iter().enumerate() {
        for (column, cell) in row.iter().enumerate() {
            if let Some(byte) = cell.as_bytes().iter().position(|&b| b == 0) {
                return Some(WriteError::Nul {
                    record: starting_record + i,
                    column,
                    byte,
                });
            }
        }
    }
    None
}

fn cmp_vectors(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        match x.as_bytes().cmp(y.as_bytes()) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
    }
    a.len().cmp(&b.len())
}

fn quote_field(field: &str, first: bool, sole: bool, delimiter: u8, quote: u8) -> String {
    let bytes = field.as_bytes();
    let needs_quote = bytes.contains(&delimiter)
        || bytes.contains(&quote)
        || bytes.contains(&b'\r')
        || bytes.contains(&b'\n')
        || bytes.first().is_some_and(|b| *b == b' ' || *b == b'\t')
        || bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t')
        || (first && field.starts_with('\u{feff}'))
        || (sole && field.is_empty());

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

fn serialize_records(
    records: &[Vec<String>],
    delimiter: u8,
    quote: u8,
    terminator: &str,
    final_terminator: bool,
) -> String {
    let mut out = String::new();
    for (ri, row) in records.iter().enumerate() {
        if ri > 0 {
            out.push_str(terminator);
        }
        for (ci, field) in row.iter().enumerate() {
            if ci > 0 {
                out.push(delimiter as char);
            }
            let quoted = quote_field(field, ri == 0 && ci == 0, row.len() == 1, delimiter, quote);
            out.push_str(&quoted);
        }
    }
    if final_terminator && !records.is_empty() {
        out.push_str(terminator);
    }
    out
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    if let Some(header) = &table.header {
        if let Some(err) = rows_have_nul(std::slice::from_ref(header), 0) {
            return Err(err);
        }
    }
    let data_start = usize::from(table.header.is_some());
    if let Some(err) = rows_have_nul(&table.rows, data_start) {
        return Err(err);
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

    if let Some(header) = &table.header {
        if header.is_empty() {
            return Err(WriteError::ZeroColumns { record: 0 });
        }
    }
    for (i, row) in table.rows.iter().enumerate() {
        if row.is_empty() {
            return Err(WriteError::ZeroColumns {
                record: data_start + i,
            });
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

    let mut records: Vec<Vec<String>> = Vec::new();

    if config.mode == WriteMode::Preserve {
        if let Some(header) = &table.header {
            records.push(header.clone());
        }
        records.extend(table.rows.iter().cloned());
        let ending = match config.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        };
        return Ok(serialize_records(
            &records,
            config.delimiter,
            config.quote,
            ending,
            config.final_terminator,
        ));
    }

    if let Some(header) = &table.header {
        let mut kept = Vec::new();
        for (index, name) in header.iter().enumerate() {
            let keep = match table.duplicates {
                DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                    header[..index].iter().all(|n| n != name)
                }
                DuplicatePolicy::KeepLast => header[index + 1..].iter().all(|n| n != name),
            };
            if keep {
                kept.push((name.clone(), index));
            }
        }
        kept.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        let indices: Vec<usize> = kept.iter().map(|(_, index)| *index).collect();
        records.push(kept.into_iter().map(|(name, _)| name).collect());

        let mut projected: Vec<Vec<String>> = table
            .rows
            .iter()
            .map(|row| indices.iter().map(|&idx| row[idx].clone()).collect())
            .collect();
        projected.sort_by(|a, b| cmp_vectors(a, b));
        records.extend(projected);
    } else {
        let mut sorted = table.rows.clone();
        sorted.sort_by(|a, b| cmp_vectors(a, b));
        records = sorted;
    }

    Ok(serialize_records(
        &records,
        config.delimiter,
        config.quote,
        "\n",
        true,
    ))
}

fn main() {}