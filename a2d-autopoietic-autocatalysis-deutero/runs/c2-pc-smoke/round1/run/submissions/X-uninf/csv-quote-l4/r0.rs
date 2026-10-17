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
    let mut pos = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };
    if pos == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut state = 0u8; // 0: field start, 1: unquoted, 2: quoted, 3: after quote
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

        if state != 2 && b == b'\r' && bytes.get(pos + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: pos,
            });
        }

        if state == 2 {
            if b == quote {
                if bytes.get(pos + 1) == Some(&quote) {
                    field.push(quote as char);
                    pos += 2;
                } else {
                    state = 3;
                    pos += 1;
                }
            } else {
                let ch = input[pos..].chars().next().unwrap();
                field.push(ch);
                pos += ch.len_utf8();
            }
            continue;
        }

        if state == 3 {
            if b == delimiter {
                row.push(std::mem::take(&mut field));
                state = 0;
                pos += 1;
            } else if b == b'\n' || b == b'\r' {
                row.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut row));
                state = 0;
                record_index += 1;
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
            row.push(std::mem::take(&mut field));
            state = 0;
            pos += 1;
        } else if b == b'\n' || b == b'\r' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            state = 0;
            record_index += 1;
            pos += if b == b'\r' { 2 } else { 1 };
        } else if b == quote {
            if state == 0 {
                quote_start = pos;
                state = 2;
                pos += 1;
            } else {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: pos,
                });
            }
        } else {
            let ch = input[pos..].chars().next().unwrap();
            field.push(ch);
            state = 1;
            pos += ch.len_utf8();
        }
    }

    if state == 2 {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_start,
        });
    }

    if !row.is_empty() || !field.is_empty() || state != 0 {
        row.push(field);
        records.push(row);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn duplicate_header(header: &[String]) -> Option<CsvError> {
    let mut seen: Vec<(&str, usize)> = Vec::new();
    for (second, name) in header.iter().enumerate() {
        if let Some((_, first)) = seen.iter().find(|(prior, _)| *prior == name) {
            return Some(CsvError::DuplicateHeader {
                name: name.clone(),
                first: *first,
                second,
            });
        }
        seen.push((name, second));
    }
    None
}

fn table_from_records(
    records: Vec<Vec<String>>,
    config: Config,
) -> Result<Table, CsvError> {
    let mut iter = records.into_iter();
    let header = if config.headers { iter.next() } else { None };
    let rows: Vec<Vec<String>> = iter.collect();

    if let Some(ref header) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            if let Some(error) = duplicate_header(header) {
                return Err(error);
            }
        }
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
    finished: Option<Result<Table, CsvError>>,
}

