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
    let b = s.as_bytes();
    if b.is_empty() {
        return Err(VersionError::Empty);
    }

    let mut pos = 0;
    let mut core = [0u64; 3];

    for (i, slot) in core.iter_mut().enumerate() {
        let start = pos;
        while pos < b.len() && b[pos].is_ascii_digit() {
            pos += 1;
        }
        if start == pos {
            return Err(if pos == b.len() || matches!(b[pos], b'.' | b'-' | b'+') {
                VersionError::MissingCore { at: pos }
            } else {
                VersionError::InvalidChar { at: pos }
            });
        }
        if pos - start > 1 && b[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }
        *slot = s[start..pos]
            .parse::<u64>()
            .map_err(|_| VersionError::Overflow { at: start })?;

        if i < 2 {
            if pos < b.len() && b[pos] == b'.' {
                pos += 1;
            } else {
                return Err(if pos == b.len() || matches!(b[pos], b'-' | b'+') {
                    VersionError::MissingCore { at: pos }
                } else {
                    VersionError::InvalidChar { at: pos }
                });
            }
        }
    }

    let (pre, build);
    if pos == b.len() {
        pre = Vec::new();
        build = Vec::new();
    } else if b[pos] == b'-' {
        pos += 1;
        let start = pos;
        let plus = b[pos..].iter().position(|&x| x == b'+').map(|i| pos + i);
        let end = plus.unwrap_or(b.len());
        pre = parse_identifiers(s, start, end, true)?;
        pos = end;
        if pos < b.len() {
            pos += 1;
            build = parse_identifiers(s, pos, b.len(), false)?;
        } else {
            build = Vec::new();
        }
    } else if b[pos] == b'+' {
        pos += 1;
        pre = Vec::new();
        build = parse_identifiers(s, pos, b.len(), false)?;
    } else {
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
    check_pre_numeric: bool,
) -> Result<Vec<String>, VersionError> {
    let b = s.as_bytes();
    if start == end {
        return Err(VersionError::EmptyIdentifier { at: start });
    }
    let mut out = Vec::new();
    let mut id_start = start;
    for i in start..=end {
        if i == end || b[i] == b'.' {
            if i == id_start {
                return Err(VersionError::EmptyIdentifier { at: id_start });
            }
            for j in id_start..i {
                if !b[j].is_ascii_alphanumeric() && b[j] != b'-' {
                    return Err(VersionError::InvalidChar { at: j });
                }
            }
            if check_pre_numeric
                && b[id_start..i].iter().all(u8::is_ascii_digit)
                && i - id_start > 1
                && b[id_start] == b'0'
            {
                return Err(VersionError::LeadingZero { at: id_start });
            }
            out.push(s[id_start..i].to_string());
            id_start = i + 1;
        } else if !b[i].is_ascii_alphanumeric() && b[i] != b'-' {
            return Err(VersionError::InvalidChar { at: i });
        }
    }
    Ok(out)
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

    for (x, y) in a.iter().zip(b) {
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
    (a.major, a.minor, a.patch)
        .cmp(&(b.major, b.minor, b.patch))
        .then_with(|| compare_pre(&a.pre, &b.pre))
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

#[derive(Clone, Debug)]
enum Op {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug)]
struct Constraint {
    op: Op,
    version: Version,
    anchor: bool,
}

#[derive(Clone, Copy)]
struct Token<'a> {
    text: &'a str,
    start: usize,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokens(s: &str) -> Vec<Token<'_>> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && is_ws(b[i]) {
            i += 1;
        }
        if i == b.len() {
            break;
        }
        let start = i;
        while i < b.len() && !is_ws(b[i]) {
            i += 1;
        }
        out.push(Token { text: &s[start..i], start });
    }
    out
}

fn core_eq(a: &Version, b: &Version) -> bool {
    (a.major, a.minor, a.patch) == (b.major, b.minor, b.patch)
}

fn full_operand(token: Token<'_>, offset: usize) -> Result<Version, RangeError> {
    parse_version(token.text)
        .map_err(|error| RangeError::Version { at: token.start + offset, error })
}

#[derive(Clone, Debug)]
struct Partial {
    nums: Vec<u64>,
    full: Option<Version>,
}

impl Partial {
    fn k(&self) -> usize {
        self.nums.len()
    }

    fn padded(&self) -> Version {
        Version {
            major: *self.nums.first().unwrap_or(&0),
            minor: *self.nums.get(1).unwrap_or(&0),
            patch: *self.nums.get(2).unwrap_or(&0),
            pre: self.full.as_ref().map(|v| v.pre.clone()).unwrap_or_default(),
            build: self.full.as_ref().map(|v| v.build.clone()).unwrap_or_default(),
        }
    }
}

