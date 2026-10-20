use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Vec<String>,
    pub build: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VersionError {
    Empty,
    MissingCore { at: usize },
    LeadingZero { at: usize },
    Overflow { at: usize },
    EmptyIdentifier { at: usize },
    InvalidChar { at: usize },
}

fn is_core_terminator(b: Option<u8>) -> bool {
    matches!(b, None | Some(b'.' | b'-' | b'+'))
}

fn parse_core_field(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }

    if *pos == start {
        return Err(if is_core_terminator(bytes.get(*pos).copied()) {
            VersionError::MissingCore { at: start }
        } else {
            VersionError::InvalidChar { at: start }
        });
    }

    if *pos - start > 1 && bytes[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }

    let mut value = 0u64;
    for &digit in &bytes[start..*pos] {
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add((digit - b'0') as u64))
            .ok_or(VersionError::Overflow { at: start })?;
    }
    Ok(value)
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    pre: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut ident_start = start;

    loop {
        let mut i = ident_start;
        while i < end && bytes[i] != b'.' {
            if !bytes[i].is_ascii_alphanumeric() && bytes[i] != b'-' {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }

        if i == ident_start {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }

        if pre {
            let ident = &bytes[ident_start..i];
            if ident.iter().all(u8::is_ascii_digit)
                && ident.len() > 1
                && ident[0] == b'0'
            {
                return Err(VersionError::LeadingZero { at: ident_start });
            }
        }

        result.push(s[ident_start..i].to_string());
        if i == end {
            break;
        }
        ident_start = i + 1;
    }

    Ok(result)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let bytes = s.as_bytes();
    let mut pos = 0;

    let major = parse_core_field(s, &mut pos)?;
    if bytes.get(pos) != Some(&b'.') {
        return Err(if is_core_terminator(bytes.get(pos).copied()) {
            VersionError::MissingCore { at: pos }
        } else {
            VersionError::InvalidChar { at: pos }
        });
    }
    pos += 1;

    let minor = parse_core_field(s, &mut pos)?;
    if bytes.get(pos) != Some(&b'.') {
        return Err(if is_core_terminator(bytes.get(pos).copied()) {
            VersionError::MissingCore { at: pos }
        } else {
            VersionError::InvalidChar { at: pos }
        });
    }
    pos += 1;

    let patch = parse_core_field(s, &mut pos)?;
    match bytes.get(pos).copied() {
        None | Some(b'-' | b'+') => {}
        Some(_) => return Err(VersionError::InvalidChar { at: pos }),
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if bytes.get(pos) == Some(&b'-') {
        pos += 1;
        let pre_start = pos;
        let pre_end = bytes[pos..]
            .iter()
            .position(|&b| b == b'+')
            .map(|i| pos + i)
            .unwrap_or(bytes.len());
        pre = parse_identifiers(s, pre_start, pre_end, true)?;
        pos = pre_end;
    }

    if bytes.get(pos) == Some(&b'+') {
        pos += 1;
        build = parse_identifiers(s, pos, bytes.len(), false)?;
    }

    Ok(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn cmp_numeric_identifiers(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

fn cmp_pre_identifier(a: &str, b: &str) -> Ordering {
    let an = numeric_identifier(a);
    let bn = numeric_identifier(b);
    match (an, bn) {
        (true, true) => cmp_numeric_identifiers(a, b),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch)
        .cmp(&(b.major, b.minor, b.patch))
        .then_with(|| match (a.pre.is_empty(), b.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => {
                for (x, y) in a.pre.iter().zip(&b.pre) {
                    let ord = cmp_pre_identifier(x, y);
                    if ord != Ordering::Equal {
                        return ord;
                    }
                }
                a.pre.len().cmp(&b.pre.len())
            }
        })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Debug)]
struct Comparator {
    op: Op,
    version: Version,
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug)]
struct Arm {
    comparators: Vec<Comparator>,
    admission_anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

#[derive(Clone, Debug)]
struct Token<'a> {
    text: &'a str,
    start: usize,
}

#[derive(Clone, Debug)]
struct Partial {
    numbers: Vec<u64>,
    field_count: usize,
    full: Option<Version>,
}

fn is_range_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokenize(s: &str) -> Vec<Token<'_>> {
    let bytes = s.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && is_range_space(bytes[i]) {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !is_range_space(bytes[i]) {
            i += 1;
        }
        tokens.push(Token {
            text: &s[start..i],
            start,
        });
    }
    tokens
}

fn version_error(at: usize, error: VersionError) -> RangeError {
    RangeError::Version { at, error }
}

fn parse_full_operand(text: &str, start: usize) -> Result<Version, RangeError> {
    parse_version(text).map_err(|e| version_error(start, e))
}

fn add_comparator(arm: &mut Arm, op: Op, version: Version, anchor: bool) {
    if anchor && !version.pre.is_empty() {
        arm.admission_anchors
            .push((version.major, version.minor, version.patch));
    }
    arm.comparators.push(Comparator { op, version });
}

fn syntax(at: usize) -> RangeError {
    RangeError::Syntax { at }
}

fn parse_partial(text: &str, start: usize, allow_full: bool) -> Result<Partial, RangeError> {
    let bytes = text.as_bytes();
    let has_sign = bytes.iter().any(|&b| b == b'-' || b == b'+');
    let fields: Vec<&str> = text.split('.').collect();
    let has_wildcard = fields
        .iter()
        .any(|f| matches!(*f, "*" | "x" | "X"));

    if allow_full && (has_sign || (fields.len() >= 3 && !has_wildcard)) {
        let full = parse_full_operand(text, start)?;
        return Ok(Partial {
            numbers: vec![full.major, full.minor, full.patch],
            field_count: 3,
            full: Some(full),
        });
    }

    if fields.is_empty() || fields.len() > 3 {
        return Err(syntax(start));
    }

    let mut numbers = Vec::new();
    let mut wildcard_seen = false;
    for field in &fields {
        if field.is_empty() {
            return Err(syntax(start));
        }
        if matches!(*field, "*" | "x" | "X") {
            wildcard_seen = true;
        } else {
            if wildcard_seen
                || !field.bytes().all(|b| b.is_ascii_digit())
                || (field.len() > 1 && field.as_bytes()[0] == b'0')
            {
                return Err(syntax(start));
            }
            let mut n = 0u64;
            for b in field.bytes() {
                n = n
                    .checked_mul(10)
                    .and_then(|v| v.checked_add((b - b'0') as u64))
                    .ok_or_else(|| syntax(start))?;
            }
            numbers.push(n);
        }
    }

    let numeric_count = numbers.len();
    let mut wildcard_started = false;
    for field in &fields {
        if matches!(*field, "*" | "x" | "X") {
            wildcard_started = true;
        } else if wildcard_started {
            return Err(syntax(start));
        }
    }

    Ok(Partial {
        numbers,
        field_count: numeric_count,
        full: None,
    })
}

fn padded_version(numbers: &[u64]) -> Version {
    Version {
        major: *numbers.first().unwrap_or(&0),
        minor: *numbers.get(1).unwrap_or(&0),
        patch: *numbers.get(2).unwrap_or(&0),
        pre: Vec::new(),
        build: Vec::new(),
    }
}

fn upper_version(major: u64, minor: u64, patch: u64) -> Version {
    Version {
        major,
        minor,
        patch,
        pre: vec!["0".to_string()],
        build: Vec::new(),
    }
}

fn checked_increment(value: u64, start: usize) -> Result<u64, RangeError> {
    value.checked_add(1).ok_or_else(|| syntax(start))
}

fn add_bare_partial(arm: &mut Arm, p: Partial, start: usize) -> Result<(), RangeError> {
    if let Some(full) = p.full {
        add_comparator(arm, Op::Eq, full, true);
        return Ok(());
    }

    let k = p.field_count;
    if k == 0 {
        return Ok(());
    }

    let lower = padded_version(&p.numbers);
    add_comparator(arm, Op::Ge, lower, false);

    let upper = match k {
        1 => upper_version(checked_increment(p.numbers[0], start)?, 0, 0),
        2 => upper_version(p.numbers[0], checked_increment(p.numbers[1], start)?, 0),
        _ => return Err(syntax(start)),
    };
    add_comparator(arm, Op::Lt, upper, false);
    Ok(())
}

fn add_caret_or_tilde(
    arm: &mut Arm,
    p: Partial,
    token_start: usize,
    operand_start: usize,
    caret: bool,
) -> Result<(), RangeError> {
    let k = p.field_count;
    if k == 0 {
        return Err(syntax(token_start));
    }

    let lower = p.full.clone().unwrap_or_else(|| padded_version(&p.numbers));
    let (major, minor, patch) = (lower.major, lower.minor, lower.patch);
    add_comparator(arm, Op::Ge, lower, true);

    let upper = if caret {
        if major > 0 || k == 1 {
            upper_version(checked_increment(major, operand_start)?, 0, 0)
        } else if minor > 0 || k == 2 {
            upper_version(major, checked_increment(minor, operand_start)?, 0)
        } else {
            upper_version(major, minor, checked_increment(patch, operand_start)?)
        }
    } else if k == 1 {
        upper_version(checked_increment(major, operand_start)?, 0, 0)
    } else {
        upper_version(major, checked_increment(minor, operand_start)?, 0)
    };
    add_comparator(arm, Op::Lt, upper, false);
    Ok(())
}

fn add_hyphen_range(
    arm: &mut Arm,
    lower: Partial,
    lower_start: usize,
    upper: Partial,
    upper_start: usize,
) -> Result<(), RangeError> {
    if lower.field_count == 0 {
        return Err(syntax(lower_start));
    }
    if upper.field_count == 0 {
        return Err(syntax(upper_start));
    }

    let low = lower
        .full
        .clone()
        .unwrap_or_else(|| padded_version(&lower.numbers));
    add_comparator(arm, Op::Ge, low, true);

    if let Some(full) = upper.full {
        add_comparator(arm, Op::Le, full, true);
    } else {
        let k = upper.field_count;
        let bound = match k {
            1 => upper_version(checked_increment(upper.numbers[0], upper_start)?, 0, 0),
            2 => upper_version(
                upper.numbers[0],
                checked_increment(upper.numbers[1], upper_start)?,
                0,
            ),
            3 => padded_version(&upper.numbers),
            _ => return Err(syntax(upper_start)),
        };
        if k == 3 {
            add_comparator(arm, Op::Le, bound, false);
        } else {
            add_comparator(arm, Op::Lt, bound, false);
        }
    }
    Ok(())
}

fn parse_comparator_token(arm: &mut Arm, token: &Token<'_>) -> Result<(), RangeError> {
    let (op, skip) = if token.text.starts_with(">=") {
        (Op::Ge, 2)
    } else if token.text.starts_with("<=") {
        (Op::Le, 2)
    } else if token.text.starts_with('>') {
        (Op::Gt, 1)
    } else if token.text.starts_with('<') {
        (Op::Lt, 1)
    } else if token.text.starts_with('=') {
        (Op::Eq, 1)
    } else {
        (Op::Eq, 0)
    };

    let operand = &token.text[skip..];
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(syntax(token.start));
    }
    let operand_start = token.start + skip;
    let version = parse_full_operand(operand, operand_start)?;
    add_comparator(arm, op, version, true);
    Ok(())
}

