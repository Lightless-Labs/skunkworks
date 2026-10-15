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

fn parse_dialect(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut i = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) { 3 } else { 0 };
    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut after_quote = false;
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

        if !quoted && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
        }

        if quoted {
            if b == quote {
                if bytes.get(i + 1) == Some(&quote) {
                    field.push(quote as char);
                    i += 2;
                    continue;
                }
                quoted = false;
                after_quote = true;
                i += 1;
                continue;
            }
            let ch = input[i..].chars().next().unwrap();
            field.push(ch);
            i += ch.len_utf8();
            continue;
        }

        if after_quote {
            if b == delimiter {
                row.push(std::mem::take(&mut field));
                started = false;
                after_quote = false;
                i += 1;
                continue;
            }
            if b == b'\n' || b == b'\r' {
                row.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut row));
                started = false;
                after_quote = false;
                record_index += 1;
                i += if b == b'\r' { 2 } else { 1 };
                continue;
            }
            return Err(CsvError::TrailingByte {
                record: record_index,
                byte: i,
            });
        }

        if b == delimiter {
            row.push(std::mem::take(&mut field));
            started = false;
            i += 1;
            continue;
        }

        if b == b'\n' || b == b'\r' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            started = false;
            record_index += 1;
            i += if b == b'\r' { 2 } else { 1 };
            continue;
        }

        if b == quote {
            if started {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: i,
                });
            }
            quoted = true;
            quote_start = i;
            started = true;
            i += 1;
            continue;
        }

        let ch = input[i..].chars().next().unwrap();
        field.push(ch);
        started = true;
        i += ch.len_utf8();
    }

    if quoted {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_start,
        });
    }

    if started || after_quote || !row.is_empty() {
        row.push(field);
        records.push(row);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_dialect(input, b',', b'"')
}

fn duplicate_header(header: &[String]) -> Option<CsvError> {
    for second in 0..header.len() {
        if let Some(first) = header[..second].iter().position(|name| name == &header[second]) {
            return Some(CsvError::DuplicateHeader {
                name: header[second].clone(),
                first,
                second,
            });
        }
    }
    None
}

