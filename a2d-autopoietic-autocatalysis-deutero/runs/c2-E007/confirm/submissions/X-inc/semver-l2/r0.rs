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
    let mut pos = 0;
    let mut core = [0u64; 3];

    for i in 0..3 {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if start == pos {
            if pos == bytes.len() || matches!(bytes[pos], b'.' | b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            return Err(VersionError::InvalidChar { at: pos });
        }

        if pos - start > 1 && bytes[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }
        let n = s[start..pos]
            .parse::<u64>()
            .map_err(|_| VersionError::Overflow { at: start })?;
        core[i] = n;

        if i < 2 {
            if pos >= bytes.len() {
                return Err(VersionError::MissingCore { at: pos });
            }
            match bytes[pos] {
                b'.' => pos += 1,
                b'-' | b'+' => return Err(VersionError::MissingCore { at: pos }),
                _ => return Err(VersionError::InvalidChar { at: pos }),
            }
        } else if pos < bytes.len() && !matches!(bytes[pos], b'-' | b'+') {
            return Err(VersionError::InvalidChar { at: pos });
        }
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] == b'-' {
        pos += 1;
        let end = bytes[pos..]
            .iter()
            .position(|&b| b == b'+')
            .map(|i| pos + i)
            .unwrap_or(bytes.len());
        pre = parse_identifiers(s, pos, end, true)?;
        pos = end;
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        pos += 1;
        build = parse_identifiers(s, pos, bytes.len(), false)?;
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
    s: &str,
    start: usize,
    end: usize,
    prerelease: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut id_start = start;

    loop {
        let id_end = (id_start..end)
            .find(|&i| bytes[i] == b'.')
            .unwrap_or(end);

        if id_start == id_end {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }

        let mut numeric = true;
        for i in id_start..id_end {
            let b = bytes[i];
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(VersionError::InvalidChar { at: i });
            }
            if !b.is_ascii_digit() {
                numeric = false;
            }
        }

        if prerelease && numeric && id_end - id_start > 1 && bytes[id_start] == b'0' {
            return Err(VersionError::LeadingZero { at: id_start });
        }

        result.push(s[id_start..id_end].to_string());
        if id_end == end {
            break;
        }
        id_start = id_end + 1;
    }

    Ok(result)
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    let core = (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch));
    if core != Ordering::Equal {
        return core;
    }

    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (x, y) in a.pre.iter().zip(&b.pre) {
                let xn = x.bytes().all(|b| b.is_ascii_digit());
                let yn = y.bytes().all(|b| b.is_ascii_digit());
                let ord = match (xn, yn) {
                    (true, true) => x
                        .len()
                        .cmp(&y.len())
                        .then_with(|| x.as_bytes().cmp(y.as_bytes())),
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

#[derive(Clone, Debug, Default)]
struct Arm {
    clauses: Vec<Clause>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug)]
struct Clause {
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

impl Op {
    fn test(self, actual: Ordering) -> bool {
        match self {
            Op::Eq => actual == Ordering::Equal,
            Op::Lt => actual == Ordering::Less,
            Op::Le => actual != Ordering::Greater,
            Op::Gt => actual == Ordering::Greater,
            Op::Ge => actual != Ordering::Less,
        }
    }
}

#[derive(Clone, Debug)]
struct Partial {
    fields: [Option<u64>; 3],
    k: usize,
    full: Option<Version>,
}

impl Partial {
    fn lower(&self) -> Version {
        if let Some(v) = &self.full {
            return v.clone();
        }
        Version {
            major: self.fields[0].unwrap_or(0),
            minor: self.fields[1].unwrap_or(0),
            patch: self.fields[2].unwrap_or(0),
            pre: Vec::new(),
            build: Vec::new(),
        }
    }

    fn has_numeric(&self) -> bool {
        self.k > 0
    }
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn split_tokens(s: &str, base: usize) -> Vec<(String, usize)> {
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
        out.push((s[start..i].to_string(), base + start));
    }
    out
}

fn version_operand(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

fn parse_partial(s: &str, at: usize) -> Result<Partial, RangeError> {
    let bytes = s.as_bytes();
    let has_suffix = bytes.iter().any(|&b| b == b'-' || b == b'+');
    let fields_raw: Vec<&str> = s.split('.').collect();
    let has_wildcard = fields_raw
        .iter()
        .any(|f| matches!(*f, "*" | "x" | "X"));
    if has_suffix || (fields_raw.len() >= 3 && !has_wildcard) {
        return Ok(Partial {
            fields: [None, None, None],
            k: 3,
            full: Some(version_operand(s, at)?),
        });
    }

    if fields_raw.is_empty() || fields_raw.len() > 3 {
        return Err(RangeError::Syntax { at });
    }

    let mut fields = [None; 3];
    let mut wildcard_seen = false;
    let mut k = 0;
    for (i, f) in fields_raw.iter().enumerate() {
        if f.is_empty() {
            return Err(RangeError::Syntax { at });
        }
        if matches!(*f, "*" | "x" | "X") {
            wildcard_seen = true;
            continue;
        }
        if wildcard_seen
            || !f.bytes().all(|b| b.is_ascii_digit())
            || (f.len() > 1 && f.as_bytes()[0] == b'0')
        {
            return Err(RangeError::Syntax { at });
        }
        let n = f.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
        fields[i] = Some(n);
        k += 1;
    }

    Ok(Partial {
        fields,
        k,
        full: None,
    })
}

fn add_clause(arm: &mut Arm, op: Op, version: Version, explicit: bool) {
    if explicit && !version.pre.is_empty() {
        arm.anchors.push((version.major, version.minor, version.patch));
    }
    arm.clauses.push(Clause { op, version });
}

fn bump_upper(
    lower: &Version,
    field: usize,
    at: usize,
) -> Result<Version, RangeError> {
    let mut upper = Version {
        major: lower.major,
        minor: lower.minor,
        patch: lower.patch,
        pre: vec!["0".to_string()],
        build: Vec::new(),
    };
    let slot = match field {
        0 => &mut upper.major,
        1 => &mut upper.minor,
        _ => &mut upper.patch,
    };
    *slot = slot.checked_add(1).ok_or(RangeError::Syntax { at })?;
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

fn parse_expansion(
    arm: &mut Arm,
    token: &str,
    at: usize,
) -> Result<(), RangeError> {
    let prefix = token.as_bytes()[0];
    let operand = &token[1..];
    let operand_at = at + 1;
    let p = parse_partial(operand, operand_at)?;
    if !p.has_numeric() {
        return Err(RangeError::Syntax { at });
    }

    let lower = p.lower();
    if !lower.pre.is_empty() {
        arm.anchors.push((lower.major, lower.minor, lower.patch));
    }
    add_clause(arm, Op::Ge, lower.clone(), false);

    let field = match prefix {
        b'^' => {
            if lower.major > 0 || p.k == 1 {
                0
            } else if lower.minor > 0 || p.k == 2 {
                1
            } else {
                2
            }
        }
        b'~' => {
            if p.k == 1 {
                0
            } else {
                1
            }
        }
        _ => unreachable!(),
    };
    let upper = bump_upper(&lower, field, at)?;
    add_clause(arm, Op::Lt, upper, false);
    Ok(())
}

fn parse_hyphen_arm(tokens: &[(String, usize)], arm: &mut Arm) -> Result<(), RangeError> {
    let minus_at = tokens
        .iter()
        .find(|(t, _)| t == "-")
        .map(|(_, at)| *at)
        .unwrap();
    if tokens.len() != 3 || tokens[1].0 != "-" {
        return Err(RangeError::Syntax { at: minus_at });
    }
    let low = parse_partial(&tokens[0].0, tokens[0].1)?;
    let high = parse_partial(&tokens[2].0, tokens[2].1)?;
    if !low.has_numeric() {
        return Err(RangeError::Syntax { at: tokens[0].1 });
    }
    if !high.has_numeric() {
        return Err(RangeError::Syntax { at: tokens[2].1 });
    }

    let lower = low.lower();
    let upper_full = high.full.is_some();
    let upper = if upper_full {
        high.lower()
    } else if high.k == 1 {
        bump_upper(&high.lower(), 0, tokens[2].1)?
    } else if high.k == 2 {
        bump_upper(&high.lower(), 1, tokens[2].1)?
    } else {
        high.lower()
    };

    if !lower.pre.is_empty() {
        arm.anchors.push((lower.major, lower.minor, lower.patch));
    }
    add_clause(arm, Op::Ge, lower, false);

    if upper_full {
        if !upper.pre.is_empty() {
            arm.anchors.push((upper.major, upper.minor, upper.patch));
        }
        add_clause(arm, Op::Le, upper, false);
    } else {
        add_clause(arm, Op::Lt, upper, false);
    }
    Ok(())
}

fn parse_arm(s: &str, base: usize) -> Result<Arm, RangeError> {
    let tokens = split_tokens(s, base);
    let mut arm = Arm::default();
    if tokens.iter().any(|(t, _)| t == "-") {
        parse_hyphen_arm(&tokens, &mut arm)?;
        return Ok(arm);
    }

    for (token, at) in tokens {
        let b = token.as_bytes()[0];
        match b {
            b'^' | b'~' => parse_expansion(&mut arm, &token, at)?,
            b'<' | b'>' | b'=' => {
                let (op, len) = if token.starts_with("<=") {
                    (Op::Le, 2)
                } else if token.starts_with(">=") {
                    (Op::Ge, 2)
                } else {
                    (match b {
                        b'<' => Op::Lt,
                        b'>' => Op::Gt,
                        _ => Op::Eq,
                    }, 1)
                };
                let operand = &token[len..];
                let operand_at = at + len;
                if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
                    return Err(RangeError::Syntax { at });
                }
                let v = version_operand(operand, operand_at)?;
                add_clause(&mut arm, op, v, true);
            }
            b'*' | b'x' | b'X' | b'0'..=b'9' => {
                let p = parse_partial(&token, at)?;
                if p.full.is_some() {
                    let v = p.full.unwrap();
                    add_clause(&mut arm, Op::Eq, v, true);
                } else if p.k == 1 || p.k == 2 {
                    let lower = p.lower();
                    add_clause(&mut arm, Op::Ge, lower.clone(), false);
                    let upper = bump_upper(&lower, if p.k == 1 { 0 } else { 1 }, at)?;
                    add_clause(&mut arm, Op::Lt, upper, false);
                }
            }
            _ => return Err(RangeError::Syntax { at }),
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let separators = split_tokens(s, 0);
    if separators.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms = Vec::new();
    let mut cursor = 0;
    let mut previous_sep: Option<usize> = None;
    let mut idx = 0;

    while idx < separators.len() {
        if separators[idx].0 == "||" {
            return Err(RangeError::Empty { at: separators[idx].1 });
        }
        let start = separators[idx].1;
        let mut end_idx = idx;
        while end_idx < separators.len() && separators[end_idx].0 != "||" {
            end_idx += 1;
        }
        let end = if end_idx < separators.len() {
            separators[end_idx].1
        } else {
            s.len()
        };
        let arm_text = &s[start..end];
        arms.push(parse_arm(arm_text, start)?);
        if end_idx == separators.len() {
            break;
        }
        let sep_at = separators[end_idx].1;
        let after = end_idx + 1;
        if after >= separators.len() {
            return Err(RangeError::Empty { at: s.len() });
        }
        if separators[after].0 == "||" {
            return Err(RangeError::Empty { at: sep_at });
        }
        previous_sep = Some(sep_at);
        cursor = after;
        idx = cursor;
        let _ = previous_sep;
    }

    let _ = bytes;
    Ok(Range { arms })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        if !v.pre.is_empty()
            && !arm.anchors.iter().any(|&t| t == (v.major, v.minor, v.patch))
        {
            return false;
        }
        arm.clauses
            .iter()
            .all(|c| c.op.test(compare_versions(v, &c.version)))
    })
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
        let v = parse_version(candidate).map_err(|error| CandidateError { index, error })?;
        parsed.push(v);
    }

    let mut best: Option<usize> = None;
    for i in 0..candidates.len() {
        if !matches(&parsed[i], r) {
            continue;
        }
        match best {
            None => best = Some(i),
            Some(j) => {
                let ord = compare_versions(&parsed[i], &parsed[j]);
                if (greatest && ord == Ordering::Greater)
                    || (!greatest && ord == Ordering::Less)
                {
                    best = Some(i);
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