fn parse_non_hyphen_token(arm: &mut Arm, token: &Token<'_>) -> Result<(), RangeError> {
    let bytes = token.text.as_bytes();
    match bytes.first().copied() {
        Some(b'<' | b'>' | b'=') => parse_comparator_token(arm, token),
        Some(b'^' | b'~') => {
            let caret = bytes[0] == b'^';
            let operand = &token.text[1..];
            let operand_start = token.start + 1;
            let p = parse_partial(operand, operand_start, true)?;
            add_caret_or_tilde(arm, p, token.start, operand_start, caret)
        }
        Some(b'-') => Err(syntax(token.start)),
        Some(b) if b.is_ascii_digit() || matches!(b, b'*' | b'x' | b'X') => {
            let p = parse_partial(token.text, token.start, true)?;
            add_bare_partial(arm, p, token.start)
        }
        _ => Err(syntax(token.start)),
    }
}

fn parse_arm(tokens: &[Token<'_>]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        comparators: Vec::new(),
        admission_anchors: Vec::new(),
    };

    let standalone_minus = tokens.iter().position(|t| t.text == "-");
    if let Some(index) = standalone_minus {
        if tokens.len() != 3 || index != 1 {
            return Err(syntax(tokens[index].start));
        }

        let lower = parse_partial(tokens[0].text, tokens[0].start, true)?;
        let upper = parse_partial(tokens[2].text, tokens[2].start, true)?;
        return add_hyphen_range(
            &mut arm,
            lower,
            tokens[0].start,
            upper,
            tokens[2].start,
        )
        .map(|_| arm);
    }

    for token in tokens {
        parse_non_hyphen_token(&mut arm, token)?;
    }
    Ok(arm)
}

