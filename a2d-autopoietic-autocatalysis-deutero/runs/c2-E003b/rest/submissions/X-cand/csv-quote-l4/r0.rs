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

fn parse_with_dialect(
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
    let mut records = Vec::new();
    if pos == bytes.len() {
        return Ok(records);
    }

    let mut record = 0usize;
    let mut fields = Vec::new();
    let mut field = Vec::new();
    let mut state = 0u8; // 0: field start, 1: unquoted, 2: quoted, 3: closed quote
    let mut opening_quote = 0usize;
    let mut record_started = false;

    while pos < bytes.len() {
        let b = bytes[pos];

        if b == 0 {
            return Err(CsvError::Nul { record, byte: pos });
        }

        if state != 2 && b == b'\r' {
            if bytes.get(pos + 1) != Some(&b'\n') {
                return Err(CsvError::BareCr { record, byte: pos });
            }
            fields.push(String::from_utf8(field).unwrap());
            records.push(fields);
            fields = Vec::new();
            field = Vec::new();
            state = 0;
            record_started = false;
            record += 1;
            pos += 2;
            continue;
        }

        if state == 2 {
            if b == quote {
                if bytes.get(pos + 1) == Some(&quote) {
                    field.push(quote);
                    pos += 2;
                } else {
                    state = 3;
                    pos += 1;
                }
            } else {
                field.push(b);
                pos += 1;
            }
            record_started = true;
            continue;
        }

        if state == 3 {
            if b == delimiter {
                fields.push(String::from_utf8(field).unwrap());
                field = Vec::new();
                state = 0;
                record_started = true;
                pos += 1;
            } else if b == b'\n' {
                fields.push(String::from_utf8(field).unwrap());
                records.push(fields);
                fields = Vec::new();
                field = Vec::new();
                state = 0;
                record_started = false;
                record += 1;
                pos += 1;
            } else {
                return Err(CsvError::TrailingByte { record, byte: pos });
            }
            continue;
        }

        if b == b'\n' {
            fields.push(String::from_utf8(field).unwrap());
            records.push(fields);
            fields = Vec::new();
            field = Vec::new();
            state = 0;
            record_started = false;
            record += 1;
            pos += 1;
        } else if b == delimiter {
            fields.push(String::from_utf8(field).unwrap());
            field = Vec::new();
            state = 0;
            record_started = true;
            pos += 1;
        } else if b == quote {
            if state == 0 {
                state = 2;
                opening_quote = pos;
                record_started = true;
                pos += 1;
            } else {
                return Err(CsvError::UnexpectedQuote { record, byte: pos });
            }
        } else {
            field.push(b);
            state = 1;
            record_started = true;
            pos += 1;
        }
    }

    if state == 2 {
        return Err(CsvError::UnclosedQuote {
            record,
            byte: opening_quote,
        });
    }
    if record_started || !fields.is_empty() || !field.is_empty() || state == 3 {
        fields.push(String::from_utf8(field).unwrap());
        records.push(fields);
    }
    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn make_table(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_dialect(config.delimiter, config.quote).map_err(|component| {
        CsvError::InvalidConfig { component }
    })?;

    let records = parse_with_dialect(input, config.delimiter, config.quote)?;
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

    if let Some(ref header) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            let mut seen: HashMap<&str, usize> = HashMap::new();
            for (second, name) in header.iter().enumerate() {
                if let Some(&first) = seen.get(name.as_str()) {
                    return Err(CsvError::DuplicateHeader {
                        name: name.clone(),
                        first,
                        second,
                    });
                }
                seen.insert(name, second);
            }
        }
        for (i, row) in rows.iter().enumerate() {
            if row.len() != header.len() {
                return Err(CsvError::HeaderWidth {
                    record: i + 1,
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
        let result = make_table(&self.input, self.config);
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
    (b'1'..=b'9').contains(&b[start]) && b[start + 1..].iter().all(u8::is_ascii_digit)
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
    let integral = &b[start..dot];
    if integral.is_empty() {
        return false;
    }
    if integral[0] == b'0' {
        integral.len() == 1
    } else {
        (b'1'..=b'9').contains(&integral[0])
            && integral[1..].iter().all(u8::is_ascii_digit)
    }
}

fn evaluate_cell(
    text: &str,
    spec: &ColumnSpec,
) -> Result<Value, CellErrorKind> {
    if spec.nullable && text.is_empty() {
        return Ok(Value::Null);
    }
    if text.is_empty() && spec.kind != ColumnType::Text {
        return Err(CellErrorKind::Empty);
    }
    match spec.kind {
        ColumnType::Text => Ok(Value::Text(text.to_string())),
        ColumnType::Integer => {
            if !integer_syntax(text) {
                return Err(CellErrorKind::InvalidInteger);
            }
            text.parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| CellErrorKind::IntegerOverflow)
        }
        ColumnType::Decimal => {
            if !decimal_syntax(text) || text == "-0.00" {
                return Err(CellErrorKind::InvalidDecimal);
            }
            let negative = text.starts_with('-');
            let body = if negative { &text[1..] } else { text };
            let (whole, frac) = body.split_once('.').unwrap();
            let magnitude_text = format!("{}{}", whole, frac);
            let magnitude = magnitude_text
                .parse::<i128>()
                .map_err(|_| CellErrorKind::DecimalOverflow)?;
            let signed = if negative { -magnitude } else { magnitude };
            i64::try_from(signed)
                .map(Value::Decimal)
                .map_err(|_| CellErrorKind::DecimalOverflow)
        }
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

    'rows: for (row_index, row) in table.rows.iter().enumerate() {
        let mut output = vec![None; columns.len()];
        for (spec_index, spec) in columns.iter().enumerate() {
            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };
            let (column, value) = match resolved {
                None => (None, Err(CellErrorKind::UnknownColumn)),
                Some(index) => match row.get(index) {
                    None => (Some(index), Err(CellErrorKind::MissingCell)),
                    Some(text) => (Some(index), evaluate_cell(text, spec)),
                },
            };
            match value {
                Ok(value) => output[spec_index] = Some(value),
                Err(kind) => {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column,
                        kind,
                    });
                    match policy {
                        ErrorPolicy::Collect => {}
                        ErrorPolicy::FirstPerRow => break,
                        ErrorPolicy::Stop => {
                            result.rows.push(output);
                            break 'rows;
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
    Nul { record: usize, column: usize, byte: usize },
    Width { record: usize, expected: usize, actual: usize },
    ZeroColumns { record: usize },
    DuplicateHeader {
        name: String,
        first: usize,
        second: usize,
    },
}

fn find_duplicate(header: &[String]) -> Option<(String, usize, usize)> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (second, name) in header.iter().enumerate() {
        if let Some(&first) = seen.get(name.as_str()) {
            return Some((name.clone(), first, second));
        }
        seen.insert(name, second);
    }
    None
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

fn encode_field(
    field: &str,
    delimiter: u8,
    quote: u8,
    first_field_of_first_record: bool,
    sole_empty: bool,
) -> String {
    let bytes = field.as_bytes();
    let edge_space = bytes.first().is_some_and(|b| *b == b' ' || *b == b'\t')
        || bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t');
    let starts_bom = first_field_of_first_record && field.starts_with('\u{feff}');
    let must_quote = field.is_empty() && sole_empty
        || edge_space
        || starts_bom
        || bytes
            .iter()
            .any(|b| *b == delimiter || *b == quote || *b == b'\r' || *b == b'\n');

    if !must_quote {
        return field.to_string();
    }
    let mut out = String::with_capacity(field.len() + 2);
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
    out
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let mut originals: Vec<&[String]> = Vec::new();
    if let Some(header) = &table.header {
        originals.push(header);
    }
    for row in &table.rows {
        originals.push(row);
    }

    for (record, row) in originals.iter().enumerate() {
        for (column, field) in row.iter().enumerate() {
            if let Some(byte) = field.as_bytes().iter().position(|b| *b == 0) {
                return Err(WriteError::Nul {
                    record,
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

    for (record, row) in originals.iter().enumerate() {
        if row.is_empty() {
            return Err(WriteError::ZeroColumns { record });
        }
    }

    if config.mode == WriteMode::Canonical {
        if let Some(header) = &table.header {
            if table.duplicates == DuplicatePolicy::Reject {
                if let Some((name, first, second)) = find_duplicate(header) {
                    return Err(WriteError::DuplicateHeader {
                        name,
                        first,
                        second,
                    });
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
                let mut keep = Vec::new();
                match table.duplicates {
                    DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                        let mut seen: HashMap<&str, usize> = HashMap::new();
                        for (i, name) in header.iter().enumerate() {
                            if !seen.contains_key(name.as_str()) {
                                seen.insert(name, i);
                                keep.push(i);
                            }
                        }
                    }
                    DuplicatePolicy::KeepLast => {
                        let mut last: HashMap<&str, usize> = HashMap::new();
                        for (i, name) in header.iter().enumerate() {
                            last.insert(name, i);
                        }
                        keep.extend(last.values().copied());
                    }
                }
                keep.sort_by(|a, b| header[*a].as_bytes().cmp(header[*b].as_bytes()));
                let projected_header = keep.iter().map(|&i| header[i].clone()).collect();
                output_records.push(projected_header);
                let mut rows: Vec<Vec<String>> = table
                    .rows
                    .iter()
                    .map(|row| keep.iter().map(|&i| row[i].clone()).collect())
                    .collect();
                rows.sort_by(|a, b| compare_rows(a, b));
                output_records.extend(rows);
            } else {
                let mut rows = table.rows.clone();
                rows.sort_by(|a, b| compare_rows(a, b));
                output_records.extend(rows);
            }
        }
    }

    if output_records.is_empty() {
        return Ok(String::new());
    }

    let canonical = config.mode == WriteMode::Canonical;
    let ending: &[u8] = if canonical {
        b"\n"
    } else {
        match config.line_ending {
            LineEnding::Lf => b"\n",
            LineEnding::CrLf => b"\r\n",
        }
    };
    let mut output = Vec::new();
    for (record_index, row) in output_records.iter().enumerate() {
        if record_index > 0 {
            output.extend_from_slice(ending);
        }
        for (column_index, field) in row.iter().enumerate() {
            if column_index > 0 {
                output.push(config.delimiter);
            }
            let encoded = encode_field(
                field,
                config.delimiter,
                config.quote,
                record_index == 0 && column_index == 0,
                row.len() == 1,
            );
            output.extend_from_slice(encoded.as_bytes());
        }
    }
    if canonical || config.final_terminator {
        output.extend_from_slice(ending);
    }
    Ok(String::from_utf8(output).unwrap())
}