fn is_wild(s: &str) -> bool {
    matches!(s, "*" | "x" | "X")
}

fn parse_partial(token: Token<'_>, value: &str, value_offset: usize) -> Result<Partial, RangeError> {
    if value.is_empty() {
        return Err(RangeError::Syntax { at: token.start });
    }
    let fields: Vec<&str> = value.split('.').collect();
    let has_suffix = value.contains('-') || value.contains('+');
    let no_wild = !fields.iter().any(|f| is_wild(f));
    if has_suffix || (fields.len() >= 3 && no_wild) {
        let v = full_operand(
            Token { text: value, start: token.start + value_offset },
            0,
        )?;
        return Ok(Partial {
            nums: vec![v.major, v.minor, v.patch],
            full: Some(v),
        });
    }
    if fields.len() > 3 {
        return Err(RangeError::Syntax { at: token.start });
    }
    let mut nums = Vec::new();
    let mut wild_seen = false;
    for f in fields {
        if f.is_empty() {
            return Err(RangeError::Syntax { at: token.start });
        }
        if is_wild(f) {
            wild_seen = true;
            continue;
        }
        if wild_seen || !f.bytes().all(|b| b.is_ascii_digit()) {
            return Err(RangeError::Syntax { at: token.start });
        }
        if f.len() > 1 && f.starts_with('0') {
            return Err(RangeError::Syntax { at: token.start });
        }
        let n = f.parse::<u64>().map_err(|_| RangeError::Syntax { at: token.start })?;
        nums.push(n);
    }
    Ok(Partial { nums, full: None })
}

fn bump(v: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let mut out = Version {
        major: v.major,
        minor: v.minor,
        patch: v.patch,
        pre: vec!["0".to_string()],
        build: Vec::new(),
    };
    let target = match field {
        0 => &mut out.major,
        1 => &mut out.minor,
        _ => &mut out.patch,
    };
    *target = target.checked_add(1).ok_or(RangeError::Syntax { at })?;
    if field == 0 {
        out.minor = 0;
        out.patch = 0;
    } else if field == 1 {
        out.patch = 0;
    }
    Ok(out)
}

fn add(arm: &mut Vec<Constraint>, op: Op, version: Version, anchor: bool) {
    arm.push(Constraint { op, version, anchor });
}

fn bare_expansion(token: Token<'_>, arm: &mut Vec<Constraint>) -> Result<(), RangeError> {
    let p = parse_partial(token, token.text, 0)?;
    if let Some(full) = p.full {
        add(arm, Op::Eq, full.clone(), !full.pre.is_empty());
        return Ok(());
    }
    let k = p.k();
    let lower = p.padded();
    match k {
        0 => Ok(()),
        1 | 2 => {
            add(arm, Op::Ge, lower.clone(), false);
            let upper = bump(&lower, if k == 1 { 0 } else { 1 }, token.start)?;
            add(arm, Op::Lt, upper, false);
            Ok(())
        }
        _ => {
            add(arm, Op::Eq, lower, false);
            Ok(())
        }
    }
}

fn expand_caret_tilde(
    token: Token<'_>,
    value: &str,
    offset: usize,
    caret: bool,
    arm: &mut Vec<Constraint>,
) -> Result<(), RangeError> {
    let p = parse_partial(token, value, offset)?;
    let k = p.k();
    if k == 0 {
        return Err(RangeError::Syntax { at: token.start });
    }
    let lower = p.padded();
    let anchor = !lower.pre.is_empty();
    add(arm, Op::Ge, lower.clone(), anchor);
    let field = if caret {
        if lower.major > 0 || k == 1 { 0 }
        else if lower.minor > 0 || k == 2 { 1 }
        else { 2 }
    } else if k == 1 {
        0
    } else {
        1
    };
    let upper = bump(&lower, field, token.start)?;
    add(arm, Op::Lt, upper, false);
    Ok(())
}

fn comparator(token: Token<'_>, op_len: usize, op: Op, arm: &mut Vec<Constraint>) -> Result<(), RangeError> {
    let operand = &token.text[op_len..];
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: token.start });
    }
    let v = full_operand(
        Token { text: operand, start: token.start + op_len },
        0,
    )?;
    let anchor = !v.pre.is_empty();
    add(arm, op, v, anchor);
    Ok(())
}

