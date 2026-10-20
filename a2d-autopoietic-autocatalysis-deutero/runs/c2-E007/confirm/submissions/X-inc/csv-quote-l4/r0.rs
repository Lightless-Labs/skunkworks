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
            DuplicatePolicy::KeepLast => header.iter().rposition(|s| s == name),
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|s| s == name)
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
    let mut field = String::new();
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut quote_start = 0usize;
    let mut record_index = 0usize;
    let mut field_started = false;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if !in_quotes && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
        }

        if in_quotes {
            if b == quote {
                if bytes.get(i + 1) == Some(&quote) {
                    field.push(quote as char);
                    i += 2;
                } else {
                    in_quotes = false;
                    after_quote = true;
                    i += 1;
                }
            } else {
                let start = i;
                while i < bytes.len()
                    && bytes[i] != quote
                    && bytes[i] != 0
                {
                    i += 1;
                }
                field.push_str(std::str::from_utf8(&bytes[start..i]).unwrap());
            }
            continue;
        }

        if after_quote {
            if b == delimiter {
                row.push(std::mem::take(&mut field));
                field_started = false;
                after_quote = false;
                i += 1;
            } else if b == b'\n' || b == b'\r' {
                row.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut row));
                record_index += 1;
                field_started = false;
                after_quote = false;
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
            row.push(std::mem::take(&mut field));
            field_started = false;
            i += 1;
        } else if b == b'\n' || b == b'\r' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            record_index += 1;
            field_started = false;
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
            let start = i;
            while i < bytes.len()
                && bytes[i] != delimiter
                && bytes[i] != quote
                && bytes[i] != b'\n'
                && bytes[i] != b'\r'
                && bytes[i] != 0
            {
                i += 1;
            }
            field.push_str(std::str::from_utf8(&bytes[start..i]).unwrap());
            field_started = true;
        }
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_start,
        });
    }

    if !row.is_empty() || !field.is_empty() || field_started || after_quote {
        row.push(field);
        records.push(row);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
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

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    if !config.headers {
        return Ok(Table {
            header: None,
            rows: records,
            duplicates: config.duplicates,
        });
    }

    let mut iter = records.into_iter();
    let Some(header) = iter.next() else {
        return Ok(Table {
            header: None,
            rows: Vec::new(),
            duplicates: config.duplicates,
        });
    };

    if config.duplicates == DuplicatePolicy::Reject {
        if let Some(error) = duplicate_header(&header) {
            return Err(error);
        }
    }

    let rows: Vec<Vec<String>> = iter.collect();
    for (index, row) in rows.iter().enumerate() {
        if row.len() != header.len() {
            return Err(CsvError::HeaderWidth {
                record: index + 1,
                expected: header.len(),
                actual: row.len(),
            });
        }
    }

    Ok(Table {
        header: Some(header),
        rows,
        duplicates: config.duplicates,
    })
}

pub struct StreamReader {
    config: Config,
    input: String,
    finished: bool,
    result: Option<Result<Table, CsvError>>,
}

