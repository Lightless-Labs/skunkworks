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

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return Err(VersionError::Empty);
    }
    let mut pos = 0;
    let mut core = [0u64; 3];

    for (i, slot) in core.iter_mut().enumerate() {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos == start {
            return Err(if pos == bytes.len()
                || matches!(bytes[pos], b'.' | b'-' | b'+')
            {
                VersionError::MissingCore { at: pos }
            } else {
                VersionError::InvalidChar { at: pos }
            });
        }
        let digits = &bytes[start..pos];
        if digits.len() > 1 && digits[0] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }
        let mut n = 0u64;
        for &d in digits {
            n = match n.checked_mul(10).and_then(|v| v.checked_add((d - b'0') as u64)) {
                Some(v) => v,
                None => return Err(VersionError::Overflow { at: start }),
            };
        }
        *slot = n;

        if i < 2 {
            if pos == bytes.len() || matches!(bytes[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            if bytes[pos] != b'.' {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
        } else if pos < bytes.len() && !matches!(bytes[pos], b'-' | b'+') {
            return Err(VersionError::InvalidChar { at: pos });
        }
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] == b'-' {
        pos += 1;
        let start = pos;
        let plus = bytes[pos..].iter().position(|&b| b == b'+').map(|i| pos + i);
        let end = plus.unwrap_or(bytes.len());
        pre = parse_identifiers(bytes, start, end, true)?;
        pos = end;
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        pos += 1;
        build = parse_identifiers(bytes, pos, bytes.len(), false)?;
        pos = bytes.len();
    }

    if pos != bytes.len() {
        return Err(VersionError::InvalidChar { at: pos });
    }

    Ok(Version {
        major: core[0],
        minor: core[1],
        patch: core[2],
        pre,
        build,
    })
}

fn parse_identifiers(
    bytes: &[u8],
    start: usize,
    end: usize,
    is_pre: bool,
) -> Result<Vec<String>, VersionError> {
    let mut out = Vec::new();
    let mut id_start = start;
    loop {
        let id_end = bytes[id_start..end]
            .iter()
            .position(|&b| b == b'.')
            .map(|i| id_start + i)
            .unwrap_or(end);

        if id_start == id_end {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        for i in id_start..id_end {
            let b = bytes[i];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: i });
            }
        }
        let numeric = bytes[id_start..id_end].iter().all(u8::is_ascii_digit);
        if is_pre && numeric && id_end - id_start > 1 && bytes[id_start] == b'0' {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        out.push(String::from_utf8(bytes[id_start..id_end].to_vec()).unwrap());

        if id_end == end {
            break;
        }
        id_start = id_end + 1;
        if id_start == end {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
    }
    Ok(out)
}

fn core_cmp(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch))
}

fn is_numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn numeric_identifier_cmp(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    let core = core_cmp(a, b);
    if core != Ordering::Equal {
        return core;
    }
    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (x, y) in a.pre.iter().zip(&b.pre) {
                let xn = is_numeric_identifier(x);
                let yn = is_numeric_identifier(y);
                let ord = match (xn, yn) {
                    (true, true) => numeric_identifier_cmp(x, y),
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    (false, false) => x.as_bytes().cmp(y.as_bytes()),
                };
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            a.pre.len().cmp(&b.pre.len())
        }
    }
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Vec<Constraint>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Debug)]
struct Constraint {
    op: CmpOp,
    version: Version,
    anchor: bool,
}

#[derive(Clone, Copy, Debug)]
enum CmpOp {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug)]
struct Token<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokenize<'a>(s: &'a str) -> Vec<Token<'a>> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && is_ws(bytes[i]) {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !is_ws(bytes[i]) {
            i += 1;
        }
        out.push(Token {
            text: &s[start..i],
            start,
            end: i,
        });
    }
    out
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let tokens = tokenize(s);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms = Vec::new();
    let mut arm_tokens = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].text == "||" {
            if arm_tokens.is_empty() {
                return Err(RangeError::Empty { at: tokens[i].start });
            }
            arms.push(parse_arm(&arm_tokens)?);
            arm_tokens.clear();
        } else {
            arm_tokens.push(tokens[i].clone());
        }
        i += 1;
    }
    if arm_tokens.is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(&arm_tokens)?);
    Ok(Range { arms })
}

