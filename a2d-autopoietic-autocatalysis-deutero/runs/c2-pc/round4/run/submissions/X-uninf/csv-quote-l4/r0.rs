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
    let start = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };

    if start == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut i = start;
    let mut record_index = 0usize;
    let mut field_started = false;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut quote_start = 0usize;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
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
                let ch = input[i..].chars().next().expect("valid UTF-8");
                field.push(ch);
                i += ch.len_utf8();
            }
            continue;
        }

        if b == b'\r' && (i + 1 >= bytes.len() || bytes[i + 1] != b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
        }

        if after_quote {
            if b == delimiter {
                record.push(std::mem::take(&mut field));
                field_started = false;
                after_quote = false;
                i += 1;
            } else if b == b'\n' || b == b'\r' {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
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
            record.push(std::mem::take(&mut field));
            field_started = false;
            i += 1;
        } else if b == b'\n' || b == b'\r' {
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
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
            quote_start = i;
            field_started = true;
            i += 1;
        } else {
            let ch = input[i..].chars().next().expect("valid UTF-8");
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

    if after_quote || field_started || !record.is_empty() {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
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
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|v| v == name)
            }
        }
    }
}

fn duplicate_header(header: &[String]) -> Option<CsvError> {
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

fn table_from_records(
    records: Vec<Vec<String>>,
    config: Config,
) -> Result<Table, CsvError> {
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

    if let Some(ref h) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            if let Some(err) = duplicate_header(h) {
                return Err(err);
            }
        }
        for (i, row) in rows.iter().enumerate() {
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
        rows,
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
        let result = parse_with_dialect(&self.input, self.config.delimiter, self.config.quote)
            .and_then(|records| table_from_records(records, self.config));
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
    let digits = if b[0] == b'-' { &b[1..] } else { b };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return false;
    }
    if digits.len() > 1 && digits[0] == b'0' {
        return false;
    }
    !(b[0] == b'-' && digits == b"0")
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    let b = if b.first() == Some(&b'-') {
        &b[1..]
    } else {
        b
    };
    if b.len() < 4 || b[b.len() - 3] != b'.' {
        return false;
    }
    let whole = &b[..b.len() - 3];
    let frac = &b[b.len() - 2..];
    if whole.is_empty()
        || !whole.iter().all(u8::is_ascii_digit)
        || !frac.iter().all(u8::is_ascii_digit)
        || (whole.len() > 1 && whole[0] == b'0')
    {
        return false;
    }
    !(s.starts_with('-') && whole == b"0" && frac == b"00")
}

fn evaluate_cell(
    text: Option<&String>,
    resolved_column: Option<usize>,
    spec: &ColumnSpec,
    row: usize,
    spec_index: usize,
) -> (Option<Value>, Option<CellError>) {
    let make_error = |column, kind| {
        CellError {
            row,
            spec: spec_index,
            column,
            kind,
        }
    };

    let Some(column) = resolved_column else {
        return (
            None,
            Some(make_error(None, CellErrorKind::UnknownColumn)),
        );
    };
    let Some(text) = text else {
        return (None, Some(make_error(Some(column), CellErrorKind::MissingCell)));
    };

    if text.is_empty() {
        if spec.nullable {
            return (Some(Value::Null), None);
        }
        if spec.kind != ColumnType::Text {
            return (None, Some(make_error(Some(column), CellErrorKind::Empty)));
        }
    }

    let invalid = |kind| (None, Some(make_error(Some(column), kind)));
    match spec.kind {
        ColumnType::Text => (Some(Value::Text(text.clone())), None),
        ColumnType::Integer => {
            if !integer_syntax(text) {
                return invalid(CellErrorKind::InvalidInteger);
            }
            match text.parse::<i64>() {
                Ok(v) => (Some(Value::Integer(v)), None),
                Err(_) => invalid(CellErrorKind::IntegerOverflow),
            }
        }
        ColumnType::Decimal => {
            if !decimal_syntax(text) {
                return invalid(CellErrorKind::InvalidDecimal);
            }
            let negative = text.starts_with('-');
            let body = if negative { &text[1..] } else { text.as_str() };
            let dot = body.len() - 3;
            let whole = &body[..dot];
            let frac = &body[dot + 1..];
            let magnitude = whole.parse::<i128>().unwrap_or(i128::MAX)
                .saturating_mul(100)
                .saturating_add(frac.parse::<i128>().unwrap_or(0));
            let signed = if negative { -magnitude } else { magnitude };
            if signed < i64::MIN as i128 || signed > i64::MAX as i128 {
                invalid(CellErrorKind::DecimalOverflow)
            } else {
                (Some(Value::Decimal(signed as i64)), None)
            }
        }
        ColumnType::Boolean => match text.as_str() {
            "yes" | "1" => (Some(Value::Boolean(true)), None),
            "no" | "0" => (Some(Value::Boolean(false)), None),
            _ => invalid(CellErrorKind::InvalidBoolean),
        },
    }
}

pub fn extract_columns(
    table: &Table,
    columns: &[ColumnSpec],
    policy: ErrorPolicy,
) -> TypedResult {
    let mut out = TypedResult {
        rows: Vec::new(),
        errors: Vec::new(),
    };

    for (row_index, row) in table.rows.iter().enumerate() {
        let mut output_row = Vec::with_capacity(columns.len());
        let mut stopped = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if stopped {
                output_row.push(None);
                continue;
            }

            let resolved = match &spec.column {
                ColumnRef::Index(i) => Some(*i),
                ColumnRef::Name(name) => table.column_index(name),
            };
            let cell = resolved.and_then(|i| row.get(i));
            let (value, error) = evaluate_cell(cell, resolved, spec, row_index, spec_index);
            output_row.push(value);

            if let Some(error) = error {
                out.errors.push(error);
                match policy {
                    ErrorPolicy::Collect => {}
                    ErrorPolicy::FirstPerRow => stopped = true,
                    ErrorPolicy::Stop => {
                        output_row.resize(columns.len(), None);
                        out.rows.push(output_row);
                        return out;
                    }
                }
            }
        }
        out.rows.push(output_row);
    }

    out
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

