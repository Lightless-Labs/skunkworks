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
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
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
    let start = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) { 3 } else { 0 };
    if start == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut i = start;
    let mut record = 0usize;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut quote_start = 0usize;
    let mut field_started = false;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul { record, byte: i });
        }

        if !in_quotes && b == b'\r' && (i + 1 >= bytes.len() || bytes[i + 1] != b'\n') {
            return Err(CsvError::BareCr { record, byte: i });
        }

        if in_quotes {
            if b == quote {
                if i + 1 < bytes.len() && bytes[i + 1] == quote {
                    field.push(quote as char);
                    i += 2;
                    continue;
                } else {
                    in_quotes = false;
                    after_quote = true;
                    i += 1;
                    continue;
                }
            }
            let ch = input[i..].chars().next().unwrap();
            field.push(ch);
            i += ch.len_utf8();
            continue;
        }

        if after_quote {
            if b == delimiter {
                row.push(std::mem::take(&mut field));
                after_quote = false;
                field_started = false;
                i += 1;
                continue;
            }
            if b == b'\n' || b == b'\r' {
                row.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut row));
                record += 1;
                after_quote = false;
                field_started = false;
                i += if b == b'\r' { 2 } else { 1 };
                continue;
            }
            return Err(CsvError::TrailingByte { record, byte: i });
        }

        if b == quote {
            if field_started || !field.is_empty() {
                return Err(CsvError::UnexpectedQuote { record, byte: i });
            }
            in_quotes = true;
            quote_start = i;
            field_started = true;
            i += 1;
            continue;
        }
        if b == delimiter {
            row.push(std::mem::take(&mut field));
            field_started = false;
            i += 1;
            continue;
        }
        if b == b'\n' || b == b'\r' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            record += 1;
            field_started = false;
            i += if b == b'\r' { 2 } else { 1 };
            continue;
        }

        let ch = input[i..].chars().next().unwrap();
        field.push(ch);
        field_started = true;
        i += ch.len_utf8();
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record,
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
    parse_with(input, b',', b'"')
}

