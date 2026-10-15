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

    if pos == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = Vec::new();
    let mut record_index = 0usize;
    let mut field_start = true;
    let mut in_quotes = false;
    let mut quote_open = 0usize;
    let mut after_quote = false;
    let mut record_has_content = false;

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
                if bytes.get(pos + 1) == Some(&quote) {
                    field.push(quote);
                    pos += 2;
                } else {
                    in_quotes = false;
                    after_quote = true;
                    pos += 1;
                }
            } else {
                field.push(byte);
                pos += 1;
            }
            record_has_content = true;
            continue;
        }

        if byte == b'\r' && bytes.get(pos + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: pos,
            });
        }

        if after_quote {
            if byte == delimiter {
                fields.push(String::from_utf8(field).unwrap());
                field = Vec::new();
                field_start = true;
                after_quote = false;
                record_has_content = true;
                pos += 1;
            } else if byte == b'\n' || byte == b'\r' {
                fields.push(String::from_utf8(field).unwrap());
                records.push(fields);
                fields = Vec::new();
                field = Vec::new();
                field_start = true;
                after_quote = false;
                record_has_content = false;
                record_index += 1;
                pos += if byte == b'\r' { 2 } else { 1 };
            } else {
                return Err(CsvError::TrailingByte {
                    record: record_index,
                    byte: pos,
                });
            }
            continue;
        }

        if byte == quote {
            if field_start {
                in_quotes = true;
                quote_open = pos;
                field_start = false;
                record_has_content = true;
                pos += 1;
            } else {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: pos,
                });
            }
        } else if byte == delimiter {
            fields.push(String::from_utf8(field).unwrap());
            field = Vec::new();
            field_start = true;
            record_has_content = true;
            pos += 1;
        } else if byte == b'\n' || byte == b'\r' {
            fields.push(String::from_utf8(field).unwrap());
            records.push(fields);
            fields = Vec::new();
            field = Vec::new();
            field_start = true;
            record_has_content = false;
            record_index += 1;
            pos += if byte == b'\r' { 2 } else { 1 };
        } else {
            field.push(byte);
            field_start = false;
            record_has_content = true;
            pos += 1;
        }
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_open,
        });
    }

    if record_has_content || !fields.is_empty() || !field.is_empty() || after_quote {
        fields.push(String::from_utf8(field).unwrap());
        records.push(fields);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_dialect(input, b',', b'"')
}

fn duplicate_header(header: &[String]) -> Option<CsvError> {
    for second in 0..header.len() {
        if let Some(first) = header[..second]
            .iter()
            .position(|value| value == &header[second])
        {
            return Some(CsvError::DuplicateHeader {
                name: header[second].clone(),
                first,
                second,
            });
        }
    }
    None
}

