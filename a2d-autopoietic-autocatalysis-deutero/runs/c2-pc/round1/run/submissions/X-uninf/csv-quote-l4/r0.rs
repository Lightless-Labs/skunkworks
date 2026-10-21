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
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|v| v == name)
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

fn parse_with(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut start = 0;
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        start = 3;
    }
    if start == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut i = start;
    let mut record = 0usize;
    let mut at_field_start = true;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut opening_quote = 0usize;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul { record, byte: i });
        }

        if !in_quotes && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr { record, byte: i });
        }

        if in_quotes {
            if b == quote {
                if bytes.get(i + 1) == Some(&quote) {
                    field.push(quote as char);
                    i += 2;
                    continue;
                }
                in_quotes = false;
                after_quote = true;
                i += 1;
                continue;
            }
            let ch = input[i..].chars().next().unwrap();
            field.push(ch);
            i += ch.len_utf8();
            continue;
        }

        if after_quote {
            if b == delimiter {
                row.push(std::mem::take(&mut field));
                at_field_start = true;
                after_quote = false;
                i += 1;
                continue;
            }
            if b == b'\n' || b == b'\r' {
                row.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut row));
                record += 1;
                at_field_start = true;
                after_quote = false;
                i += if b == b'\r' { 2 } else { 1 };
                continue;
            }
            return Err(CsvError::TrailingByte { record, byte: i });
        }

        if b == delimiter {
            row.push(std::mem::take(&mut field));
            at_field_start = true;
            i += 1;
            continue;
        }
        if b == b'\n' || b == b'\r' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            record += 1;
            at_field_start = true;
            i += if b == b'\r' { 2 } else { 1 };
            continue;
        }
        if b == quote {
            if at_field_start {
                in_quotes = true;
                at_field_start = false;
                opening_quote = i;
                i += 1;
                continue;
            }
            return Err(CsvError::UnexpectedQuote { record, byte: i });
        }

        let ch = input[i..].chars().next().unwrap();
        field.push(ch);
        at_field_start = false;
        i += ch.len_utf8();
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record,
            byte: opening_quote,
        });
    }

    if after_quote || !row.is_empty() || !field.is_empty() || !at_field_start {
        row.push(field);
        records.push(row);
    } else if !row.is_empty() {
        records.push(row);
    }
    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with(input, b',', b'"')
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
        seen.insert(name, second);
    }
    None
}

