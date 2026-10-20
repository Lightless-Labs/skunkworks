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
            DuplicatePolicy::KeepLast => header.iter().rposition(|v| v == name),
            DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                header.iter().position(|v| v == name)
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

fn parse_dialect(
    input: &str,
    delimiter: u8,
    quote: u8,
) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut i = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };
    let mut records = Vec::new();
    if i == bytes.len() {
        return Ok(records);
    }

    let mut record = 0usize;
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut state = 0u8; // 0: field start, 1: unquoted, 2: quoted, 3: after quote
    let mut quote_start = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        if b == 0 {
            return Err(CsvError::Nul { record, byte: i });
        }

        if state != 2 && b == b'\r' {
            if i + 1 >= bytes.len() || bytes[i + 1] != b'\n' {
                return Err(CsvError::BareCr { record, byte: i });
            }
            fields.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut fields));
            record += 1;
            state = 0;
            i += 2;
            continue;
        }

        match state {
            0 => {
                if b == delimiter {
                    fields.push(std::mem::take(&mut field));
                } else if b == quote {
                    quote_start = i;
                    state = 2;
                } else if b == b'\n' {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    state = 1;
                    i += ch.len_utf8();
                    continue;
                }
            }
            1 => {
                if b == delimiter {
                    fields.push(std::mem::take(&mut field));
                    state = 0;
                } else if b == quote {
                    return Err(CsvError::UnexpectedQuote { record, byte: i });
                } else if b == b'\n' {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    state = 0;
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    i += ch.len_utf8();
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
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    i += ch.len_utf8();
                    continue;
                }
            }
            _ => {
                if b == delimiter {
                    fields.push(std::mem::take(&mut field));
                    state = 0;
                } else if b == b'\n' {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    state = 0;
                } else {
                    return Err(CsvError::TrailingByte { record, byte: i });
                }
            }
        }
        i += 1;
    }

    if state == 2 {
        return Err(CsvError::UnclosedQuote {
            record,
            byte: quote_start,
        });
    }
    if state != 0 || !fields.is_empty() || !field.is_empty() {
        fields.push(field);
        records.push(fields);
    }
    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_dialect(input, b',', b'"')
}

