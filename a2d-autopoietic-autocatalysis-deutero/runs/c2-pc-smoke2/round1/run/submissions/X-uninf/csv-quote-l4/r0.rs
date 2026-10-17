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
    let mut pos = if input.starts_with('\u{feff}') { 3 } else { 0 };

    if pos == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut record_index = 0usize;
    let mut field_started = false;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut quote_start = 0usize;

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
                let ch = input[pos..].chars().next().expect("valid UTF-8");
                field.push(ch);
                pos += ch.len_utf8();
            }
            continue;
        }

        if after_quote {
            if b == delimiter {
                record.push(std::mem::take(&mut field));
                field_started = false;
                after_quote = false;
                pos += 1;
            } else if b == b'\n' || b == b'\r' {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                record_index += 1;
                field_started = false;
                after_quote = false;
                pos += if b == b'\r' { 2 } else { 1 };
            } else {
                return Err(CsvError::TrailingByte {
                    record: record_index,
                    byte: pos,
                });
            }
            continue;
        }

        if b == delimiter {
            record.push(std::mem::take(&mut field));
            field_started = false;
            pos += 1;
        } else if b == b'\n' || b == b'\r' {
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            record_index += 1;
            field_started = false;
            pos += if b == b'\r' { 2 } else { 1 };
        } else if b == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: pos,
                });
            }
            in_quotes = true;
            quote_start = pos;
            field_started = true;
            pos += 1;
        } else {
            let ch = input[pos..].chars().next().expect("valid UTF-8");
            field.push(ch);
            field_started = true;
            pos += ch.len_utf8();
        }
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_start,
        });
    }

    if !record.is_empty() || !field.is_empty() || field_started || after_quote {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn find_duplicate_header(header: &[String]) -> Option<CsvError> {
    for second in 0..header.len() {
        if let Some(first) = header[..second].iter().position(|name| name == &header[second]) {
            return Some(CsvError::DuplicateHeader {
                name: header[second].clone(),
                first,
                second,
            });
        }
    }
    None
}

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let (header, rows, header_record_index) = if config.headers {
        if records.is_empty() {
            (None, Vec::new(), None)
        } else {
            let mut iter = records.into_iter();
            (iter.next(), iter.collect(), Some(0usize))
        }
    } else {
        (None, records, None)
    };

    if let Some(ref header_cells) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            if let Some(error) = find_duplicate_header(header_cells) {
                return Err(error);
            }
        }

        let expected = header_cells.len();
        for (idx, row) in rows.iter().enumerate() {
            if row.len() != expected {
                return Err(CsvError::HeaderWidth {
                    record: header_record_index.unwrap_or(0) + idx + 1,
                    expected,
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
        validate_dialect(config.delimiter, config.quote).map_err(|component| {
            CsvError::InvalidConfig { component }
        })?;
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

        let result = parse_with_dialect(&self.input, self.config.delimiter, self.config.quote)
            .and_then(|records| build_table(records, self.config));
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

fn valid_integer_syntax(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let start = usize::from(bytes[0] == b'-');
    if start == bytes.len() {
        return false;
    }
    if bytes[start] == b'0' {
        return start == 0 && bytes.len() == 1;
    }
    if !(b'1'..=b'9').contains(&bytes[start]) {
        return false;
    }
    bytes[start + 1..].iter().all(u8::is_ascii_digit)
}

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    if !valid_integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>()
        .map_err(|_| CellErrorKind::IntegerOverflow)
}

fn valid_decimal_syntax(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let negative = bytes[0] == b'-';
    let start = usize::from(negative);
    let dot = match bytes[start..].iter().position(|&b| b == b'.') {
        Some(index) => start + index,
        None => return false,
    };
    if bytes.len() != dot + 3 {
        return false;
    }
    let integer = &bytes[start..dot];
    if integer.is_empty() {
        return false;
    }
    let integer_ok = if integer[0] == b'0' {
        integer.len() == 1
    } else {
        (b'1'..=b'9').contains(&integer[0])
            && integer[1..].iter().all(u8::is_ascii_digit)
    };
    if !integer_ok || !bytes[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    !(negative && integer == b"0" && bytes[dot + 1..] == *b"00")
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !valid_decimal_syntax(s) {
        return Err(CellErrorKind::InvalidDecimal);
    }

    let negative = s.starts_with('-');
    let unsigned = if negative { &s[1..] } else { s };
    let dot = unsigned.find('.').expect("validated decimal");
    let whole = unsigned[..dot]
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let frac = unsigned[dot + 1..]
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let mut hundredths = whole
        .checked_mul(100)
        .and_then(|v| v.checked_add(frac))
        .ok_or(CellErrorKind::DecimalOverflow)?;
    if negative {
        hundredths = -hundredths;
    }
    i64::try_from(hundredths).map_err(|_| CellErrorKind::DecimalOverflow)
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
        ColumnType::Text => Ok(Value::Text(text.to_owned())),
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
    let mut output_rows = Vec::new();
    let mut errors = Vec::new();

    for (row_index, row) in table.rows.iter().enumerate() {
        let mut values = vec![None; columns.len()];
        let mut failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let error = if let Some(column_index) = resolved {
                if let Some(text) = row.get(column_index) {
                    match evaluate_cell(text, spec.kind, spec.nullable) {
                        Ok(value) => {
                            values[spec_index] = Some(value);
                            None
                        }
                        Err(kind) => Some(CellError {
                            row: row_index,
                            spec: spec_index,
                            column: Some(column_index),
                            kind,
                        }),
                    }
                } else {
                    Some(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column_index),
                        kind: CellErrorKind::MissingCell,
                    })
                }
            } else {
                Some(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: None,
                    kind: CellErrorKind::UnknownColumn,
                })
            };

            if let Some(error) = error {
                errors.push(error);
                failed = true;
                if policy != ErrorPolicy::Collect {
                    break;
                }
            }
        }

        output_rows.push(values);
        if failed && policy == ErrorPolicy::Stop {
            break;
        }
    }

    TypedResult {
        rows: output_rows,
        errors,
    }
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
    InvalidConfig {
        component: ConfigPart,
    },
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

fn canonical_projection(
    header: &[String],
    policy: DuplicatePolicy,
) -> Vec<usize> {
    let mut chosen: Vec<(usize, usize)> = Vec::new();
    for (index, name) in header.iter().enumerate() {
        if let Some(existing) = chosen.iter_mut().find(|(i, _)| header[*i] == *name) {
            if policy == DuplicatePolicy::KeepLast {
                existing.0 = index;
            }
        } else {
            chosen.push((index, index));
        }
    }

    chosen.sort_by(|a, b| header[a.0].as_bytes().cmp(header[b.0].as_bytes()));
    chosen.into_iter().map(|(index, _)| index).collect()
}

fn compare_rows(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (left, right) in a.iter().zip(b.iter()) {
        match left.as_bytes().cmp(right.as_bytes()) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
    }
    a.len().cmp(&b.len())
}

fn should_quote(
    field: &str,
    delimiter: u8,
    quote: u8,
    first_record_first_field: bool,
    sole_empty_field: bool,
) -> bool {
    let bytes = field.as_bytes();
    bytes.contains(&delimiter)
        || bytes.contains(&quote)
        || bytes.contains(&b'\r')
        || bytes.contains(&b'\n')
        || bytes.first().is_some_and(|b| *b == b' ' || *b == b'\t')
        || bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t')
        || (first_record_first_field && field.starts_with('\u{feff}'))
        || (sole_empty_field && field.is_empty())
}

fn encode_record(
    record: &[String],
    delimiter: u8,
    quote: u8,
    first_output_record: bool,
) -> String {
    let mut out = String::new();
    let sole_empty = record.len() == 1 && record[0].is_empty();

    for (index, field) in record.iter().enumerate() {
        if index > 0 {
            out.push(delimiter as char);
        }

        if should_quote(
            field,
            delimiter,
            quote,
            first_output_record && index == 0,
            sole_empty,
        ) {
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

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote).map_err(|component| {
        WriteError::InvalidConfig { component }
    })?;

    let mut original_records: Vec<&[String]> = Vec::new();
    if let Some(header) = &table.header {
        original_records.push(header);
    }
    original_records.extend(table.rows.iter().map(Vec::as_slice));

    for (record_index, record) in original_records.iter().enumerate() {
        for (column_index, field) in record.iter().enumerate() {
            if let Some(byte) = field.as_bytes().iter().position(|&b| b == 0) {
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

    if config.mode == WriteMode::Canonical {
        if table.header.is_some() && table.duplicates == DuplicatePolicy::Reject {
            if let Some(error) = table.header.as_ref().and_then(|h| find_duplicate_header(h)) {
                if let CsvError::DuplicateHeader {
                    name,
                    first,
                    second,
                } = error
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

    let mut output_records: Vec<Vec<String>> = match config.mode {
        WriteMode::Preserve => original_records
            .into_iter()
            .map(|record| record.to_vec())
            .collect(),
        WriteMode::Canonical => {
            if let Some(header) = &table.header {
                let projection = canonical_projection(header, table.duplicates);
                let projected_header = projection
                    .iter()
                    .map(|&index| header[index].clone())
                    .collect::<Vec<_>>();
                let mut projected_rows = table
                    .rows
                    .iter()
                    .map(|row| {
                        projection
                            .iter()
                            .map(|&index| row[index].clone())
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                projected_rows.sort_by(|a, b| compare_rows(a, b));
                let mut records = Vec::with_capacity(projected_rows.len() + 1);
                records.push(projected_header);
                records.extend(projected_rows);
                records
            } else {
                let mut rows = table.rows.clone();
                rows.sort_by(|a, b| compare_rows(a, b));
                rows
            }
        }
    };

    if output_records.is_empty() {
        return Ok(String::new());
    }

    let mut output = String::new();
    let canonical = config.mode == WriteMode::Canonical;
    let terminator = if canonical {
        "\n"
    } else {
        match config.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    };

    for (index, record) in output_records.iter().enumerate() {
        if index > 0 {
            output.push_str(terminator);
        }
        output.push_str(&encode_record(
            record,
            config.delimiter,
            config.quote,
            index == 0,
        ));
    }

    if canonical || config.final_terminator {
        output.push_str(terminator);
    }

    output_records.clear();
    Ok(output)
}