impl StreamReader {
    pub fn new(config: Config) -> Result<Self, CsvError> {
        validate_dialect(config.delimiter, config.quote)
            .map_err(|component| CsvError::InvalidConfig { component })?;
        Ok(Self {
            config,
            input: String::new(),
            finished: false,
            result: None,
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
        if let Some(result) = &self.result {
            return result.clone();
        }
        self.finished = true;
        let result = parse_with_dialect(&self.input, self.config.delimiter, self.config.quote)
            .and_then(|records| build_table(records, self.config));
        self.result = Some(result.clone());
        result
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.finished = false;
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
    let start = usize::from(b.first() == Some(&b'-'));
    if b.len() < start + 4 {
        return false;
    }
    let dot = b.len() - 3;
    if b[dot] != b'.'
        || !b[dot + 1..].iter().all(u8::is_ascii_digit)
        || dot < start + 1
    {
        return false;
    }
    let whole = &b[start..dot];
    if whole == b"0" {
        return start == 0 || b[1..dot].iter().any(|&x| x != b'0');
    }
    if whole.is_empty() || !(b'1'..=b'9').contains(&whole[0]) {
        return false;
    }
    whole[1..].iter().all(u8::is_ascii_digit)
}

fn evaluate_cell(
    text: Option<&String>,
    column: Option<usize>,
    spec: &ColumnSpec,
    row_index: usize,
    spec_index: usize,
) -> (Option<Value>, Option<CellError>) {
    let error = |kind| CellError {
        row: row_index,
        spec: spec_index,
        column,
        kind,
    };

    let Some(text) = text else {
        let kind = if column.is_none() {
            CellErrorKind::UnknownColumn
        } else {
            CellErrorKind::MissingCell
        };
        return (None, Some(error(kind)));
    };

    if text.is_empty() {
        if spec.nullable {
            return (Some(Value::Null), None);
        }
        if spec.kind != ColumnType::Text {
            return (None, Some(error(CellErrorKind::Empty)));
        }
    }

    match spec.kind {
        ColumnType::Text => (Some(Value::Text(text.clone())), None),
        ColumnType::Integer => {
            if !integer_syntax(text) {
                return (None, Some(error(CellErrorKind::InvalidInteger)));
            }
            match text.parse::<i64>() {
                Ok(v) => (Some(Value::Integer(v)), None),
                Err(_) => (None, Some(error(CellErrorKind::IntegerOverflow))),
            }
        }
        ColumnType::Decimal => {
            if !decimal_syntax(text) {
                return (None, Some(error(CellErrorKind::InvalidDecimal)));
            }
            let negative = text.starts_with('-');
            let digits = text.trim_start_matches('-').replace('.', "");
            let magnitude = digits.parse::<i128>().unwrap_or(i128::MAX);
            let signed = if negative { -magnitude } else { magnitude };
            if signed < i64::MIN as i128 || signed > i64::MAX as i128 {
                (None, Some(error(CellErrorKind::DecimalOverflow)))
            } else {
                (Some(Value::Decimal(signed as i64)), None)
            }
        }
        ColumnType::Boolean => match text.as_str() {
            "yes" | "1" => (Some(Value::Boolean(true)), None),
            "no" | "0" => (Some(Value::Boolean(false)), None),
            _ => (None, Some(error(CellErrorKind::InvalidBoolean))),
        },
    }
}

pub fn extract_columns(table: &Table, columns: &[ColumnSpec], policy: ErrorPolicy) -> TypedResult {
    let mut result = TypedResult {
        rows: Vec::new(),
        errors: Vec::new(),
    };

    for (row_index, row) in table.rows.iter().enumerate() {
        let mut output = Vec::with_capacity(columns.len());
        let mut failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if policy == ErrorPolicy::Stop && failed {
                output.push(None);
                continue;
            }

            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };
            let column = resolved;
            let cell = resolved.and_then(|index| row.get(index));
            let (value, error) = evaluate_cell(cell, column, spec, row_index, spec_index);
            output.push(value);

            if let Some(error) = error {
                result.errors.push(error);
                failed = true;
                if policy == ErrorPolicy::FirstPerRow || policy == ErrorPolicy::Stop {
                    output.extend((spec_index + 1..columns.len()).map(|_| None));
                    break;
                }
            }
        }

        result.rows.push(output);
        if policy == ErrorPolicy::Stop && failed {
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
    Nul { record: usize, column: usize, byte: usize },
    Width { record: usize, expected: usize, actual: usize },
    ZeroColumns { record: usize },
    DuplicateHeader { name: String, first: usize, second: usize },
}

fn records_from_table(table: &Table) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    if let Some(header) = &table.header {
        records.push(header.clone());
    }
    records.extend(table.rows.iter().cloned());
    records
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let original = records_from_table(table);

    for (record_index, record) in original.iter().enumerate() {
        for (column_index, cell) in record.iter().enumerate() {
            if let Some(byte) = cell.as_bytes().iter().position(|&b| b == 0) {
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

    for (record_index, record) in original.iter().enumerate() {
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

    let mut output_records = original;
    if config.mode == WriteMode::Canonical {
        if let Some(header) = &table.header {
            let mut chosen = Vec::new();
            for (index, name) in header.iter().enumerate() {
                let matching: Vec<usize> = header
                    .iter()
                    .enumerate()
                    .filter_map(|(i, other)| (other == name).then_some(i))
                    .collect();
                let keep = match table.duplicates {
                    DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                        matching.first().copied() == Some(index)
                    }
                    DuplicatePolicy::KeepLast => matching.last().copied() == Some(index),
                };
                if keep {
                    chosen.push(index);
                }
            }
            chosen.sort_by(|&a, &b| header[a].as_bytes().cmp(header[b].as_bytes()));

            let projected_header: Vec<String> =
                chosen.iter().map(|&i| header[i].clone()).collect();
            let mut projected_rows: Vec<Vec<String>> = table
                .rows
                .iter()
                .map(|row| chosen.iter().map(|&i| row[i].clone()).collect())
                .collect();
            projected_rows.sort_by(|a, b| {
                a.iter()
                    .map(String::as_bytes)
                    .cmp(b.iter().map(String::as_bytes))
            });

            output_records.clear();
            output_records.push(projected_header);
            output_records.extend(projected_rows);
        } else {
            output_records.sort_by(|a, b| {
                a.iter()
                    .map(String::as_bytes)
                    .cmp(b.iter().map(String::as_bytes))
            });
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

    let mut bytes = Vec::new();
    for (record_index, record) in output_records.iter().enumerate() {
        if record_index > 0 {
            bytes.extend_from_slice(ending);
        }
        for (column_index, field) in record.iter().enumerate() {
            if column_index > 0 {
                bytes.push(config.delimiter);
            }
            let fb = field.as_bytes();
            let quote_field = fb.contains(&config.delimiter)
                || fb.contains(&config.quote)
                || fb.contains(&b'\r')
                || fb.contains(&b'\n')
                || fb.first().is_some_and(|&b| b == b' ' || b == b'\t')
                || fb.last().is_some_and(|&b| b == b' ' || b == b'\t')
                || (record_index == 0 && column_index == 0 && field.starts_with('\u{feff}'))
                || (record.len() == 1 && field.is_empty());

            if quote_field {
                bytes.push(config.quote);
                for &b in fb {
                    bytes.push(b);
                    if b == config.quote {
                        bytes.push(b);
                    }
                }
                bytes.push(config.quote);
            } else {
                bytes.extend_from_slice(fb);
            }
        }
    }

    if canonical || config.final_terminator {
        bytes.extend_from_slice(ending);
    }

    Ok(String::from_utf8(bytes).unwrap())
}