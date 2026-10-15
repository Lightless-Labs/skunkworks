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

fn core_error(s: &str, at: usize) -> VersionError {
    match s.as_bytes().get(at) {
        None | Some(b'.' | b'-' | b'+') => VersionError::MissingCore { at },
        _ => VersionError::InvalidChar { at },
    }
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let bytes = s.as_bytes();
    let mut pos = 0;
    let mut values = [0u64; 3];

    for field in 0..3 {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos == start {
            return Err(core_error(s, start));
        }
        if pos - start > 1 && bytes[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }
        let mut value = 0u64;
        for &b in &bytes[start..pos] {
            value = value
                .checked_mul(10)
                .and_then(|v| v.checked_add((b - b'0') as u64))
                .ok_or(VersionError::Overflow { at: start })?;
        }
        values[field] = value;

        if field < 2 {
            if pos >= bytes.len() || bytes[pos] != b'.' {
                return Err(core_error(s, pos));
            }
            pos += 1;
        }
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] == b'-' {
        pos += 1;
        let pre_start = pos;
        let plus = bytes[pos..]
            .iter()
            .position(|&b| b == b'+')
            .map(|i| pos + i)
            .unwrap_or(bytes.len());
        parse_identifiers(s, pos, plus, true, &mut pre)?;
        pos = plus;
        if pos == pre_start {
            return Err(VersionError::EmptyIdentifier { at: pos });
        }
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        pos += 1;
        parse_identifiers(s, pos, bytes.len(), false, &mut build)?;
        pos = bytes.len();
    }

    if pos != bytes.len() {
        return Err(VersionError::InvalidChar { at: pos });
    }

    Ok(Version {
        major: values[0],
        minor: values[1],
        patch: values[2],
        pre,
        build,
    })
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    prerelease: bool,
    output: &mut Vec<String>,
) -> Result<(), VersionError> {
    let bytes = s.as_bytes();
    let mut at = start;
    loop {
        let next = bytes[at..end]
            .iter()
            .position(|&b| b == b'.')
            .map(|i| at + i)
            .unwrap_or(end);
        if next == at {
            return Err(VersionError::EmptyIdentifier { at });
        }
        for i in at..next {
            let b = bytes[i];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: i });
            }
        }
        let numeric = bytes[at..next].iter().all(u8::is_ascii_digit);
        if prerelease && numeric && next - at > 1 && bytes[at] == b'0' {
            return Err(VersionError::LeadingZero { at });
        }
        output.push(s[at..next].to_string());
        if next == end {
            break;
        }
        at = next + 1;
        if at == end {
            return Err(VersionError::EmptyIdentifier { at });
        }
    }
    Ok(())
}