fn table_from_input(input: &str, config: Config) -> Result<Table, CsvError> {
    let records = parse_with(input, config.delimiter, config.quote)?;
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
            if let Some(error) = duplicate_header(header) {
                return Err(error);
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
        let result = table_from_input(&self.input, self.config);
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
    if digits.is_empty() {
        return false;
    }
    if digits == b"0" {
        return b[0] != b'-';
    }
    digits[0].is_ascii_digit()
        && digits[0] != b'0'
        && digits.iter().all(u8::is_ascii_digit)
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    let body = if b.first() == Some(&b'-') { &b[1..] } else { b };
    let Some(dot) = body.iter().position(|&c| c == b'.') else {
        return false;
    };
    if body[dot + 1..].len() != 2 || !body[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    let integer = &body[..dot];
    if integer == b"0" {
        return b.first() != Some(&b'-') || body[dot + 1..] != *b"00";
    }
    !integer.is_empty()
        && integer[0].is_ascii_digit()
        && integer[0] != b'0'
        && integer.iter().all(u8::is_ascii_digit)
}

fn parse_decimal(s: &str) -> Option<i64> {
    let negative = s.starts_with('-');
    let body = if negative { &s[1..] } else { s };
    let dot = body.find('.')?;
    let whole: i128 = body[..dot].parse().ok()?;
    let frac: i128 = body[dot + 1..].parse().ok()?;
    let value = whole.checked_mul(100)?.checked_add(frac)?;
    let signed = if negative { -value } else { value };
    i64::try_from(signed).ok()
}

fn eval_cell(s: &str, spec: &ColumnSpec) -> Result<Value, CellErrorKind> {
    if s.is_empty() {
        if spec.nullable {
            return Ok(Value::Null);
        }
        if spec.kind != ColumnType::Text {
            return Err(CellErrorKind::Empty);
        }
    }

    match spec.kind {
        ColumnType::Text => Ok(Value::Text(s.to_owned())),
        ColumnType::Integer => {
            if !integer_syntax(s) {
                return Err(CellErrorKind::InvalidInteger);
            }
            s.parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| CellErrorKind::IntegerOverflow)
        }
        ColumnType::Decimal => {
            if !decimal_syntax(s) {
                return Err(CellErrorKind::InvalidDecimal);
            }
            parse_decimal(s)
                .map(Value::Decimal)
                .ok_or(CellErrorKind::DecimalOverflow)
        }
        ColumnType::Boolean => match s {
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
        let mut out = vec![None; columns.len()];
        for (spec_index, spec) in columns.iter().enumerate() {
            let column = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };
            let (value, error) = match column {
                None => (
                    None,
                    Some(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: None,
                        kind: CellErrorKind::UnknownColumn,
                    }),
                ),
                Some(index) => match row.get(index) {
                    None => (
                        None,
                        Some(CellError {
                            row: row_index,
                            spec: spec_index,
                            column: Some(index),
                            kind: CellErrorKind::MissingCell,
                        }),
                    ),
                    Some(cell) => match eval_cell(cell, spec) {
                        Ok(value) => (Some(value), None),
                        Err(kind) => (
                            None,
                            Some(CellError {
                                row: row_index,
                                spec: spec_index,
                                column: Some(index),
                                kind,
                            }),
                        ),
                    },
                },
            };

            out[spec_index] = value;
            if let Some(error) = error {
                result.errors.push(error);
                if policy == ErrorPolicy::Stop {
                    result.rows.push(out);
                    return result;
                }
                if policy == ErrorPolicy::FirstPerRow {
                    break;
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

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let mut records: Vec<&[String]> = Vec::new();
    if let Some(header) = &table.header {
        records.push(header);
    }
    records.extend(table.rows.iter().map(Vec::as_slice));

    for (record, row) in records.iter().enumerate() {
        for (column, cell) in row.iter().enumerate() {
            if let Some(byte) = cell.as_bytes().iter().position(|&b| b == 0) {
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

    if config.mode == WriteMode::Canonical {
        if let Some(header) = &table.header {
            if table.duplicates == DuplicatePolicy::Reject {
                if let Some(CsvError::DuplicateHeader {
                    name,
                    first,
                    second,
                }) = duplicate_header(header)
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

    let mut output_records: Vec<Vec<String>> = Vec::new();
    if config.mode == WriteMode::Preserve {
        output_records.extend(records.iter().map(|row| row.to_vec()));
    } else if let Some(header) = &table.header {
        let mut chosen: HashMap<&str, usize> = HashMap::new();
        for (i, name) in header.iter().enumerate() {
            match table.duplicates {
                DuplicatePolicy::KeepLast => {
                    chosen.insert(name.as_str(), i);
                }
                DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                    chosen.entry(name.as_str()).or_insert(i);
                }
            }
        }
        let mut columns: Vec<usize> = chosen.values().copied().collect();
        columns.sort_by(|&a, &b| header[a].as_bytes().cmp(header[b].as_bytes()));
        output_records.push(columns.iter().map(|&i| header[i].clone()).collect());

        let mut projected: Vec<Vec<String>> = table
            .rows
            .iter()
            .map(|row| columns.iter().map(|&i| row[i].clone()).collect())
            .collect();
        projected.sort_by(|a, b| compare_rows(a, b));
        output_records.extend(projected);
    } else {
        let mut rows = table.rows.clone();
        rows.sort_by(|a, b| compare_rows(a, b));
        output_records = rows;
    }

    let canonical = config.mode == WriteMode::Canonical;
    let line_ending = if canonical {
        "\n"
    } else {
        match config.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    };
    let mut output = String::new();
    for (ri, row) in output_records.iter().enumerate() {
        if ri > 0 {
            output.push_str(line_ending);
        }
        for (ci, cell) in row.iter().enumerate() {
            if ci > 0 {
                output.push(config.delimiter as char);
            }
            let bytes = cell.as_bytes();
            let quote_it = bytes.contains(&config.delimiter)
                || bytes.contains(&config.quote)
                || bytes.contains(&b'\r')
                || bytes.contains(&b'\n')
                || bytes.first().is_some_and(|&b| b == b' ' || b == b'\t')
                || bytes.last().is_some_and(|&b| b == b' ' || b == b'\t')
                || (ri == 0 && ci == 0 && cell.starts_with('\u{feff}'))
                || (row.len() == 1 && cell.is_empty());
            if quote_it {
                output.push(config.quote as char);
                for ch in cell.chars() {
                    if ch as u32 == config.quote as u32 {
                        output.push(config.quote as char);
                        output.push(config.quote as char);
                    } else {
                        output.push(ch);
                    }
                }
                output.push(config.quote as char);
            } else {
                output.push_str(cell);
            }
        }
    }
    if !output_records.is_empty() {
        if canonical || config.final_terminator {
            output.push_str(line_ending);
        }
    }
    Ok(output)
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