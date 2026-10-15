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
            DuplicatePolicy::KeepLast => header.iter().rposition(|n| n == name),
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|n| n == name)
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
    let mut start = 0;
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        start = 3;
    }
    if start == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut i = start;
    let mut record_index = 0usize;
    let mut field_started = false;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut opening_quote = 0usize;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if !in_quotes && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
        }

        if in_quotes {
            if b == quote {
                if bytes.get(i + 1) == Some(&quote) {
                    field.push(quote as char);
                    i += 2;
                    continue;
                }
                in_quotes = false;
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
                record.push(std::mem::take(&mut field));
                field_started = false;
                after_quote = false;
                i += 1;
                continue;
            }
            if b == b'\n' || b == b'\r' {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                record_index += 1;
                field_started = false;
                after_quote = false;
                i += if b == b'\r' { 2 } else { 1 };
                continue;
            }
            return Err(CsvError::TrailingByte {
                record: record_index,
                byte: i,
            });
        }

        if b == delimiter {
            record.push(std::mem::take(&mut field));
            field_started = false;
            i += 1;
            continue;
        }

        if b == b'\n' || b == b'\r' {
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            record_index += 1;
            field_started = false;
            i += if b == b'\r' { 2 } else { 1 };
            continue;
        }

        if b == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: i,
                });
            }
            field_started = true;
            in_quotes = true;
            opening_quote = i;
            i += 1;
            continue;
        }

        let ch = input[i..].chars().next().unwrap();
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

fn duplicate_header(header: &[String]) -> Option<CsvError> {
    for second in 0..header.len() {
        if let Some(first) = header[..second].iter().position(|n| n == &header[second]) {
            return Some(CsvError::DuplicateHeader {
                name: header[second].clone(),
                first,
                second,
            });
        }
    }
    None
}

fn table_from_records(
    records: Vec<Vec<String>>,
    config: Config,
) -> Result<Table, CsvError> {
    let mut records = records.into_iter();
    let header = if config.headers {
        records.next()
    } else {
        None
    };

    if let Some(ref h) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            if let Some(error) = duplicate_header(h) {
                return Err(error);
            }
        }
    }

    let rows: Vec<Vec<String>> = records.collect();
    if let Some(ref h) = header {
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

fn valid_integer_syntax(s: &str) -> bool {
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
    (b'1'..=b'9').contains(&b[start])
        && b[start + 1..].iter().all(u8::is_ascii_digit)
}

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    if !valid_integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>()
        .map_err(|_| CellErrorKind::IntegerOverflow)
}

