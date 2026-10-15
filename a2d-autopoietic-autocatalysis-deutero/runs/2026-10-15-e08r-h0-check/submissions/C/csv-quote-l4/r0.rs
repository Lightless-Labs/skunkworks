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
            DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                header.iter().position(|s| s == name)
            }
        }
    }
}

fn validate_dialect(delimiter: u8, quote: u8) -> Result<(), ConfigPart> {
    if !(0x21..=0x7e).contains(&delimiter) {
        return Err(ConfigPart::Delimiter);
    }
    if !(0x21..=0x7e).contains(&quote) || delimiter == quote {
        return Err(ConfigPart::Quote);
    }
    Ok(())
}

fn parse_dialect(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    if i == bytes.len() {
        return Ok(Vec::new());
    }

    // State is represented by whether a field has started, and whether its
    // opening quote has closed. This keeps record and field boundaries explicit.
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut field_started = false;
    let mut quoted = false;
    let mut closed_quote = false;
    let mut opening_quote = 0;
    let mut record_index = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul { record: record_index, byte: i });
        }
        if !quoted && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr { record: record_index, byte: i });
        }

        if quoted {
            if b == quote {
                if bytes.get(i + 1) == Some(&quote) {
                    field.push(quote as char);
                    i += 2;
                } else {
                    quoted = false;
                    closed_quote = true;
                    i += 1;
                }
            } else {
                let ch = input[i..].chars().next().unwrap();
                field.push(ch);
                i += ch.len_utf8();
            }
            continue;
        }

        if closed_quote {
            if b == delimiter {
                record.push(std::mem::take(&mut field));
                field_started = false;
                closed_quote = false;
                i += 1;
            } else if b == b'\n' || b == b'\r' {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                field_started = false;
                closed_quote = false;
                record_index += 1;
                i += if b == b'\r' { 2 } else { 1 };
            } else {
                return Err(CsvError::TrailingByte { record: record_index, byte: i });
            }
            continue;
        }

        if b == delimiter {
            record.push(std::mem::take(&mut field));
            field_started = false;
            i += 1;
        } else if b == b'\n' || b == b'\r' {
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            field_started = false;
            record_index += 1;
            i += if b == b'\r' { 2 } else { 1 };
        } else if b == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote { record: record_index, byte: i });
            }
            quoted = true;
            field_started = true;
            opening_quote = i;
            i += 1;
        } else {
            let ch = input[i..].chars().next().unwrap();
            field.push(ch);
            field_started = true;
            i += ch.len_utf8();
        }
    }

    if quoted {
        return Err(CsvError::UnclosedQuote { record: record_index, byte: opening_quote });
    }
    if !record.is_empty() || field_started || closed_quote || !field.is_empty() {
        record.push(field);
        records.push(record);
    }
    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_dialect(input, b',', b'"')
}