fn make_table(
    records: Vec<Vec<String>>,
    config: Config,
) -> Result<Table, CsvError> {
    let (header, rows) = if config.headers {
        if records.is_empty() {
            (None, Vec::new())
        } else {
            let mut records = records;
            let header = records.remove(0);
            (Some(header), records)
        }
    } else {
        (None, records)
    };

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
        let result = parse_dialect(&self.input, self.config.delimiter, self.config.quote)
            .and_then(|records| make_table(records, self.config));
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
    let Some(dot) = body.iter().position(|&c| c == b'.') else {
        return false;
    };
    if body[dot + 1..].len() != 2
        || !body[dot + 1..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    let whole = &body[..dot];
    if whole.is_empty() || !(whole == b"0" || ((b'1'..=b'9').contains(&whole[0])
        && whole[1..].iter().all(u8::is_ascii_digit)))
    {
        return false;
    }
    !(negative && whole == b"0" && body[dot + 1..] == *b"00")
}

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    if !integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>().map_err(|_| CellErrorKind::IntegerOverflow)
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !decimal_syntax(s) {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let negative = s.starts_with('-');
    let body = if negative { &s[1..] } else { s };
    let digits: String = body.chars().filter(|&c| c != '.').collect();
    let magnitude = digits
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn evaluate_cell(
    table: &Table,
    row: &[String],
    row_index: usize,
    spec: &ColumnSpec,
    spec_index: usize,
) -> (Option<Value>, Option<CellError>) {
    let column = match &spec.column {
        ColumnRef::Index(index) => Some(*index),
        ColumnRef::Name(name) => table.column_index(name),
    };
    let Some(column) = column else {
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
    let Some(cell) = row.get(column) else {
        return (
            None,
            Some(CellError {
                row: row_index,
                spec: spec_index,
                column: Some(column),
                kind: CellErrorKind::MissingCell,
            }),
        );
    };

    let failure = |kind| {
        (
            None,
            Some(CellError {
                row: row_index,
                spec: spec_index,
                column: Some(column),
                kind,
            }),
        )
    };

    if spec.nullable && cell.is_empty() {
        return (Some(Value::Null), None);
    }
    if cell.is_empty() && spec.kind != ColumnType::Text {
        return failure(CellErrorKind::Empty);
    }

    let value = match spec.kind {
        ColumnType::Text => Ok(Value::Text(cell.clone())),
        ColumnType::Integer => parse_integer(cell).map(Value::Integer),
        ColumnType::Decimal => parse_decimal(cell).map(Value::Decimal),
        ColumnType::Boolean => match cell.as_str() {
            "yes" | "1" => Ok(Value::Boolean(true)),
            "no" | "0" => Ok(Value::Boolean(false)),
            _ => Err(CellErrorKind::InvalidBoolean),
        },
    };

    match value {
        Ok(value) => (Some(value), None),
        Err(kind) => failure(kind),
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
            if policy == ErrorPolicy::FirstPerRow && row_failed {
                break;
            }
            let (value, error) = evaluate_cell(table, row, row_index, spec, spec_index);
            output[spec_index] = value;
            if let Some(error) = error {
                result.errors.push(error);
                row_failed = true;
                if policy == ErrorPolicy::Stop {
                    result.rows.push(output);
                    return result;
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

fn table_records(table: &Table) -> Vec<&[String]> {
    let mut records = Vec::new();
    if let Some(header) = &table.header {
        records.push(header.as_slice());
    }
    records.extend(table.rows.iter().map(Vec::as_slice));
    records
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let records = table_records(table);

    for (record_index, record) in records.iter().enumerate() {
        for (column, field) in record.iter().enumerate() {
            if let Some(byte) = field.as_bytes().iter().position(|&b| b == 0) {
                return Err(WriteError::Nul {
                    record: record_index,
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

    for (record_index, record) in records.iter().enumerate() {
        if record.is_empty() {
            return Err(WriteError::ZeroColumns {
                record: record_index,
            });
        }
    }

    if config.mode == WriteMode::Canonical {
        if table.header.is_some() && table.duplicates == DuplicatePolicy::Reject {
            if let Some(header) = &table.header {
                if let Some(CsvError::DuplicateHeader {
                    name,
                    first,
                    second,
                }) = duplicate_header(header)
                {
                    return Err(WriteError::DuplicateHeader {
                        name,
                        first,
                        second,
                    });
                }
            }
        }
    }

    let mut output_records: Vec<Vec<String>> = match config.mode {
        WriteMode::Preserve => records.into_iter().map(|r| r.to_vec()).collect(),
        WriteMode::Canonical => {
            if let Some(header) = &table.header {
                let mut retained: Vec<usize> = Vec::new();
                for column in 0..header.len() {
                    match table.duplicates {
                        DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                            if !header[..column].iter().any(|n| n == &header[column]) {
                                retained.push(column);
                            }
                        }
                        DuplicatePolicy::KeepLast => {
                            if !header[column + 1..].iter().any(|n| n == &header[column]) {
                                retained.push(column);
                            }
                        }
                    }
                }
                retained.sort_by(|&a, &b| header[a].as_bytes().cmp(header[b].as_bytes()));

                let projected_header = retained.iter().map(|&i| header[i].clone()).collect();
                let mut projected_rows: Vec<Vec<String>> = table
                    .rows
                    .iter()
                    .map(|row| retained.iter().map(|&i| row[i].clone()).collect())
                    .collect();
                projected_rows.sort_by(|a, b| compare_rows(a, b));

                let mut all = vec![projected_header];
                all.extend(projected_rows);
                all
            } else {
                let mut rows: Vec<Vec<String>> = table.rows.clone();
                rows.sort_by(|a, b| compare_rows(a, b));
                rows
            }
        }
    };

    if output_records.is_empty() {
        return Ok(String::new());
    }

    let delimiter = config.delimiter as char;
    let quote = config.quote as char;
    let canonical = config.mode == WriteMode::Canonical;
    let ending = match (canonical, config.line_ending) {
        (true, _) | (false, LineEnding::Lf) => "\n",
        (false, LineEnding::CrLf) => "\r\n",
    };
    let final_terminator = canonical || config.final_terminator;

    let mut output = String::new();
    for (record_index, record) in output_records.iter_mut().enumerate() {
        if record_index > 0 {
            output.push_str(ending);
        }
        for (column_index, field) in record.iter().enumerate() {
            if column_index > 0 {
                output.push(delimiter);
            }
            let bytes = field.as_bytes();
            let sole_empty = record.len() == 1 && field.is_empty();
            let starts_bom = record_index == 0
                && column_index == 0
                && field.starts_with('\u{feff}');
            let edge_space = field.starts_with(' ')
                || field.starts_with('\t')
                || field.ends_with(' ')
                || field.ends_with('\t');
            let needs_quote = sole_empty
                || starts_bom
                || edge_space
                || bytes.iter().any(|&b| {
                    b == config.delimiter || b == config.quote || b == b'\r' || b == b'\n'
                });

            if needs_quote {
                output.push(quote);
                for ch in field.chars() {
                    if ch == quote {
                        output.push(quote);
                    }
                    output.push(ch);
                }
                output.push(quote);
            } else {
                output.push_str(field);
            }
        }
    }
    if final_terminator {
        output.push_str(ending);
    }
    Ok(output)
}

fn compare_rows(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (left, right) in a.iter().zip(b.iter()) {
        match left.as_bytes().cmp(right.as_bytes()) {
            std::cmp::Ordering::Equal => {}
            order => return order,
        }
    }
    a.len().cmp(&b.len())
}

fn main() {}