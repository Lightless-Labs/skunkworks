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
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let bytes = s.as_bytes();
    let mut pos = 0usize;
    let mut core = [0u64; 3];

    for (index, slot) in core.iter_mut().enumerate() {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos == start {
            if pos == bytes.len() || matches!(bytes[pos], b'.' | b'-' | b'+') {
                return Err(VersionError::MissingCore { at: start });
            }
            return Err(VersionError::InvalidChar { at: start });
        }

        if pos - start > 1 && bytes[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }
        let value = s[start..pos]
            .parse::<u64>()
            .map_err(|_| VersionError::Overflow { at: start })?;
        *slot = value;

        if index < 2 {
            if pos >= bytes.len() {
                return Err(VersionError::MissingCore { at: pos });
            }
            if bytes[pos] == b'.' {
                pos += 1;
            } else if matches!(bytes[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            } else {
                return Err(VersionError::InvalidChar { at: pos });
            }
        }
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() {
        match bytes[pos] {
            b'-' => {
                pos += 1;
                let (ids, next) = parse_identifiers(s, pos, true)?;
                pre = ids;
                pos = next;
                if pos < bytes.len() {
                    if bytes[pos] != b'+' {
                        return Err(VersionError::InvalidChar { at: pos });
                    }
                    pos += 1;
                    let (ids, next) = parse_identifiers(s, pos, false)?;
                    build = ids;
                    pos = next;
                }
            }
            b'+' => {
                pos += 1;
                let (ids, next) = parse_identifiers(s, pos, false)?;
                build = ids;
                pos = next;
            }
            _ => return Err(VersionError::InvalidChar { at: pos }),
        }
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
    s: &str,
    start: usize,
    validate_pre_numeric: bool,
) -> Result<(Vec<String>, usize), VersionError> {
    let bytes = s.as_bytes();
    let mut ids = Vec::new();
    let mut pos = start;

    loop {
        let id_start = pos;
        while pos < bytes.len() && bytes[pos] != b'.' && bytes[pos] != b'+' {
            let b = bytes[pos];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
        }

        if pos == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }

        let id = &s[id_start..pos];
        if validate_pre_numeric
            && id.bytes().all(|b| b.is_ascii_digit())
            && id.len() > 1
            && id.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        ids.push(id.to_string());

        if pos == bytes.len() || bytes[pos] == b'+' {
            return Ok((ids, pos));
        }
        pos += 1;
        if pos == bytes.len() {
            return Err(VersionError::EmptyIdentifier { at: pos });
        }
    }
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch)
        .cmp(&(b.major, b.minor, b.patch))
        .then_with(|| compare_pre(&a.pre, &b.pre))
}

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }

    for (left, right) in a.iter().zip(b.iter()) {
        let left_num = left.bytes().all(|c| c.is_ascii_digit());
        let right_num = right.bytes().all(|c| c.is_ascii_digit());
        let ordering = match (left_num, right_num) {
            (true, true) => left
                .len()
                .cmp(&right.len())
                .then_with(|| left.as_bytes().cmp(right.as_bytes())),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => left.as_bytes().cmp(right.as_bytes()),
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    a.len().cmp(&b.len())
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
    op: Op,
    version: Version,
    anchor: bool,
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Constraint {
    fn matches(&self, candidate: &Version) -> bool {
        let ord = compare_versions(candidate, &self.version);
        match self.op {
            Op::Eq => ord == Ordering::Equal,
            Op::Lt => ord == Ordering::Less,
            Op::Le => ord != Ordering::Greater,
            Op::Gt => ord == Ordering::Greater,
            Op::Ge => ord != Ordering::Less,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Token<'a> {
    text: &'a str,
    start: usize,
}

fn is_range_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokenize(s: &str) -> Vec<Token<'_>> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        while pos < bytes.len() && is_range_ws(bytes[pos]) {
            pos += 1;
        }
        if pos == bytes.len() {
            break;
        }
        let start = pos;
        while pos < bytes.len() && !is_range_ws(bytes[pos]) {
            pos += 1;
        }
        out.push(Token {
            text: &s[start..pos],
            start,
        });
    }
    out
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let tokens = tokenize(s);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms = Vec::<Vec<Constraint>>::new();
    let mut arm_tokens = Vec::<Token<'_>>::new();
    let mut i = 0;

    while i < tokens.len() {
        if tokens[i].text == "||" {
            if arm_tokens.is_empty() {
                return Err(RangeError::Empty {
                    at: tokens[i].start,
                });
            }
            arms.push(parse_arm(&arm_tokens)?);
            arm_tokens.clear();
            i += 1;
            if i == tokens.len() {
                return Err(RangeError::Empty { at: s.len() });
            }
        } else {
            arm_tokens.push(tokens[i]);
            i += 1;
        }
    }

    if !arm_tokens.is_empty() {
        arms.push(parse_arm(&arm_tokens)?);
    }
    Ok(Range { arms })
}

fn parse_arm(tokens: &[Token<'_>]) -> Result<Vec<Constraint>, RangeError> {
    if let Some(minus) = tokens.iter().find(|t| t.text == "-") {
        if tokens.len() != 3 || tokens[1].text != "-" {
            return Err(RangeError::Syntax { at: minus.start });
        }
        let lower = parse_partial_operand(tokens[0])?;
        let upper = parse_partial_operand(tokens[2])?;
        if lower.numeric_count == 0 {
            return Err(RangeError::Syntax { at: tokens[0].start });
        }
        if upper.numeric_count == 0 {
            return Err(RangeError::Syntax { at: tokens[2].start });
        }
        let mut out = vec![Constraint {
            op: Op::Ge,
            version: lower.lower,
            anchor: lower.anchor,
        }];
        if let Some(v) = upper.full {
            out.push(Constraint {
                op: Op::Le,
                anchor: !v.pre.is_empty(),
                version: v,
            });
        } else {
            let upper_bound = next_partial_bound(
                &upper.lower,
                upper.numeric_count,
                tokens[2].start,
                PartialBound::Hyphen,
            )?;
            out.push(Constraint {
                op: Op::Lt,
                version: upper_bound,
                anchor: false,
            });
        }
        return Ok(out);
    }

    let mut out = Vec::new();
    for token in tokens {
        let text = token.text;
        let first = text.as_bytes().first().copied();
        match first {
            Some(b'<') | Some(b'>') | Some(b'=') => {
                let (op, width) = if text.starts_with("<=") {
                    (Op::Le, 2)
                } else if text.starts_with(">=") {
                    (Op::Ge, 2)
                } else {
                    match first.unwrap() {
                        b'<' => (Op::Lt, 1),
                        b'>' => (Op::Gt, 1),
                        _ => (Op::Eq, 1),
                    }
                };
                let operand = &text[width..];
                if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
                    return Err(RangeError::Syntax { at: token.start });
                }
                let version = parse_version(operand).map_err(|error| RangeError::Version {
                    at: token.start + width,
                    error,
                })?;
                out.push(Constraint {
                    op,
                    anchor: !version.pre.is_empty(),
                    version,
                });
            }
            Some(b'^') | Some(b'~') => {
                let is_caret = first == Some(b'^');
                let operand_start = token.start + 1;
                let operand = &text[1..];
                let parsed = parse_partial_text(operand, operand_start)?;
                if parsed.numeric_count == 0 {
                    return Err(RangeError::Syntax { at: token.start });
                }
                out.push(Constraint {
                    op: Op::Ge,
                    anchor: parsed.anchor,
                    version: parsed.lower.clone(),
                });
                let bound = next_partial_bound(
                    &parsed.lower,
                    parsed.numeric_count,
                    operand_start,
                    if is_caret {
                        PartialBound::Caret
                    } else {
                        PartialBound::Tilde
                    },
                )?;
                out.push(Constraint {
                    op: Op::Lt,
                    version: bound,
                    anchor: false,
                });
            }
            Some(b'-') => return Err(RangeError::Syntax { at: token.start }),
            Some(b',') | Some(b'(') | Some(b')') | Some(b'&') => {
                return Err(RangeError::Syntax { at: token.start })
            }
            Some(b) if b.is_ascii_digit() || matches!(b, b'*' | b'x' | b'X') => {
                let parsed = parse_partial_text(text, token.start)?;
                if parsed.numeric_count == 0 {
                    continue;
                }
                if parsed.full.is_some() {
                    out.push(Constraint {
                        op: Op::Eq,
                        anchor: parsed.anchor,
                        version: parsed.full.unwrap(),
                    });
                } else {
                    out.push(Constraint {
                        op: Op::Ge,
                        anchor: false,
                        version: parsed.lower.clone(),
                    });
                    if parsed.numeric_count < 3 {
                        let bound = next_partial_bound(
                            &parsed.lower,
                            parsed.numeric_count,
                            token.start,
                            PartialBound::Bare,
                        )?;
                        out.push(Constraint {
                            op: Op::Lt,
                            version: bound,
                            anchor: false,
                        });
                    }
                }
            }
            _ => return Err(RangeError::Syntax { at: token.start }),
        }
    }
    Ok(out)
}

#[derive(Clone, Copy)]
enum PartialBound {
    Bare,
    Caret,
    Tilde,
    Hyphen,
}

struct ParsedOperand {
    lower: Version,
    full: Option<Version>,
    numeric_count: usize,
    anchor: bool,
}

fn parse_partial_operand(token: Token<'_>) -> Result<ParsedOperand, RangeError> {
    parse_partial_text(token.text, token.start)
}

fn parse_partial_text(text: &str, at: usize) -> Result<ParsedOperand, RangeError> {
    if text.is_empty() {
        return Err(RangeError::Syntax { at });
    }
    let fields: Vec<&str> = text.split('.').collect();
    let has_suffix = text.contains('-') || text.contains('+');
    let has_three_nonwild = fields.len() >= 3
        && !fields.iter().any(|field| matches!(*field, "*" | "x" | "X"));

    if has_suffix || has_three_nonwild {
        let version = parse_version(text).map_err(|error| RangeError::Version { at, error })?;
        return Ok(ParsedOperand {
            lower: version.clone(),
            anchor: !version.pre.is_empty(),
            full: Some(version),
            numeric_count: 3,
        });
    }

    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at });
    }

    let mut numbers = Vec::new();
    let mut wildcard_seen = false;
    for field in &fields {
        if field.is_empty() {
            return Err(RangeError::Syntax { at });
        }
        if matches!(*field, "*" | "x" | "X") {
            wildcard_seen = true;
            continue;
        }
        if wildcard_seen
            || !field.bytes().all(|b| b.is_ascii_digit())
            || (field.len() > 1 && field.as_bytes()[0] == b'0')
        {
            return Err(RangeError::Syntax { at });
        }
        let value = field.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
        numbers.push(value);
    }

    let count = numbers.len();
    let mut vals = [0u64; 3];
    for (i, value) in numbers.iter().enumerate() {
        vals[i] = *value;
    }
    let lower = Version {
        major: vals[0],
        minor: vals[1],
        patch: vals[2],
        pre: Vec::new(),
        build: Vec::new(),
    };
    Ok(ParsedOperand {
        lower,
        full: None,
        numeric_count: count,
        anchor: false,
    })
}

