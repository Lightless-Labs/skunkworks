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
    let mut i = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };
    let mut records = Vec::new();
    if i == bytes.len() {
        return Ok(records);
    }

    let mut record = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut field_started = false;
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
                i += 1;
                while i < bytes.len()
                    && bytes[i] != quote
                    && bytes[i] != 0
                {
                    i += 1;
                }
                field.push_str(&input[start..i]);
            }
            continue;
        }

        if after_quote {
            if b == delimiter {
                record.push(std::mem::take(&mut field));
                after_quote = false;
                field_started = false;
                i += 1;
            } else if b == b'\n' || b == b'\r' {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                record_index += 1;
                after_quote = false;
                field_started = false;
                if b == b'\r' {
                    i += 2;
                } else {
                    i += 1;
                }
            } else {
                return Err(CsvError::TrailingByte {
                    record: record_index,
                    byte: i,
                });
            }
            continue;
        }

        if b == quote {
            if !field_started {
                in_quotes = true;
                quote_start = i;
                field_started = true;
                i += 1;
            } else {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: i,
                });
            }
        } else if b == delimiter {
            record.push(std::mem::take(&mut field));
            field_started = false;
            i += 1;
        } else if b == b'\n' || b == b'\r' {
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            record_index += 1;
            field_started = false;
            if b == b'\r' {
                i += 2;
            } else {
                i += 1;
            }
        } else {
            let start = i;
            i += 1;
            while i < bytes.len()
                && bytes[i] != delimiter
                && bytes[i] != quote
                && bytes[i] != b'\n'
                && bytes[i] != b'\r'
                && bytes[i] != 0
            {
                i += 1;
            }
            field.push_str(&input[start..i]);
            field_started = true;
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

fn duplicate_header_error(header: &[String]) -> Option<CsvError> {
    let mut seen = std::collections::HashMap::<&str, usize>::new();
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

    if config.duplicates == DuplicatePolicy::Reject {
        if let Some(header) = header.as_ref() {
            if let Some(error) = duplicate_header_error(header) {
                return Err(error);
            }
        }
    }

    if let Some(header) = header.as_ref() {
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

fn integer_syntax(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let digits = if bytes[0] == b'-' { &bytes[1..] } else { bytes };
    if digits.is_empty() {
        return false;
    }
    if digits == b"0" {
        return bytes[0] != b'-';
    }
    digits[0].is_ascii_digit() && digits[0] != b'0' && digits.iter().all(u8::is_ascii_digit)
}

fn decimal_syntax(s: &str) -> bool {
    let bytes = s.as_bytes();
    let body = if bytes.first() == Some(&b'-') {
        &bytes[1..]
    } else {
        bytes
    };
    let Some(dot) = body.iter().position(|&b| b == b'.') else {
        return false;
    };
    if body[dot + 1..].len() != 2
        || !body[dot + 1..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    let whole = &body[..dot];
    if whole == b"0" {
        return s.as_bytes().first() != Some(&b'-') || body[dot + 1..] != *b"00";
    }
    !whole.is_empty()
        && whole[0].is_ascii_digit()
        && whole[0] != b'0'
        && whole.iter().all(u8::is_ascii_digit)
}

fn parse_decimal_hundredths(s: &str) -> Option<i64> {
    let negative = s.starts_with('-');
    let body = if negative { &s[1..] } else { s };
    let dot = body.find('.')?;
    let whole: i128 = body[..dot].parse().ok()?;
    let frac: i128 = body[dot + 1..].parse().ok()?;
    let magnitude = whole.checked_mul(100)?.checked_add(frac)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).ok()
}

pub fn extract_columns(table: &Table, columns: &[ColumnSpec], policy: ErrorPolicy) -> TypedResult {
    let mut output = TypedResult {
        rows: Vec::new(),
        errors: Vec::new(),
    };

    'rows: for (row_index, row) in table.rows.iter().enumerate() {
        let mut out_row = Vec::with_capacity(columns.len());
        for (spec_index, spec) in columns.iter().enumerate() {
            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let Some(column_index) = resolved else {
                output.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: None,
                    kind: CellErrorKind::UnknownColumn,
                });
                out_row.push(None);
                if policy != ErrorPolicy::Collect {
                    out_row.resize(columns.len(), None);
                    output.rows.push(out_row);
                    if policy == ErrorPolicy::Stop {
                        break 'rows;
                    }
                    continue 'rows;
                }
                continue;
            };

            let Some(cell) = row.get(column_index) else {
                output.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: Some(column_index),
                    kind: CellErrorKind::MissingCell,
                });
                out_row.push(None);
                if policy != ErrorPolicy::Collect {
                    out_row.resize(columns.len(), None);
                    output.rows.push(out_row);
                    if policy == ErrorPolicy::Stop {
                        break 'rows;
                    }
                    continue 'rows;
                }
                continue;
            };

            if cell.is_empty() {
                if spec.nullable {
                    out_row.push(Some(Value::Null));
                    continue;
                }
                if spec.kind != ColumnType::Text {
                    output.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column_index),
                        kind: CellErrorKind::Empty,
                    });
                    out_row.push(None);
                    if policy != ErrorPolicy::Collect {
                        out_row.resize(columns.len(), None);
                        output.rows.push(out_row);
                        if policy == ErrorPolicy::Stop {
                            break 'rows;
                        }
                        continue 'rows;
                    }
                    continue;
                }
            }

            let converted = match spec.kind {
                ColumnType::Text => Ok(Value::Text(cell.clone())),
                ColumnType::Integer => {
                    if !integer_syntax(cell) {
                        Err(CellErrorKind::InvalidInteger)
                    } else {
                        match cell.parse::<i64>() {
                            Ok(value) => Ok(Value::Integer(value)),
                            Err(_) => Err(CellErrorKind::IntegerOverflow),
                        }
                    }
                }
                ColumnType::Decimal => {
                    if !decimal_syntax(cell) {
                        Err(CellErrorKind::InvalidDecimal)
                    } else {
                        parse_decimal_hundredths(cell)
                            .map(Value::Decimal)
                            .ok_or(CellErrorKind::DecimalOverflow)
                    }
                }
                ColumnType::Boolean => match cell.as_str() {
                    "yes" | "1" => Ok(Value::Boolean(true)),
                    "no" | "0" => Ok(Value::Boolean(false)),
                    _ => Err(CellErrorKind::InvalidBoolean),
                },
            };

            match converted {
                Ok(value) => out_row.push(Some(value)),
                Err(kind) => {
                    output.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column_index),
                        kind,
                    });
                    out_row.push(None);
                    if policy != ErrorPolicy::Collect {
                        out_row.resize(columns.len(), None);
                        output.rows.push(out_row);
                        if policy == ErrorPolicy::Stop {
                            break 'rows;
                        }
                        continue 'rows;
                    }
                }
            }
        }
        output.rows.push(out_row);
    }

    output
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

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let mut original_records: Vec<(usize, &Vec<String>)> = Vec::new();
    if let Some(header) = table.header.as_ref() {
        original_records.push((0, header));
    }
    for (i, row) in table.rows.iter().enumerate() {
        original_records.push((i + usize::from(table.header.is_some()), row));
    }

    for (record, row) in &original_records {
        for (column, cell) in row.iter().enumerate() {
            if let Some(byte) = cell.as_bytes().iter().position(|&b| b == 0) {
                return Err(WriteError::Nul {
                    record: *record,
                    column,
                    byte,
                });
            }
        }
    }

    if let Some(header) = table.header.as_ref() {
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

    for (record, row) in &original_records {
        if row.is_empty() {
            return Err(WriteError::ZeroColumns { record: *record });
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
        }) = duplicate_header_error(table.header.as_ref().unwrap())
        {
            return Err(WriteError::DuplicateHeader {
                name,
                first,
                second,
            });
        }
    }

    let mut records: Vec<Vec<String>> = match config.mode {
        WriteMode::Preserve => original_records
            .iter()
            .map(|(_, row)| (*row).clone())
            .collect(),
        WriteMode::Canonical => {
            if let Some(header) = table.header.as_ref() {
                let mut retained = std::collections::HashMap::<String, usize>::new();
                for (index, name) in header.iter().enumerate() {
                    match table.duplicates {
                        DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                            retained.entry(name.clone()).or_insert(index);
                        }
                        DuplicatePolicy::KeepLast => {
                            retained.insert(name.clone(), index);
                        }
                    }
                }
                let mut columns: Vec<(String, usize)> = retained.into_iter().collect();
                columns.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

                let output_header = columns.iter().map(|(name, _)| name.clone()).collect();
                let mut projected_rows = Vec::with_capacity(table.rows.len());
                for row in &table.rows {
                    projected_rows.push(
                        columns
                            .iter()
                            .map(|(_, index)| row[*index].clone())
                            .collect::<Vec<_>>(),
                    );
                }
                projected_rows.sort_by(|a, b| compare_rows(a, b));
                let mut all = vec![output_header];
                all.extend(projected_rows);
                all
            } else {
                let mut rows = table.rows.clone();
                rows.sort_by(|a, b| compare_rows(a, b));
                rows
            }
        }
    };

    if records.is_empty() {
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
    let final_terminator = canonical || config.final_terminator;
    let mut output = String::new();

    for (record_index, row) in records.iter().enumerate() {
        if record_index > 0 {
            output.push_str(std::str::from_utf8(ending).unwrap());
        }
        for (column_index, field) in row.iter().enumerate() {
            if column_index > 0 {
                output.push(config.delimiter as char);
            }
            let bytes = field.as_bytes();
            let starts_or_ends_space_tab = bytes.first().is_some_and(|b| *b == b' ' || *b == b'\t')
                || bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t');
            let starts_bom = record_index == 0
                && column_index == 0
                && field.starts_with('\u{feff}');
            let sole_empty = row.len() == 1 && field.is_empty();
            let needs_quote = field.as_bytes().contains(&config.delimiter)
                || field.as_bytes().contains(&config.quote)
                || field.contains('\r')
                || field.contains('\n')
                || starts_or_ends_space_tab
                || starts_bom
                || sole_empty;

            if needs_quote {
                output.push(config.quote as char);
                for ch in field.chars() {
                    if ch as u32 == config.quote as u32 {
                        output.push(config.quote as char);
                    }
                    output.push(ch);
                }
                output.push(config.quote as char);
            } else {
                output.push_str(field);
            }
        }
    }

    if final_terminator {
        output.push_str(std::str::from_utf8(ending).unwrap());
    }

    records.clear();
    Ok(output)
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