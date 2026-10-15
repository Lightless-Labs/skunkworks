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

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_dialect(input, b',', b'"')
}

fn parse_dialect(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = Vec::new();
    // 0 = field start, 1 = unquoted, 2 = quoted, 3 = closed quote.
    let mut state = 0;
    let mut opening = 0;
    let mut active = false;
    while i < bytes.len() {
        let b = bytes[i];
        let record = records.len();
        if b == 0 {
            return Err(CsvError::Nul { record, byte: i });
        }
        if state == 2 {
            if b == quote {
                if i + 1 < bytes.len() && bytes[i + 1] == quote {
                    field.push(quote);
                    i += 2;
                } else {
                    state = 3;
                    i += 1;
                }
            } else {
                field.push(b);
                i += 1;
            }
            continue;
        }
        if b == b'\r' && (i + 1 == bytes.len() || bytes[i + 1] != b'\n') {
            return Err(CsvError::BareCr { record, byte: i });
        }
        if b == delimiter {
            row.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
            state = 0;
            active = true;
            i += 1;
        } else if b == b'\n' || b == b'\r' {
            row.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
            records.push(std::mem::take(&mut row));
            state = 0;
            active = false;
            i += if b == b'\r' { 2 } else { 1 };
        } else if state == 3 {
            return Err(CsvError::TrailingByte { record, byte: i });
        } else if b == quote {
            if state == 1 {
                return Err(CsvError::UnexpectedQuote { record, byte: i });
            }
            state = 2;
            opening = i;
            active = true;
            i += 1;
        } else {
            field.push(b);
            state = 1;
            active = true;
            i += 1;
        }
    }
    if state == 2 {
        return Err(CsvError::UnclosedQuote { record: records.len(), byte: opening });
    }
    if active {
        row.push(String::from_utf8(field).unwrap());
        records.push(row);
    }
    Ok(records)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigPart { Delimiter, Quote }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicatePolicy { Reject, KeepFirst, KeepLast }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub delimiter: u8, pub quote: u8,
    pub headers: bool, pub duplicates: DuplicatePolicy,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub header: Option<Vec<String>>, pub rows: Vec<Vec<String>>,
    pub duplicates: DuplicatePolicy,
}
impl Table {
    pub fn column_index(&self, name: &str) -> Option<usize> {
        let h = self.header.as_ref()?;
        match self.duplicates {
            DuplicatePolicy::KeepLast => h.iter().rposition(|s| s == name),
            _ => h.iter().position(|s| s == name),
        }
    }
}

fn validate_dialect(delimiter: u8, quote: u8) -> Result<(), ConfigPart> {
    if !(0x21..=0x7e).contains(&delimiter) { return Err(ConfigPart::Delimiter); }
    if !(0x21..=0x7e).contains(&quote) || delimiter == quote {
        return Err(ConfigPart::Quote);
    }
    Ok(())
}

fn duplicate(header: &[String]) -> Option<(String, usize, usize)> {
    let mut seen = std::collections::HashMap::new();
    for (second, name) in header.iter().enumerate() {
        if let Some(&first) = seen.get(name) { return Some((name.clone(), first, second)); }
        seen.insert(name, second);
    }
    None
}