fn table_from_records(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let mut header = None;
    let rows;
    if config.headers {
        if !records.is_empty() {
            header = Some(records[0].clone());
            rows = records[1..].to_vec();
        } else {
            rows = Vec::new();
        }
    } else {
        rows = records;
    }

    if let Some(h) = &header {
        if config.duplicates == DuplicatePolicy::Reject {
            let mut seen: Vec<(&str, usize)> = Vec::new();
            for (second, name) in h.iter().enumerate() {
                if let Some((_, first)) = seen.iter().find(|(n, _)| *n == name) {
                    return Err(CsvError::DuplicateHeader {
                        name: name.clone(),
                        first: *first,
                        second,
                    });
                }
                seen.push((name, second));
            }
        }

        for (offset, row) in rows.iter().enumerate() {
            if row.len() != h.len() {
                return Err(CsvError::HeaderWidth {
                    record: offset + 1,
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
        validate_config(config.delimiter, config.quote)
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
        let result = parse_with(&self.input, self.config.delimiter, self.config.quote)
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
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return false;
    }
    if digits.len() > 1 && digits[0] == b'0' {
        return false;
    }
    !(b[0] == b'-' && digits == b"0")
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    let body = if b.first() == Some(&b'-') { &b[1..] } else { b };
    let Some(dot) = body.iter().position(|&x| x == b'.') else {
        return false;
    };
    if body[dot + 1..].len() != 2 || !body[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    let integral = &body[..dot];
    if integral.is_empty() || !integral.iter().all(u8::is_ascii_digit) {
        return false;
    }
    if integral.len() > 1 && integral[0] == b'0' {
        return false;
    }
    !(b.first() == Some(&b'-') && integral == b"0" && body[dot + 1..] == *b"00")
}

fn evaluate(cell: &str, spec: &ColumnSpec) -> Result<Value, CellErrorKind> {
    if spec.nullable && cell.is_empty() {
        return Ok(Value::Null);
    }
    if cell.is_empty() && spec.kind != ColumnType::Text {
        return Err(CellErrorKind::Empty);
    }

    match spec.kind {
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
            let negative = cell.starts_with('-');
            let body = if negative { &cell[1..] } else { cell };
            let mut parts = body.split('.');
            let whole = parts.next().unwrap().parse::<i128>().unwrap();
            let frac = parts.next().unwrap().parse::<i128>().unwrap();
            let value = whole * 100 + frac;
            let signed = if negative { -value } else { value };
            i64::try_from(signed)
                .map(Value::Decimal)
                .map_err(|_| CellErrorKind::DecimalOverflow)
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
        let mut failed = false;
        for (spec_index, spec) in columns.iter().enumerate() {
            if failed {
                break;
            }
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
                    Some(cell) => (Some(index), evaluate(cell, spec)),
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
                    if policy != ErrorPolicy::Collect {
                        failed = true;
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

fn duplicate_header(
    header: &[String],
) -> Option<(String, usize, usize)> {
    let mut seen: Vec<(&str, usize)> = Vec::new();
    for (second, name) in header.iter().enumerate() {
        if let Some((_, first)) = seen.iter().find(|(n, _)| *n == name) {
            return Some((name.clone(), *first, second));
        }
        seen.push((name, second));
    }
    None
}

fn compare_rows(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        match x.as_bytes().cmp(y.as_bytes()) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
    }
    a.len().cmp(&b.len())
}

fn encode_field(
    out: &mut String,
    field: &str,
    delimiter: u8,
    quote: u8,
    first_field_first_record: bool,
    sole_empty: bool,
) {
    let bytes = field.as_bytes();
    let quote_it = bytes.contains(&delimiter)
        || bytes.contains(&quote)
        || bytes.contains(&b'\r')
        || bytes.contains(&b'\n')
        || bytes.first().is_some_and(|&b| b == b' ' || b == b'\t')
        || bytes.last().is_some_and(|&b| b == b' ' || b == b'\t')
        || (first_field_first_record && field.starts_with('\u{feff}'))
        || (sole_empty && field.is_empty());

    if !quote_it {
        out.push_str(field);
        return;
    }
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
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_config(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let mut original_records: Vec<&Vec<String>> = Vec::new();
    if let Some(header) = &table.header {
        original_records.push(header);
    }
    original_records.extend(table.rows.iter());

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

    let mut records: Vec<Vec<String>> = original_records.iter().map(|r| (*r).clone()).collect();
    if config.mode == WriteMode::Canonical {
        if let Some(header) = &table.header {
            if table.duplicates == DuplicatePolicy::Reject {
                if let Some((name, first, second)) = duplicate_header(header) {
                    return Err(WriteError::DuplicateHeader {
                        name,
                        first,
                        second,
                    });
                }
            }

            let mut chosen: Vec<usize> = Vec::new();
            for (idx, name) in header.iter().enumerate() {
                let candidates: Vec<usize> = header
                    .iter()
                    .enumerate()
                    .filter_map(|(j, other)| if other == name { Some(j) } else { None })
                    .collect();
                let keep = match table.duplicates {
                    DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => candidates[0],
                    DuplicatePolicy::KeepLast => *candidates.last().unwrap(),
                };
                if !chosen.contains(&keep) {
                    chosen.push(keep);
                }
                let _ = idx;
            }
            chosen.sort_by(|&a, &b| header[a].as_bytes().cmp(header[b].as_bytes()));

            let projected_header: Vec<String> =
                chosen.iter().map(|&i| header[i].clone()).collect();
            let mut projected_rows: Vec<Vec<String>> = table
                .rows
                .iter()
                .map(|row| chosen.iter().map(|&i| row[i].clone()).collect())
                .collect();
            projected_rows.sort_by(|a, b| compare_rows(a, b));

            records.clear();
            records.push(projected_header);
            records.extend(projected_rows);
        } else {
            let mut rows = table.rows.clone();
            rows.sort_by(|a, b| compare_rows(a, b));
            records = rows;
        }
    }

    if records.is_empty() {
        return Ok(String::new());
    }

    let canonical = config.mode == WriteMode::Canonical;
    let line_ending = if canonical {
        "\n"
    } else {
        match config.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    };

    let mut out = String::new();
    for (record_index, record) in records.iter().enumerate() {
        if record_index > 0 {
            out.push_str(line_ending);
        }
        for (column_index, field) in record.iter().enumerate() {
            if column_index > 0 {
                out.push(config.delimiter as char);
            }
            encode_field(
                &mut out,
                field,
                config.delimiter,
                config.quote,
                record_index == 0 && column_index == 0,
                record.len() == 1,
            );
        }
    }
    if canonical || config.final_terminator {
        out.push_str(line_ending);
    }
    Ok(out)
}