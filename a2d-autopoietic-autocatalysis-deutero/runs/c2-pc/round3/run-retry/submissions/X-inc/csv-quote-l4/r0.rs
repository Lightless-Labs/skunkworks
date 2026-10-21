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

pub struct StreamReader {
    config: Config,
    input: String,
    result: Option<Result<Table, CsvError>>,
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
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    if i == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut state = 0u8; // 0: field start, 1: unquoted, 2: quoted, 3: after quote
    let mut opening_quote = 0usize;
    let mut record_index = 0usize;
    let mut field_start = i;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if state != 2 && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
        }

        match state {
            0 => {
                if b == delimiter {
                    record.push(String::new());
                    i += 1;
                    field_start = i;
                } else if b == quote {
                    opening_quote = i;
                    state = 2;
                    i += 1;
                    field_start = i;
                } else if b == b'\n' {
                    record.push(String::new());
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    i += 1;
                    field_start = i;
                } else if b == b'\r' {
                    record.push(String::new());
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    i += 2;
                    field_start = i;
                } else {
                    state = 1;
                    i += 1;
                }
            }
            1 => {
                if b == delimiter {
                    field.push_str(&input[field_start..i]);
                    record.push(std::mem::take(&mut field));
                    state = 0;
                    i += 1;
                    field_start = i;
                } else if b == quote {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: i,
                    });
                } else if b == b'\n' {
                    field.push_str(&input[field_start..i]);
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = 0;
                    i += 1;
                    field_start = i;
                } else if b == b'\r' {
                    field.push_str(&input[field_start..i]);
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = 0;
                    i += 2;
                    field_start = i;
                } else {
                    i += 1;
                }
            }
            2 => {
                if b == quote {
                    field.push_str(&input[field_start..i]);
                    if bytes.get(i + 1) == Some(&quote) {
                        field.push(quote as char);
                        i += 2;
                        field_start = i;
                    } else {
                        state = 3;
                        i += 1;
                        field_start = i;
                    }
                } else {
                    i += 1;
                }
            }
            _ => {
                if b == delimiter {
                    record.push(std::mem::take(&mut field));
                    state = 0;
                    i += 1;
                    field_start = i;
                } else if b == b'\n' {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = 0;
                    i += 1;
                    field_start = i;
                } else if b == b'\r' {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    state = 0;
                    i += 2;
                    field_start = i;
                } else {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: i,
                    });
                }
            }
        }
    }

    if state == 2 {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: opening_quote,
        });
    }

    if state == 1 {
        field.push_str(&input[field_start..]);
        record.push(field);
        records.push(record);
    } else if state == 3 || !record.is_empty() {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
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

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    if !config.headers {
        return Ok(Table {
            header: None,
            rows: records,
            duplicates: config.duplicates,
        });
    }

    let mut iter = records.into_iter();
    let header = iter.next();
    let rows: Vec<Vec<String>> = iter.collect();

    if let Some(ref header_values) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            if let Some(error) = duplicate_header(header_values) {
                return Err(error);
            }
        }
        for (index, row) in rows.iter().enumerate() {
            if row.len() != header_values.len() {
                return Err(CsvError::HeaderWidth {
                    record: index + 1,
                    expected: header_values.len(),
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
        validate_dialect(config.delimiter, config.quote)
            .map_err(|component| CsvError::InvalidConfig { component })?;
        Ok(Self {
            config,
            input: String::new(),
            result: None,
        })
    }

    pub fn feed(&mut self, chunk: &str) -> Result<(), CsvError> {
        if self.result.is_some() {
            return Err(CsvError::ReaderFinished);
        }
        self.input.push_str(chunk);
        Ok(())
    }

    pub fn finish(&mut self) -> Result<Table, CsvError> {
        if let Some(result) = &self.result {
            return result.clone();
        }
        let result = parse_with_dialect(&self.input, self.config.delimiter, self.config.quote)
            .and_then(|records| build_table(records, self.config));
        self.result = Some(result.clone());
        result
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.result = None;
    }
}

pub fn read_csv(input: &str, config: Config) -> Result<Table, CsvError> {
    let mut reader = StreamReader::new(config)?;
    reader.feed(input)?;
    reader.finish()
}

fn integer_syntax(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let digits = if bytes[0] == b'-' { &bytes[1..] } else { bytes };
    if digits.is_empty() || (digits.len() > 1 && digits[0] == b'0') {
        return false;
    }
    if !digits.iter().all(u8::is_ascii_digit) {
        return false;
    }
    if bytes[0] == b'-' && digits == b"0" {
        return false;
    }
    true
}

fn decimal_syntax(value: &str) -> bool {
    let bytes = value.as_bytes();
    let body = if bytes.first() == Some(&b'-') {
        &bytes[1..]
    } else {
        bytes
    };
    let Some(dot) = body.iter().position(|&b| b == b'.') else {
        return false;
    };
    let integral = &body[..dot];
    let fractional = &body[dot + 1..];
    if fractional.len() != 2 || !fractional.iter().all(u8::is_ascii_digit) {
        return false;
    }
    if integral.is_empty()
        || (integral.len() > 1 && integral[0] == b'0')
        || !integral.iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    if bytes.first() == Some(&b'-') && integral == b"0" && fractional == b"00" {
        return false;
    }
    true
}

fn evaluate_cell(
    value: &str,
    spec: &ColumnSpec,
    row: usize,
    spec_index: usize,
    column: usize,
) -> Result<Value, CellError> {
    let error = |kind| CellError {
        row,
        spec: spec_index,
        column: Some(column),
        kind,
    };

    if value.is_empty() {
        if spec.nullable {
            return Ok(Value::Null);
        }
        if spec.kind != ColumnType::Text {
            return Err(error(CellErrorKind::Empty));
        }
    }

    match spec.kind {
        ColumnType::Text => Ok(Value::Text(value.to_string())),
        ColumnType::Integer => {
            if !integer_syntax(value) {
                return Err(error(CellErrorKind::InvalidInteger));
            }
            match value.parse::<i64>() {
                Ok(number) => Ok(Value::Integer(number)),
                Err(_) => Err(error(CellErrorKind::IntegerOverflow)),
            }
        }
        ColumnType::Decimal => {
            if !decimal_syntax(value) {
                return Err(error(CellErrorKind::InvalidDecimal));
            }
            let negative = value.starts_with('-');
            let body = if negative { &value[1..] } else { value };
            let (whole, fraction) = body.split_once('.').unwrap();
            let magnitude = format!("{whole}{fraction}");
            let signed = if negative {
                format!("-{magnitude}")
            } else {
                magnitude
            };
            match signed.parse::<i64>() {
                Ok(number) => Ok(Value::Decimal(number)),
                Err(_) => Err(error(CellErrorKind::DecimalOverflow)),
            }
        }
        ColumnType::Boolean => match value {
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
        rows: Vec::new(),
        errors: Vec::new(),
    };

    for (row_index, row) in table.rows.iter().enumerate() {
        let mut output = vec![None; columns.len()];
        let mut failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let evaluated = match resolved {
                None => Err(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: None,
                    kind: CellErrorKind::UnknownColumn,
                }),
                Some(index) => match row.get(index) {
                    None => Err(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(index),
                        kind: CellErrorKind::MissingCell,
                    }),
                    Some(value) => evaluate_cell(value, spec, row_index, spec_index, index),
                },
            };

            match evaluated {
                Ok(value) => output[spec_index] = Some(value),
                Err(error) => {
                    result.errors.push(error);
                    failed = true;
                    if policy != ErrorPolicy::Collect {
                        break;
                    }
                }
            }
        }

        result.rows.push(output);
        if failed && policy == ErrorPolicy::Stop {
            break;
        }
    }

    result
}

