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

fn parse_core_number(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let start = *pos;
    let bytes = s.as_bytes();
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if *pos == start {
        return Err(match bytes.get(start) {
            None | Some(b'.' | b'-' | b'+') => VersionError::MissingCore { at: start },
            _ => VersionError::InvalidChar { at: start },
        });
    }

    let digits = &s[start..*pos];
    if digits.len() > 1 && digits.as_bytes()[0] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    digits
        .parse::<u64>()
        .map_err(|_| VersionError::Overflow { at: start })
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    prerelease: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut id_start = start;

    loop {
        let mut i = id_start;
        while i < end && bytes[i] != b'.' {
            let b = bytes[i];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }

        if i == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }

        let id = &s[id_start..i];
        if prerelease
            && id.len() > 1
            && id.as_bytes().iter().all(u8::is_ascii_digit)
            && id.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        out.push(id.to_string());

        if i == end {
            break;
        }
        id_start = i + 1;
        if id_start > end {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
    }

    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let mut pos = 0;
    let major = parse_core_number(s, &mut pos)?;
    if s.as_bytes().get(pos) != Some(&b'.') {
        return Err(match s.as_bytes().get(pos) {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: pos },
            _ => VersionError::InvalidChar { at: pos },
        });
    }
    pos += 1;

    let minor = parse_core_number(s, &mut pos)?;
    if s.as_bytes().get(pos) != Some(&b'.') {
        return Err(match s.as_bytes().get(pos) {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: pos },
            _ => VersionError::InvalidChar { at: pos },
        });
    }
    pos += 1;

    let patch = parse_core_number(s, &mut pos)?;
    match s.as_bytes().get(pos) {
        None | Some(b'-' | b'+') => {}
        _ => return Err(VersionError::InvalidChar { at: pos }),
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if s.as_bytes().get(pos) == Some(&b'-') {
        let pre_start = pos + 1;
        let build_plus = s.as_bytes()[pre_start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|n| pre_start + n);
        let pre_end = build_plus.unwrap_or(s.len());
        pre = parse_identifiers(s, pre_start, pre_end, true)?;
        if let Some(plus) = build_plus {
            build = parse_identifiers(s, plus + 1, s.len(), false)?;
        }
    } else if s.as_bytes().get(pos) == Some(&b'+') {
        build = parse_identifiers(s, pos + 1, s.len(), false)?;
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
    !s.is_empty() && s.as_bytes().iter().all(u8::is_ascii_digit)
}

fn compare_identifiers(a: &str, b: &str) -> Ordering {
    let an = numeric_identifier(a);
    let bn = numeric_identifier(b);
    match (an, bn) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())),
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
                    let ord = compare_identifiers(x, y);
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
pub struct Range {
    arms: Vec<Arm>,
}

#[derive(Clone, Debug)]
struct Comparator {
    op: Op,
    version: Version,
    grants_admission: bool,
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
    anchors: Vec<(u64, u64, u64)>,
}

impl Arm {
    fn new() -> Self {
        Self {
            comparators: Vec::new(),
            anchors: Vec::new(),
        }
    }

    fn add(&mut self, op: Op, version: Version, grants_admission: bool) {
        if grants_admission && !version.pre.is_empty() {
            self.anchors.push((version.major, version.minor, version.patch));
        }
        self.comparators.push(Comparator {
            op,
            version,
            grants_admission,
        });
    }

    fn matches(&self, v: &Version) -> bool {
        if !v.pre.is_empty()
            && !self
                .anchors
                .iter()
                .any(|&(ma, mi, pa)| (ma, mi, pa) == (v.major, v.minor, v.patch))
        {
            return false;
        }
        self.comparators.iter().all(|c| {
            let ord = compare_versions(v, &c.version);
            match c.op {
                Op::Eq => ord == Ordering::Equal,
                Op::Lt => ord == Ordering::Less,
                Op::Le => ord != Ordering::Greater,
                Op::Gt => ord == Ordering::Greater,
                Op::Ge => ord != Ordering::Less,
            }
        })
    }
}

