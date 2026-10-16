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

fn parse_core_field(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let start = *pos;
    let bytes = s.as_bytes();
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if *pos == start {
        if start == bytes.len() || matches!(bytes[start], b'.' | b'-' | b'+') {
            return Err(VersionError::MissingCore { at: start });
        }
        return Err(VersionError::InvalidChar { at: start });
    }
    let digits = &bytes[start..*pos];
    if digits.len() > 1 && digits[0] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    let text = std::str::from_utf8(digits).unwrap();
    text.parse::<u64>()
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
    let mut i = start;
    loop {
        if i == end || bytes[i] == b'.' {
            if i == id_start {
                return Err(VersionError::EmptyIdentifier { at: id_start });
            }
            if prerelease {
                let part = &bytes[id_start..i];
                if part.iter().all(u8::is_ascii_digit)
                    && part.len() > 1
                    && part[0] == b'0'
                {
                    return Err(VersionError::LeadingZero { at: id_start });
                }
            }
            out.push(s[id_start..i].to_string());
            if i == end {
                break;
            }
            id_start = i + 1;
        } else if !bytes[i].is_ascii_alphanumeric() && bytes[i] != b'-' {
            return Err(VersionError::InvalidChar { at: i });
        }
        i += 1;
    }
    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let mut pos = 0;
    let major = parse_core_field(s, &mut pos)?;
    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        if pos == s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;
    let minor = parse_core_field(s, &mut pos)?;
    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        if pos == s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;
    let patch = parse_core_field(s, &mut pos)?;

    let bytes = s.as_bytes();
    let mut pre = Vec::new();
    let mut build = Vec::new();
    if pos < s.len() {
        match bytes[pos] {
            b'-' => {
                let pre_start = pos + 1;
                let plus = bytes[pre_start..]
                    .iter()
                    .position(|&b| b == b'+')
                    .map(|n| pre_start + n)
                    .unwrap_or(s.len());
                pre = parse_identifiers(s, pre_start, plus, true)?;
                pos = plus;
                if pos < s.len() {
                    build = parse_identifiers(s, pos + 1, s.len(), false)?;
                }
            }
            b'+' => {
                build = parse_identifiers(s, pos + 1, s.len(), false)?;
            }
            _ => return Err(VersionError::InvalidChar { at: pos }),
        }
    }
    Ok(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn is_numeric(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    for (x, y) in a.iter().zip(b.iter()) {
        let xn = is_numeric(x);
        let yn = is_numeric(y);
        let ord = match (xn, yn) {
            (true, true) => x.len().cmp(&y.len()).then_with(|| x.as_bytes().cmp(y.as_bytes())),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => x.as_bytes().cmp(y.as_bytes()),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    a.len().cmp(&b.len())
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    core(a)
        .cmp(&core(b))
        .then_with(|| compare_pre(&a.pre, &b.pre))
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

#[derive(Clone, Debug)]
struct Partial {
    nums: Vec<u64>,
    full: Option<Version>,
    wildcard: bool,
}

#[derive(Clone, Copy)]
struct Token<'a> {
    text: &'a str,
    start: usize,
}

fn range_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokens<'a>(s: &'a str, base: usize) -> Vec<Token<'a>> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && range_ws(bytes[i]) {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !range_ws(bytes[i]) {
            i += 1;
        }
        out.push(Token {
            text: &s[start..i],
            start: base + start,
        });
    }
    out
}

fn syntax(at: usize) -> RangeError {
    RangeError::Syntax { at }
}

fn parse_partial(token: Token<'_>) -> Result<Partial, RangeError> {
    let s = token.text;
    if s.contains('-') || s.contains('+') {
        return parse_version(s)
            .map(|v| Partial {
                nums: vec![v.major, v.minor, v.patch],
                full: Some(v),
                wildcard: false,
            })
            .map_err(|error| RangeError::Version {
                at: token.start,
                error,
            });
    }
    let fields: Vec<&str> = s.split('.').collect();
    let has_wild = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));
    if fields.len() >= 3 && !has_wild {
        return parse_version(s)
            .map(|v| Partial {
                nums: vec![v.major, v.minor, v.patch],
                full: Some(v),
                wildcard: false,
            })
            .map_err(|error| RangeError::Version {
                at: token.start,
                error,
            });
    }
    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|f| f.is_empty()) {
        return Err(syntax(token.start));
    }
    let mut nums = Vec::new();
    let mut saw_wild = false;
    for field in fields {
        if matches!(field, "*" | "x" | "X") {
            saw_wild = true;
        } else {
            if saw_wild
                || !field.bytes().all(|b| b.is_ascii_digit())
                || (field.len() > 1 && field.starts_with('0'))
            {
                return Err(syntax(token.start));
            }
            let n = field.parse::<u64>().map_err(|_| syntax(token.start))?;
            nums.push(n);
        }
    }
    Ok(Partial {
        wildcard: nums.is_empty(),
        nums,
        full: None,
    })
}