pub struct StreamReader {
    config: Config,
    input: String,
    result: Option<Result<Table, CsvError>>,
}
impl StreamReader {
    pub fn new(config: Config) -> Result<Self, CsvError> {
        validate_dialect(config.delimiter, config.quote)
            .map_err(|component| CsvError::InvalidConfig { component })?;
        Ok(Self { config, input: String::new(), result: None })
    }
    pub fn feed(&mut self, chunk: &str) -> Result<(), CsvError> {
        if self.result.is_some() { return Err(CsvError::ReaderFinished); }
        self.input.push_str(chunk);
        Ok(())
    }
    pub fn finish(&mut self) -> Result<Table, CsvError> {
        if let Some(result) = &self.result { return result.clone(); }
        let result = self.complete();
        self.result = Some(result.clone());
        result
    }
    pub fn reset(&mut self) {
        self.input.clear();
        self.result = None;
    }
    fn complete(&self) -> Result<Table, CsvError> {
        let records = parse_dialect(&self.input, self.config.delimiter, self.config.quote)?;
        let mut iter = records.into_iter();
        let header = if self.config.headers { iter.next() } else { None };
        let rows: Vec<Vec<String>> = iter.collect();
        if let Some(h) = &header {
            if self.config.duplicates == DuplicatePolicy::Reject {
                if let Some((name, first, second)) = duplicate(h) {
                    return Err(CsvError::DuplicateHeader { name, first, second });
                }
            }
            for (r, row) in rows.iter().enumerate() {
                if row.len() != h.len() {
                    return Err(CsvError::HeaderWidth {
                        record: r + 1, expected: h.len(), actual: row.len(),
                    });
                }
            }
        }
        Ok(Table { header, rows, duplicates: self.config.duplicates })
    }
}
pub fn read_csv(input: &str, config: Config) -> Result<Table, CsvError> {
    let mut reader = StreamReader::new(config)?;
    reader.feed(input)?;
    reader.finish()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnRef { Index(usize), Name(String) }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType { Text, Integer, Decimal, Boolean }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnSpec {
    pub column: ColumnRef, pub kind: ColumnType, pub nullable: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value { Null, Text(String), Integer(i64), Decimal(i64), Boolean(bool) }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellErrorKind {
    UnknownColumn, MissingCell, Empty, InvalidInteger, IntegerOverflow,
    InvalidDecimal, DecimalOverflow, InvalidBoolean,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellError {
    pub row: usize, pub spec: usize, pub column: Option<usize>, pub kind: CellErrorKind,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorPolicy { Collect, FirstPerRow, Stop }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedResult {
    pub rows: Vec<Vec<Option<Value>>>, pub errors: Vec<CellError>,
}

fn canonical_digits(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit)
        && (bytes.len() == 1 || bytes[0] != b'0')
}
fn signed_magnitude(digits: impl Iterator<Item = u8>, negative: bool) -> Option<i64> {
    let limit = if negative { (i64::MAX as u64) + 1 } else { i64::MAX as u64 };
    let mut magnitude = 0u64;
    for digit in digits {
        magnitude = magnitude.checked_mul(10)?.checked_add((digit - b'0') as u64)?;
        if magnitude > limit { return None; }
    }
    if negative && magnitude == (i64::MAX as u64) + 1 { Some(i64::MIN) }
    else if negative { Some(-(magnitude as i64)) }
    else { Some(magnitude as i64) }
}
fn cell_value(text: &str, spec: &ColumnSpec) -> Result<Value, CellErrorKind> {
    use CellErrorKind::*;
    if text.is_empty() && spec.nullable { return Ok(Value::Null); }
    if spec.kind == ColumnType::Text { return Ok(Value::Text(text.to_owned())); }
    if text.is_empty() { return Err(Empty); }
    let negative = text.starts_with('-');
    let magnitude = if negative { &text.as_bytes()[1..] } else { text.as_bytes() };
    match spec.kind {
        ColumnType::Text => unreachable!(),
        ColumnType::Integer => {
            if !canonical_digits(magnitude) || (negative && magnitude == b"0") {
                return Err(InvalidInteger);
            }
            signed_magnitude(magnitude.iter().copied(), negative)
                .map(Value::Integer).ok_or(IntegerOverflow)
        }
        ColumnType::Decimal => {
            let Some(dot) = magnitude.iter().position(|&b| b == b'.') else {
                return Err(InvalidDecimal);
            };
            let integer = &magnitude[..dot];
            let fraction = &magnitude[dot+1..];
            if !canonical_digits(integer) || fraction.len() != 2
                || !fraction.iter().all(u8::is_ascii_digit)
                || (negative && integer == b"0" && fraction == b"00") {
                return Err(InvalidDecimal);
            }
            signed_magnitude(integer.iter().chain(fraction.iter()).copied(), negative)
                .map(Value::Decimal).ok_or(DecimalOverflow)
        }
        ColumnType::Boolean => match text {
            "yes" | "1" => Ok(Value::Boolean(true)),
            "no" | "0" => Ok(Value::Boolean(false)),
            _ => Err(InvalidBoolean),
        },
    }
}
pub fn extract_columns(table: &Table, columns: &[ColumnSpec], policy: ErrorPolicy) -> TypedResult {
    let indexes: Vec<Option<usize>> = columns.iter().map(|spec| match &spec.column {
        ColumnRef::Index(i) => Some(*i),
        ColumnRef::Name(name) => table.column_index(name),
    }).collect();
    let mut result = TypedResult { rows: Vec::new(), errors: Vec::new() };
    for (row_index, row) in table.rows.iter().enumerate() {
        let mut values = vec![None; columns.len()];
        let mut failed = false;
        for (spec_index, spec) in columns.iter().enumerate() {
            let index = indexes[spec_index];
            let value = match index {
                None => Err(CellErrorKind::UnknownColumn),
                Some(i) => match row.get(i) {
                    None => Err(CellErrorKind::MissingCell),
                    Some(text) => cell_value(text, spec),
                },
            };
            match value {
                Ok(value) => values[spec_index] = Some(value),
                Err(kind) => {
                    result.errors.push(CellError {
                        row: row_index, spec: spec_index, column: index, kind,
                    });
                    failed = true;
                    if policy != ErrorPolicy::Collect { break; }
                }
            }
        }
        result.rows.push(values);
        if failed && policy == ErrorPolicy::Stop { break; }
    }
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding { Lf, CrLf }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode { Preserve, Canonical }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteConfig {
    pub delimiter: u8, pub quote: u8, pub line_ending: LineEnding,
    pub final_terminator: bool, pub mode: WriteMode,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    InvalidConfig { component: ConfigPart },
    Nul { record: usize, column: usize, byte: usize },
    Width { record: usize, expected: usize, actual: usize },
    ZeroColumns { record: usize },
    DuplicateHeader { name: String, first: usize, second: usize },
}
pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;
    let original: Vec<&Vec<String>> = table.header.iter().chain(table.rows.iter()).collect();
    for (record, row) in original.iter().enumerate() {
        for (column, field) in row.iter().enumerate() {
            if let Some(byte) = field.as_bytes().iter().position(|&b| b == 0) {
                return Err(WriteError::Nul { record, column, byte });
            }
        }
    }
    if let Some(header) = &table.header {
        for (r, row) in table.rows.iter().enumerate() {
            if row.len() != header.len() {
                return Err(WriteError::Width { record: r + 1, expected: header.len(), actual: row.len() });
            }
        }
    }
    for (record, row) in original.iter().enumerate() {
        if row.is_empty() { return Err(WriteError::ZeroColumns { record }); }
    }
    let mut header = table.header.clone();
    let mut rows = table.rows.clone();
    if config.mode == WriteMode::Canonical {
        if let Some(h) = &table.header {
            if table.duplicates == DuplicatePolicy::Reject {
                if let Some((name, first, second)) = duplicate(h) {
                    return Err(WriteError::DuplicateHeader { name, first, second });
                }
            }
            let mut selected: std::collections::BTreeMap<&String, usize> = std::collections::BTreeMap::new();
            for (i, name) in h.iter().enumerate() {
                if table.duplicates == DuplicatePolicy::KeepLast {
                    selected.insert(name, i);
                } else {
                    selected.entry(name).or_insert(i);
                }
            }
            let indexes: Vec<usize> = selected.values().copied().collect();
            header = Some(indexes.iter().map(|&i| h[i].clone()).collect());
            rows = table.rows.iter().map(|row| indexes.iter().map(|&i| row[i].clone()).collect()).collect();
        }
        rows.sort();
    }
    let records: Vec<&Vec<String>> = header.iter().chain(rows.iter()).collect();
    let canonical = config.mode == WriteMode::Canonical;
    let ending = if canonical || config.line_ending == LineEnding::Lf { "\n" } else { "\r\n" };
    let mut output = Vec::new();
    for (r, row) in records.iter().enumerate() {
        if r > 0 { output.extend_from_slice(ending.as_bytes()); }
        for (c, field) in row.iter().enumerate() {
            if c > 0 { output.push(config.delimiter); }
            let raw = field.as_bytes();
            let quoted = raw.iter().any(|&b| b == config.delimiter || b == config.quote || b == b'\r' || b == b'\n')
                || matches!(raw.first(), Some(b' ' | b'\t'))
                || matches!(raw.last(), Some(b' ' | b'\t'))
                || (r == 0 && c == 0 && field.starts_with('\u{feff}'))
                || (row.len() == 1 && field.is_empty());
            if quoted { output.push(config.quote); }
            for &b in raw {
                output.push(b);
                if quoted && b == config.quote { output.push(b); }
            }
            if quoted { output.push(config.quote); }
        }
    }
    if !records.is_empty() && (canonical || config.final_terminator) {
        output.extend_from_slice(ending.as_bytes());
    }
    Ok(String::from_utf8(output).unwrap())
}
