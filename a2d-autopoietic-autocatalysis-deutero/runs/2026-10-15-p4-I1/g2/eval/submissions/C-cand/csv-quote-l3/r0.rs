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

pub struct StreamReader {
    config: Config,
    input: String,
    cached: Option<Result<Table, CsvError>>,
}

fn validate_config(config: Config) -> Result<(), CsvError> {
    if !(0x21..=0x7e).contains(&config.delimiter) {
        return Err(CsvError::InvalidConfig {
            component: ConfigPart::Delimiter,
        });
    }
    if !(0x21..=0x7e).contains(&config.quote) {
        return Err(CsvError::InvalidConfig {
            component: ConfigPart::Quote,
        });
    }
    if config.delimiter == config.quote {
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
    let mut pos = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut field_started = false;
    let mut quote_start = 0usize;
    let mut record_index = 0usize;

    while pos < bytes.len() {
        let b = bytes[pos];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: pos,
            });
        }

        if !in_quotes && b == b'\r' {
            if bytes.get(pos + 1) != Some(&b'\n') {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: pos,
                });
            }
            records.push({
                record.push(std::mem::take(&mut field));
                std::mem::take(&mut record)
            });
            pos += 2;
            record_index += 1;
            after_quote = false;
            field_started = false;
            continue;
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
                let ch = input[pos..].chars().next().unwrap();
                field.push(ch);
                pos += ch.len_utf8();
            }
            continue;
        }

        if after_quote {
            if b == delimiter {
                record.push(std::mem::take(&mut field));
                after_quote = false;
                field_started = false;
                pos += 1;
                continue;
            }
            if b == b'\n' {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                pos += 1;
                record_index += 1;
                after_quote = false;
                field_started = false;
                continue;
            }
            return Err(CsvError::TrailingByte {
                record: record_index,
                byte: pos,
            });
        }

        if b == delimiter {
            record.push(std::mem::take(&mut field));
            field_started = false;
            pos += 1;
        } else if b == b'\n' {
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            pos += 1;
            record_index += 1;
            field_started = false;
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
            let ch = input[pos..].chars().next().unwrap();
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

fn table_from_input(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_config(config)?;
    let mut parsed = parse_with_dialect(input, config.delimiter, config.quote)?;

    let (header, rows) = if config.headers && !parsed.is_empty() {
        let header = parsed.remove(0);
        if config.duplicates == DuplicatePolicy::Reject {
            for second in 0..header.len() {
                if let Some(first) = header[..second].iter().position(|name| name == &header[second]) {
                    return Err(CsvError::DuplicateHeader {
                        name: header[second].clone(),
                        first,
                        second,
                    });
                }
            }
        }
        for (index, row) in parsed.iter().enumerate() {
            if row.len() != header.len() {
                return Err(CsvError::HeaderWidth {
                    record: index + 1,
                    expected: header.len(),
                    actual: row.len(),
                });
            }
        }
        (Some(header), parsed)
    } else {
        (None, parsed)
    };

    Ok(Table {
        header,
        rows,
        duplicates: config.duplicates,
    })
}

impl StreamReader {
    pub fn new(config: Config) -> Result<Self, CsvError> {
        validate_config(config)?;
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
        let result = table_from_input(&self.input, self.config);
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
        return start == 0 && b.len() == 1;
    }
    if !(b'1'..=b'9').contains(&b[start]) {
        return false;
    }
    b[start + 1..].iter().all(u8::is_ascii_digit)
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    let start = usize::from(b.first() == Some(&b'-'));
    if start >= b.len() {
        return false;
    }
    let dot = match b[start..].iter().position(|&x| x == b'.') {
        Some(i) => start + i,
        None => return false,
    };
    if b.len() != dot + 3 {
        return false;
    }
    let integral = &b[start..dot];
    if integral.is_empty() || !b[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    if integral[0] == b'0' {
        if integral.len() != 1 {
            return false;
        }
    } else if !(b'1'..=b'9').contains(&integral[0])
        || !integral[1..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    !(start == 1 && integral == b"0" && b[dot + 1..] == *b"00")
}

fn parse_decimal_value(s: &str) -> Option<i64> {
    let negative = s.starts_with('-');
    let unsigned = if negative { &s[1..] } else { s };
    let (whole, fraction) = unsigned.split_once('.')?;
    let whole: i128 = whole.parse().ok()?;
    let frac: i128 = fraction.parse().ok()?;
    let magnitude = whole.checked_mul(100)?.checked_add(frac)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).ok()
}

fn evaluate_cell(
    row_index: usize,
    spec_index: usize,
    row: &[String],
    table: &Table,
    spec: &ColumnSpec,
) -> Result<Value, CellError> {
    let column = match &spec.column {
        ColumnRef::Index(index) => Some(*index),
        ColumnRef::Name(name) => table.column_index(name),
    };
    let Some(column) = column else {
        return Err(CellError {
            row: row_index,
            spec: spec_index,
            column: None,
            kind: CellErrorKind::UnknownColumn,
        });
    };
    let Some(cell) = row.get(column) else {
        return Err(CellError {
            row: row_index,
            spec: spec_index,
            column: Some(column),
            kind: CellErrorKind::MissingCell,
        });
    };

    if cell.is_empty() {
        if spec.nullable {
            return Ok(Value::Null);
        }
        if spec.kind != ColumnType::Text {
            return Err(CellError {
                row: row_index,
                spec: spec_index,
                column: Some(column),
                kind: CellErrorKind::Empty,
            });
        }
    }

    let error = |kind| CellError {
        row: row_index,
        spec: spec_index,
        column: Some(column),
        kind,
    };

    match spec.kind {
        ColumnType::Text => Ok(Value::Text(cell.clone())),
        ColumnType::Integer => {
            if !integer_syntax(cell) {
                return Err(error(CellErrorKind::InvalidInteger));
            }
            cell.parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| error(CellErrorKind::IntegerOverflow))
        }
        ColumnType::Decimal => {
            if !decimal_syntax(cell) {
                return Err(error(CellErrorKind::InvalidDecimal));
            }
            parse_decimal_value(cell)
                .map(Value::Decimal)
                .ok_or_else(|| error(CellErrorKind::DecimalOverflow))
        }
        ColumnType::Boolean => match cell.as_str() {
            "yes" | "1" => Ok(Value::Boolean(true)),
            "no" | "0" => Ok(Value::Boolean(false)),
            _ => Err(error(CellErrorKind::InvalidBoolean)),
        },
    }
}

pub fn extract_columns(
    table: &Table,
    columns: &[ColumnSpec],
    policy: ErrorPolicy,
) -> TypedResult {
    let mut result = TypedResult {
        rows: Vec::with_capacity(table.rows.len()),
        errors: Vec::new(),
    };

    for (row_index, row) in table.rows.iter().enumerate() {
        let mut output = Vec::with_capacity(columns.len());
        let mut failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if failed {
                output.push(None);
                continue;
            }
            match evaluate_cell(row_index, spec_index, row, table, spec) {
                Ok(value) => output.push(Some(value)),
                Err(error) => {
                    result.errors.push(error);
                    output.push(None);
                    if policy != ErrorPolicy::Collect {
                        failed = true;
                    }
                    if policy == ErrorPolicy::Stop {
                        break;
                    }
                }
            }
        }

        if policy == ErrorPolicy::Stop && !result.errors.is_empty() {
            output.resize(columns.len(), None);
            result.rows.push(output);
            break;
        }

        result.rows.push(output);
    }

    result
}

fn main() {}