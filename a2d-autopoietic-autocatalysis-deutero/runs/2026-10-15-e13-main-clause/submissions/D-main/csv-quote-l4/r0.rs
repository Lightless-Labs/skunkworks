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
    cached: Option<Result<Table, CsvError>>,
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

fn parse_with_dialect(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut record_index = 0usize;
    let mut at_field_start = true;
    let mut in_quotes = false;
    let mut closed_quote = false;
    let mut quote_start = 0usize;
    let mut any_record_content = false;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if !in_quotes && b == b'\r' {
            if i + 1 >= bytes.len() || bytes[i + 1] != b'\n' {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: i,
                });
            }
            fields.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut fields));
            i += 2;
            record_index += 1;
            at_field_start = true;
            closed_quote = false;
            any_record_content = false;
            continue;
        }

        if in_quotes {
            if b == quote {
                if i + 1 < bytes.len() && bytes[i + 1] == quote {
                    field.push(quote as char);
                    i += 2;
                } else {
                    in_quotes = false;
                    closed_quote = true;
                    i += 1;
                }
            } else {
                let start = i;
                while i < bytes.len() && bytes[i] != quote && bytes[i] != 0 {
                    i += 1;
                }
                field.push_str(std::str::from_utf8(&bytes[start..i]).unwrap());
            }
            continue;
        }

        if closed_quote {
            if b == delimiter {
                fields.push(std::mem::take(&mut field));
                at_field_start = true;
                closed_quote = false;
                any_record_content = true;
                i += 1;
                continue;
            }
            if b == b'\n' {
                fields.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut fields));
                i += 1;
                record_index += 1;
                at_field_start = true;
                closed_quote = false;
                any_record_content = false;
                continue;
            }
            return Err(CsvError::TrailingByte {
                record: record_index,
                byte: i,
            });
        }

        if b == delimiter {
            fields.push(std::mem::take(&mut field));
            at_field_start = true;
            any_record_content = true;
            i += 1;
        } else if b == b'\n' {
            fields.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut fields));
            i += 1;
            record_index += 1;
            at_field_start = true;
            any_record_content = false;
        } else if b == quote {
            if !at_field_start {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: i,
                });
            }
            in_quotes = true;
            quote_start = i;
            at_field_start = false;
            any_record_content = true;
            i += 1;
        } else {
            let start = i;
            while i < bytes.len()
                && bytes[i] != delimiter
                && bytes[i] != quote
                && bytes[i] != b'\n'
                && bytes[i] != b'\r'
                && bytes[i] != 0
            {
                i += 1;
            }
            field.push_str(std::str::from_utf8(&bytes[start..i]).unwrap());
            at_field_start = false;
            any_record_content = true;
        }
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_start,
        });
    }

    if any_record_content || !fields.is_empty() || !field.is_empty() || closed_quote {
        fields.push(field);
        records.push(fields);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn make_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let (header, rows, header_record) = if config.headers {
        if records.is_empty() {
            (None, Vec::new(), None)
        } else {
            let mut iter = records.into_iter();
            (iter.next(), iter.collect::<Vec<_>>(), Some(0usize))
        }
    } else {
        (None, records, None)
    };

    if let Some(header) = &header {
        if config.duplicates == DuplicatePolicy::Reject {
            for second in 0..header.len() {
                if let Some(first) = header[..second].iter().position(|v| v == &header[second]) {
                    return Err(CsvError::DuplicateHeader {
                        name: header[second].clone(),
                        first,
                        second,
                    });
                }
            }
        }
        for (row_index, row) in rows.iter().enumerate() {
            if row.len() != header.len() {
                return Err(CsvError::HeaderWidth {
                    record: row_index + 1,
                    expected: header.len(),
                    actual: row.len(),
                });
            }
        }
    }

    let _ = header_record;
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
            .and_then(|records| make_table(records, self.config));
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
        return start + 1 == b.len() && start == 0;
    }
    if !(b'1'..=b'9').contains(&b[start]) {
        return false;
    }
    b[start + 1..].iter().all(u8::is_ascii_digit)
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    let start = usize::from(b.first() == Some(&b'-'));
    if b.len() < start + 4 {
        return false;
    }
    let dot = b.len() - 3;
    if b[dot] != b'.' || !b[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    let int = &b[start..dot];
    if int == b"0" {
        return start == 0;
    }
    !int.is_empty()
        && (b'1'..=b'9').contains(&int[0])
        && int[1..].iter().all(u8::is_ascii_digit)
}