fn padded(p: &Partial) -> (u64, u64, u64) {
    (
        *p.nums.first().unwrap_or(&0),
        *p.nums.get(1).unwrap_or(&0),
        *p.nums.get(2).unwrap_or(&0),
    )
}

fn version_at(tuple: (u64, u64, u64), pre: Vec<String>) -> Version {
    Version {
        major: tuple.0,
        minor: tuple.1,
        patch: tuple.2,
        pre,
        build: Vec::new(),
    }
}

fn add_check(arm: &mut Arm, op: Op, v: Version, anchor: bool) {
    if anchor && !v.pre.is_empty() {
        arm.anchors.push(core(&v));
    }
    arm.checks.push(Check { op, version: v });
}

fn increment_upper(
    token: Token<'_>,
    p: &Partial,
    kind: &str,
) -> Result<Version, RangeError> {
    let (mut major, mut minor, mut patch) = padded(p);
    let k = p.nums.len();
    let field = if kind == "caret" {
        if major > 0 || k == 1 {
            &mut major
        } else if minor > 0 || k == 2 {
            &mut minor
        } else {
            &mut patch
        }
    } else if k == 1 {
        &mut major
    } else {
        &mut minor
    };
    *field = field.checked_add(1).ok_or_else(|| syntax(token.start))?;
    if kind == "caret" {
        if major > 0 || (k == 1 && major == 0) {
            minor = 0;
            patch = 0;
        } else if minor > 0 || k == 2 {
            patch = 0;
        }
    } else if k == 1 {
        minor = 0;
        patch = 0;
    } else {
        patch = 0;
    }
    Ok(version_at((major, minor, patch), vec!["0".to_string()]))
}

fn add_partial_bare(arm: &mut Arm, token: Token<'_>) -> Result<(), RangeError> {
    let p = parse_partial(token)?;
    if let Some(v) = p.full {
        add_check(arm, Op::Eq, v, true);
    } else if !p.wildcard && p.nums.len() < 3 {
        let lower = version_at(padded(&p), Vec::new());
        let tuple = padded(&p);
        add_check(arm, Op::Ge, lower, false);
        let upper = if p.nums.len() == 1 {
            let n = tuple.0.checked_add(1).ok_or_else(|| syntax(token.start))?;
            version_at((n, 0, 0), vec!["0".to_string()])
        } else {
            let n = tuple.1.checked_add(1).ok_or_else(|| syntax(token.start))?;
            version_at((tuple.0, n, 0), vec!["0".to_string()])
        };
        add_check(arm, Op::Lt, upper, false);
    }
    Ok(())
}

fn add_caret_tilde(
    arm: &mut Arm,
    token: Token<'_>,
    caret: bool,
) -> Result<(), RangeError> {
    let operand = Token {
        text: &token.text[1..],
        start: token.start + 1,
    };
    let p = parse_partial(operand)?;
    if p.wildcard {
        return Err(syntax(token.start));
    }
    let lower = if let Some(v) = p.full.clone() {
        v
    } else {
        version_at(padded(&p), Vec::new())
    };
    add_check(arm, Op::Ge, lower, true);
    let upper = increment_upper(operand, &p, if caret { "caret" } else { "tilde" })?;
    add_check(arm, Op::Lt, upper, false);
    Ok(())
}

fn add_explicit(arm: &mut Arm, token: Token<'_>) -> Result<(), RangeError> {
    let bytes = token.text.as_bytes();
    let (op, n) = if bytes.starts_with(b">=") {
        (Op::Ge, 2)
    } else if bytes.starts_with(b"<=") {
        (Op::Le, 2)
    } else if bytes.starts_with(b">") {
        (Op::Gt, 1)
    } else if bytes.starts_with(b"<") {
        (Op::Lt, 1)
    } else {
        (Op::Eq, 1)
    };
    let operand = &token.text[n..];
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(syntax(token.start));
    }
    let v = parse_version(operand).map_err(|error| RangeError::Version {
        at: token.start + n,
        error,
    })?;
    add_check(arm, op, v, true);
    Ok(())
}

