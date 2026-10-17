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
    let mut pos = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };
    let mut record_index = 0usize;
    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut field_started = false;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut opening_quote = 0usize;

    while pos < bytes.len() {
        let b = bytes[pos];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: pos,
            });
        }

        if !in_quotes && b == b'\r' && bytes.get(pos + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: pos,
            });
        }

        if in_quotes {
            if b == quote {
                if bytes.get(pos + 1) == Some(&quote) {
                    field.push(quote as char);
                    pos += 2;
                } else {
                    in_quotes = false;
                    after_quote = true;
                    pos += 1;
                }
            } else {
                let ch = input[pos..].chars().next().unwrap();
                field.push(ch);
                pos += ch.len_utf8();
            }
            continue;
        }

        if after_quote {
            if b == delimiter {
                row.push(std::mem::take(&mut field));
                field_started = false;
                after_quote = false;
                pos += 1;
                continue;
            }
            if b == b'\n' || b == b'\r' {
                row.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut row));
                field_started = false;
                after_quote = false;
                record_index += 1;
                pos += if b == b'\r' { 2 } else { 1 };
                continue;
            }
            return Err(CsvError::TrailingByte {
                record: record_index,
                byte: pos,
            });
        }

        if b == delimiter {
            row.push(std::mem::take(&mut field));
            field_started = false;
            pos += 1;
        } else if b == b'\n' || b == b'\r' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            field_started = false;
            record_index += 1;
            pos += if b == b'\r' { 2 } else { 1 };
        } else if b == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: pos,
                });
            }
            in_quotes = true;
            opening_quote = pos;
            field_started = true;
            pos += 1;
        } else {
            let ch = input[pos..].chars().next().unwrap();
            field.push(ch);
            field_started = true;
            pos += ch.len_utf8();
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
    parse_dialect(input, b',', b'"')
}