fn numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn compare_numeric_identifiers(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch)
        .cmp(&(b.major, b.minor, b.patch))
        .then_with(|| {
            if a.pre.is_empty() {
                return if b.pre.is_empty() {
                    Ordering::Equal
                } else {
                    Ordering::Greater
                };
            }
            if b.pre.is_empty() {
                return Ordering::Less;
            }
            for (x, y) in a.pre.iter().zip(&b.pre) {
                let xn = numeric_identifier(x);
                let yn = numeric_identifier(y);
                let order = match (xn, yn) {
                    (true, true) => compare_numeric_identifiers(x, y),
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    (false, false) => x.as_bytes().cmp(y.as_bytes()),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
            a.pre.len().cmp(&b.pre.len())
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
struct Arm {
    checks: Vec<Check>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug)]
struct Check {
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

impl Check {
    fn matches(&self, candidate: &Version) -> bool {
        let ordering = compare_versions(candidate, &self.version);
        match self.op {
            Op::Eq => ordering == Ordering::Equal,
            Op::Lt => ordering == Ordering::Less,
            Op::Le => ordering != Ordering::Greater,
            Op::Gt => ordering == Ordering::Greater,
            Op::Ge => ordering != Ordering::Less,
        }
    }
}

#[derive(Clone, Debug)]
struct Partial {
    fields: Vec<u64>,
    count: usize,
    full: Option<Version>,
}

impl Partial {
    fn padded(&self) -> Version {
        if let Some(v) = &self.full {
            return v.clone();
        }
        Version {
            major: *self.fields.first().unwrap_or(&0),
            minor: *self.fields.get(1).unwrap_or(&0),
            patch: *self.fields.get(2).unwrap_or(&0),
            pre: Vec::new(),
            build: Vec::new(),
        }
    }
}

fn range_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokenize(s: &str, base: usize) -> Vec<(String, usize)> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && range_space(bytes[i]) {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !range_space(bytes[i]) {
            i += 1;
        }
        result.push((s[start..i].to_string(), base + start));
    }
    result
}

fn parse_full_operand(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

fn syntax(at: usize) -> RangeError {
    RangeError::Syntax { at }
}

fn is_wildcard(s: &str) -> bool {
    matches!(s, "*" | "x" | "X")
}

fn partial_operand(s: &str, at: usize) -> Result<Partial, RangeError> {
    if s.contains('-') || s.contains('+') {
        return Ok(Partial {
            fields: Vec::new(),
            count: 3,
            full: Some(parse_full_operand(s, at)?),
        });
    }
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() >= 3 && parts.iter().all(|p| !is_wildcard(p)) {
        return Ok(Partial {
            fields: Vec::new(),
            count: 3,
            full: Some(parse_full_operand(s, at)?),
        });
    }
    if parts.is_empty() || parts.len() > 3 {
        return Err(syntax(at));
    }
    let mut fields = Vec::new();
    let mut wild = false;
    for p in &parts {
        if p.is_empty() {
            return Err(syntax(at));
        }
        if is_wildcard(p) {
            wild = true;
            continue;
        }
        if wild
            || !p.bytes().all(|b| b.is_ascii_digit())
            || (p.len() > 1 && p.starts_with('0'))
        {
            return Err(syntax(at));
        }
        let mut n = 0u64;
        for b in p.bytes() {
            n = n
                .checked_mul(10)
                .and_then(|v| v.checked_add((b - b'0') as u64))
                .ok_or_else(|| syntax(at))?;
        }
        fields.push(n);
    }
    let count = fields.len();
    Ok(Partial {
        fields,
        count,
        full: None,
    })
}

fn bump(v: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let mut upper = Version {
        major: v.major,
        minor: v.minor,
        patch: v.patch,
        pre: vec!["0".to_string()],
        build: Vec::new(),
    };
    match field {
        0 => {
            upper.major = upper.major.checked_add(1).ok_or_else(|| syntax(at))?;
            upper.minor = 0;
            upper.patch = 0;
        }
        1 => {
            upper.minor = upper.minor.checked_add(1).ok_or_else(|| syntax(at))?;
            upper.patch = 0;
        }
        _ => upper.patch = upper.patch.checked_add(1).ok_or_else(|| syntax(at))?,
    }
    Ok(upper)
}

fn add_check(arm: &mut Arm, op: Op, version: Version, anchor: bool) {
    if anchor && !version.pre.is_empty() {
        arm.anchors
            .push((version.major, version.minor, version.patch));
    }
    arm.checks.push(Check { op, version });
}

fn expand_bare(arm: &mut Arm, token: &str, at: usize) -> Result<(), RangeError> {
    let p = partial_operand(token, at)?;
    if let Some(v) = p.full {
        add_check(arm, Op::Eq, v, true);
        return Ok(());
    }
    if p.count == 0 {
        return Ok(());
    }
    let lower = p.padded();
    add_check(arm, Op::Ge, lower.clone(), true);
    if p.count < 3 {
        let upper = bump(&lower, if p.count == 1 { 0 } else { 1 }, at)?;
        add_check(arm, Op::Lt, upper, false);
    }
    Ok(())
}

fn expand_caret_tilde(
    arm: &mut Arm,
    token: &str,
    at: usize,
    caret: bool,
) -> Result<(), RangeError> {
    let operand = &token[1..];
    let op_at = at + 1;
    let p = partial_operand(operand, op_at)?;
    if p.count == 0 && p.full.is_none() {
        return Err(syntax(at));
    }
    let lower = p.padded();
    add_check(arm, Op::Ge, lower.clone(), true);

    let bound_field = if caret {
        if lower.major > 0 || p.count == 1 {
            0
        } else if lower.minor > 0 || p.count == 2 {
            1
        } else {
            2
        }
    } else if p.count == 1 {
        0
    } else {
        1
    };
    add_check(arm, Op::Lt, bump(&lower, bound_field, op_at)?, false);
    Ok(())
}

fn parse_comparator(token: &str, at: usize) -> Result<Check, RangeError> {
    let (op, len) = if token.starts_with(">=") {
        (Op::Ge, 2)
    } else if token.starts_with("<=") {
        (Op::Le, 2)
    } else if token.starts_with('>') {
        (Op::Gt, 1)
    } else if token.starts_with('<') {
        (Op::Lt, 1)
    } else if token.starts_with('=') {
        (Op::Eq, 1)
    } else {
        unreachable!()
    };
    let operand = &token[len..];
    let operand_at = at + len;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(syntax(at));
    }
    Ok(Check {
        op,
        version: parse_full_operand(operand, operand_at)?,
    })
}

fn parse_hyphen_arm(tokens: &[(String, usize)]) -> Result<Arm, RangeError> {
    let first_minus = tokens
        .iter()
        .find(|(s, _)| s == "-")
        .map(|(_, at)| *at)
        .unwrap();
    if tokens.len() != 3 || tokens[1].0 != "-" {
        return Err(syntax(first_minus));
    }
    let lower = partial_operand(&tokens[0].0, tokens[0].1)?;
    let upper = partial_operand(&tokens[2].0, tokens[2].1)?;
    if lower.count == 0 {
        return Err(syntax(tokens[0].1));
    }
    if upper.count == 0 {
        return Err(syntax(tokens[2].1));
    }
    let mut arm = Arm {
        checks: Vec::new(),
        anchors: Vec::new(),
    };
    let lo = lower.padded();
    add_check(&mut arm, Op::Ge, lo, true);

    if let Some(v) = upper.full {
        add_check(&mut arm, Op::Le, v, true);
    } else {
        let hi = upper.padded();
        if upper.count == 3 {
            add_check(&mut arm, Op::Le, hi, false);
        } else {
            let bound = bump(&hi, if upper.count == 1 { 0 } else { 1 }, tokens[2].1)?;
            add_check(&mut arm, Op::Lt, bound, false);
        }
    }
    Ok(arm)
}

fn parse_arm(tokens: &[(String, usize)]) -> Result<Arm, RangeError> {
    if tokens.iter().any(|(s, _)| s == "-") {
        return parse_hyphen_arm(tokens);
    }
    let mut arm = Arm {
        checks: Vec::new(),
        anchors: Vec::new(),
    };
    for (token, at) in tokens {
        if token.starts_with('>') || token.starts_with('<') || token.starts_with('=') {
            let c = parse_comparator(token, *at)?;
            add_check(&mut arm, c.op, c.version, true);
        } else if token.starts_with('^') {
            expand_caret_tilde(&mut arm, token, *at, true)?;
        } else if token.starts_with('~') {
            expand_caret_tilde(&mut arm, token, *at, false)?;
        } else if token.starts_with('*')
            || token.starts_with('x')
            || token.starts_with('X')
            || token.as_bytes().first().is_some_and(u8::is_ascii_digit)
        {
            expand_bare(&mut arm, token, *at)?;
        } else {
            return Err(syntax(*at));
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let tokens = tokenize(s, 0);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    let mut segments: Vec<Vec<(String, usize)>> = vec![Vec::new()];
    let mut separators = Vec::new();
    for token in tokens {
        if token.0 == "||" {
            separators.push(token.1);
            segments.push(Vec::new());
        } else {
            segments.last_mut().unwrap().push(token);
        }
    }

    let mut arms = Vec::new();
    for (i, segment) in segments.iter().enumerate() {
        if segment.is_empty() {
            let at = if i < separators.len() {
                separators[i]
            } else {
                s.len()
            };
            return Err(RangeError::Empty { at });
        }
        arms.push(parse_arm(segment)?);
    }
    Ok(Range { arms })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        arm.checks.iter().all(|c| c.matches(v))
            && (v.pre.is_empty()
                || arm
                    .anchors
                    .iter()
                    .any(|&(major, minor, patch)| {
                        (v.major, v.minor, v.patch) == (major, minor, patch)
                    }))
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
        if let Some(current) = best {
            let ordering = compare_versions(version, &parsed[current]);
            if (greatest && ordering == Ordering::Greater)
                || (!greatest && ordering == Ordering::Less)
            {
                best = Some(index);
            }
        } else {
            best = Some(index);
        }
    }
    Ok(best.map(|index| candidates[index].clone()))
}

fn main() {}