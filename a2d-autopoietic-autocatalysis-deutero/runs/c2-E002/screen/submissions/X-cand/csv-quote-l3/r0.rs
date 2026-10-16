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
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut i = start;
    let mut record_index = 0usize;
    let mut record_started = false;

    #[derive(Clone, Copy)]
    enum State {
        FieldStart,
        Unquoted,
        Quoted(usize),
        AfterQuote,
    }

    let mut state = State::FieldStart;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        let outside = !matches!(state, State::Quoted(_));
        if outside && b == b'\r' && (i + 1 >= bytes.len() || bytes[i + 1] != b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
        }

        match state {
            State::Quoted(opening) => {
                if b == quote {
                    if i + 1 < bytes.len() && bytes[i + 1] == quote {
                        field.push(quote as char);
                        i += 2;
                    } else {
                        state = State::AfterQuote;
                        i += 1;
                    }
                } else {
                    let ch = input[i..].chars().next().expect("valid UTF-8 boundary");
                    field.push(ch);
                    i += ch.len_utf8();
                }
                let _ = opening;
            }
            State::FieldStart => {
                record_started = true;
                if b == quote {
                    state = State::Quoted(i);
                    i += 1;
                } else if b == delimiter {
                    fields.push(String::new());
                    i += 1;
                } else if b == b'\n' {
                    fields.push(String::new());
                    records.push(std::mem::take(&mut fields));
                    record_index += 1;
                    record_started = false;
                    i += 1;
                } else if b == b'\r' {
                    fields.push(String::new());
                    records.push(std::mem::take(&mut fields));
                    record_index += 1;
                    record_started = false;
                    i += 2;
                } else {
                    let ch = input[i..].chars().next().expect("valid UTF-8 boundary");
                    field.push(ch);
                    state = State::Unquoted;
                    i += ch.len_utf8();
                }
            }
            State::Unquoted => {
                if b == quote {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: i,
                    });
                } else if b == delimiter {
                    fields.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    i += 1;
                } else if b == b'\n' {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record_index += 1;
                    record_started = false;
                    state = State::FieldStart;
                    i += 1;
                } else if b == b'\r' {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record_index += 1;
                    record_started = false;
                    state = State::FieldStart;
                    i += 2;
                } else {
                    let ch = input[i..].chars().next().expect("valid UTF-8 boundary");
                    field.push(ch);
                    i += ch.len_utf8();
                }
            }
            State::AfterQuote => {
                if b == delimiter {
                    fields.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    i += 1;
                } else if b == b'\n' {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record_index += 1;
                    record_started = false;
                    state = State::FieldStart;
                    i += 1;
                } else if b == b'\r' {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record_index += 1;
                    record_started = false;
                    state = State::FieldStart;
                    i += 2;
                } else {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: i,
                    });
                }
            }
        }
    }

    if let State::Quoted(opening) = state {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: opening,
        });
    }

    if record_started || !fields.is_empty() || !field.is_empty() {
        fields.push(field);
        records.push(fields);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with(input, b',', b'"')
}

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let mut iter = records.into_iter();
    let header = if config.headers {
        iter.next()
    } else {
        None
    };
    let rows: Vec<Vec<String>> = iter.collect();

    if let Some(ref names) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            let mut seen: Vec<(&str, usize)> = Vec::new();
            for (second, name) in names.iter().enumerate() {
                if let Some((_, first)) = seen.iter().find(|(prior, _)| *prior == name) {
                    return Err(CsvError::DuplicateHeader {
                        name: name.clone(),
                        first: *first,
                        second,
                    });
                }
                seen.push((name, second));
            }
        }

        for (row_index, row) in rows.iter().enumerate() {
            if row.len() != names.len() {
                return Err(CsvError::HeaderWidth {
                    record: row_index + 1,
                    expected: names.len(),
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
        let result = parse_with(&self.input, self.config.delimiter, self.config.quote)
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

fn integer_syntax(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }

    let digits = if bytes[0] == b'-' {
        &bytes[1..]
    } else {
        bytes
    };
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

fn decimal_cents(s: &str) -> Result<i64, CellErrorKind> {
    let bytes = s.as_bytes();
    let syntax_ok = if bytes.is_empty() {
        false
    } else {
        let (negative, body) = if bytes[0] == b'-' {
            (true, &bytes[1..])
        } else {
            (false, bytes)
        };
        let parts: Vec<&[u8]> = body.split(|&b| b == b'.').collect();
        if parts.len() != 2 || parts[1].len() != 2 {
            false
        } else {
            let whole = parts[0];
            let frac = parts[1];
            let whole_ok = if whole == b"0" {
                true
            } else {
                !whole.is_empty()
                    && whole[0].is_ascii_digit()
                    && whole[0] != b'0'
                    && whole.iter().all(u8::is_ascii_digit)
            };
            let frac_ok = frac.iter().all(u8::is_ascii_digit);
            let is_negative_zero = negative && whole == b"0" && frac == b"00";
            whole_ok && frac_ok && !is_negative_zero
        }
    };

    if !syntax_ok {
        return Err(CellErrorKind::InvalidDecimal);
    }
    s.parse::<f64>().ok();
    let negative = bytes[0] == b'-';
    let body = if negative { &s[1..] } else { s };
    let dot = body.find('.').expect("validated decimal");
    let whole = &body[..dot];
    let frac = &body[dot + 1..];
    let whole_value = whole
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let frac_value = frac
        .parse::<i128>()
        .expect("validated fractional digits");
    let magnitude = whole_value
        .checked_mul(100)
        .and_then(|v| v.checked_add(frac_value))
        .ok_or(CellErrorKind::DecimalOverflow)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn evaluate_cell(text: &str, kind: ColumnType, nullable: bool) -> Result<Value, CellErrorKind> {
    if text.is_empty() {
        if nullable {
            return Ok(Value::Null);
        }
        if kind != ColumnType::Text {
            return Err(CellErrorKind::Empty);
        }
    }

    match kind {
        ColumnType::Text => Ok(Value::Text(text.to_string())),
        ColumnType::Integer => {
            if !integer_syntax(text) {
                return Err(CellErrorKind::InvalidInteger);
            }
            text.parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| CellErrorKind::IntegerOverflow)
        }
        ColumnType::Decimal => decimal_cents(text).map(Value::Decimal),
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
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let mut stop = false;

    for (row_index, row) in table.rows.iter().enumerate() {
        if stop {
            break;
        }
        let mut values = Vec::with_capacity(columns.len());
        let mut row_failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if row_failed {
                values.push(None);
                continue;
            }

            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let result = if let Some(column_index) = resolved {
                if let Some(text) = row.get(column_index) {
                    evaluate_cell(text, spec.kind, spec.nullable)
                        .map(Some)
                        .map_err(|kind| (Some(column_index), kind))
                } else {
                    Err((Some(column_index), CellErrorKind::MissingCell))
                }
            } else {
                Err((None, CellErrorKind::UnknownColumn))
            };

            match result {
                Ok(value) => values.push(value),
                Err((column, kind)) => {
                    values.push(None);
                    errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column,
                        kind,
                    });
                    match policy {
                        ErrorPolicy::Collect => {}
                        ErrorPolicy::FirstPerRow => row_failed = true,
                        ErrorPolicy::Stop => {
                            row_failed = true;
                            stop = true;
                        }
                    }
                }
            }
        }

        output.push(values);
    }

    TypedResult {
        rows: output,
        errors,
    }
}