fn eval_cell(
    table: &Table,
    row_index: usize,
    row: &[String],
    spec_index: usize,
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

    let invalid = |kind| CellError {
        row: row_index,
        spec: spec_index,
        column: Some(column),
        kind,
    };

    match spec.kind {
        ColumnType::Text => Ok(Value::Text(cell.clone())),
        ColumnType::Integer => {
            if !integer_syntax(cell) {
                return Err(invalid(CellErrorKind::InvalidInteger));
            }
            cell.parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| invalid(CellErrorKind::IntegerOverflow))
        }
        ColumnType::Decimal => {
            if !decimal_syntax(cell) {
                return Err(invalid(CellErrorKind::InvalidDecimal));
            }
            let negative = cell.starts_with('-');
            let body = if negative { &cell[1..] } else { cell.as_str() };
            let dot = body.len() - 3;
            let whole = body[..dot].parse::<i128>().unwrap();
            let frac = body[dot + 1..].parse::<i128>().unwrap();
            let magnitude = whole * 100 + frac;
            let signed = if negative { -magnitude } else { magnitude };
            i64::try_from(signed)
                .map(Value::Decimal)
                .map_err(|_| invalid(CellErrorKind::DecimalOverflow))
        }
        ColumnType::Boolean => match cell.as_str() {
            "yes" | "1" => Ok(Value::Boolean(true)),
            "no" | "0" => Ok(Value::Boolean(false)),
            _ => Err(invalid(CellErrorKind::InvalidBoolean)),
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
    'rows: for (row_index, row) in table.rows.iter().enumerate() {
        let mut output = vec![None; columns.len()];
        for (spec_index, spec) in columns.iter().enumerate() {
            match eval_cell(table, row_index, row, spec_index, spec) {
                Ok(value) => output[spec_index] = Some(value),
                Err(error) => {
                    result.errors.push(error);
                    match policy {
                        ErrorPolicy::Collect => {}
                        ErrorPolicy::FirstPerRow => break,
                        ErrorPolicy::Stop => {
                            result.rows.push(output);
                            break 'rows;
                        }
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
    ZeroColumns {
        record: usize,
    },
    DuplicateHeader {
        name: String,
        first: usize,
        second: usize,
    },
}

fn quote_field(s: &str, delimiter: u8, quote: u8, first: bool, sole: bool) -> String {
    let bytes = s.as_bytes();
    let must_quote = bytes.contains(&delimiter)
        || bytes.contains(&quote)
        || bytes.contains(&b'\r')
        || bytes.contains(&b'\n')
        || bytes.first().is_some_and(|b| *b == b' ' || *b == b'\t')
        || bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t')
        || (first && s.starts_with('\u{feff}'))
        || (sole && s.is_empty());
    if !must_quote {
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

    let mut original_records: Vec<&[String]> = Vec::new();
    if let Some(header) = &table.header {
        original_records.push(header);
    }
    original_records.extend(table.rows.iter().map(Vec::as_slice));

    let mut record_base = 0usize;
    for (record_index, record) in original_records.iter().enumerate() {
        for (column, cell) in record.iter().enumerate() {
            if let Some(byte) = cell.as_bytes().iter().position(|b| *b == 0) {
                return Err(WriteError::Nul {
                    record: record_index,
                    column,
                    byte,
                });
            }
        }
        record_base += 1;
    }
    let _ = record_base;

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

    if config.mode == WriteMode::Canonical
        && table.header.is_some()
        && table.duplicates == DuplicatePolicy::Reject
    {
        let header = table.header.as_ref().unwrap();
        for second in 0..header.len() {
            if let Some(first) = header[..second].iter().position(|v| v == &header[second]) {
                return Err(WriteError::DuplicateHeader {
                    name: header[second].clone(),
                    first,
                    second,
                });
            }
        }
    }

    let mut output_records: Vec<Vec<String>> = match config.mode {
        WriteMode::Preserve => original_records.iter().map(|r| r.to_vec()).collect(),
        WriteMode::Canonical => {
            if let Some(header) = &table.header {
                let mut selected = Vec::new();
                for (index, name) in header.iter().enumerate() {
                    let prior = header[..index].iter().position(|v| v == name);
                    let later = header[index + 1..].iter().position(|v| v == name);
                    let keep = match table.duplicates {
                        DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => prior.is_none(),
                        DuplicatePolicy::KeepLast => later.is_none(),
                    };
                    if keep {
                        selected.push((name.clone(), index));
                    }
                }
                selected.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
                let new_header = selected.iter().map(|(name, _)| name.clone()).collect();
                let mut rows: Vec<Vec<String>> = table
                    .rows
                    .iter()
                    .map(|row| selected.iter().map(|(_, index)| row[*index].clone()).collect())
                    .collect();
                rows.sort_by(|a, b| compare_rows(a, b));
                let mut all = vec![new_header];
                all.extend(rows);
                all
            } else {
                let mut rows = table.rows.clone();
                rows.sort_by(|a, b| compare_rows(a, b));
                rows
            }
        }
    };

    if output_records.is_empty() {
        return Ok(String::new());
    }

    let canonical = config.mode == WriteMode::Canonical;
    let separator = if canonical || config.line_ending == LineEnding::Lf {
        "\n"
    } else {
        "\r\n"
    };
    let final_terminator = canonical || config.final_terminator;
    let mut out = String::new();
    for (ri, record) in output_records.iter().enumerate() {
        if ri > 0 {
            out.push_str(separator);
        }
        for (ci, cell) in record.iter().enumerate() {
            if ci > 0 {
                out.push(config.delimiter as char);
            }
            out.push_str(&quote_field(
                cell,
                config.delimiter,
                config.quote,
                ri == 0 && ci == 0,
                record.len() == 1,
            ));
        }
    }
    if final_terminator {
        out.push_str(separator);
    }
    output_records.clear();
    Ok(out)
}

fn compare_rows(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (left, right) in a.iter().zip(b.iter()) {
        let ordering = left.as_bytes().cmp(right.as_bytes());
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }
    a.len().cmp(&b.len())
}

fn main() {}