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
    let start = if input.starts_with('\u{feff}') { 3 } else { 0 };
    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut i = start;
    let mut record = 0usize;
    let mut state = 0u8; // 0: field start, 1: unquoted, 2: quoted, 3: after quote
    let mut quote_start = 0usize;
    let mut record_started = false;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul { record, byte: i });
        }

        if state != 2 && b == b'\r' {
            if i + 1 >= bytes.len() || bytes[i + 1] != b'\n' {
                return Err(CsvError::BareCr { record, byte: i });
            }
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            record += 1;
            record_started = false;
            state = 0;
            i += 2;
            continue;
        }

        match state {
            0 => {
                if b == delimiter {
                    row.push(std::mem::take(&mut field));
                    record_started = true;
                    i += 1;
                } else if b == quote {
                    quote_start = i;
                    state = 2;
                    record_started = true;
                    i += 1;
                } else if b == b'\n' {
                    row.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut row));
                    record += 1;
                    record_started = false;
                    i += 1;
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    state = 1;
                    record_started = true;
                    i += ch.len_utf8();
                }
            }
            1 => {
                if b == delimiter {
                    row.push(std::mem::take(&mut field));
                    state = 0;
                    record_started = true;
                    i += 1;
                } else if b == b'\n' {
                    row.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut row));
                    record += 1;
                    record_started = false;
                    state = 0;
                    i += 1;
                } else if b == quote {
                    return Err(CsvError::UnexpectedQuote { record, byte: i });
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    i += ch.len_utf8();
                }
            }
            2 => {
                if b == quote {
                    if i + 1 < bytes.len() && bytes[i + 1] == quote {
                        field.push(quote as char);
                        i += 2;
                    } else {
                        state = 3;
                        i += 1;
                    }
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    i += ch.len_utf8();
                }
            }
            3 => {
                if b == delimiter {
                    row.push(std::mem::take(&mut field));
                    state = 0;
                    record_started = true;
                    i += 1;
                } else if b == b'\n' {
                    row.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut row));
                    record += 1;
                    record_started = false;
                    state = 0;
                    i += 1;
                } else {
                    return Err(CsvError::TrailingByte { record, byte: i });
                }
            }
            _ => unreachable!(),
        }
    }

    if state == 2 {
        return Err(CsvError::UnclosedQuote {
            record,
            byte: quote_start,
        });
    }

    if record_started || !row.is_empty() || !field.is_empty() || state == 3 {
        row.push(field);
        records.push(row);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn check_duplicate_header(header: &[String]) -> Option<CsvError> {
    for second in 0..header.len() {
        for first in 0..second {
            if header[first] == header[second] {
                return Some(CsvError::DuplicateHeader {
                    name: header[second].clone(),
                    first,
                    second,
                });
            }
        }
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

    if let Some(ref h) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            if let Some(error) = check_duplicate_header(h) {
                return Err(error);
            }
        }
        for (idx, row) in rows.iter().enumerate() {
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
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    let negative = b[0] == b'-';
    let mut i = usize::from(negative);
    if i == b.len() {
        return false;
    }
    if b[i] == b'0' {
        return i + 1 == b.len() && !negative;
    }
    if !(b'1'..=b'9').contains(&b[i]) {
        return false;
    }
    i += 1;
    while i < b.len() {
        if !b[i].is_ascii_digit() {
            return false;
        }
        i += 1;
    }
    true
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    let mut i = usize::from(b[0] == b'-');
    if i >= b.len() {
        return false;
    }
    let int_start = i;
    if b[i] == b'0' {
        i += 1;
        if i < b.len() && b[i].is_ascii_digit() {
            return false;
        }
    } else if (b'1'..=b'9').contains(&b[i]) {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    } else {
        return false;
    }
    if i >= b.len() || b[i] != b'.' {
        return false;
    }
    let integer_is_zero = i == int_start + 1 && b[int_start] == b'0';
    i += 1;
    if i + 2 != b.len() || !b[i].is_ascii_digit() || !b[i + 1].is_ascii_digit() {
        return false;
    }
    if b[0] == b'-' && integer_is_zero && b[i] == b'0' && b[i + 1] == b'0' {
        return false;
    }
    true
}

fn parse_decimal_value(s: &str) -> Option<i64> {
    let negative = s.starts_with('-');
    let body = if negative { &s[1..] } else { s };
    let dot = body.find('.')?;
    let whole: i128 = body[..dot].parse().ok()?;
    let frac: i128 = body[dot + 1..].parse().ok()?;
    let magnitude = whole.checked_mul(100)?.checked_add(frac)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).ok()
}

fn evaluate_cell(
    row: &[String],
    table: &Table,
    spec: &ColumnSpec,
    row_index: usize,
    spec_index: usize,
) -> (Option<Value>, Option<CellError>) {
    let column = match &spec.column {
        ColumnRef::Index(i) => Some(*i),
        ColumnRef::Name(name) => table.column_index(name),
    };

    let Some(column_index) = column else {
        return (
            None,
            Some(CellError {
                row: row_index,
                spec: spec_index,
                column: None,
                kind: CellErrorKind::UnknownColumn,
            }),
        );
    };

    let Some(cell) = row.get(column_index) else {
        return (
            None,
            Some(CellError {
                row: row_index,
                spec: spec_index,
                column: Some(column_index),
                kind: CellErrorKind::MissingCell,
            }),
        );
    };

    if cell.is_empty() {
        if spec.nullable {
            return (Some(Value::Null), None);
        }
        if spec.kind != ColumnType::Text {
            return (
                None,
                Some(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: Some(column_index),
                    kind: CellErrorKind::Empty,
                }),
            );
        }
    }

    let error = |kind| {
        Some(CellError {
            row: row_index,
            spec: spec_index,
            column: Some(column_index),
            kind,
        })
    };

    match spec.kind {
        ColumnType::Text => (Some(Value::Text(cell.clone())), None),
        ColumnType::Boolean => match cell.as_str() {
            "yes" | "1" => (Some(Value::Boolean(true)), None),
            "no" | "0" => (Some(Value::Boolean(false)), None),
            _ => (None, error(CellErrorKind::InvalidBoolean)),
        },
        ColumnType::Integer => {
            if !integer_syntax(cell) {
                return (None, error(CellErrorKind::InvalidInteger));
            }
            match cell.parse::<i64>() {
                Ok(v) => (Some(Value::Integer(v)), None),
                Err(_) => (None, error(CellErrorKind::IntegerOverflow)),
            }
        }
        ColumnType::Decimal => {
            if !decimal_syntax(cell) {
                return (None, error(CellErrorKind::InvalidDecimal));
            }
            match parse_decimal_value(cell) {
                Some(v) => (Some(Value::Decimal(v)), None),
                None => (None, error(CellErrorKind::DecimalOverflow)),
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
        rows: Vec::with_capacity(table.rows.len()),
        errors: Vec::new(),
    };
    let mut stopped = false;

    for (row_index, row) in table.rows.iter().enumerate() {
        if stopped {
            break;
        }
        let mut output = vec![None; columns.len()];
        let mut row_failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if row_failed {
                break;
            }
            let (value, error) = evaluate_cell(row, table, spec, row_index, spec_index);
            output[spec_index] = value;
            if let Some(error) = error {
                result.errors.push(error);
                match policy {
                    ErrorPolicy::Collect => {}
                    ErrorPolicy::FirstPerRow => row_failed = true,
                    ErrorPolicy::Stop => {
                        row_failed = true;
                        stopped = true;
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

fn map_duplicate(error: CsvError) -> WriteError {
    match error {
        CsvError::DuplicateHeader {
            name,
            first,
            second,
        } => WriteError::DuplicateHeader {
            name,
            first,
            second,
        },
        _ => unreachable!(),
    }
}

fn compare_vectors(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (left, right) in a.iter().zip(b.iter()) {
        let ord = left.as_bytes().cmp(right.as_bytes());
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    a.len().cmp(&b.len())
}

fn quote_field(
    field: &str,
    first_field_first_record: bool,
    sole_empty: bool,
    delimiter: u8,
    quote: u8,
) -> String {
    let bytes = field.as_bytes();
    let needs_quote = bytes.contains(&delimiter)
        || bytes.contains(&quote)
        || bytes.contains(&b'\r')
        || bytes.contains(&b'\n')
        || bytes.first().is_some_and(|b| *b == b' ' || *b == b'\t')
        || bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t')
        || (first_field_first_record && field.starts_with('\u{feff}'))
        || (sole_empty && field.is_empty());

    if !needs_quote {
        return field.to_string();
    }

    let mut out = String::with_capacity(field.len() + 2);
    out.push(quote as char);
    for ch in field.chars() {
        if ch as u32 == quote as u32 {
            out.push(quote as char);
        }
        out.push(ch);
    }
    out.push(quote as char);
    out
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_config(config.delimiter, config.quote).map_err(|e| match e {
        CsvError::InvalidConfig { component } => WriteError::InvalidConfig { component },
        _ => unreachable!(),
    })?;

    let mut original_records: Vec<&[String]> = Vec::new();
    if let Some(header) = &table.header {
        original_records.push(header);
    }
    original_records.extend(table.rows.iter().map(Vec::as_slice));

    for (record_index, row) in original_records.iter().enumerate() {
        for (column, field) in row.iter().enumerate() {
            if let Some(byte) = field.as_bytes().iter().position(|b| *b == 0) {
                return Err(WriteError::Nul {
                    record: record_index,
                    column,
                    byte,
                });
            }
        }
    }

    if let Some(header) = &table.header {
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

    for (record, row) in original_records.iter().enumerate() {
        if row.is_empty() {
            return Err(WriteError::ZeroColumns { record });
        }
    }

    if config.mode == WriteMode::Canonical
        && table.header.is_some()
        && table.duplicates == DuplicatePolicy::Reject
    {
        if let Some(header) = &table.header {
            if let Some(error) = check_duplicate_header(header) {
                return Err(map_duplicate(error));
            }
        }
    }

    let records: Vec<Vec<String>> = match config.mode {
        WriteMode::Preserve => original_records.iter().map(|r| r.to_vec()).collect(),
        WriteMode::Canonical => {
            let mut output = Vec::new();
            if let Some(header) = &table.header {
                let mut chosen: Vec<(String, usize)> = Vec::new();
                for (index, name) in header.iter().enumerate() {
                    if let Some(position) = chosen.iter().position(|(n, _)| n == name) {
                        match table.duplicates {
                            DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {}
                            DuplicatePolicy::KeepLast => chosen[position].1 = index,
                        }
                    } else {
                        chosen.push((name.clone(), index));
                    }
                }
                chosen.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
                let indexes: Vec<usize> = chosen.iter().map(|(_, i)| *i).collect();
                output.push(chosen.into_iter().map(|(name, _)| name).collect());
                for row in &table.rows {
                    output.push(indexes.iter().map(|i| row[*i].clone()).collect());
                }
            } else {
                output.extend(table.rows.iter().cloned());
            }
            let start = usize::from(table.header.is_some());
            output[start..].sort_by(|a, b| compare_vectors(a, b));
            output
        }
    };

    if records.is_empty() {
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
    for (record_index, row) in records.iter().enumerate() {
        if record_index > 0 {
            output.push_str(line_ending);
        }
        let sole_empty = row.len() == 1 && row[0].is_empty();
        for (column_index, field) in row.iter().enumerate() {
            if column_index > 0 {
                output.push(config.delimiter as char);
            }
            output.push_str(&quote_field(
                field,
                record_index == 0 && column_index == 0,
                sole_empty,
                config.delimiter,
                config.quote,
            ));
        }
    }
    if final_terminator {
        output.push_str(line_ending);
    }

    Ok(output)
}