fn first_duplicate(header: &[String]) -> Option<CsvError> {
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

fn table_from_records(
    records: Vec<Vec<String>>,
    config: Config,
) -> Result<Table, CsvError> {
    let mut iter = records.into_iter();
    let header = if config.headers { iter.next() } else { None };
    let rows: Vec<Vec<String>> = iter.collect();

    if let Some(h) = &header {
        if config.duplicates == DuplicatePolicy::Reject {
            if let Some(error) = first_duplicate(h) {
                return Err(error);
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
        if let Some(cached) = &self.cached {
            return cached.clone();
        }
        let result = parse_dialect(&self.input, self.config.delimiter, self.config.quote)
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
        return start + 1 == b.len() && start == 0;
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
    let dot = match b[start..].iter().position(|&c| c == b'.') {
        Some(p) => start + p,
        None => return false,
    };
    if b.len() - dot != 3 {
        return false;
    }
    let integer = &b[start..dot];
    if integer.is_empty()
        || !(integer == b"0"
            || (integer[0] >= b'1'
                && integer[0] <= b'9'
                && integer[1..].iter().all(u8::is_ascii_digit)))
        || !b[dot + 1..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    !(start == 1 && integer == b"0" && b[dot + 1..] == *b"00")
}

fn parse_decimal(s: &str) -> Option<i64> {
    let negative = s.starts_with('-');
    let body = if negative { &s[1..] } else { s };
    let mut parts = body.split('.');
    let whole: i128 = parts.next()?.parse().ok()?;
    let frac: i128 = parts.next()?.parse().ok()?;
    let magnitude = whole.checked_mul(100)?.checked_add(frac)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).ok()
}

fn evaluate_cell(
    raw: Option<&String>,
    column: Option<usize>,
    spec: &ColumnSpec,
    row: usize,
    spec_index: usize,
) -> (Option<Value>, Option<CellError>) {
    let error = |kind| {
        Some(CellError {
            row,
            spec: spec_index,
            column,
            kind,
        })
    };
    let Some(value) = raw else {
        return (
            None,
            Some(CellError {
                row,
                spec: spec_index,
                column,
                kind: CellErrorKind::MissingCell,
            }),
        );
    };

    if spec.kind == ColumnType::Text {
        if spec.nullable && value.is_empty() {
            return (Some(Value::Null), None);
        }
        return (Some(Value::Text(value.clone())), None);
    }
    if value.is_empty() {
        if spec.nullable {
            return (Some(Value::Null), None);
        }
        return (None, error(CellErrorKind::Empty));
    }

    match spec.kind {
        ColumnType::Text => unreachable!(),
        ColumnType::Integer => {
            if !integer_syntax(value) {
                return (None, error(CellErrorKind::InvalidInteger));
            }
            match value.parse::<i64>() {
                Ok(n) => (Some(Value::Integer(n)), None),
                Err(_) => (None, error(CellErrorKind::IntegerOverflow)),
            }
        }
        ColumnType::Decimal => {
            if !decimal_syntax(value) {
                return (None, error(CellErrorKind::InvalidDecimal));
            }
            match parse_decimal(value) {
                Some(n) => (Some(Value::Decimal(n)), None),
                None => (None, error(CellErrorKind::DecimalOverflow)),
            }
        }
        ColumnType::Boolean => match value.as_str() {
            "yes" | "1" => (Some(Value::Boolean(true)), None),
            "no" | "0" => (Some(Value::Boolean(false)), None),
            _ => (None, error(CellErrorKind::InvalidBoolean)),
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
    'rows: for (row_index, row) in table.rows.iter().enumerate() {
        let mut out = vec![None; columns.len()];
        for (spec_index, spec) in columns.iter().enumerate() {
            let resolved = match &spec.column {
                ColumnRef::Index(i) => Some(*i),
                ColumnRef::Name(name) => table.column_index(name),
            };
            let Some(column) = resolved else {
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: None,
                    kind: CellErrorKind::UnknownColumn,
                });
                if policy == ErrorPolicy::Stop {
                    result.rows.push(out);
                    break 'rows;
                }
                if policy == ErrorPolicy::FirstPerRow {
                    break;
                }
                continue;
            };
            let (value, err) =
                evaluate_cell(row.get(column), Some(column), spec, row_index, spec_index);
            out[spec_index] = value;
            if let Some(err) = err {
                result.errors.push(err);
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

fn row_has_nul(row: &[String]) -> Option<(usize, usize)> {
    row.iter().enumerate().find_map(|(column, field)| {
        field
            .as_bytes()
            .iter()
            .position(|&b| b == 0)
            .map(|byte| (column, byte))
    })
}

fn field_needs_quote(
    field: &str,
    delimiter: u8,
    quote: u8,
    first_field_first_record: bool,
    sole_empty: bool,
) -> bool {
    let bytes = field.as_bytes();
    bytes.contains(&delimiter)
        || bytes.contains(&quote)
        || bytes.contains(&b'\r')
        || bytes.contains(&b'\n')
        || bytes.first().is_some_and(|&b| b == b' ' || b == b'\t')
        || bytes.last().is_some_and(|&b| b == b' ' || b == b'\t')
        || (first_field_first_record && field.starts_with('\u{feff}'))
        || (sole_empty && field.is_empty())
}

fn encode_row(
    row: &[String],
    delimiter: u8,
    quote: u8,
    first_record: bool,
) -> String {
    let mut out = String::new();
    for (index, field) in row.iter().enumerate() {
        if index > 0 {
            out.push(delimiter as char);
        }
        let quote_field = field_needs_quote(
            field,
            delimiter,
            quote,
            first_record && index == 0,
            row.len() == 1,
        );
        if quote_field {
            out.push(quote as char);
            for ch in field.chars() {
                if ch == quote as char {
                    out.push(quote as char);
                    out.push(quote as char);
                } else {
                    out.push(ch);
                }
            }
            out.push(quote as char);
        } else {
            out.push_str(field);
        }
    }
    out
}

fn canonical_header_columns(
    header: &[String],
    policy: DuplicatePolicy,
) -> (Vec<usize>, Vec<String>) {
    let mut selected: Vec<(String, usize)> = Vec::new();
    for (index, name) in header.iter().enumerate() {
        if let Some(existing) = selected.iter_mut().find(|(n, _)| n == name) {
            if policy == DuplicatePolicy::KeepLast {
                existing.1 = index;
            }
        } else {
            selected.push((name.clone(), index));
        }
    }
    selected.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let names = selected.iter().map(|(n, _)| n.clone()).collect();
    let indices = selected.into_iter().map(|(_, i)| i).collect();
    (indices, names)
}

fn compare_rows(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (left, right) in a.iter().zip(b) {
        let ordering = left.as_bytes().cmp(right.as_bytes());
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }
    a.len().cmp(&b.len())
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    if let Some(header) = &table.header {
        if let Some((column, byte)) = row_has_nul(header) {
            return Err(WriteError::Nul {
                record: 0,
                column,
                byte,
            });
        }
    }
    for (index, row) in table.rows.iter().enumerate() {
        if let Some((column, byte)) = row_has_nul(row) {
            return Err(WriteError::Nul {
                record: index + usize::from(table.header.is_some()),
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

    if table.header.as_ref().is_some_and(Vec::is_empty) {
        return Err(WriteError::ZeroColumns { record: 0 });
    }
    for (index, row) in table.rows.iter().enumerate() {
        if row.is_empty() {
            return Err(WriteError::ZeroColumns {
                record: index + usize::from(table.header.is_some()),
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
                }) = first_duplicate(header)
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
    match config.mode {
        WriteMode::Preserve => {
            if let Some(header) = &table.header {
                records.push(header.clone());
            }
            records.extend(table.rows.iter().cloned());
        }
        WriteMode::Canonical => {
            if let Some(header) = &table.header {
                let (indices, names) = canonical_header_columns(header, table.duplicates);
                records.push(names);
                let mut rows: Vec<Vec<String>> = table
                    .rows
                    .iter()
                    .map(|row| indices.iter().map(|&i| row[i].clone()).collect())
                    .collect();
                rows.sort_by(|a, b| compare_rows(a, b));
                records.extend(rows);
            } else {
                let mut rows = table.rows.clone();
                rows.sort_by(|a, b| compare_rows(a, b));
                records = rows;
            }
        }
    }

    if records.is_empty() {
        return Ok(String::new());
    }

    let mut output = String::new();
    for (index, row) in records.iter().enumerate() {
        if index > 0 {
            output.push_str(match config.mode {
                WriteMode::Canonical => "\n",
                WriteMode::Preserve => match config.line_ending {
                    LineEnding::Lf => "\n",
                    LineEnding::CrLf => "\r\n",
                },
            });
        }
        output.push_str(&encode_row(
            row,
            config.delimiter,
            config.quote,
            index == 0,
        ));
    }
    let final_terminator = config.mode == WriteMode::Canonical || config.final_terminator;
    if final_terminator {
        output.push_str(match config.mode {
            WriteMode::Canonical => "\n",
            WriteMode::Preserve => match config.line_ending {
                LineEnding::Lf => "\n",
                LineEnding::CrLf => "\r\n",
            },
        });
    }
    Ok(output)
}