fn table_from_records(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
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

    if config.duplicates == DuplicatePolicy::Reject {
        if let Some(ref header) = header {
            if let Some(error) = duplicate_header(header) {
                return Err(error);
            }
        }
    }

    if let Some(ref header) = header {
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
        self.finished = true;
        let result = parse_dialect(&self.input, self.config.delimiter, self.config.quote)
            .and_then(|records| table_from_records(records, self.config));
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

fn valid_integer_syntax(value: &str) -> bool {
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
    bytes[start].is_ascii_digit()
        && bytes[start] != b'0'
        && bytes[start + 1..].iter().all(u8::is_ascii_digit)
}

fn parse_integer(value: &str) -> Result<i64, CellErrorKind> {
    if !valid_integer_syntax(value) {
        return Err(CellErrorKind::InvalidInteger);
    }
    value
        .parse::<i64>()
        .map_err(|_| CellErrorKind::IntegerOverflow)
}

fn valid_decimal_syntax(value: &str) -> bool {
    let bytes = value.as_bytes();
    let start = usize::from(bytes.first() == Some(&b'-'));
    if bytes.len() < start + 4 {
        return false;
    }

    let dot = bytes.len() - 3;
    if bytes[dot] != b'.'
        || !bytes[dot + 1..].iter().all(u8::is_ascii_digit)
        || dot <= start
    {
        return false;
    }

    let integral = &bytes[start..dot];
    if integral == b"0" {
        return start == 0;
    }

    integral[0].is_ascii_digit()
        && integral[0] != b'0'
        && integral[1..].iter().all(u8::is_ascii_digit)
}

fn parse_decimal(value: &str) -> Result<i64, CellErrorKind> {
    if !valid_decimal_syntax(value) {
        return Err(CellErrorKind::InvalidDecimal);
    }

    let negative = value.starts_with('-');
    let unsigned = if negative { &value[1..] } else { value };
    let magnitude = unsigned
        .as_bytes()
        .iter()
        .filter(|&&byte| byte != b'.')
        .try_fold(0i128, |acc, &byte| {
            acc.checked_mul(10)?.checked_add((byte - b'0') as i128)
        })
        .ok_or(CellErrorKind::DecimalOverflow)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn evaluate_cell(raw: Option<&String>, spec: &ColumnSpec) -> Result<Value, CellErrorKind> {
    let raw = raw.ok_or(CellErrorKind::MissingCell)?;
    if spec.nullable && raw.is_empty() {
        return Ok(Value::Null);
    }
    if raw.is_empty() {
        return match spec.kind {
            ColumnType::Text => Ok(Value::Text(String::new())),
            _ => Err(CellErrorKind::Empty),
        };
    }

    match spec.kind {
        ColumnType::Text => Ok(Value::Text(raw.clone())),
        ColumnType::Integer => parse_integer(raw).map(Value::Integer),
        ColumnType::Decimal => parse_decimal(raw).map(Value::Decimal),
        ColumnType::Boolean => match raw.as_str() {
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
        let mut output = Vec::with_capacity(columns.len());

        for (spec_index, spec) in columns.iter().enumerate() {
            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let (column, value_result) = match resolved {
                None => (None, Err(CellErrorKind::UnknownColumn)),
                Some(index) => (Some(index), evaluate_cell(row.get(index), spec)),
            };

            match value_result {
                Ok(value) => output.push(Some(value)),
                Err(kind) => {
                    output.push(None);
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column,
                        kind,
                    });

                    if policy != ErrorPolicy::Collect {
                        output.resize(columns.len(), None);
                        result.rows.push(output);
                        if policy == ErrorPolicy::Stop {
                            break 'rows;
                        }
                        continue 'rows;
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
    ZeroColumns { record: usize },
    DuplicateHeader {
        name: String,
        first: usize,
        second: usize,
    },
}

fn write_duplicate(header: &[String]) -> Option<WriteError> {
    for second in 0..header.len() {
        if let Some(first) = header[..second]
            .iter()
            .position(|value| value == &header[second])
        {
            return Some(WriteError::DuplicateHeader {
                name: header[second].clone(),
                first,
                second,
            });
        }
    }
    None
}

fn scan_nul_record(record: &[String], record_index: usize) -> Option<WriteError> {
    for (column, value) in record.iter().enumerate() {
        if let Some(byte) = value.as_bytes().iter().position(|&byte| byte == 0) {
            return Some(WriteError::Nul {
                record: record_index,
                column,
                byte,
            });
        }
    }
    None
}

fn compare_rows(left: &[String], right: &[String]) -> std::cmp::Ordering {
    for (a, b) in left.iter().zip(right.iter()) {
        let ordering = a.as_bytes().cmp(b.as_bytes());
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
}

fn encode_field(
    value: &str,
    delimiter: u8,
    quote: u8,
    first_field_first_record: bool,
    sole_empty: bool,
) -> String {
    let bytes = value.as_bytes();
    let quote_it = bytes.contains(&delimiter)
        || bytes.contains(&quote)
        || bytes.contains(&b'\r')
        || bytes.contains(&b'\n')
        || bytes.first().is_some_and(|&byte| byte == b' ' || byte == b'\t')
        || bytes.last().is_some_and(|&byte| byte == b' ' || byte == b'\t')
        || (first_field_first_record && value.starts_with('\u{feff}'))
        || (sole_empty && value.is_empty());

    if !quote_it {
        return value.to_string();
    }

    let mut output = String::with_capacity(value.len() + 2);
    output.push(quote as char);
    for character in value.chars() {
        if character as u32 == quote as u32 {
            output.push(quote as char);
        }
        output.push(character);
    }
    output.push(quote as char);
    output
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    if let Some(header) = &table.header {
        if let Some(error) = scan_nul_record(header, 0) {
            return Err(error);
        }
    }
    let data_start = usize::from(table.header.is_some());
    for (index, row) in table.rows.iter().enumerate() {
        if let Some(error) = scan_nul_record(row, index + data_start) {
            return Err(error);
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

    if let Some(header) = &table.header {
        if header.is_empty() {
            return Err(WriteError::ZeroColumns { record: 0 });
        }
    }
    for (index, row) in table.rows.iter().enumerate() {
        if row.is_empty() {
            return Err(WriteError::ZeroColumns {
                record: index + data_start,
            });
        }
    }

    if config.mode == WriteMode::Canonical
        && table.header.is_some()
        && table.duplicates == DuplicatePolicy::Reject
    {
        if let Some(error) = write_duplicate(table.header.as_ref().unwrap()) {
            return Err(error);
        }
    }

    let mut output_records: Vec<Vec<String>> = Vec::new();
    if config.mode == WriteMode::Preserve {
        if let Some(header) = &table.header {
            output_records.push(header.clone());
        }
        output_records.extend(table.rows.clone());
    } else if let Some(header) = &table.header {
        let mut retained = Vec::new();
        for (index, name) in header.iter().enumerate() {
            let selected = match table.duplicates {
                DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                    header[..index].iter().all(|prior| prior != name)
                }
                DuplicatePolicy::KeepLast => {
                    header[index + 1..].iter().all(|later| later != name)
                }
            };
            if selected {
                retained.push((name.clone(), index));
            }
        }
        retained.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
        output_records.push(
            retained
                .iter()
                .map(|(name, _)| name.clone())
                .collect(),
        );

        let indexes: Vec<usize> = retained.iter().map(|(_, index)| *index).collect();
        let mut projected: Vec<Vec<String>> = table
            .rows
            .iter()
            .map(|row| indexes.iter().map(|&index| row[index].clone()).collect())
            .collect();
        projected.sort_by(|left, right| compare_rows(left, right));
        output_records.extend(projected);
    } else {
        let mut rows = table.rows.clone();
        rows.sort_by(|left, right| compare_rows(left, right));
        output_records = rows;
    }

    if output_records.is_empty() {
        return Ok(String::new());
    }

    let mut encoded_records = Vec::with_capacity(output_records.len());
    for (record_index, record) in output_records.iter().enumerate() {
        let sole_empty = record.len() == 1 && record[0].is_empty();
        let fields: Vec<String> = record
            .iter()
            .enumerate()
            .map(|(column, value)| {
                encode_field(
                    value,
                    config.delimiter,
                    config.quote,
                    record_index == 0 && column == 0,
                    sole_empty,
                )
            })
            .collect();
        encoded_records.push(fields.join(&(config.delimiter as char).to_string()));
    }

    let ending = match config.mode {
        WriteMode::Canonical => "\n",
        WriteMode::Preserve => match config.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        },
    };

    let mut output = encoded_records.join(ending);
    if config.mode == WriteMode::Canonical || config.final_terminator {
        output.push_str(ending);
    }
    Ok(output)
}

fn main() {}