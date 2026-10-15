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
            DuplicatePolicy::KeepLast => header.iter().rposition(|cell| cell == name),
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|cell| cell == name)
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
    let mut field = Vec::new();
    let mut field_started = false;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut quote_start = 0usize;
    let mut record_index = 0usize;

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
                    field.push(quote);
                    i += 2;
                } else {
                    in_quotes = false;
                    after_quote = true;
                    i += 1;
                }
            } else {
                field.push(b);
                i += 1;
            }
            continue;
        }

        if b == b'\r' {
            if i + 1 >= bytes.len() || bytes[i + 1] != b'\n' {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: i,
                });
            }
            if after_quote || field_started || !row.is_empty() {
                row.push(String::from_utf8(field).unwrap());
                field = Vec::new();
                field_started = false;
                after_quote = false;
            } else {
                row.push(String::new());
            }
            records.push(row);
            row = Vec::new();
            record_index += 1;
            i += 2;
            continue;
        }

        if b == b'\n' {
            if after_quote || field_started || !row.is_empty() {
                row.push(String::from_utf8(field).unwrap());
                field = Vec::new();
                field_started = false;
                after_quote = false;
            } else {
                row.push(String::new());
            }
            records.push(row);
            row = Vec::new();
            record_index += 1;
            i += 1;
            continue;
        }

        if after_quote {
            if b == delimiter {
                row.push(String::from_utf8(field).unwrap());
                field = Vec::new();
                field_started = false;
                after_quote = false;
                i += 1;
                continue;
            }
            return Err(CsvError::TrailingByte {
                record: record_index,
                byte: i,
            });
        }

        if b == delimiter {
            row.push(String::from_utf8(field).unwrap());
            field = Vec::new();
            field_started = false;
            i += 1;
            continue;
        }

        if b == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: i,
                });
            }
            field_started = true;
            in_quotes = true;
            quote_start = i;
            i += 1;
            continue;
        }

        field.push(b);
        field_started = true;
        i += 1;
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_start,
        });
    }

    if after_quote || field_started || !row.is_empty() {
        row.push(String::from_utf8(field).unwrap());
        records.push(row);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
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

        let result = parse_with_dialect(
            &self.input,
            self.config.delimiter,
            self.config.quote,
        )
        .and_then(|records| build_table(records, self.config));

        self.finished = Some(result.clone());
        result
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.finished = None;
    }
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

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let mut iter = records.into_iter();
    let header = if config.headers { iter.next() } else { None };
    let rows: Vec<Vec<String>> = iter.collect();

    if let Some(header_fields) = &header {
        if config.duplicates == DuplicatePolicy::Reject {
            if let Some(error) = duplicate_header(header_fields) {
                return Err(error);
            }
        }
        for (index, row) in rows.iter().enumerate() {
            if row.len() != header_fields.len() {
                return Err(CsvError::HeaderWidth {
                    record: index + 1,
                    expected: header_fields.len(),
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

pub fn read_csv(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| CsvError::InvalidConfig { component })?;
    let records = parse_with_dialect(input, config.delimiter, config.quote)?;
    build_table(records, config)
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
        return start + 1 == bytes.len();
    }
    if !(b'1'..=b'9').contains(&bytes[start]) {
        return false;
    }
    bytes[start + 1..].iter().all(u8::is_ascii_digit)
}

fn decimal_value(s: &str) -> Result<i64, CellErrorKind> {
    let bytes = s.as_bytes();
    let start = usize::from(bytes.first() == Some(&b'-'));
    if bytes.len() < start + 4 {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let dot = bytes.len() - 3;
    if bytes[dot] != b'.'
        || !bytes[dot + 1..].iter().all(u8::is_ascii_digit)
        || dot <= start
    {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let integral = &bytes[start..dot];
    if integral == b"0" {
        if start == 1 {
            return Err(CellErrorKind::InvalidDecimal);
        }
    } else if integral.first().is_none_or(|b| !(b'1'..=b'9').contains(b))
        || !integral[1..].iter().all(u8::is_ascii_digit)
    {
        return Err(CellErrorKind::InvalidDecimal);
    }

    let mut magnitude: i128 = 0;
    for &b in integral {
        magnitude = magnitude * 10 + (b - b'0') as i128;
    }
    magnitude = magnitude * 100
        + (bytes[dot + 1] - b'0') as i128 * 10
        + (bytes[dot + 2] - b'0') as i128;
    if start == 1 {
        magnitude = -magnitude;
    }
    i64::try_from(magnitude).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn evaluate_cell(
    cell: &str,
    spec: &ColumnSpec,
) -> Result<Value, CellErrorKind> {
    if spec.nullable && cell.is_empty() {
        return Ok(Value::Null);
    }
    if cell.is_empty() && spec.kind != ColumnType::Text {
        return Err(CellErrorKind::Empty);
    }

    match spec.kind {
        ColumnType::Text => Ok(Value::Text(cell.to_string())),
        ColumnType::Integer => {
            if !integer_syntax(cell) {
                return Err(CellErrorKind::InvalidInteger);
            }
            cell.parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| CellErrorKind::IntegerOverflow)
        }
        ColumnType::Decimal => decimal_value(cell).map(Value::Decimal),
        ColumnType::Boolean => match cell {
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
                continue;
            }

            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let (column, evaluated) = match resolved {
                None => (None, Err(CellErrorKind::UnknownColumn)),
                Some(index) => match row.get(index) {
                    None => (Some(index), Err(CellErrorKind::MissingCell)),
                    Some(cell) => (Some(index), evaluate_cell(cell, spec)),
                },
            };

            match evaluated {
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
                        ErrorPolicy::FirstPerRow => row_failed = true,
                        ErrorPolicy::Stop => {
                            result.rows.push(output);
                            return result;
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
    DuplicateHeader { name: String, first: usize, second: usize },
}

fn all_original_records<'a>(table: &'a Table) -> Vec<(usize, &'a [String])> {
    let mut records = Vec::new();
    let mut next_record = 0;
    if let Some(header) = &table.header {
        records.push((0, header.as_slice()));
        next_record = 1;
    }
    for row in &table.rows {
        records.push((next_record, row.as_slice()));
        next_record += 1;
    }
    records
}

fn quote_field(
    field: &str,
    delimiter: u8,
    quote: u8,
    first_field_first_record: bool,
    sole_empty: bool,
) -> String {
    let bytes = field.as_bytes();
    let starts_boundary_space =
        bytes.first().is_some_and(|b| *b == b' ' || *b == b'\t');
    let ends_boundary_space =
        bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t');
    let starts_bom = first_field_first_record && field.starts_with('\u{feff}');
    let needs_quote = field.as_bytes().contains(&delimiter)
        || field.as_bytes().contains(&quote)
        || field.as_bytes().contains(&b'\r')
        || field.as_bytes().contains(&b'\n')
        || starts_boundary_space
        || ends_boundary_space
        || starts_bom
        || sole_empty;

    if !needs_quote {
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

fn duplicate_header_error(header: &[String]) -> Option<WriteError> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (second, name) in header.iter().enumerate() {
        if let Some(&first) = seen.get(name.as_str()) {
            return Some(WriteError::DuplicateHeader {
                name: name.clone(),
                first,
                second,
            });
        }
        seen.insert(name.as_str(), second);
    }
    None
}

fn canonical_header_columns(
    header: &[String],
    policy: DuplicatePolicy,
) -> Vec<usize> {
    let mut chosen: HashMap<&str, usize> = HashMap::new();
    for (index, name) in header.iter().enumerate() {
        match policy {
            DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                chosen.entry(name.as_str()).or_insert(index);
            }
            DuplicatePolicy::KeepLast => {
                chosen.insert(name.as_str(), index);
            }
        }
    }
    let mut columns: Vec<usize> = chosen.into_values().collect();
    columns.sort_by(|&a, &b| header[a].as_bytes().cmp(header[b].as_bytes()));
    columns
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

fn encode_records(
    records: &[Vec<String>],
    delimiter: u8,
    quote: u8,
    ending: &str,
    final_terminator: bool,
) -> String {
    if records.is_empty() {
        return String::new();
    }

    let mut output = String::new();
    for (record_index, record) in records.iter().enumerate() {
        if record_index > 0 {
            output.push_str(ending);
        }
        for (column_index, field) in record.iter().enumerate() {
            if column_index > 0 {
                output.push(delimiter as char);
            }
            let sole_empty = record.len() == 1 && field.is_empty();
            output.push_str(&quote_field(
                field,
                delimiter,
                quote,
                record_index == 0 && column_index == 0,
                sole_empty,
            ));
        }
    }
    if final_terminator {
        output.push_str(ending);
    }
    output
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let originals = all_original_records(table);

    for (record, cells) in &originals {
        for (column, cell) in cells.iter().enumerate() {
            if let Some(byte) = cell.as_bytes().iter().position(|b| *b == 0) {
                return Err(WriteError::Nul {
                    record: *record,
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

    for (record, cells) in &originals {
        if cells.is_empty() {
            return Err(WriteError::ZeroColumns { record: *record });
        }
    }

    if config.mode == WriteMode::Canonical {
        if let (Some(header), DuplicatePolicy::Reject) =
            (&table.header, table.duplicates)
        {
            if let Some(error) = duplicate_header_error(header) {
                return Err(error);
            }
        }
    }

    let mut output_records = Vec::new();

    if config.mode == WriteMode::Preserve {
        if let Some(header) = &table.header {
            output_records.push(header.clone());
        }
        output_records.extend(table.rows.iter().cloned());

        let ending = match config.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        };
        return Ok(encode_records(
            &output_records,
            config.delimiter,
            config.quote,
            ending,
            config.final_terminator,
        ));
    }

    let mut rows = table.rows.clone();
    if let Some(header) = &table.header {
        let columns = canonical_header_columns(header, table.duplicates);
        let projected_header: Vec<String> =
            columns.iter().map(|&index| header[index].clone()).collect();
        rows = rows
            .iter()
            .map(|row| columns.iter().map(|&index| row[index].clone()).collect())
            .collect();
        rows.sort_by(|a, b| compare_rows(a, b));
        output_records.push(projected_header);
    } else {
        rows.sort_by(|a, b| compare_rows(a, b));
    }
    output_records.extend(rows);

    Ok(encode_records(
        &output_records,
        config.delimiter,
        config.quote,
        "\n",
        true,
    ))
}

fn main() {}