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
            DuplicatePolicy::KeepLast => header.iter().rposition(|s| s == name),
            DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                header.iter().position(|s| s == name)
            }
        }
    }
}

fn validate_config(delimiter: u8, quote: u8) -> Result<(), ConfigPart> {
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
    let mut quoted = false;
    let mut after_quote = false;
    let mut quote_open = 0usize;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if !quoted && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
        }

        if quoted {
            if b == quote {
                if bytes.get(i + 1) == Some(&quote) {
                    field.push(quote as char);
                    i += 2;
                } else {
                    quoted = false;
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
            quoted = true;
            quote_open = i;
            field_started = true;
            i += 1;
        } else {
            let ch = input[i..].chars().next().unwrap();
            field.push(ch);
            field_started = true;
            i += ch.len_utf8();
        }
    }

    if quoted {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_open,
        });
    }

    if after_quote || field_started || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with(input, b',', b'"')
}

fn duplicate_header(header: &[String]) -> Option<CsvError> {
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

fn table_from_input(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_config(config.delimiter, config.quote).map_err(|component| {
        CsvError::InvalidConfig { component }
    })?;

    let mut records = parse_with(input, config.delimiter, config.quote)?;
    let header = if config.headers && !records.is_empty() {
        Some(records.remove(0))
    } else {
        None
    };

    if let Some(ref h) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            if let Some(error) = duplicate_header(h) {
                return Err(error);
            }
        }
        for (idx, row) in records.iter().enumerate() {
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
        rows: records,
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
        validate_config(config.delimiter, config.quote)
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
        if self.finished.is_none() {
            self.finished = Some(table_from_input(&self.input, self.config));
        }
        self.finished.as_ref().unwrap().clone()
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.finished = None;
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

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    let b = s.as_bytes();
    if b.is_empty() {
        return Err(CellErrorKind::InvalidInteger);
    }
    let digits = if b[0] == b'-' { &b[1..] } else { b };
    if digits.is_empty()
        || (digits[0] == b'0' && digits.len() != 1)
        || (digits[0] != b'0' && !digits[0].is_ascii_digit())
        || digits.iter().any(|d| !d.is_ascii_digit())
        || (b[0] == b'-' && digits == b"0")
    {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>().map_err(|_| CellErrorKind::IntegerOverflow)
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    let b = s.as_bytes();
    let body = if b.first() == Some(&b'-') {
        &b[1..]
    } else {
        b
    };
    if body.len() < 4 {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let dot = body.len() - 3;
    let integral = &body[..dot];
    let fraction = &body[dot + 1..];
    if body[dot] != b'.'
        || fraction.len() != 2
        || !fraction.iter().all(u8::is_ascii_digit)
        || integral.is_empty()
        || (integral.len() > 1 && integral[0] == b'0')
        || !integral.iter().all(u8::is_ascii_digit)
        || (b.first() == Some(&b'-') && integral == b"0" && fraction == b"00")
    {
        return Err(CellErrorKind::InvalidDecimal);
    }

    let negative = b.first() == Some(&b'-');
    let mut value: i128 = 0;
    for &d in integral {
        value = value * 10 + (d - b'0') as i128;
    }
    value = value * 100 + (fraction[0] - b'0') as i128 * 10 + (fraction[1] - b'0') as i128;
    if negative {
        value = -value;
    }
    i64::try_from(value).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn evaluate_cell(s: &str, spec: &ColumnSpec) -> Result<Value, CellErrorKind> {
    if spec.nullable && s.is_empty() {
        return Ok(Value::Null);
    }
    match spec.kind {
        ColumnType::Text => Ok(Value::Text(s.to_string())),
        ColumnType::Integer => {
            if s.is_empty() {
                Err(CellErrorKind::Empty)
            } else {
                parse_integer(s).map(Value::Integer)
            }
        }
        ColumnType::Decimal => {
            if s.is_empty() {
                Err(CellErrorKind::Empty)
            } else {
                parse_decimal(s).map(Value::Decimal)
            }
        }
        ColumnType::Boolean => {
            if s.is_empty() {
                Err(CellErrorKind::Empty)
            } else {
                match s {
                    "yes" | "1" => Ok(Value::Boolean(true)),
                    "no" | "0" => Ok(Value::Boolean(false)),
                    _ => Err(CellErrorKind::InvalidBoolean),
                }
            }
        }
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
                None => (
                    None,
                    Err(CellErrorKind::UnknownColumn),
                ),
                Some(index) => match row.get(index) {
                    None => (Some(index), Err(CellErrorKind::MissingCell)),
                    Some(cell) => (Some(index), evaluate_cell(cell, spec)),
                },
            };
            match value {
                Ok(v) => output[spec_index] = Some(v),
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
    DuplicateHeader { name: String, first: usize, second: usize },
}

fn write_duplicate_header(header: &[String]) -> Option<WriteError> {
    for second in 0..header.len() {
        if let Some(first) = header[..second].iter().position(|s| s == &header[second]) {
            return Some(WriteError::DuplicateHeader {
                name: header[second].clone(),
                first,
                second,
            });
        }
    }
    None
}

fn bytes_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.as_bytes().cmp(b.as_bytes())
}

fn row_cmp(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        let ordering = bytes_cmp(x, y);
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }
    a.len().cmp(&b.len())
}

fn encode_field(
    field: &str,
    delimiter: u8,
    quote: u8,
    first_field_first_record: bool,
    sole_empty_field: bool,
) -> String {
    let bytes = field.as_bytes();
    let starts_bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
    let quote_it = bytes.contains(&delimiter)
        || bytes.contains(&quote)
        || bytes.contains(&b'\r')
        || bytes.contains(&b'\n')
        || bytes.first() == Some(&b' ')
        || bytes.first() == Some(&b'\t')
        || bytes.last() == Some(&b' ')
        || bytes.last() == Some(&b'\t')
        || (first_field_first_record && starts_bom)
        || (sole_empty_field && field.is_empty());

    if !quote_it {
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
    validate_config(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let mut original_records: Vec<&[String]> = Vec::new();
    if let Some(header) = table.header.as_ref() {
        original_records.push(header);
    }
    for row in &table.rows {
        original_records.push(row);
    }

    for (record_index, row) in original_records.iter().enumerate() {
        for (column_index, field) in row.iter().enumerate() {
            if let Some(byte) = field.as_bytes().iter().position(|b| *b == 0) {
                return Err(WriteError::Nul {
                    record: record_index,
                    column: column_index,
                    byte,
                });
            }
        }
    }

    if let Some(header) = table.header.as_ref() {
        for (idx, row) in table.rows.iter().enumerate() {
            if row.len() != header.len() {
                return Err(WriteError::Width {
                    record: idx + 1,
                    expected: header.len(),
                    actual: row.len(),
                });
            }
        }
    }

    for (record_index, row) in original_records.iter().enumerate() {
        if row.is_empty() {
            return Err(WriteError::ZeroColumns {
                record: record_index,
            });
        }
    }

    if config.mode == WriteMode::Canonical {
        if let Some(header) = table.header.as_ref() {
            if table.duplicates == DuplicatePolicy::Reject {
                if let Some(error) = write_duplicate_header(header) {
                    return Err(error);
                }
            }
        }
    }

    let mut output_records: Vec<Vec<String>> = Vec::new();

    match config.mode {
        WriteMode::Preserve => {
            if let Some(header) = table.header.as_ref() {
                output_records.push(header.clone());
            }
            output_records.extend(table.rows.iter().cloned());
        }
        WriteMode::Canonical => {
            if let Some(header) = table.header.as_ref() {
                let mut retained = Vec::new();
                for (idx, name) in header.iter().enumerate() {
                    let keep = match table.duplicates {
                        DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                            !header[..idx].iter().any(|s| s == name)
                        }
                        DuplicatePolicy::KeepLast => {
                            !header[idx + 1..].iter().any(|s| s == name)
                        }
                    };
                    if keep {
                        retained.push((name.clone(), idx));
                    }
                }
                retained.sort_by(|a, b| bytes_cmp(&a.0, &b.0));
                output_records.push(retained.iter().map(|(name, _)| name.clone()).collect());
                let mut projected: Vec<Vec<String>> = table
                    .rows
                    .iter()
                    .map(|row| retained.iter().map(|(_, idx)| row[*idx].clone()).collect())
                    .collect();
                projected.sort_by(|a, b| row_cmp(a, b));
                output_records.extend(projected);
            } else {
                let mut rows = table.rows.clone();
                rows.sort_by(|a, b| row_cmp(a, b));
                output_records.extend(rows);
            }
        }
    }

    if output_records.is_empty() {
        return Ok(String::new());
    }

    let (line_ending, final_terminator) = match config.mode {
        WriteMode::Preserve => (
            match config.line_ending {
                LineEnding::Lf => "\n",
                LineEnding::CrLf => "\r\n",
            },
            config.final_terminator,
        ),
        WriteMode::Canonical => ("\n", true),
    };

    let mut output = String::new();
    for (record_index, row) in output_records.iter().enumerate() {
        if record_index > 0 {
            output.push_str(line_ending);
        }
        for (column_index, field) in row.iter().enumerate() {
            if column_index > 0 {
                output.push(config.delimiter as char);
            }
            output.push_str(&encode_field(
                field,
                config.delimiter,
                config.quote,
                record_index == 0 && column_index == 0,
                row.len() == 1 && field.is_empty(),
            ));
        }
    }
    if final_terminator {
        output.push_str(line_ending);
    }
    Ok(output)
}