fn valid_decimal_syntax(s: &str) -> bool {
    let Some((whole, fraction)) = s.split_once('.') else {
        return false;
    };
    if fraction.len() != 2 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let whole_bytes = whole.as_bytes();
    if whole_bytes.is_empty() {
        return false;
    }
    let start = usize::from(whole_bytes[0] == b'-');
    if start == whole_bytes.len() {
        return false;
    }
    if whole_bytes[start] == b'0' {
        if start + 1 != whole_bytes.len() {
            return false;
        }
        if start == 1 && fraction == "00" {
            return false;
        }
        return true;
    }
    (b'1'..=b'9').contains(&whole_bytes[start])
        && whole_bytes[start + 1..].iter().all(u8::is_ascii_digit)
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !valid_decimal_syntax(s) {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let negative = s.starts_with('-');
    let body = if negative { &s[1..] } else { s };
    let (whole, frac) = body.split_once('.').unwrap();
    let whole_value = whole.parse::<i128>().map_err(|_| CellErrorKind::DecimalOverflow)?;
    let frac_value = frac.parse::<i128>().unwrap();
    let magnitude = whole_value
        .checked_mul(100)
        .and_then(|v| v.checked_add(frac_value))
        .ok_or(CellErrorKind::DecimalOverflow)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn convert_cell(s: &str, kind: ColumnType) -> Result<Value, CellErrorKind> {
    match kind {
        ColumnType::Text => Ok(Value::Text(s.to_string())),
        ColumnType::Integer => parse_integer(s).map(Value::Integer),
        ColumnType::Decimal => parse_decimal(s).map(Value::Decimal),
        ColumnType::Boolean => match s {
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
    let mut rows = Vec::new();
    let mut errors = Vec::new();

    for (row_index, row) in table.rows.iter().enumerate() {
        let mut output = Vec::with_capacity(columns.len());
        let mut row_failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if row_failed {
                output.push(None);
                continue;
            }

            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let (column, cell) = match resolved {
                None => {
                    let error = CellError {
                        row: row_index,
                        spec: spec_index,
                        column: None,
                        kind: CellErrorKind::UnknownColumn,
                    };
                    errors.push(error);
                    output.push(None);
                    row_failed = policy != ErrorPolicy::Collect;
                    if policy == ErrorPolicy::Stop {
                        rows.push(output);
                        return TypedResult { rows, errors };
                    }
                    continue;
                }
                Some(index) => match row.get(index) {
                    None => {
                        let error = CellError {
                            row: row_index,
                            spec: spec_index,
                            column: Some(index),
                            kind: CellErrorKind::MissingCell,
                        };
                        errors.push(error);
                        output.push(None);
                        row_failed = policy != ErrorPolicy::Collect;
                        if policy == ErrorPolicy::Stop {
                            rows.push(output);
                            return TypedResult { rows, errors };
                        }
                        continue;
                    }
                    Some(cell) => (index, cell),
                },
            };

            let result = if cell.is_empty() && spec.nullable {
                Ok(Value::Null)
            } else if cell.is_empty() && spec.kind != ColumnType::Text {
                Err(CellErrorKind::Empty)
            } else {
                convert_cell(cell, spec.kind)
            };

            match result {
                Ok(value) => output.push(Some(value)),
                Err(kind) => {
                    errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column),
                        kind,
                    });
                    output.push(None);
                    row_failed = policy != ErrorPolicy::Collect;
                    if policy == ErrorPolicy::Stop {
                        rows.push(output);
                        return TypedResult { rows, errors };
                    }
                }
            }
        }

        rows.push(output);
    }

    TypedResult { rows, errors }
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

fn preflight_write(table: &Table) -> Result<(), WriteError> {
    let mut record = 0usize;
    if let Some(header) = &table.header {
        for (column, field) in header.iter().enumerate() {
            if let Some(byte) = field.as_bytes().iter().position(|b| *b == 0) {
                return Err(WriteError::Nul { record, column, byte });
            }
        }
        record = 1;
    }

    for row in &table.rows {
        for (column, field) in row.iter().enumerate() {
            if let Some(byte) = field.as_bytes().iter().position(|b| *b == 0) {
                return Err(WriteError::Nul { record, column, byte });
            }
        }
        record += 1;
    }

    if let Some(header) = &table.header {
        for (idx, row) in table.rows.iter().enumerate() {
            if row.len() != header.len() {
                return Err(WriteError::Width {
                    record: idx + 1,
                    expected: header.len(),
                    actual: row.len(),
                });
            }
        }
    }

    if let Some(header) = &table.header {
        if header.is_empty() {
            return Err(WriteError::ZeroColumns { record: 0 });
        }
    }
    let row_start = usize::from(table.header.is_some());
    for (idx, row) in table.rows.iter().enumerate() {
        if row.is_empty() {
            return Err(WriteError::ZeroColumns {
                record: row_start + idx,
            });
        }
    }
    Ok(())
}

fn quote_field(field: &str, first_field_first_record: bool, sole_empty: bool, cfg: WriteConfig) -> String {
    let needs_quote = field.as_bytes().contains(&cfg.delimiter)
        || field.as_bytes().contains(&cfg.quote)
        || field.contains('\r')
        || field.contains('\n')
        || field.starts_with(' ')
        || field.starts_with('\t')
        || field.ends_with(' ')
        || field.ends_with('\t')
        || (first_field_first_record && field.starts_with('\u{feff}'))
        || (sole_empty && field.is_empty());

    if !needs_quote {
        return field.to_string();
    }

    let mut out = String::new();
    out.push(cfg.quote as char);
    for ch in field.chars() {
        if ch as u32 == cfg.quote as u32 {
            out.push(cfg.quote as char);
            out.push(cfg.quote as char);
        } else {
            out.push(ch);
        }
    }
    out.push(cfg.quote as char);
    out
}

fn cmp_rows(a: &Vec<String>, b: &Vec<String>) -> std::cmp::Ordering {
    for (left, right) in a.iter().zip(b.iter()) {
        let order = left.as_bytes().cmp(right.as_bytes());
        if order != std::cmp::Ordering::Equal {
            return order;
        }
    }
    a.len().cmp(&b.len())
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;
    preflight_write(table)?;

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
                if table.duplicates == DuplicatePolicy::Reject {
                    if let Some(CsvError::DuplicateHeader { name, first, second }) =
                        duplicate_header(header)
                    {
                        return Err(WriteError::DuplicateHeader { name, first, second });
                    }
                }
                let mut retained = Vec::new();
                for (idx, name) in header.iter().enumerate() {
                    let chosen = match table.duplicates {
                        DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                            header[..=idx].iter().position(|v| v == name) == Some(idx)
                        }
                        DuplicatePolicy::KeepLast => {
                            header[idx..].iter().position(|v| v == name) == Some(0)
                        }
                    };
                    if chosen {
                        retained.push((name.clone(), idx));
                    }
                }
                retained.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
                let projected_header: Vec<String> =
                    retained.iter().map(|(name, _)| name.clone()).collect();
                let mut projected_rows: Vec<Vec<String>> = table
                    .rows
                    .iter()
                    .map(|row| retained.iter().map(|(_, idx)| row[*idx].clone()).collect())
                    .collect();
                projected_rows.sort_by(cmp_rows);
                records.push(projected_header);
                records.extend(projected_rows);
            } else {
                let mut rows = table.rows.clone();
                rows.sort_by(cmp_rows);
                records = rows;
            }
        }
    }

    let canonical = config.mode == WriteMode::Canonical;
    let mut output = String::new();
    let line_ending = if canonical {
        "\n"
    } else {
        match config.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    };

    for (record_index, record) in records.iter().enumerate() {
        if record_index > 0 {
            output.push_str(line_ending);
        }
        let sole_empty = record.len() == 1 && record[0].is_empty();
        for (column, field) in record.iter().enumerate() {
            if column > 0 {
                output.push(config.delimiter as char);
            }
            output.push_str(&quote_field(
                field,
                record_index == 0 && column == 0,
                sole_empty,
                config,
            ));
        }
    }

    if canonical {
        if !records.is_empty() {
            output.push('\n');
        }
    } else if config.final_terminator && !records.is_empty() {
        output.push_str(line_ending);
    }

    Ok(output)
}