#[derive(Clone, Debug)]
enum ParsedOperand {
    Full(Version),
    Partial {
        fields: Vec<u64>,
        numeric_count: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FieldKind {
    Numeric,
    Wildcard,
}

fn syntax(at: usize) -> RangeError {
    RangeError::Syntax { at }
}

fn version_error(at: usize, error: VersionError) -> RangeError {
    RangeError::Version { at, error }
}

fn field_kind(field: &str) -> Option<FieldKind> {
    if field == "*" || field == "x" || field == "X" {
        Some(FieldKind::Wildcard)
    } else if !field.is_empty() && field.as_bytes().iter().all(u8::is_ascii_digit) {
        Some(FieldKind::Numeric)
    } else {
        None
    }
}

fn looks_like_full_version(s: &str) -> bool {
    s.contains('-')
        || s.contains('+')
        || (s.split('.').count() >= 3
            && !s.split('.').any(|f| f == "*" || f == "x" || f == "X"))
}

fn parse_operand(s: &str, at: usize) -> Result<ParsedOperand, RangeError> {
    if s.is_empty() {
        return Err(syntax(at));
    }
    if looks_like_full_version(s) {
        return parse_version(s)
            .map(ParsedOperand::Full)
            .map_err(|e| version_error(at, e));
    }

    let parts: Vec<&str> = s.split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return Err(syntax(at));
    }

    let mut fields = Vec::new();
    let mut numeric_count = 0;
    let mut wildcard_seen = false;
    for part in parts {
        match field_kind(part) {
            Some(FieldKind::Numeric) if !wildcard_seen => {
                if part.len() > 1 && part.as_bytes()[0] == b'0' {
                    return Err(syntax(at));
                }
                let n = part.parse::<u64>().map_err(|_| syntax(at))?;
                fields.push(n);
                numeric_count += 1;
            }
            Some(FieldKind::Wildcard) => {
                wildcard_seen = true;
            }
            _ => return Err(syntax(at)),
        }
    }

    Ok(ParsedOperand::Partial {
        fields,
        numeric_count,
    })
}

fn padded_version(fields: &[u64]) -> Version {
    Version {
        major: *fields.first().unwrap_or(&0),
        minor: *fields.get(1).unwrap_or(&0),
        patch: *fields.get(2).unwrap_or(&0),
        pre: Vec::new(),
        build: Vec::new(),
    }
}

fn full_or_lower(op: &ParsedOperand) -> Version {
    match op {
        ParsedOperand::Full(v) => v.clone(),
        ParsedOperand::Partial { fields, .. } => padded_version(fields),
    }
}

fn bump_upper(v: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let mut upper = Version {
        major: v.major,
        minor: v.minor,
        patch: v.patch,
        pre: vec!["0".to_string()],
        build: Vec::new(),
    };
    let target = match field {
        0 => &mut upper.major,
        1 => &mut upper.minor,
        _ => &mut upper.patch,
    };
    *target = target.checked_add(1).ok_or_else(|| syntax(at))?;
    match field {
        0 => {
            upper.minor = 0;
            upper.patch = 0;
        }
        1 => upper.patch = 0,
        _ => {}
    }
    Ok(upper)
}

fn add_partial_bare(
    arm: &mut Arm,
    operand: ParsedOperand,
    at: usize,
) -> Result<(), RangeError> {
    match operand {
        ParsedOperand::Full(v) => arm.add(Op::Eq, v, true),
        ParsedOperand::Partial {
            fields,
            numeric_count,
        } => match numeric_count {
            0 => {}
            1 | 2 => {
                let lower = padded_version(&fields);
                let upper = bump_upper(&lower, numeric_count - 1, at)?;
                arm.add(Op::Ge, lower, true);
                arm.add(Op::Lt, upper, false);
            }
            _ => arm.add(Op::Eq, padded_version(&fields), true),
        },
    }
    Ok(())
}

fn add_caret_tilde(
    arm: &mut Arm,
    operand: ParsedOperand,
    at: usize,
    caret: bool,
) -> Result<(), RangeError> {
    let (lower, numeric_count) = match operand {
        ParsedOperand::Full(v) => (v, 3),
        ParsedOperand::Partial {
            fields,
            numeric_count,
        } => (padded_version(&fields), numeric_count),
    };
    if numeric_count == 0 {
        return Err(syntax(at));
    }

    let bump_field = if caret {
        if lower.major > 0 || numeric_count == 1 {
            0
        } else if lower.minor > 0 || numeric_count == 2 {
            1
        } else {
            2
        }
    } else if numeric_count == 1 {
        0
    } else {
        1
    };
    let upper = bump_upper(&lower, bump_field, at)?;
    arm.add(Op::Ge, lower, true);
    arm.add(Op::Lt, upper, false);
    Ok(())
}

fn add_hyphen(
    arm: &mut Arm,
    lower: ParsedOperand,
    upper: ParsedOperand,
    lower_at: usize,
    upper_at: usize,
) -> Result<(), RangeError> {
    let (lower, lower_count) = match lower {
        ParsedOperand::Full(v) => (v, 3),
        ParsedOperand::Partial {
            fields,
            numeric_count,
        } => (padded_version(&fields), numeric_count),
    };
    let (upper_version, upper_count, upper_full) = match upper {
        ParsedOperand::Full(v) => (v, 3, true),
        ParsedOperand::Partial {
            fields,
            numeric_count,
        } => (padded_version(&fields), numeric_count, false),
    };
    if lower_count == 0 {
        return Err(syntax(lower_at));
    }
    if upper_count == 0 {
        return Err(syntax(upper_at));
    }

    arm.add(Op::Ge, lower, true);
    if upper_full {
        arm.add(Op::Le, upper_version, true);
    } else {
        let upper_bound = bump_upper(&upper_version, upper_count - 1, upper_at)?;
        arm.add(Op::Lt, upper_bound, false);
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Token<'a> {
    text: &'a str,
    start: usize,
}

fn is_range_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokens(s: &str, start: usize, end: usize) -> Vec<Token<'_>> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = start;
    while i < end {
        while i < end && is_range_ws(bytes[i]) {
            i += 1;
        }
        if i >= end {
            break;
        }
        let token_start = i;
        while i < end && !is_range_ws(bytes[i]) {
            i += 1;
        }
        out.push(Token {
            text: &s[token_start..i],
            start: token_start,
        });
    }
    out
}

fn has_standalone_minus(ts: &[Token<'_>]) -> Option<usize> {
    ts.iter().find(|t| t.text == "-").map(|t| t.start)
}

fn parse_arm(s: &str, ts: &[Token<'_>]) -> Result<Arm, RangeError> {
    if let Some(minus_at) = has_standalone_minus(ts) {
        if ts.len() != 3 || ts[1].text != "-" {
            return Err(syntax(minus_at));
        }
        let lower = parse_operand(ts[0].text, ts[0].start)?;
        let upper = parse_operand(ts[2].text, ts[2].start)?;
        let mut arm = Arm::new();
        add_hyphen(&mut arm, lower, upper, ts[0].start, ts[2].start)?;
        return Ok(arm);
    }

    let mut arm = Arm::new();
    for token in ts {
        let t = token.text;
        if t.is_empty() {
            return Err(syntax(token.start));
        }

        if t.starts_with('^') || t.starts_with('~') {
            let operand = &t[1..];
            let at = token.start + 1;
            let parsed = parse_operand(operand, at)?;
            add_caret_tilde(&mut arm, parsed, at, t.as_bytes()[0] == b'^')?;
            continue;
        }

        let first = t.as_bytes()[0];
        if matches!(first, b'<' | b'>' | b'=') {
            let (op, operator_len) = if t.starts_with("<=") {
                (Op::Le, 2)
            } else if t.starts_with(">=") {
                (Op::Ge, 2)
            } else if first == b'<' {
                (Op::Lt, 1)
            } else if first == b'>' {
                (Op::Gt, 1)
            } else {
                (Op::Eq, 1)
            };
            let operand = &t[operator_len..];
            let at = token.start + operator_len;
            if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
                return Err(syntax(token.start));
            }
            let version = parse_version(operand).map_err(|e| version_error(at, e))?;
            arm.add(op, version, true);
            continue;
        }

        if !first.is_ascii_digit() && !matches!(first, b'*' | b'x' | b'X') {
            return Err(syntax(token.start));
        }
        let parsed = parse_operand(t, token.start)?;
        add_partial_bare(&mut arm, parsed, token.start)?;
    }

    let _ = s;
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = Vec::new();
    let mut arm_start = 0;
    let mut i = 0;

    while i < bytes.len() {
        while i < bytes.len() && is_range_ws(bytes[i]) {
            i += 1;
        }
        if i + 1 < bytes.len() && bytes[i] == b'|' && bytes[i + 1] == b'|' {
            let ts = tokens(s, arm_start, i);
            if ts.is_empty() {
                return Err(RangeError::Empty { at: i });
            }
            arms.push(parse_arm(s, &ts)?);
            i += 2;
            while i < bytes.len() && is_range_ws(bytes[i]) {
                i += 1;
            }
            arm_start = i;
            continue;
        }
        while i < bytes.len() && !is_range_ws(bytes[i]) {
            i += 1;
        }
    }

    let ts = tokens(s, arm_start, bytes.len());
    if ts.is_empty() {
        if arms.is_empty() {
            return Err(RangeError::Empty { at: 0 });
        }
        return Err(RangeError::Empty { at: bytes.len() });
    }
    arms.push(parse_arm(s, &ts)?);
    Ok(Range { arms })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| arm.matches(v))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn satisfying(
    candidates: &[String],
    r: &Range,
    greatest: bool,
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
            Some(old) => {
                let ord = compare_versions(version, &parsed[old]);
                if (greatest && ord == Ordering::Greater)
                    || (!greatest && ord == Ordering::Less)
                {
                    best = Some(index);
                }
            }
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

pub fn max_satisfying(
    candidates: &[String],
    r: &Range,
) -> Result<Option<String>, CandidateError> {
    satisfying(candidates, r, true)
}

pub fn min_satisfying(
    candidates: &[String],
    r: &Range,
) -> Result<Option<String>, CandidateError> {
    satisfying(candidates, r, false)
}