fn duplicate_header_error(header: &[String]) -> Option<CsvError> {
    for second in 0..header.len() {
        if let Some(first) = header[..second].iter().position(|v| v == &header[second]) {
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

    if let Some(ref h) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            if let Some(err) = duplicate_header_error(h) {
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
    finished: Option<Result<Table, CsvError>>,
}

impl StreamReader {
    pub fn new(config: Config) -> Result<Self, CsvError> {
        validate_dialect(config.delimiter, config.quote)
            .map_err(|component| CsvError::InvalidConfig { component })?;
        Ok(Self {
            config,
            input: String::new(),
            finished: None,
        })
    }

    pub fn feed(&mut self, chunk: &str) -> Result<(), CsvError> {
        if self.finished.is_some() {
            return Err(CsvError::ReaderFinished);
        }
        self.input.push_str(chunk);
        Ok(())
    }

    pub fn finish(&mut self) -> Result<Table, CsvError> {
        if let Some(result) = &self.finished {
            return result.clone();
        }
        let result = parse_dialect(&self.input, self.config.delimiter, self.config.quote)
            .and_then(|records| table_from_records(records, self.config));
        self.finished = Some(result.clone());
        result
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.finished = None;
    }
}

pub fn read_csv(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| CsvError::InvalidConfig { component })?;
    let records = parse_dialect(input, config.delimiter, config.quote)?;
    table_from_records(records, config)
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
    if b.is_empty() {
        return false;
    }
    let start = usize::from(b[0] == b'-');
    if start >= b.len() {
        return false;
    }
    let dot = match b[start..].iter().position(|&v| v == b'.') {
        Some(p) => start + p,
        None => return false,
    };
    if b.len() != dot + 3 {
        return false;
    }
    let integral = &b[start..dot];
    if integral.is_empty() {
        return false;
    }
    let integral_ok = if integral == b"0" {
        true
    } else {
        (b'1'..=b'9').contains(&integral[0])
            && integral[1..].iter().all(u8::is_ascii_digit)
    };
    if !integral_ok || !b[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    !(b[0] == b'-' && integral == b"0" && b[dot + 1] == b'0' && b[dot + 2] == b'0')
}

fn evaluate_cell(
    text: Option<&String>,
    spec: &ColumnSpec,
    row_index: usize,
    spec_index: usize,
    resolved: Option<usize>,
) -> Result<Value, CellError> {
    if let ColumnRef::Name(_) = spec.column {
        if resolved.is_none() {
            return Err(CellError {
                row: row_index,
                spec: spec_index,
                column: None,
                kind: CellErrorKind::UnknownColumn,
            });
        }
    }
    let column = resolved;
    let Some(text) = text else {
        return Err(CellError {
            row: row_index,
            spec: spec_index,
            column,
            kind: CellErrorKind::MissingCell,
        });
    };
    if text.is_empty() {
        if spec.nullable {
            return Ok(Value::Null);
        }
        if spec.kind != ColumnType::Text {
            return Err(CellError {
                row: row_index,
                spec: spec_index,
                column,
                kind: CellErrorKind::Empty,
            });
        }
    }

    match spec.kind {
        ColumnType::Text => Ok(Value::Text(text.clone())),
        ColumnType::Integer => {
            if !integer_syntax(text) {
                return Err(CellError {
                    row: row_index,
                    spec: spec_index,
                    column,
                    kind: CellErrorKind::InvalidInteger,
                });
            }
            match text.parse::<i64>() {
                Ok(v) => Ok(Value::Integer(v)),
                Err(_) => Err(CellError {
                    row: row_index,
                    spec: spec_index,
                    column,
                    kind: CellErrorKind::IntegerOverflow,
                }),
            }
        }
        ColumnType::Decimal => {
            if !decimal_syntax(text) {
                return Err(CellError {
                    row: row_index,
                    spec: spec_index,
                    column,
                    kind: CellErrorKind::InvalidDecimal,
                });
            }
            let negative = text.starts_with('-');
            let unsigned = if negative { &text[1..] } else { text };
            let digits = unsigned.replace('.', "");
            let magnitude = match digits.parse::<i128>() {
                Ok(v) => v,
                Err(_) => {
                    return Err(CellError {
                        row: row_index,
                        spec: spec_index,
                        column,
                        kind: CellErrorKind::DecimalOverflow,
                    });
                }
            };
            let signed = if negative { -magnitude } else { magnitude };
            if signed < i64::MIN as i128 || signed > i64::MAX as i128 {
                Err(CellError {
                    row: row_index,
                    spec: spec_index,
                    column,
                    kind: CellErrorKind::DecimalOverflow,
                })
            } else {
                Ok(Value::Decimal(signed as i64))
            }
        }
        ColumnType::Boolean => match text.as_str() {
            "yes" | "1" => Ok(Value::Boolean(true)),
            "no" | "0" => Ok(Value::Boolean(false)),
            _ => Err(CellError {
                row: row_index,
                spec: spec_index,
                column,
                kind: CellErrorKind::InvalidBoolean,
            }),
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
    let mut stop_all = false;

    for (row_index, row) in table.rows.iter().enumerate() {
        if stop_all {
            break;
        }
        let mut output = vec![None; columns.len()];
        let mut row_failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if row_failed {
                break;
            }
            let resolved = match &spec.column {
                ColumnRef::Index(i) => Some(*i),
                ColumnRef::Name(name) => table.column_index(name),
            };
            let cell = resolved.and_then(|i| row.get(i));
            match evaluate_cell(cell, spec, row_index, spec_index, resolved) {
                Ok(value) => output[spec_index] = Some(value),
                Err(error) => {
                    result.errors.push(error);
                    match policy {
                        ErrorPolicy::Collect => {}
                        ErrorPolicy::FirstPerRow => row_failed = true,
                        ErrorPolicy::Stop => {
                            row_failed = true;
                            stop_all = true;
                        }
                    }
                }
            }
        }
        result.rows.push(output);
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

fn check_nuls(table: &Table) -> Result<(), WriteError> {
    let mut record = 0usize;
    if let Some(header) = &table.header {
        for (column, cell) in header.iter().enumerate() {
            if let Some(byte) = cell.as_bytes().iter().position(|&b| b == 0) {
                return Err(WriteError::Nul {
                    record,
                    column,
                    byte,
                });
            }
        }
        record = 1;
    }
    for row in &table.rows {
        for (column, cell) in row.iter().enumerate() {
            if let Some(byte) = cell.as_bytes().iter().position(|&b| b == 0) {
                return Err(WriteError::Nul {
                    record,
                    column,
                    byte,
                });
            }
        }
        record += 1;
    }
    Ok(())
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

fn encode_field(field: &str, delimiter: u8, quote: u8, special_first_bom: bool, sole_empty: bool) -> String {
    let bytes = field.as_bytes();
    let edge_space = bytes.first().is_some_and(|b| *b == b' ' || *b == b'\t')
        || bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t');
    let needs_quote = field.as_bytes().contains(&delimiter)
        || field.as_bytes().contains(&quote)
        || field.contains('\r')
        || field.contains('\n')
        || edge_space
        || (special_first_bom && field.starts_with('\u{feff}'))
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

    check_nuls(table)?;

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

    let mut records: Vec<Vec<String>> = Vec::new();
    if let Some(header) = &table.header {
        records.push(header.clone());
    }
    records.extend(table.rows.iter().cloned());

    for (record, row) in records.iter().enumerate() {
        if row.is_empty() {
            return Err(WriteError::ZeroColumns { record });
        }
    }

    if config.mode == WriteMode::Canonical {
        if table.header.is_some() && table.duplicates == DuplicatePolicy::Reject {
            if let Some(header) = &table.header {
                for second in 0..header.len() {
                    if let Some(first) = header[..second]
                        .iter()
                        .position(|name| name == &header[second])
                    {
                        return Err(WriteError::DuplicateHeader {
                            name: header[second].clone(),
                            first,
                            second,
                        });
                    }
                }
            }
        }
    }

    if config.mode == WriteMode::Canonical {
        if let Some(header) = &table.header {
            let mut retained: Vec<usize> = Vec::new();
            for index in 0..header.len() {
                let earlier = header[..index].iter().any(|v| v == &header[index]);
                let later = header[index + 1..].iter().any(|v| v == &header[index]);
                let keep = match table.duplicates {
                    DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => !earlier,
                    DuplicatePolicy::KeepLast => !later,
                };
                if keep {
                    retained.push(index);
                }
            }
            retained.sort_by(|&a, &b| header[a].as_bytes().cmp(header[b].as_bytes()));

            let sorted_header: Vec<String> =
                retained.iter().map(|&i| header[i].clone()).collect();
            let mut projected_rows: Vec<Vec<String>> = table
                .rows
                .iter()
                .map(|row| retained.iter().map(|&i| row[i].clone()).collect())
                .collect();
            projected_rows.sort_by(|a, b| compare_rows(a, b));

            records.clear();
            records.push(sorted_header);
            records.extend(projected_rows);
        } else {
            records = table.rows.clone();
            records.sort_by(|a, b| compare_rows(a, b));
        }
    }

    if records.is_empty() {
        return Ok(String::new());
    }

    let mut out = String::new();
    let canonical = config.mode == WriteMode::Canonical;
    let line_ending = if canonical {
        "\n"
    } else {
        match config.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    };

    for (record_index, row) in records.iter().enumerate() {
        if record_index > 0 {
            // Previous record separator was emitted below.
        }
        if record_index > 0 {
            // separators are emitted after each prior record
        }
        for (column_index, field) in row.iter().enumerate() {
            if column_index > 0 {
                out.push(config.delimiter as char);
            }
            let special_first_bom = record_index == 0 && column_index == 0;
            let sole_empty = row.len() == 1 && field.is_empty();
            out.push_str(&encode_field(
                field,
                config.delimiter,
                config.quote,
                special_first_bom,
                sole_empty,
            ));
        }

        let has_next = record_index + 1 < records.len();
        let terminate = canonical || has_next || config.final_terminator;
        if terminate {
            out.push_str(line_ending);
        }
    }

    Ok(out)
}