fn is_or_token(token: &Token<'_>) -> bool {
    token.text == "||"
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let tokens = tokenize(s);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms = Vec::new();
    let mut current = Vec::new();
    let mut last_or_start = None;

    for token in &tokens {
        if is_or_token(token) {
            if current.is_empty() {
                return Err(RangeError::Empty { at: token.start });
            }
            arms.push(parse_arm(&current)?);
            current.clear();
            last_or_start = Some(token.start);
        } else {
            current.push(token.clone());
            last_or_start = None;
        }
    }

    if current.is_empty() {
        return Err(RangeError::Empty {
            at: last_or_or_start(last_or_start, s.len()),
        });
    }
    arms.push(parse_arm(&current)?);
    Ok(Range { arms })
}

fn last_or_or_start(last_or_start: Option<usize>, input_len: usize) -> usize {
    if last_or_start.is_some() {
        input_len
    } else {
        input_len
    }
}

fn comparator_matches(v: &Version, c: &Comparator) -> bool {
    let ord = compare_versions(v, &c.version);
    match c.op {
        Op::Eq => ord == Ordering::Equal,
        Op::Lt => ord == Ordering::Less,
        Op::Le => ord != Ordering::Greater,
        Op::Gt => ord == Ordering::Greater,
        Op::Ge => ord != Ordering::Less,
    }
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        if !arm
            .comparators
            .iter()
            .all(|comparator| comparator_matches(v, comparator))
        {
            return false;
        }

        v.pre.is_empty()
            || arm
                .admission_anchors
                .iter()
                .any(|&(major, minor, patch)| {
                    (v.major, v.minor, v.patch) == (major, minor, patch)
                })
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn satisfying_extreme(
    candidates: &[String],
    r: &Range,
    greatest: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let version =
            parse_version(candidate).map_err(|error| CandidateError { index, error })?;
        parsed.push(version);
    }

    let mut best: Option<usize> = None;
    for (index, version) in parsed.iter().enumerate() {
        if !matches(version, r) {
            continue;
        }
        match best {
            None => best = Some(index),
            Some(best_index) => {
                let ord = compare_versions(version, &parsed[best_index]);
                if (greatest && ord == Ordering::Greater)
                    || (!greatest && ord == Ordering::Less)
                {
                    best = Some(index);
                }
            }
        }
    }

    Ok(best.map(|index| candidates[index].clone()))
}

pub fn max_satisfying(
    candidates: &[String],
    r: &Range,
) -> Result<Option<String>, CandidateError> {
    satisfying_extreme(candidates, r, true)
}

pub fn min_satisfying(
    candidates: &[String],
    r: &Range,
) -> Result<Option<String>, CandidateError> {
    satisfying_extreme(candidates, r, false)
}