fn bytewise_vec_cmp(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (left, right) in a.iter().zip(b.iter()) {
        let order = left.as_bytes().cmp(right.as_bytes());
        if order != std::cmp::Ordering::Equal {
            return order;
        }
    }
    a.len().cmp(&b.len())
}

fn find_duplicate(header: &[String]) -> Option<(String, usize, usize)> {
    for second in 0..header.len() {
        if let Some(first) = header[..second]
            .iter()
            .position(|value| value == &header[second])
        {
            return Some((header[second].clone(), first, second));
        }
    }
    None
}

fn write_field(
    output: &mut String,
    value: &str,
    delimiter: u8,
    quote: u8,
    first_field_first_record: bool,
    sole_empty: bool,
) {
    let bytes = value.as_bytes();
    let edge_space = bytes.first().is_some_and(|b| *b == b' ' || *b == b'\t')
        || bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t');
    let initial_bom = first_field_first_record && value.starts_with('\u{feff}');
    let must_quote = bytes.contains(&delimiter)
        || bytes.contains(&quote)
        || value.contains('\r')
        || value.contains('\n')
        || edge_space
        || initial_bom
        || sole_empty;

    if !must_quote {
        output.push_str(value);
        return;
    }

    output.push(quote as char);
    for ch in value.chars() {
        if ch == quote as char {
            output.push(ch);
        }
        output.push(ch);
    }
    output.push(quote as char);
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let mut original_records: Vec<&[String]> = Vec::new();
    if let Some(header) = table.header.as_ref() {
        original_records.push(header);
    }
    for row in &table.rows {
        original_records.push(row);
    }

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

    if let Some(header) = table.header.as_ref() {
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

    for (record_index, record) in original_records.iter().enumerate() {
        if record.is_empty() {
            return Err(WriteError::ZeroColumns {
                record: record_index,
            });
        }
    }

    if config.mode == WriteMode::Canonical {
        if let Some(header) = table.header.as_ref() {
            if table.duplicates == DuplicatePolicy::Reject {
                if let Some((name, first, second)) = find_duplicate(header) {
                    return Err(WriteError::DuplicateHeader {
                        name,
                        first,
                        second,
                    });
                }
            }
        }
    }

    let mut output_records: Vec<Vec<String>> = Vec::new();

    match config.mode {
        WriteMode::Preserve => {
            if let Some(header) = &table.header {
                output_records.push(header.clone());
            }
            output_records.extend(table.rows.iter().cloned());
        }
        WriteMode::Canonical => {
            if let Some(header) = &table.header {
                let mut selected = Vec::new();
                for (index, name) in header.iter().enumerate() {
                    let retained = match table.duplicates {
                        DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                            !header[..index].iter().any(|prior| prior == name)
                        }
                        DuplicatePolicy::KeepLast => {
                            !header[index + 1..].iter().any(|later| later == name)
                        }
                    };
                    if retained {
                        selected.push((name.clone(), index));
                    }
                }
                selected.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

                let projected_header: Vec<String> =
                    selected.iter().map(|(name, _)| name.clone()).collect();
                output_records.push(projected_header);
                let mut rows: Vec<Vec<String>> = table
                    .rows
                    .iter()
                    .map(|row| selected.iter().map(|(_, index)| row[*index].clone()).collect())
                    .collect();
                rows.sort_by(|a, b| bytewise_vec_cmp(a, b));
                output_records.extend(rows);
            } else {
                let mut rows = table.rows.clone();
                rows.sort_by(|a, b| bytewise_vec_cmp(a, b));
                output_records.extend(rows);
            }
        }
    }

    if output_records.is_empty() {
        return Ok(String::new());
    }

    let mut output = String::new();
    let canonical = config.mode == WriteMode::Canonical;
    let ending = if canonical {
        "\n"
    } else {
        match config.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    };

    for (record_index, record) in output_records.iter().enumerate() {
        if record_index > 0 {
            output.push_str(ending);
        }
        let sole_empty = record.len() == 1 && record[0].is_empty();
        for (column_index, field) in record.iter().enumerate() {
            if column_index > 0 {
                output.push(config.delimiter as char);
            }
            write_field(
                &mut output,
                field,
                config.delimiter,
                config.quote,
                record_index == 0 && column_index == 0,
                sole_empty,
            );
        }
    }

    if canonical || config.final_terminator {
        output.push_str(ending);
    }
    Ok(output)
}