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
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|value| value == name)
            }
        }
    }
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
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    if i == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut record_index = 0usize;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut field_started = false;
    let mut opening_quote = 0usize;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if !in_quotes && byte == b'\r' && (i + 1 >= bytes.len() || bytes[i + 1] != b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
        }

        if in_quotes {
            if byte == quote {
                if i + 1 < bytes.len() && bytes[i + 1] == quote {
                    field.push(quote as char);
                    i += 2;
                } else {
                    in_quotes = false;
                    after_quote = true;
                    i += 1;
                }
            } else {
                let ch = input[i..].chars().next().expect("valid UTF-8");
                field.push(ch);
                i += ch.len_utf8();
            }
            continue;
        }

        if after_quote {
            if byte == delimiter {
                record.push(std::mem::take(&mut field));
                after_quote = false;
                field_started = false;
                i += 1;
                continue;
            }
            if byte == b'\n' || byte == b'\r' {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                record_index += 1;
                after_quote = false;
                field_started = false;
                i += if byte == b'\r' { 2 } else { 1 };
                continue;
            }
            return Err(CsvError::TrailingByte {
                record: record_index,
                byte: i,
            });
        }

        if byte == delimiter {
            record.push(std::mem::take(&mut field));
            field_started = false;
            i += 1;
            continue;
        }

        if byte == b'\n' || byte == b'\r' {
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            record_index += 1;
            field_started = false;
            i += if byte == b'\r' { 2 } else { 1 };
            continue;
        }

        if byte == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: i,
                });
            }
            in_quotes = true;
            opening_quote = i;
            field_started = true;
            i += 1;
            continue;
        }

        let ch = input[i..].chars().next().expect("valid UTF-8");
        field.push(ch);
        field_started = true;
        i += ch.len_utf8();
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: opening_quote,
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

fn build_table(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_config(config)?;
    let mut records = parse_with_dialect(input, config.delimiter, config.quote)?;

    let (header, rows) = if config.headers && !records.is_empty() {
        let header = records.remove(0);

        if config.duplicates == DuplicatePolicy::Reject {
            for second in 0..header.len() {
                if let Some(first) = header[..second]
                    .iter()
                    .position(|name| name == &header[second])
                {
                    return Err(CsvError::DuplicateHeader {
                        name: header[second].clone(),
                        first,
                        second,
                    });
                }
            }
        }

        for (offset, row) in records.iter().enumerate() {
            if row.len() != header.len() {
                return Err(CsvError::HeaderWidth {
                    record: offset + 1,
                    expected: header.len(),
                    actual: row.len(),
                });
            }
        }
        (Some(header), records)
    } else {
        (None, records)
    };

    Ok(Table {
        header,
        rows,
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
        validate_config(config)?;
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
        let result = build_table(&self.input, self.config);
        self.finished = Some(result.clone());
        result
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

fn integer_syntax(value: &str) -> bool {
    let bytes = value.as_bytes();
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
    digits[0].is_ascii_digit()
        && digits[0] != b'0'
        && digits.iter().all(u8::is_ascii_digit)
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
    let body = if bytes.first() == Some(&b'-') {
        &bytes[1..]
    } else {
        bytes
    };
    let Some(dot) = body.iter().position(|&byte| byte == b'.') else {
        return false;
    };
    if body[dot + 1..].len() != 2
        || !body[dot + 1..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }

    let integral = &body[..dot];
    if integral == b"0" {
        return bytes.first() != Some(&b'-');
    }
    !integral.is_empty()
        && integral[0].is_ascii_digit()
        && integral[0] != b'0'
        && integral.iter().all(u8::is_ascii_digit)
}

fn parse_decimal(value: &str) -> Result<i64, CellErrorKind> {
    if !decimal_syntax(value) {
        return Err(CellErrorKind::InvalidDecimal);
    }

    let negative = value.starts_with('-');
    let body = if negative { &value[1..] } else { value };
    let dot = body.find('.').expect("validated decimal");
    let integral = body[..dot]
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let fraction = body[dot + 1..]
        .parse::<i128>()
        .expect("validated fractional digits");
    let magnitude = integral * 100 + fraction;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn convert_cell(value: &str, spec: &ColumnSpec) -> Result<Value, CellErrorKind> {
    if value.is_empty() {
        if spec.nullable {
            return Ok(Value::Null);
        }
        if spec.kind != ColumnType::Text {
            return Err(CellErrorKind::Empty);
        }
    }

    match spec.kind {
        ColumnType::Text => Ok(Value::Text(value.to_owned())),
        ColumnType::Integer => parse_integer(value).map(Value::Integer),
        ColumnType::Decimal => parse_decimal(value).map(Value::Decimal),
        ColumnType::Boolean => match value {
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
        let mut output = Vec::with_capacity(columns.len());
        let mut stopped = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if stopped {
                output.push(None);
                continue;
            }

            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let evaluated = match resolved {
                None => Err((None, CellErrorKind::UnknownColumn)),
                Some(column) => match row.get(column) {
                    None => Err((Some(column), CellErrorKind::MissingCell)),
                    Some(value) => convert_cell(value, spec).map_err(|kind| (Some(column), kind)),
                },
            };

            match evaluated {
                Ok(value) => output.push(Some(value)),
                Err((column, kind)) => {
                    output.push(None);
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column,
                        kind,
                    });
                    if policy != ErrorPolicy::Collect {
                        stopped = true;
                    }
                    if policy == ErrorPolicy::Stop {
                        result.rows.push(output);
                        return result;
                    }
                }
            }
        }

        result.rows.push(output);
    }

    result
}