fn next_partial_bound(
    lower: &Version,
    numeric_count: usize,
    at: usize,
    kind: PartialBound,
) -> Result<Version, RangeError> {
    let mut result = Version {
        major: lower.major,
        minor: lower.minor,
        patch: lower.patch,
        pre: vec!["0".to_string()],
        build: Vec::new(),
    };

    let bump = |value: u64| value.checked_add(1).ok_or(RangeError::Syntax { at });

    match kind {
        PartialBound::Bare | PartialBound::Hyphen => match numeric_count {
            1 => {
                result.major = bump(lower.major)?;
                result.minor = 0;
                result.patch = 0;
            }
            2 => {
                result.minor = bump(lower.minor)?;
                result.patch = 0;
            }
            _ => return Err(RangeError::Syntax { at }),
        },
        PartialBound::Tilde => {
            if numeric_count == 1 {
                result.major = bump(lower.major)?;
                result.minor = 0;
                result.patch = 0;
            } else {
                result.minor = bump(lower.minor)?;
                result.patch = 0;
            }
        }
        PartialBound::Caret => {
            if lower.major > 0 || numeric_count == 1 {
                result.major = bump(lower.major)?;
                result.minor = 0;
                result.patch = 0;
            } else if lower.minor > 0 || numeric_count == 2 {
                result.minor = bump(lower.minor)?;
                result.patch = 0;
            } else {
                result.patch = bump(lower.patch)?;
            }
        }
    }
    Ok(result)
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        if !v.pre.is_empty()
            && !arm.iter().any(|constraint| {
                constraint.anchor
                    && !constraint.version.pre.is_empty()
                    && same_core(v, &constraint.version)
            })
        {
            return false;
        }
        arm.iter().all(|constraint| constraint.matches(v))
    })
}

fn same_core(a: &Version, b: &Version) -> bool {
    (a.major, a.minor, a.patch) == (b.major, b.minor, b.patch)
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
    maximum: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let version = parse_version(candidate).map_err(|error| CandidateError { index, error })?;
        parsed.push(version);
    }

    let mut best: Option<usize> = None;
    for (index, version) in parsed.iter().enumerate() {
        if !matches(version, r) {
            continue;
        }
        match best {
            None => best = Some(index),
            Some(previous) => {
                let ordering = compare_versions(version, &parsed[previous]);
                if (maximum && ordering == Ordering::Greater)
                    || (!maximum && ordering == Ordering::Less)
                {
                    best = Some(index);
                }
            }
        }
    }
    Ok(best.map(|index| candidates[index].clone()))
}