fn parse_arm(tokens: &[Token<'_>]) -> Result<Vec<Constraint>, RangeError> {
    if let Some(minus) = tokens.iter().find(|t| t.text == "-") {
        if tokens.len() != 3 || tokens[1].text != "-" {
            return Err(RangeError::Syntax { at: minus.start });
        }
        return parse_hyphen(&tokens[0], &tokens[2]);
    }

    let mut constraints = Vec::new();
    for token in tokens {
        let t = token.text;
        if t.starts_with('^') || t.starts_with('~') {
            let caret = t.starts_with('^');
            let operand_start = token.start + 1;
            let operand = &t[1..];
            let parsed = parse_partial_or_full(operand, operand_start)?;
            if parsed.numeric_fields == 0 {
                return Err(RangeError::Syntax { at: token.start });
            }
            let lower = parsed.version;
            let upper = expanded_upper(&lower, parsed.numeric_fields, caret, operand_start)?;
            add_constraint(&mut constraints, CmpOp::Ge, lower.clone(), !lower.pre.is_empty());
            add_constraint(&mut constraints, CmpOp::Lt, upper, false);
            continue;
        }

        if matches!(t.as_bytes().first(), Some(b'<' | b'>' | b'=')) {
            let (op, n) = if t.starts_with("<=") {
                (CmpOp::Le, 2)
            } else if t.starts_with(">=") {
                (CmpOp::Ge, 2)
            } else if t.starts_with('<') {
                (CmpOp::Lt, 1)
            } else if t.starts_with('>') {
                (CmpOp::Gt, 1)
            } else {
                (CmpOp::Eq, 1)
            };
            let operand_start = token.start + n;
            let operand = &t[n..];
            if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
                return Err(RangeError::Syntax { at: token.start });
            }
            let version = parse_version(operand).map_err(|error| RangeError::Version {
                at: operand_start,
                error,
            })?;
            let anchor = !version.pre.is_empty();
            add_constraint(&mut constraints, op, version, anchor);
            continue;
        }

        let first = t.as_bytes().first().copied();
        if !matches!(first, Some(b'0'..=b'9' | b'*' | b'x' | b'X')) {
            return Err(RangeError::Syntax { at: token.start });
        }
        let parsed = parse_partial_or_full(t, token.start)?;
        if parsed.full {
            let version = parsed.version;
            let anchor = !version.pre.is_empty();
            add_constraint(&mut constraints, CmpOp::Eq, version, anchor);
        } else if parsed.numeric_fields == 0 {
            // Wildcard-only bare operands impose no bound.
        } else if parsed.numeric_fields == 3 {
            let version = parsed.version;
            let anchor = !version.pre.is_empty();
            add_constraint(&mut constraints, CmpOp::Eq, version, anchor);
        } else {
            let lower = parsed.version;
            let upper = next_partial_upper(&lower, parsed.numeric_fields, token.start)?;
            add_constraint(&mut constraints, CmpOp::Ge, lower, false);
            add_constraint(&mut constraints, CmpOp::Lt, upper, false);
        }
    }
    Ok(constraints)
}

#[derive(Clone, Debug)]
struct ParsedOperand {
    version: Version,
    numeric_fields: usize,
    full: bool,
}

fn parse_partial_or_full(s: &str, at: usize) -> Result<ParsedOperand, RangeError> {
    let fields: Vec<&str> = s.split('.').collect();
    let has_suffix = s.contains('-') || s.contains('+');
    let has_wildcard = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));
    if has_suffix || (fields.len() >= 3 && !has_wildcard) {
        let version = parse_version(s).map_err(|error| RangeError::Version { at, error })?;
        return Ok(ParsedOperand {
            version,
            numeric_fields: 3,
            full: true,
        });
    }

    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at });
    }
    let mut values = [0u64; 3];
    let mut numeric_fields = 0;
    let mut wildcard_seen = false;

    for (i, field) in fields.iter().enumerate() {
        if field.is_empty() {
            return Err(RangeError::Syntax { at });
        }
        if matches!(*field, "*" | "x" | "X") {
            wildcard_seen = true;
            continue;
        }
        if wildcard_seen || !field.bytes().all(|b| b.is_ascii_digit()) {
            return Err(RangeError::Syntax { at });
        }
        if field.len() > 1 && field.as_bytes()[0] == b'0' {
            return Err(RangeError::Syntax { at });
        }
        let mut n = 0u64;
        for b in field.bytes() {
            n = n
                .checked_mul(10)
                .and_then(|v| v.checked_add((b - b'0') as u64))
                .ok_or(RangeError::Syntax { at })?;
        }
        values[i] = n;
        numeric_fields += 1;
    }

    if numeric_fields > 0 {
        for i in numeric_fields..3 {
            values[i] = 0;
        }
    }
    Ok(ParsedOperand {
        version: Version {
            major: values[0],
            minor: values[1],
            patch: values[2],
            pre: Vec::new(),
            build: Vec::new(),
        },
        numeric_fields,
        full: false,
    })
}

