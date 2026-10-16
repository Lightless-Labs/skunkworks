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
            DuplicatePolicy::KeepLast => header.iter().rposition(|item| item == name),
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|item| item == name)
            }
        }
    }
}

fn validate_config(delimiter: u8, quote: u8) -> Result<(), CsvError> {
    if !(0x21..=0x7e).contains(&delimiter) {
        return Err(CsvError::InvalidConfig {
            component: ConfigPart::Delimiter,
        });
    }
    if !(0x21..=0x7e).contains(&quote) {
        return Err(CsvError::InvalidConfig {
            component: ConfigPart::Quote,
        });
    }
    if delimiter == quote {
        return Err(CsvError::InvalidConfig {
            component: ConfigPart::Quote,
        });
    }
    Ok(())
}

fn parse_with_dialect(
    input: &str,
    delimiter: u8,
    quote: u8,
) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = Vec::new();
    let mut record_started = false;
    let mut field_started = false;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut quote_open = 0usize;
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

        if after_quote {
            if b == delimiter {
                fields.push(String::from_utf8(field).expect("CSV field remains UTF-8"));
                field = Vec::new();
                field_started = false;
                record_started = true;
                after_quote = false;
                i += 1;
            } else if b == b'\n' || b == b'\r' {
                fields.push(String::from_utf8(field).expect("CSV field remains UTF-8"));
                field = Vec::new();
                records.push(fields);
                fields = Vec::new();
                record_started = false;
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

        if b == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: i,
                });
            }
            in_quotes = true;
            quote_open = i;
            field_started = true;
            record_started = true;
            i += 1;
        } else if b == delimiter {
            fields.push(String::from_utf8(field).expect("CSV field remains UTF-8"));
            field = Vec::new();
            field_started = false;
            record_started = true;
            i += 1;
        } else if b == b'\n' || b == b'\r' {
            fields.push(String::from_utf8(field).expect("CSV field remains UTF-8"));
            field = Vec::new();
            records.push(fields);
            fields = Vec::new();
            record_started = false;
            field_started = false;
            record_index += 1;
            i += if b == b'\r' { 2 } else { 1 };
        } else {
            field.push(b);
            field_started = true;
            record_started = true;
            i += 1;
        }
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_open,
        });
    }

    if record_started || field_started || !fields.is_empty() || after_quote {
        fields.push(String::from_utf8(field).expect("CSV field remains UTF-8"));
        records.push(fields);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn table_from_records(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let mut iter = records.into_iter();
    let header = if config.headers {
        iter.next()
    } else {
        None
    };

    if let Some(header) = &header {
        if config.duplicates == DuplicatePolicy::Reject {
            let mut seen = std::collections::HashMap::<&str, usize>::new();
            for (second, name) in header.iter().enumerate() {
                if let Some(&first) = seen.get(name.as_str()) {
                    return Err(CsvError::DuplicateHeader {
                        name: name.clone(),
                        first,
                        second,
                    });
                }
                seen.insert(name.as_str(), second);
            }
        }
    }

    let rows: Vec<Vec<String>> = iter.collect();
    if let Some(header) = &header {
        for (idx, row) in rows.iter().enumerate() {
            if row.len() != header.len() {
                return Err(CsvError::HeaderWidth {
                    record: idx + 1,
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
        validate_config(config.delimiter, config.quote)?;
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
            .and_then(|records| table_from_records(records, self.config));
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
    let digits = if b[0] == b'-' { &b[1..] } else { b };
    if digits.is_empty() {
        return false;
    }
    if digits == b"0" {
        return b[0] != b'-';
    }
    (b'1'..=b'9').contains(&digits[0]) && digits[1..].iter().all(u8::is_ascii_digit)
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    let (negative, body) = if b.first() == Some(&b'-') {
        (true, &b[1..])
    } else {
        (false, b)
    };
    let Some(dot) = body.iter().position(|&x| x == b'.') else {
        return false;
    };
    let whole = &body[..dot];
    let frac = &body[dot + 1..];
    if frac.len() != 2 || !frac.iter().all(u8::is_ascii_digit) || whole.is_empty() {
        return false;
    }
    let whole_ok = if whole == b"0" {
        true
    } else {
        (b'1'..=b'9').contains(&whole[0]) && whole[1..].iter().all(u8::is_ascii_digit)
    };
    if !whole_ok {
        return false;
    }
    !(negative && whole == b"0" && frac == b"00")
}

fn evaluate_cell(
    cell: Option<&String>,
    column: Option<usize>,
    spec: &ColumnSpec,
) -> Result<Value, CellErrorKind> {
    let Some(cell) = cell else {
        return Err(CellErrorKind::MissingCell);
    };

    if spec.nullable && cell.is_empty() {
        return Ok(Value::Null);
    }

    match spec.kind {
        ColumnType::Text => Ok(Value::Text(cell.clone())),
        ColumnType::Integer => {
            if cell.is_empty() {
                return Err(CellErrorKind::Empty);
            }
            if !integer_syntax(cell) {
                return Err(CellErrorKind::InvalidInteger);
            }
            cell.parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| CellErrorKind::IntegerOverflow)
        }
        ColumnType::Decimal => {
            if cell.is_empty() {
                return Err(CellErrorKind::Empty);
            }
            if !decimal_syntax(cell) {
                return Err(CellErrorKind::InvalidDecimal);
            }
            let negative = cell.starts_with('-');
            let unsigned = if negative { &cell[1..] } else { cell.as_str() };
            let mut pieces = unsigned.split('.');
            let whole = pieces.next().unwrap_or("");
            let frac = pieces.next().unwrap_or("");
            let magnitude = whole
                .parse::<i128>()
                .unwrap_or(i128::MAX)
                .saturating_mul(100)
                .saturating_add(frac.parse::<i128>().unwrap_or(0));
            let signed = if negative { -magnitude } else { magnitude };
            i64::try_from(signed)
                .map(Value::Decimal)
                .map_err(|_| CellErrorKind::DecimalOverflow)
        }
        ColumnType::Boolean => {
            if cell.is_empty() {
                return Err(CellErrorKind::Empty);
            }
            match cell.as_str() {
                "yes" | "1" => Ok(Value::Boolean(true)),
                "no" | "0" => Ok(Value::Boolean(false)),
                _ => Err(CellErrorKind::InvalidBoolean),
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

    for (row_index, row) in table.rows.iter().enumerate() {
        let mut output = vec![None; columns.len()];
        let mut failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if failed {
                break;
            }

            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let evaluated = if matches!(&spec.column, ColumnRef::Name(_)) && resolved.is_none() {
                Err(CellErrorKind::UnknownColumn)
            } else {
                let index = resolved.expect("resolved column index");
                evaluate_cell(row.get(index), Some(index), spec)
            };

            match evaluated {
                Ok(value) => output[spec_index] = Some(value),
                Err(kind) => {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: resolved,
                        kind,
                    });
                    match policy {
                        ErrorPolicy::Collect => {}
                        ErrorPolicy::FirstPerRow | ErrorPolicy::Stop => failed = true,
                    }
                }
            }

            if policy == ErrorPolicy::Stop && failed {
                result.rows.push(output);
                return result;
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

fn validate_write_config(config: WriteConfig) -> Result<(), WriteError> {
    if !(0x21..=0x7e).contains(&config.delimiter) {
        return Err(WriteError::InvalidConfig {
            component: ConfigPart::Delimiter,
        });
    }
    if !(0x21..=0x7e).contains(&config.quote) {
        return Err(WriteError::InvalidConfig {
            component: ConfigPart::Quote,
        });
    }
    if config.delimiter == config.quote {
        return Err(WriteError::InvalidConfig {
            component: ConfigPart::Quote,
        });
    }
    Ok(())
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_write_config(config)?;

    let mut original_records: Vec<&Vec<String>> = Vec::new();
    if let Some(header) = &table.header {
        original_records.push(header);
    }
    original_records.extend(table.rows.iter());

    for (record_index, record) in original_records.iter().enumerate() {
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
        let header = table.header.as_ref().unwrap();
        let mut seen = std::collections::HashMap::<&str, usize>::new();
        for (second, name) in header.iter().enumerate() {
            if let Some(&first) = seen.get(name.as_str()) {
                return Err(WriteError::DuplicateHeader {
                    name: name.clone(),
                    first,
                    second,
                });
            }
            seen.insert(name.as_str(), second);
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
                let mut retained = Vec::<(String, usize)>::new();
                for (index, name) in header.iter().enumerate() {
                    match table.duplicates {
                        DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                            if !retained.iter().any(|(existing, _)| existing == name) {
                                retained.push((name.clone(), index));
                            }
                        }
                        DuplicatePolicy::KeepLast => {
                            if let Some(existing) =
                                retained.iter_mut().find(|(existing, _)| existing == name)
                            {
                                existing.1 = index;
                            } else {
                                retained.push((name.clone(), index));
                            }
                        }
                    }
                }
                retained.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
                let indexes: Vec<usize> = retained.iter().map(|(_, index)| *index).collect();
                output_records.push(retained.into_iter().map(|(name, _)| name).collect());

                let mut rows: Vec<Vec<String>> = table
                    .rows
                    .iter()
                    .map(|row| indexes.iter().map(|&i| row[i].clone()).collect())
                    .collect();
                rows.sort_by(|a, b| compare_string_vectors(a, b));
                output_records.extend(rows);
            } else {
                let mut rows = table.rows.clone();
                rows.sort_by(|a, b| compare_string_vectors(a, b));
                output_records.extend(rows);
            }
        }
    }

    if output_records.is_empty() {
        return Ok(String::new());
    }

    let canonical = config.mode == WriteMode::Canonical;
    let terminator = if canonical {
        "\n"
    } else {
        match config.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    };
    let mut output = String::new();

    for (record_index, record) in output_records.iter().enumerate() {
        if record_index > 0 {
            output.push_str(terminator);
        }
        for (column_index, field) in record.iter().enumerate() {
            if column_index > 0 {
                output.push(config.delimiter as char);
            }
            let bytes = field.as_bytes();
            let contains_special = bytes.iter().any(|&b| {
                b == config.delimiter || b == config.quote || b == b'\r' || b == b'\n'
            });
            let edge_space = bytes.first().is_some_and(|b| *b == b' ' || *b == b'\t')
                || bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t');
            let bom_first = record_index == 0 && column_index == 0 && field.starts_with('\u{feff}');
            let sole_empty = record.len() == 1 && field.is_empty();
            let quote_field = contains_special || edge_space || bom_first || sole_empty;

            if quote_field {
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

    if canonical || config.final_terminator {
        output.push_str(terminator);
    }

    Ok(output)
}

fn compare_string_vectors(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (left, right) in a.iter().zip(b.iter()) {
        let ordering = left.as_bytes().cmp(right.as_bytes());
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }
    a.len().cmp(&b.len())
}