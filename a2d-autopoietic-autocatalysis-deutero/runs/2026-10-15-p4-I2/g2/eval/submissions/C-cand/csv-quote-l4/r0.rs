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
    let mut pos = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };

    if pos == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = Vec::new();
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut field_start = true;
    let mut quote_start = 0usize;
    let mut record_index = 0usize;
    let mut ended_with_terminator = false;

    while pos < bytes.len() {
        let byte = bytes[pos];

        if byte == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: pos,
            });
        }

        if in_quotes {
            if byte == quote {
                if pos + 1 < bytes.len() && bytes[pos + 1] == quote {
                    field.push(quote);
                    pos += 2;
                    continue;
                }
                in_quotes = false;
                after_quote = true;
                pos += 1;
                continue;
            }
            field.push(byte);
            pos += 1;
            continue;
        }

        if byte == b'\r' {
            if pos + 1 >= bytes.len() || bytes[pos + 1] != b'\n' {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: pos,
                });
            }

            row.push(String::from_utf8(field).unwrap());
            records.push(row);
            row = Vec::new();
            field = Vec::new();
            field_start = true;
            after_quote = false;
            pos += 2;
            record_index += 1;
            ended_with_terminator = true;
            continue;
        }

        if after_quote {
            if byte == delimiter {
                row.push(String::from_utf8(field).unwrap());
                field = Vec::new();
                field_start = true;
                after_quote = false;
                pos += 1;
                ended_with_terminator = false;
                continue;
            }
            if byte == b'\n' {
                row.push(String::from_utf8(field).unwrap());
                records.push(row);
                row = Vec::new();
                field = Vec::new();
                field_start = true;
                after_quote = false;
                pos += 1;
                record_index += 1;
                ended_with_terminator = true;
                continue;
            }
            return Err(CsvError::TrailingByte {
                record: record_index,
                byte: pos,
            });
        }

        if byte == quote {
            if field_start {
                in_quotes = true;
                quote_start = pos;
                field_start = false;
                pos += 1;
                ended_with_terminator = false;
                continue;
            }
            return Err(CsvError::UnexpectedQuote {
                record: record_index,
                byte: pos,
            });
        }

        if byte == delimiter {
            row.push(String::from_utf8(field).unwrap());
            field = Vec::new();
            field_start = true;
            pos += 1;
            ended_with_terminator = false;
            continue;
        }

        if byte == b'\n' {
            row.push(String::from_utf8(field).unwrap());
            records.push(row);
            row = Vec::new();
            field = Vec::new();
            field_start = true;
            pos += 1;
            record_index += 1;
            ended_with_terminator = true;
            continue;
        }

        field.push(byte);
        field_start = false;
        pos += 1;
        ended_with_terminator = false;
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_start,
        });
    }

    if !ended_with_terminator {
        row.push(String::from_utf8(field).unwrap());
        records.push(row);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn duplicate_header(header: &[String]) -> Option<CsvError> {
    let mut seen: Vec<(&str, usize)> = Vec::new();
    for (second, name) in header.iter().enumerate() {
        if let Some((_, first)) = seen.iter().find(|(seen_name, _)| *seen_name == name) {
            return Some(CsvError::DuplicateHeader {
                name: name.clone(),
                first: *first,
                second,
            });
        }
        seen.push((name, second));
    }
    None
}

fn table_from_records(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let mut iter = records.into_iter();
    let header = if config.headers { iter.next() } else { None };
    let rows: Vec<Vec<String>> = iter.collect();

    if let Some(ref header) = header {
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

pub struct StreamReader {
    config: Config,
    input: String,
    finished: bool,
    cached: Option<Result<Table, CsvError>>,
}

impl StreamReader {
    pub fn new(config: Config) -> Result<Self, CsvError> {
        validate_dialect(config.delimiter, config.quote)
            .map_err(|component| CsvError::InvalidConfig { component })?;
        Ok(Self {
            config,
            input: String::new(),
            finished: false,
            cached: None,
        })
    }

    pub fn feed(&mut self, chunk: &str) -> Result<(), CsvError> {
        if self.finished {
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
        self.finished = true;
        self.cached = Some(result.clone());
        result
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.finished = false;
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

fn integer_syntax(value: &str) -> bool {
    let bytes = value.as_bytes();
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

fn parse_integer(value: &str) -> Result<i64, CellErrorKind> {
    if !integer_syntax(value) {
        return Err(CellErrorKind::InvalidInteger);
    }
    value
        .parse::<i64>()
        .map_err(|_| CellErrorKind::IntegerOverflow)
}

fn decimal_syntax(value: &str) -> bool {
    let bytes = value.as_bytes();
    let start = usize::from(bytes.first() == Some(&b'-'));
    if bytes.len() < start + 4 {
        return false;
    }

    let dot = bytes.len() - 3;
    if bytes[dot] != b'.' || !bytes[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }

    let integral = &bytes[start..dot];
    if integral.is_empty() {
        return false;
    }
    if integral == b"0" {
        return start == 0;
    }
    (b'1'..=b'9').contains(&integral[0])
        && integral[1..].iter().all(u8::is_ascii_digit)
}

fn parse_decimal(value: &str) -> Result<i64, CellErrorKind> {
    if !decimal_syntax(value) {
        return Err(CellErrorKind::InvalidDecimal);
    }

    let negative = value.starts_with('-');
    let body = if negative { &value[1..] } else { value };
    let (whole, fraction) = body.split_once('.').unwrap();

    let mut magnitude: i128 = 0;
    for byte in whole.bytes().chain(fraction.bytes()) {
        if byte == b'.' {
            continue;
        }
        magnitude = magnitude * 10 + i128::from(byte - b'0');
    }

    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn evaluate_cell(
    cell: &str,
    kind: ColumnType,
    nullable: bool,
) -> Result<Value, CellErrorKind> {
    if cell.is_empty() {
        if nullable {
            return Ok(Value::Null);
        }
        if kind != ColumnType::Text {
            return Err(CellErrorKind::Empty);
        }
    }

    match kind {
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
        let mut stop_all = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let (column, evaluated) = match resolved {
                None => (None, Err(CellErrorKind::UnknownColumn)),
                Some(index) => match row.get(index) {
                    None => (Some(index), Err(CellErrorKind::MissingCell)),
                    Some(cell) => (
                        Some(index),
                        evaluate_cell(cell, spec.kind, spec.nullable),
                    ),
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
                        ErrorPolicy::FirstPerRow => break,
                        ErrorPolicy::Stop => {
                            stop_all = true;
                            break;
                        }
                    }
                }
            }
        }

        result.rows.push(output);
        if stop_all {
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
    ZeroColumns {
        record: usize,
    },
    DuplicateHeader {
        name: String,
        first: usize,
        second: usize,
    },
}

fn compare_rows(a: &[String], b: &[String]) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    for (left, right) in a.iter().zip(b.iter()) {
        match left.as_bytes().cmp(right.as_bytes()) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    a.len().cmp(&b.len())
}

fn quote_field(field: &str, delimiter: u8, quote: u8, first: bool, sole: bool) -> String {
    let bytes = field.as_bytes();
    let edge_space = bytes
        .first()
        .is_some_and(|byte| *byte == b' ' || *byte == b'\t')
        || bytes
            .last()
            .is_some_and(|byte| *byte == b' ' || *byte == b'\t');
    let starts_bom = first && field.starts_with('\u{feff}');
    let must_quote = bytes.contains(&delimiter)
        || bytes.contains(&quote)
        || bytes.contains(&b'\r')
        || bytes.contains(&b'\n')
        || edge_space
        || starts_bom
        || (sole && field.is_empty());

    if !must_quote {
        return field.to_string();
    }

    let mut output = String::new();
    output.push(quote as char);
    for character in field.chars() {
        if character as u32 == quote as u32 {
            output.push(quote as char);
            output.push(quote as char);
        } else {
            output.push(character);
        }
    }
    output.push(quote as char);
    output
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let mut original_records: Vec<&[String]> = Vec::new();
    if let Some(header) = &table.header {
        original_records.push(header);
    }
    original_records.extend(table.rows.iter().map(Vec::as_slice));

    for (record_index, record) in original_records.iter().enumerate() {
        for (column_index, cell) in record.iter().enumerate() {
            if let Some(byte) = cell.as_bytes().iter().position(|byte| *byte == 0) {
                return Err(WriteError::Nul {
                    record: record_index,
                    column: column_index,
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

    for (record_index, record) in original_records.iter().enumerate() {
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

    let mut output_records: Vec<Vec<String>> = Vec::new();

    match config.mode {
        WriteMode::Preserve => {
            if let Some(header) = &table.header {
                output_records.push(header.clone());
            }
            output_records.extend(table.rows.clone());
        }
        WriteMode::Canonical => {
            if let Some(header) = &table.header {
                let mut retained = Vec::new();
                for (index, name) in header.iter().enumerate() {
                    let keep = match table.duplicates {
                        DuplicatePolicy::KeepLast => {
                            header.iter().rposition(|candidate| candidate == name) == Some(index)
                        }
                        DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                            header.iter().position(|candidate| candidate == name) == Some(index)
                        }
                    };
                    if keep {
                        retained.push(index);
                    }
                }

                retained.sort_by(|left, right| {
                    header[*left].as_bytes().cmp(header[*right].as_bytes())
                });

                output_records.push(
                    retained
                        .iter()
                        .map(|index| header[*index].clone())
                        .collect(),
                );

                let mut rows: Vec<Vec<String>> = table
                    .rows
                    .iter()
                    .map(|row| {
                        retained
                            .iter()
                            .map(|index| row[*index].clone())
                            .collect()
                    })
                    .collect();
                rows.sort_by(|left, right| compare_rows(left, right));
                output_records.extend(rows);
            } else {
                let mut rows = table.rows.clone();
                rows.sort_by(|left, right| compare_rows(left, right));
                output_records = rows;
            }
        }
    }

    if output_records.is_empty() {
        return Ok(String::new());
    }

    let mut rendered_records = Vec::with_capacity(output_records.len());
    for (record_index, record) in output_records.iter().enumerate() {
        let mut fields = Vec::with_capacity(record.len());
        for (column_index, field) in record.iter().enumerate() {
            fields.push(quote_field(
                field,
                config.delimiter,
                config.quote,
                record_index == 0 && column_index == 0,
                record.len() == 1,
            ));
        }
        rendered_records.push(fields.join(&(config.delimiter as char).to_string()));
    }

    let ending = match config.mode {
        WriteMode::Canonical => "\n",
        WriteMode::Preserve => match config.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        },
    };

    let mut output = rendered_records.join(ending);
    let append_final = match config.mode {
        WriteMode::Canonical => true,
        WriteMode::Preserve => config.final_terminator,
    };
    if append_final {
        output.push_str(ending);
    }
    Ok(output)
}

fn main() {}