fn original_records<'a>(table: &'a Table) -> Vec<&'a [String]> {
    let mut records = Vec::new();
    if let Some(header) = &table.header {
        records.push(header.as_slice());
    }
    records.extend(table.rows.iter().map(Vec::as_slice));
    records
}

fn write_duplicate(header: &[String]) -> Option<WriteError> {
    let mut seen = std::collections::HashMap::<&str, usize>::new();
    for (second, name) in header.iter().enumerate() {
        if let Some(&first) = seen.get(name.as_str()) {
            return Some(WriteError::DuplicateHeader {
                name: name.clone(),
                first,
                second,
            });
        }
        seen.insert(name, second);
    }
    None
}

fn compare_rows(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        let order = x.as_bytes().cmp(y.as_bytes());
        if order != std::cmp::Ordering::Equal {
            return order;
        }
    }
    a.len().cmp(&b.len())
}

fn emit_field(
    out: &mut String,
    field: &str,
    delimiter: u8,
    quote: u8,
    first_field_first_record: bool,
    sole_empty: bool,
) {
    let b = field.as_bytes();
    let begins_bom = first_field_first_record && b.starts_with(&[0xef, 0xbb, 0xbf]);
    let edge_space = b.first().is_some_and(|v| *v == b' ' || *v == b'\t')
        || b.last().is_some_and(|v| *v == b' ' || *v == b'\t');
    let must_quote = b.contains(&delimiter)
        || b.contains(&quote)
        || b.contains(&b'\r')
        || b.contains(&b'\n')
        || edge_space
        || begins_bom
        || sole_empty;

    if !must_quote {
        out.push_str(field);
        return;
    }

    out.push(quote as char);
    for ch in field.chars() {
        if ch as u32 == quote as u32 {
            out.push(quote as char);
            out.push(quote as char);
        } else {
            out.push(ch);
        }
    }
    out.push(quote as char);
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let records = original_records(table);

    for (record_index, record) in records.iter().enumerate() {
        for (column, field) in record.iter().enumerate() {
            if let Some(byte) = field.as_bytes().iter().position(|b| *b == 0) {
                return Err(WriteError::Nul {
                    record: record_index,
                    column,
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

    for (record, cells) in records.iter().enumerate() {
        if cells.is_empty() {
            return Err(WriteError::ZeroColumns { record });
        }
    }

    if config.mode == WriteMode::Canonical {
        if let Some(header) = &table.header {
            if table.duplicates == DuplicatePolicy::Reject {
                if let Some(error) = write_duplicate(header) {
                    return Err(error);
                }
            }
        }
    }

    let mut output_records: Vec<Vec<String>> = Vec::new();

    match config.mode {
        WriteMode::Preserve => {
            if let Some(header) = &table.header {
                output_records.push(header.clone());
            }
            output_records.extend(table.rows.iter().cloned());
        }
        WriteMode::Canonical => {
            if let Some(header) = &table.header {
                let mut retained = Vec::<(String, usize)>::new();
                match table.duplicates {
                    DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                        for (i, name) in header.iter().enumerate() {
                            if !retained.iter().any(|(n, _)| n == name) {
                                retained.push((name.clone(), i));
                            }
                        }
                    }
                    DuplicatePolicy::KeepLast => {
                        for (i, name) in header.iter().enumerate() {
                            if !header[i + 1..].iter().any(|later| later == name) {
                                retained.push((name.clone(), i));
                            }
                        }
                    }
                }
                retained.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
                let indexes: Vec<usize> = retained.iter().map(|(_, i)| *i).collect();
                output_records.push(retained.into_iter().map(|(n, _)| n).collect());

                let mut rows: Vec<Vec<String>> = table
                    .rows
                    .iter()
                    .map(|row| indexes.iter().map(|&i| row[i].clone()).collect())
                    .collect();
                rows.sort_by(|a, b| compare_rows(a, b));
                output_records.extend(rows);
            } else {
                let mut rows = table.rows.clone();
                rows.sort_by(|a, b| compare_rows(a, b));
                output_records = rows;
            }
        }
    }

    if output_records.is_empty() {
        return Ok(String::new());
    }

    let canonical = config.mode == WriteMode::Canonical;
    let line_ending: &[u8] = if canonical {
        b"\n"
    } else {
        match config.line_ending {
            LineEnding::Lf => b"\n",
            LineEnding::CrLf => b"\r\n",
        }
    };

    let mut output = String::new();
    for (record_index, record) in output_records.iter().enumerate() {
        if record_index > 0 {
            output.push_str(std::str::from_utf8(line_ending).expect("ASCII line ending"));
        }
        for (column, field) in record.iter().enumerate() {
            if column > 0 {
                output.push(config.delimiter as char);
            }
            emit_field(
                &mut output,
                field,
                config.delimiter,
                config.quote,
                record_index == 0 && column == 0,
                record.len() == 1 && field.is_empty(),
            );
        }
    }

    if canonical || config.final_terminator {
        output.push_str(std::str::from_utf8(line_ending).expect("ASCII line ending"));
    }

    Ok(output)
}