fn add_constraint(
    constraints: &mut Vec<Constraint>,
    op: CmpOp,
    version: Version,
    anchor: bool,
) {
    constraints.push(Constraint {
        op,
        version,
        anchor,
    });
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

fn increment(value: u64, at: usize) -> Result<u64, RangeError> {
    value.checked_add(1).ok_or(RangeError::Syntax { at })
}

fn next_partial_upper(lower: &Version, fields: usize, at: usize) -> Result<Version, RangeError> {
    if fields == 1 {
        Ok(upper_version(increment(lower.major, at)?, 0, 0))
    } else {
        Ok(upper_version(lower.major, increment(lower.minor, at)?, 0))
    }
}

fn expanded_upper(
    lower: &Version,
    fields: usize,
    caret: bool,
    at: usize,
) -> Result<Version, RangeError> {
    if caret {
        if lower.major > 0 || fields == 1 {
            Ok(upper_version(increment(lower.major, at)?, 0, 0))
        } else if lower.minor > 0 || fields == 2 {
            Ok(upper_version(lower.major, increment(lower.minor, at)?, 0))
        } else {
            Ok(upper_version(
                lower.major,
                lower.minor,
                increment(lower.patch, at)?,
            ))
        }
    } else if fields == 1 {
        Ok(upper_version(increment(lower.major, at)?, 0, 0))
    } else {
        Ok(upper_version(lower.major, increment(lower.minor, at)?, 0))
    }
}

fn parse_hyphen(
    lower_token: &Token<'_>,
    upper_token: &Token<'_>,
) -> Result<Vec<Constraint>, RangeError> {
    let lower = parse_partial_or_full(lower_token.text, lower_token.start)?;
    let upper = parse_partial_or_full(upper_token.text, upper_token.start)?;

    if lower.numeric_fields == 0 {
        return Err(RangeError::Syntax {
            at: lower_token.start,
        });
    }
    if upper.numeric_fields == 0 {
        return Err(RangeError::Syntax {
            at: upper_token.start,
        });
    }

    let upper_bound = if upper.full {
        upper.version.clone()
    } else if upper.numeric_fields == 1 || upper.numeric_fields == 2 {
        next_partial_upper(&upper.version, upper.numeric_fields, upper_token.start)?
    } else {
        upper.version.clone()
    };

    let mut constraints = Vec::new();
    add_constraint(
        &mut constraints,
        CmpOp::Ge,
        lower.version.clone(),
        !lower.version.pre.is_empty(),
    );
    add_constraint(
        &mut constraints,
        if upper.full { CmpOp::Le } else { CmpOp::Lt },
        upper_bound,
        upper.full && !upper.version.pre.is_empty(),
    );
    Ok(constraints)
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| arm_matches(v, arm))
}

fn arm_matches(v: &Version, arm: &[Constraint]) -> bool {
    let admitted = arm.iter().any(|c| {
        c.anchor
            && !c.version.pre.is_empty()
            && core_cmp(v, &c.version) == Ordering::Equal
    });

    if !v.pre.is_empty() && !admitted {
        return false;
    }

    arm.iter().all(|c| {
        let ord = compare_versions(v, &c.version);
        match c.op {
            CmpOp::Eq => ord == Ordering::Equal,
            CmpOp::Lt => ord == Ordering::Less,
            CmpOp::Le => ord != Ordering::Greater,
            CmpOp::Gt => ord == Ordering::Greater,
            CmpOp::Ge => ord != Ordering::Less,
        }
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

pub fn max_satisfying(
    candidates: &[String],
    r: &Range,
) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, true)
}

pub fn min_satisfying(
    candidates: &[String],
    r: &Range,
) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, false)
}

fn extrema(
    candidates: &[String],
    r: &Range,
    want_max: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let version = parse_version(candidate).map_err(|error| CandidateError { index, error })?;
        parsed.push(version);
    }

    let mut best: Option<usize> = None;
    for (i, version) in parsed.iter().enumerate() {
        if !matches(version, r) {
            continue;
        }
        best = match best {
            None => Some(i),
            Some(j) => {
                let ord = compare_versions(version, &parsed[j]);
                if (want_max && ord == Ordering::Greater) || (!want_max && ord == Ordering::Less) {
                    Some(i)
                } else {
                    Some(j)
                }
            }
        };
    }

    Ok(best.map(|i| candidates[i].clone()))
}