impl StreamReader {
    pub fn new(config: Config) -> Result<Self, CsvError> {
        validate_dialect(config.delimiter, config.quote)
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
        if let Some(result) = &self.finished {
            return result.clone();
        }
        let result = parse_with_dialect(&self.input, self.config.delimiter, self.config.quote)
            .and_then(|records| table_from_records(records, self.config));
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
    (b'1'..=b'9').contains(&b[start]) && b[start + 1..].iter().all(u8::is_ascii_digit)
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    let start = usize::from(b[0] == b'-');
    let dot = match b[start..].iter().position(|&x| x == b'.') {
        Some(i) => start + i,
        None => return false,
    };
    if b.len() != dot + 3 {
        return false;
    }
    if !b[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    let integral = &s[start..dot];
    !integral.is_empty()
        && (integral == "0"
            || (integral.as_bytes()[0].is_ascii_digit()
                && integral.as_bytes()[0] != b'0'
                && integral.as_bytes().iter().all(u8::is_ascii_digit)))
        && !(start == 1 && integral == "0" && &s[dot + 1..] == "00")
}

fn parse_decimal_hundredths(s: &str) -> Option<i64> {
    let neg = s.starts_with('-');
    let body = if neg { &s[1..] } else { s };
    let dot = body.find('.')?;
    let whole: i128 = body[..dot].parse().ok()?;
    let frac: i128 = body[dot + 1..].parse().ok()?;
    let magnitude = whole.checked_mul(100)?.checked_add(frac)?;
    let signed = if neg { -magnitude } else { magnitude };
    i64::try_from(signed).ok()
}

fn evaluate_cell(
    cell: &str,
    kind: ColumnType,
    nullable: bool,
) -> Result<Value, CellErrorKind> {
    if cell.is_empty() {
        if nullable {
            return Ok(Value::Null);
        }
        if kind != ColumnType::Text {
            return Err(CellErrorKind::Empty);
        }
    }

    match kind {
        ColumnType::Text => Ok(Value::Text(cell.to_string())),
        ColumnType::Integer => {
            if !integer_syntax(cell) {
                return Err(CellErrorKind::InvalidInteger);
            }
            cell.parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| CellErrorKind::IntegerOverflow)
        }
        ColumnType::Decimal => {
            if !decimal_syntax(cell) {
                return Err(CellErrorKind::InvalidDecimal);
            }
            parse_decimal_hundredths(cell)
                .map(Value::Decimal)
                .ok_or(CellErrorKind::DecimalOverflow)
        }
        ColumnType::Boolean => match cell {
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
        let mut output = vec![None; columns.len()];
        let mut row_failed = false;
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
                    Some(cell) => (Some(index), evaluate_cell(cell, spec.kind, spec.nullable)),
                },
            };
            match value {
                Ok(value) => output[spec_index] = Some(value),
                Err(kind) => {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column,
                        kind,
                    });
                    row_failed = true;
                    if policy != ErrorPolicy::Collect {
                        break;
                    }
                    if policy == ErrorPolicy::Stop {
                        result.rows.push(output);
                        return result;
                    }
                }
            }
        }
        result.rows.push(output);
        if row_failed && policy == ErrorPolicy::Stop {
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

fn first_duplicate(header: &[String]) -> Option<(String, usize, usize)> {
    let mut seen: Vec<(&str, usize)> = Vec::new();
    for (second, name) in header.iter().enumerate() {
        if let Some((_, first)) = seen.iter().find(|(prior, _)| *prior == name) {
            return Some((name.clone(), *first, second));
        }
        seen.push((name, second));
    }
    None
}

fn quote_field(s: &str, delimiter: u8, quote: u8, first_field_first_record: bool, sole: bool) -> String {
    let bytes = s.as_bytes();
    let needs_quote = bytes.contains(&delimiter)
        || bytes.contains(&quote)
        || bytes.contains(&b'\r')
        || bytes.contains(&b'\n')
        || bytes.first().is_some_and(|b| *b == b' ' || *b == b'\t')
        || bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t')
        || (first_field_first_record && s.starts_with('\u{feff}'))
        || (sole && s.is_empty());

    if !needs_quote {
        return s.to_string();
    }

    let mut out = String::new();
    out.push(quote as char);
    for ch in s.chars() {
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
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let mut original_records: Vec<&Vec<String>> = Vec::new();
    if let Some(header) = &table.header {
        original_records.push(header);
    }
    original_records.extend(table.rows.iter());

    for (record, row) in original_records.iter().enumerate() {
        for (column, field) in row.iter().enumerate() {
            if let Some(byte) = field.as_bytes().iter().position(|&b| b == 0) {
                return Err(WriteError::Nul {
                    record,
                    column,
                    byte,
                });
            }
        }
    }

    if let Some(header) = &table.header {
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

    for (record, row) in original_records.iter().enumerate() {
        if row.is_empty() {
            return Err(WriteError::ZeroColumns { record });
        }
    }

    if config.mode == WriteMode::Canonical {
        if let Some(header) = &table.header {
            if table.duplicates == DuplicatePolicy::Reject {
                if let Some((name, first, second)) = first_duplicate(header) {
                    return Err(WriteError::DuplicateHeader {
                        name,
                        first,
                        second,
                    });
                }
            }
        }
    }

    let mut records: Vec<Vec<String>> = Vec::new();
    match config.mode {
        WriteMode::Preserve => {
            if let Some(header) = &table.header {
                records.push(header.clone());
            }
            records.extend(table.rows.clone());
        }
        WriteMode::Canonical => {
            if let Some(header) = &table.header {
                let mut retained: Vec<(String, usize)> = Vec::new();
                for (index, name) in header.iter().enumerate() {
                    if let Some(position) = retained.iter().position(|(old, _)| old == name) {
                        match table.duplicates {
                            DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {}
                            DuplicatePolicy::KeepLast => retained[position] = (name.clone(), index),
                        }
                    } else {
                        retained.push((name.clone(), index));
                    }
                }
                retained.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
                records.push(retained.iter().map(|(name, _)| name.clone()).collect());
                let mut projected: Vec<Vec<String>> = table
                    .rows
                    .iter()
                    .map(|row| retained.iter().map(|(_, index)| row[*index].clone()).collect())
                    .collect();
                projected.sort_by(|a, b| {
                    for (x, y) in a.iter().zip(b.iter()) {
                        let ordering = x.as_bytes().cmp(y.as_bytes());
                        if ordering != std::cmp::Ordering::Equal {
                            return ordering;
                        }
                    }
                    a.len().cmp(&b.len())
                });
                records.extend(projected);
            } else {
                let mut rows = table.rows.clone();
                rows.sort_by(|a, b| {
                    for (x, y) in a.iter().zip(b.iter()) {
                        let ordering = x.as_bytes().cmp(y.as_bytes());
                        if ordering != std::cmp::Ordering::Equal {
                            return ordering;
                        }
                    }
                    a.len().cmp(&b.len())
                });
                records = rows;
            }
        }
    }

    if records.is_empty() {
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
    for (record_index, row) in records.iter().enumerate() {
        if record_index > 0 {
            output.push_str(terminator);
        }
        let sole = row.len() == 1;
        for (column_index, field) in row.iter().enumerate() {
            if column_index > 0 {
                output.push(config.delimiter as char);
            }
            output.push_str(&quote_field(
                field,
                config.delimiter,
                config.quote,
                record_index == 0 && column_index == 0,
                sole,
            ));
        }
    }
    if canonical || config.final_terminator {
        output.push_str(terminator);
    }
    Ok(output)
}