fn duplicate_header(header: &[String]) -> Option<(String, usize, usize)> {
    let mut seen = std::collections::HashMap::<&str, usize>::new();
    for (second, name) in header.iter().enumerate() {
        if let Some(&first) = seen.get(name.as_str()) {
            return Some((name.clone(), first, second));
        }
        seen.insert(name, second);
    }
    None
}

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    if !config.headers {
        return Ok(Table { header: None, rows: records, duplicates: config.duplicates });
    }
    if records.is_empty() {
        return Ok(Table { header: None, rows: Vec::new(), duplicates: config.duplicates });
    }

    let mut records = records.into_iter();
    let header = records.next().unwrap();
    let rows: Vec<Vec<String>> = records.collect();

    if config.duplicates == DuplicatePolicy::Reject {
        if let Some((name, first, second)) = duplicate_header(&header) {
            return Err(CsvError::DuplicateHeader { name, first, second });
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
    Ok(Table { header: Some(header), rows, duplicates: config.duplicates })
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
        Ok(Self { config, input: String::new(), finished: None })
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
            .and_then(|records| build_table(records, self.config));
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
    if b.is_empty() { return false; }
    let start = usize::from(b[0] == b'-');
    if start == b.len() { return false; }
    if b[start] == b'0' {
        return start == 0 && b.len() == 1;
    }
    (b'1'..=b'9').contains(&b[start]) && b[start + 1..].iter().all(u8::is_ascii_digit)
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() { return false; }
    let negative = b[0] == b'-';
    let start = usize::from(negative);
    if start >= b.len() { return false; }
    let Some(relative_dot) = b[start..].iter().position(|&x| x == b'.') else {
        return false;
    };
    let dot = start + relative_dot;
    if b.len() != dot + 3 || !b[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    let whole = &b[start..dot];
    if whole.is_empty() { return false; }
    let valid_whole = if whole[0] == b'0' {
        whole.len() == 1
    } else {
        (b'1'..=b'9').contains(&whole[0])
            && whole[1..].iter().all(u8::is_ascii_digit)
    };
    valid_whole && !(negative && whole == b"0" && &b[dot + 1..] == b"00")
}

fn eval_cell(cell: &str, spec: &ColumnSpec) -> Result<Value, CellErrorKind> {
    if cell.is_empty() {
        if spec.nullable { return Ok(Value::Null); }
        if spec.kind == ColumnType::Text { return Ok(Value::Text(String::new())); }
        return Err(CellErrorKind::Empty);
    }

    match spec.kind {
        ColumnType::Text => Ok(Value::Text(cell.to_string())),
        ColumnType::Integer => {
            if !integer_syntax(cell) { return Err(CellErrorKind::InvalidInteger); }
            cell.parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| CellErrorKind::IntegerOverflow)
        }
        ColumnType::Decimal => {
            if !decimal_syntax(cell) { return Err(CellErrorKind::InvalidDecimal); }
            let negative = cell.starts_with('-');
            let unsigned = if negative { &cell[1..] } else { cell };
            let mut parts = unsigned.split('.');
            let whole: i128 = parts.next().unwrap().parse().unwrap();
            let fraction: i128 = parts.next().unwrap().parse().unwrap();
            let magnitude = whole * 100 + fraction;
            let signed = if negative { -magnitude } else { magnitude };
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

pub fn extract_columns(table: &Table, columns: &[ColumnSpec], policy: ErrorPolicy) -> TypedResult {
    let mut result = TypedResult { rows: Vec::new(), errors: Vec::new() };
    for (row_index, row) in table.rows.iter().enumerate() {
        let mut output = vec![None; columns.len()];
        let mut row_failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            let column = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };
            let (resolved_column, value) = match column {
                None => (None, Err(CellErrorKind::UnknownColumn)),
                Some(index) => match row.get(index) {
                    None => (Some(index), Err(CellErrorKind::MissingCell)),
                    Some(cell) => (Some(index), eval_cell(cell, spec)),
                },
            };

            match value {
                Ok(value) => output[spec_index] = Some(value),
                Err(kind) => {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: resolved_column,
                        kind,
                    });
                    row_failed = true;
                    if policy != ErrorPolicy::Collect { break; }
                }
            }
        }

        result.rows.push(output);
        if row_failed && policy == ErrorPolicy::Stop { break; }
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

fn record_nul(record: &[String], record_index: usize) -> Option<WriteError> {
    for (column, field) in record.iter().enumerate() {
        if let Some(byte) = field.as_bytes().iter().position(|&b| b == 0) {
            return Some(WriteError::Nul { record: record_index, column, byte });
        }
    }
    None
}

fn compare_rows(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (left, right) in a.iter().zip(b) {
        match left.as_bytes().cmp(right.as_bytes()) {
            std::cmp::Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    a.len().cmp(&b.len())
}

fn encode_field(
    field: &str,
    delimiter: u8,
    quote: u8,
    first_output_field: bool,
    sole_empty: bool,
) -> String {
    let bytes = field.as_bytes();
    let needs_quote = bytes.contains(&delimiter)
        || bytes.contains(&quote)
        || bytes.contains(&b'\r')
        || bytes.contains(&b'\n')
        || bytes.first().is_some_and(|b| *b == b' ' || *b == b'\t')
        || bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t')
        || (first_output_field && field.starts_with('\u{feff}'))
        || (sole_empty && field.is_empty());

    if !needs_quote { return field.to_string(); }

    let mut encoded = String::new();
    encoded.push(quote as char);
    for ch in field.chars() {
        if ch as u32 == quote as u32 {
            encoded.push(quote as char);
            encoded.push(quote as char);
        } else {
            encoded.push(ch);
        }
    }
    encoded.push(quote as char);
    encoded
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let mut originals: Vec<&[String]> = Vec::new();
    if let Some(header) = &table.header { originals.push(header); }
    originals.extend(table.rows.iter().map(Vec::as_slice));

    for (record, cells) in originals.iter().enumerate() {
        if let Some(error) = record_nul(cells, record) { return Err(error); }
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

    for (record, cells) in originals.iter().enumerate() {
        if cells.is_empty() { return Err(WriteError::ZeroColumns { record }); }
    }

    let mut output_records: Vec<Vec<String>> = Vec::new();
    match config.mode {
        WriteMode::Preserve => {
            if let Some(header) = &table.header { output_records.push(header.clone()); }
            output_records.extend(table.rows.iter().cloned());
        }
        WriteMode::Canonical => {
            if let Some(header) = &table.header {
                if table.duplicates == DuplicatePolicy::Reject {
                    if let Some((name, first, second)) = duplicate_header(header) {
                        return Err(WriteError::DuplicateHeader { name, first, second });
                    }
                }

                let mut selected = std::collections::HashMap::<String, usize>::new();
                for (index, name) in header.iter().enumerate() {
                    match table.duplicates {
                        DuplicatePolicy::KeepLast => { selected.insert(name.clone(), index); }
                        DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                            selected.entry(name.clone()).or_insert(index);
                        }
                    }
                }
                let mut columns: Vec<(String, usize)> = selected.into_iter().collect();
                columns.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

                let canonical_header: Vec<String> =
                    columns.iter().map(|(name, _)| name.clone()).collect();
                let mut rows: Vec<Vec<String>> = table.rows.iter().map(|row| {
                    columns.iter().map(|(_, index)| row[*index].clone()).collect()
                }).collect();
                rows.sort_by(|a, b| compare_rows(a, b));

                output_records.push(canonical_header);
                output_records.extend(rows);
            } else {
                let mut rows = table.rows.clone();
                rows.sort_by(|a, b| compare_rows(a, b));
                output_records = rows;
            }
        }
    }

    if output_records.is_empty() { return Ok(String::new()); }
    let canonical = config.mode == WriteMode::Canonical;
    let ending = if canonical {
        "\n"
    } else {
        match config.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    };

    let mut output = String::new();
    for (record_index, record) in output_records.iter().enumerate() {
        if record_index > 0 { output.push_str(ending); }
        for (column_index, field) in record.iter().enumerate() {
            if column_index > 0 { output.push(config.delimiter as char); }
            output.push_str(&encode_field(
                field,
                config.delimiter,
                config.quote,
                record_index == 0 && column_index == 0,
                record.len() == 1 && field.is_empty(),
            ));
        }
    }
    if canonical || config.final_terminator { output.push_str(ending); }
    Ok(output)
}

fn main() {}