fn parse_hyphen_arm(arm: &mut Arm, ts: &[Token<'_>]) -> Result<(), RangeError> {
    let min = ts
        .iter()
        .filter(|t| t.text == "-")
        .map(|t| t.start)
        .min()
        .unwrap();
    if ts.len() != 3 || ts[1].text != "-" {
        return Err(syntax(min));
    }
    let lo = parse_partial(ts[0])?;
    let hi = parse_partial(ts[2])?;
    if lo.wildcard {
        return Err(syntax(ts[0].start));
    }
    if hi.wildcard {
        return Err(syntax(ts[2].start));
    }
    let lower = if let Some(v) = lo.full.clone() {
        v
    } else {
        version_at(padded(&lo), Vec::new())
    };
    add_check(arm, Op::Ge, lower, true);
    if let Some(v) = hi.full {
        add_check(arm, Op::Le, v, true);
    } else {
        let tuple = padded(&hi);
        let upper = if hi.nums.len() == 1 {
            version_at(
                (tuple.0.checked_add(1).ok_or_else(|| syntax(ts[2].start))?, 0, 0),
                vec!["0".to_string()],
            )
        } else {
            version_at(
                (
                    tuple.0,
                    tuple.1.checked_add(1).ok_or_else(|| syntax(ts[2].start))?,
                    0,
                ),
                vec!["0".to_string()],
            )
        };
        add_check(arm, Op::Lt, upper, false);
    }
    Ok(())
}

fn parse_arm(ts: &[Token<'_>]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        checks: Vec::new(),
        anchors: Vec::new(),
    };
    if ts.iter().any(|t| t.text == "-") {
        parse_hyphen_arm(&mut arm, ts)?;
        return Ok(arm);
    }
    for token in ts {
        let first = token.text.as_bytes().first().copied();
        match first {
            Some(b'^') => add_caret_tilde(&mut arm, *token, true)?,
            Some(b'~') => add_caret_tilde(&mut arm, *token, false)?,
            Some(b'<') | Some(b'>') | Some(b'=') => add_explicit(&mut arm, *token)?,
            Some(b'0'..=b'9') | Some(b'*') | Some(b'x') | Some(b'X') => {
                add_partial_bare(&mut arm, *token)?
            }
            _ => return Err(syntax(token.start)),
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut pieces: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && range_ws(bytes[i]) {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !range_ws(bytes[i]) {
            i += 1;
        }
        pieces.push((start, i));
    }
    if pieces.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms = Vec::new();
    let mut current = Vec::new();
    let mut previous_sep = None;
    for (start, end) in pieces {
        let part = &s[start..end];
        if part == "||" {
            if current.is_empty() {
                return Err(RangeError::Empty { at: start });
            }
            let ts = current;
            arms.push(parse_arm(&ts)?);
            current = Vec::new();
            previous_sep = Some(start);
        } else {
            let t = Token {
                text: part,
                start,
            };
            if part.contains("||") {
                // A glued separator remains part of its operand and is parsed normally.
            }
            current.push(t);
            previous_sep = None;
        }
    }
    if current.is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(&current)?);
    let _ = previous_sep;
    Ok(Range { arms })
}

fn check_arm(v: &Version, arm: &Arm) -> bool {
    if v.pre.len() > 0 && !arm.anchors.iter().any(|&c| c == core(v)) {
        return false;
    }
    arm.checks.iter().all(|c| {
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

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| check_arm(v, arm))
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
        match parse_version(candidate) {
            Ok(v) => parsed.push(v),
            Err(error) => return Err(CandidateError { index, error }),
        }
    }
    let mut best: Option<usize> = None;
    for (index, v) in parsed.iter().enumerate() {
        if !matches(v, r) {
            continue;
        }
        if let Some(prev) = best {
            let ord = compare_versions(v, &parsed[prev]);
            if (greatest && ord == Ordering::Greater) || (!greatest && ord == Ordering::Less) {
                best = Some(index);
            }
        } else {
            best = Some(index);
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