fn hyphen_arm(ts: &[Token<'_>], arm: &mut Vec<Constraint>) -> Result<(), RangeError> {
    let minus = ts.iter().find(|t| t.text == "-").unwrap();
    if ts.len() != 3 || ts[1].text != "-" {
        return Err(RangeError::Syntax { at: minus.start });
    }
    let lower = parse_partial(ts[0], ts[0].text, 0)?;
    let upper = parse_partial(ts[2], ts[2].text, 0)?;
    if lower.k() == 0 {
        return Err(RangeError::Syntax { at: ts[0].start });
    }
    if upper.k() == 0 {
        return Err(RangeError::Syntax { at: ts[2].start });
    }

    let low = lower.padded();
    add(arm, Op::Ge, low.clone(), lower.full.as_ref().is_some_and(|v| !v.pre.is_empty()));

    if let Some(v) = upper.full {
        add(arm, Op::Le, v.clone(), !v.pre.is_empty());
    } else {
        let high = upper.padded();
        if upper.k() == 3 {
            add(arm, Op::Le, high, false);
        } else {
            let bound = bump(&high, if upper.k() == 1 { 0 } else { 1 }, ts[2].start)?;
            add(arm, Op::Lt, bound, false);
        }
    }
    Ok(())
}

fn parse_arm(ts: &[Token<'_>]) -> Result<Vec<Constraint>, RangeError> {
    let mut arm = Vec::new();
    if ts.iter().any(|t| t.text == "-") {
        return hyphen_arm(ts, &mut arm).map(|_| arm);
    }
    for &t in ts {
        let b = t.text.as_bytes();
        match b[0] {
            b'<' => {
                if t.text.starts_with("<=") {
                    comparator(t, 2, Op::Le, &mut arm)?;
                } else {
                    comparator(t, 1, Op::Lt, &mut arm)?;
                }
            }
            b'>' => {
                if t.text.starts_with(">=") {
                    comparator(t, 2, Op::Ge, &mut arm)?;
                } else {
                    comparator(t, 1, Op::Gt, &mut arm)?;
                }
            }
            b'=' => comparator(t, 1, Op::Eq, &mut arm)?,
            b'^' => expand_caret_tilde(t, &t.text[1..], 1, true, &mut arm)?,
            b'~' => expand_caret_tilde(t, &t.text[1..], 1, false, &mut arm)?,
            b'0'..=b'9' | b'*' | b'x' | b'X' => bare_expansion(t, &mut arm)?,
            _ => return Err(RangeError::Syntax { at: t.start }),
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let ts = tokens(s);
    if ts.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    let mut arms = Vec::new();
    let mut arm_tokens: Vec<Token<'_>> = Vec::new();
    let mut i = 0;
    while i < ts.len() {
        if ts[i].text == "||" {
            if arm_tokens.is_empty() {
                return Err(RangeError::Empty { at: ts[i].start });
            }
            arms.push(parse_arm(&arm_tokens)?);
            arm_tokens.clear();
            i += 1;
            if i == ts.len() {
                return Err(RangeError::Empty { at: s.len() });
            }
        } else {
            arm_tokens.push(ts[i]);
            i += 1;
        }
    }
    if arm_tokens.is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(&arm_tokens)?);
    Ok(Range { arms })
}

fn check_constraint(v: &Version, c: &Constraint) -> bool {
    let ord = compare_versions(v, &c.version);
    match c.op {
        Op::Eq => ord == Ordering::Equal,
        Op::Lt => ord == Ordering::Less,
        Op::Le => ord != Ordering::Greater,
        Op::Gt => ord == Ordering::Greater,
        Op::Ge => ord != Ordering::Less,
    }
}

fn arm_matches(v: &Version, arm: &[Constraint]) -> bool {
    if !v.pre.is_empty()
        && !arm.iter().any(|c| c.anchor && core_eq(v, &c.version))
    {
        return false;
    }
    arm.iter().all(|c| check_constraint(v, c))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| arm_matches(v, arm))
}

fn satisfying<'a>(
    candidates: &'a [String],
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
    for (i, v) in parsed.iter().enumerate() {
        if !matches(v, r) {
            continue;
        }
        match best {
            None => best = Some(i),
            Some(j) => {
                let ord = compare_versions(v, &parsed[j]);
                if (greatest && ord == Ordering::Greater) || (!greatest && ord == Ordering::Less) {
                    best = Some(i);
                }
            }
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

pub fn max_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    satisfying(candidates, r, true)
}

pub fn min_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    satisfying(candidates, r, false)
}