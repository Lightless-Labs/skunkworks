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

fn core_missing_byte(b: Option<u8>) -> bool {
    matches!(b, None | Some(b'.' | b'-' | b'+'))
}

fn parse_core(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if *pos == start {
        let b = bytes.get(start).copied();
        return Err(if core_missing_byte(b) {
            VersionError::MissingCore { at: start }
        } else {
            VersionError::InvalidChar { at: start }
        });
    }
    if *pos - start > 1 && bytes[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    let value = s[start..*pos]
        .parse::<u64>()
        .map_err(|_| VersionError::Overflow { at: start })?;
    Ok(value)
}

fn valid_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

fn parse_identifiers(s: &str, start: usize, end: usize, is_pre: bool) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut id_start = start;
    loop {
        let mut i = id_start;
        while i < end && bytes[i] != b'.' {
            if !valid_ident_byte(bytes[i]) {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }
        if i == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        if is_pre {
            let ident = &s[id_start..i];
            if ident.bytes().all(|b| b.is_ascii_digit())
                && ident.len() > 1
                && ident.as_bytes()[0] == b'0'
            {
                return Err(VersionError::LeadingZero { at: id_start });
            }
        }
        out.push(s[id_start..i].to_string());
        if i == end {
            break;
        }
        id_start = i + 1;
    }
    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let bytes = s.as_bytes();
    let mut pos = 0;
    let major = parse_core(s, &mut pos)?;
    if bytes.get(pos) != Some(&b'.') {
        return Err(if core_missing_byte(bytes.get(pos).copied()) {
            VersionError::MissingCore { at: pos }
        } else {
            VersionError::InvalidChar { at: pos }
        });
    }
    pos += 1;
    let minor = parse_core(s, &mut pos)?;
    if bytes.get(pos) != Some(&b'.') {
        return Err(if core_missing_byte(bytes.get(pos).copied()) {
            VersionError::MissingCore { at: pos }
        } else {
            VersionError::InvalidChar { at: pos }
        });
    }
    pos += 1;
    let patch = parse_core(s, &mut pos)?;

    let mut pre = Vec::new();
    let mut build = Vec::new();
    if pos < bytes.len() && bytes[pos] != b'-' && bytes[pos] != b'+' {
        return Err(VersionError::InvalidChar { at: pos });
    }
    if bytes.get(pos) == Some(&b'-') {
        let pre_start = pos + 1;
        let plus = bytes[pre_start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|n| pre_start + n)
            .unwrap_or(bytes.len());
        pre = parse_identifiers(s, pre_start, plus, true)?;
        pos = plus;
    }
    if bytes.get(pos) == Some(&b'+') {
        let build_start = pos + 1;
        build = parse_identifiers(s, build_start, bytes.len(), false)?;
        pos = bytes.len();
    }
    if pos != bytes.len() {
        return Err(VersionError::InvalidChar { at: pos });
    }
    Ok(Version { major, minor, patch, pre, build })
}

fn is_numeric_ident(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn cmp_numeric_ident(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch)
        .cmp(&(b.major, b.minor, b.patch))
        .then_with(|| {
            match (a.pre.is_empty(), b.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => {
                    for (x, y) in a.pre.iter().zip(&b.pre) {
                        let xn = is_numeric_ident(x);
                        let yn = is_numeric_ident(y);
                        let c = match (xn, yn) {
                            (true, true) => cmp_numeric_ident(x, y),
                            (true, false) => Ordering::Less,
                            (false, true) => Ordering::Greater,
                            (false, false) => x.as_bytes().cmp(y.as_bytes()),
                        };
                        if c != Ordering::Equal {
                            return c;
                        }
                    }
                    a.pre.len().cmp(&b.pre.len())
                }
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
struct Bound {
    version: Version,
    op: Op,
    explicit_pre: bool,
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
    bounds: Vec<Bound>,
    anchors: Vec<(u64, u64, u64)>,
}

fn whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn split_tokens<'a>(s: &'a str, base: usize) -> Vec<(&'a str, usize)> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && whitespace(bytes[i]) {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !whitespace(bytes[i]) {
            i += 1;
        }
        out.push((&s[start..i], base + start));
    }
    out
}

fn version_operand(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

#[derive(Clone, Debug)]
struct Partial {
    version: Version,
    k: usize,
    full: bool,
}

fn is_wild(s: &str) -> bool {
    matches!(s, "*" | "x" | "X")
}

fn partial_or_full(s: &str, at: usize) -> Result<Partial, RangeError> {
    let fields: Vec<&str> = s.split('.').collect();
    let full_rule = s.contains('-')
        || s.contains('+')
        || (fields.len() >= 3 && !fields.iter().any(|f| is_wild(f)));
    if full_rule {
        let v = version_operand(s, at)?;
        return Ok(Partial { version: v, k: 3, full: true });
    }
    if fields.len() > 3 || fields.iter().any(|f| f.is_empty()) {
        return Err(RangeError::Syntax { at });
    }
    let mut nums = Vec::new();
    let mut wildcard_seen = false;
    for field in fields {
        if is_wild(field) {
            wildcard_seen = true;
            continue;
        }
        if wildcard_seen
            || !field.bytes().all(|b| b.is_ascii_digit())
            || (field.len() > 1 && field.starts_with('0'))
            || field.parse::<u64>().is_err()
        {
            return Err(RangeError::Syntax { at });
        }
        nums.push(field.parse::<u64>().unwrap());
    }
    let k = nums.len();
    let mut vals = [0u64; 3];
    for (i, n) in nums.into_iter().enumerate() {
        vals[i] = n;
    }
    Ok(Partial {
        version: Version { major: vals[0], minor: vals[1], patch: vals[2], pre: Vec::new(), build: Vec::new() },
        k,
        full: false,
    })
}

fn endpoint_version(p: &Partial) -> Version {
    p.version.clone()
}

fn increment_upper(p: &Partial, kind: &str, at: usize) -> Result<Version, RangeError> {
    let mut v = Version { major: p.version.major, minor: p.version.minor, patch: p.version.patch, pre: vec!["0".to_string()], build: Vec::new() };
    let result = if kind == "bare" {
        if p.k == 1 {
            v.major = v.major.checked_add(1).ok_or(RangeError::Syntax { at })?;
            v.minor = 0; v.patch = 0;
        } else {
            v.minor = v.minor.checked_add(1).ok_or(RangeError::Syntax { at })?;
            v.patch = 0;
        }
        Ok(v)
    } else if kind == "tilde" {
        if p.k == 1 {
            v.major = v.major.checked_add(1).ok_or(RangeError::Syntax { at })?;
            v.minor = 0; v.patch = 0;
        } else {
            v.minor = v.minor.checked_add(1).ok_or(RangeError::Syntax { at })?;
            v.patch = 0;
        }
        Ok(v)
    } else {
        let bump_major = p.version.major > 0 || p.k == 1;
        let bump_minor = !bump_major && (p.version.minor > 0 || p.k == 2);
        if bump_major {
            v.major = v.major.checked_add(1).ok_or(RangeError::Syntax { at })?;
            v.minor = 0; v.patch = 0;
        } else if bump_minor {
            v.minor = v.minor.checked_add(1).ok_or(RangeError::Syntax { at })?;
            v.patch = 0;
        } else {
            v.patch = v.patch.checked_add(1).ok_or(RangeError::Syntax { at })?;
        }
        Ok(v)
    };
    result
}

fn push_bound(arm: &mut Arm, version: Version, op: Op, explicit_pre: bool) {
    if explicit_pre {
        arm.anchors.push((version.major, version.minor, version.patch));
    }
    arm.bounds.push(Bound { version, op, explicit_pre });
}

fn parse_full_comparator(token: &str, at: usize, arm: &mut Arm) -> Result<(), RangeError> {
    let (op, n) = if token.starts_with(">=") { (Op::Ge, 2) }
        else if token.starts_with("<=") { (Op::Le, 2) }
        else if token.starts_with('>') { (Op::Gt, 1) }
        else if token.starts_with('<') { (Op::Lt, 1) }
        else if token.starts_with('=') { (Op::Eq, 1) }
        else { return Err(RangeError::Syntax { at }); };
    let operand = &token[n..];
    let operand_at = at + n;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at });
    }
    let v = version_operand(operand, operand_at)?;
    let explicit_pre = !v.pre.is_empty();
    push_bound(arm, v, op, explicit_pre);
    Ok(())
}

fn parse_nonhyphen_arm(tokens: &[(&str, usize)]) -> Result<Arm, RangeError> {
    let mut arm = Arm { bounds: Vec::new(), anchors: Vec::new() };
    for &(token, at) in tokens {
        let first = token.as_bytes().first().copied();
        if matches!(first, Some(b'<' | b'>' | b'=')) {
            parse_full_comparator(token, at, &mut arm)?;
            continue;
        }
        if let Some(prefix) = token.strip_prefix('^') {
            let operand_at = at + 1;
            let p = partial_or_full(prefix, operand_at)?;
            if p.k == 0 {
                return Err(RangeError::Syntax { at });
            }
            let lower = endpoint_version(&p);
            let explicit = p.full && !lower.pre.is_empty();
            push_bound(&mut arm, lower.clone(), Op::Ge, explicit);
            let upper = increment_upper(&p, "caret", operand_at)?;
            push_bound(&mut arm, upper, Op::Lt, false);
            continue;
        }
        if let Some(prefix) = token.strip_prefix('~') {
            let operand_at = at + 1;
            let p = partial_or_full(prefix, operand_at)?;
            if p.k == 0 {
                return Err(RangeError::Syntax { at });
            }
            let lower = endpoint_version(&p);
            let explicit = p.full && !lower.pre.is_empty();
            push_bound(&mut arm, lower, Op::Ge, explicit);
            let upper = increment_upper(&p, "tilde", operand_at)?;
            push_bound(&mut arm, upper, Op::Lt, false);
            continue;
        }
        if matches!(first, Some(b'*' | b'x' | b'X')) || first.map_or(false, |b| b.is_ascii_digit()) {
            let p = partial_or_full(token, at)?;
            if p.full {
                let explicit = !p.version.pre.is_empty();
                push_bound(&mut arm, p.version, Op::Eq, explicit);
            } else if p.k > 0 {
                let lower = endpoint_version(&p);
                push_bound(&mut arm, lower, Op::Ge, false);
                let upper = increment_upper(&p, "bare", at)?;
                push_bound(&mut arm, upper, Op::Lt, false);
            }
            continue;
        }
        return Err(RangeError::Syntax { at });
    }
    Ok(arm)
}

fn parse_hyphen_arm(tokens: &[(&str, usize)], minus_index: usize) -> Result<Arm, RangeError> {
    if tokens.len() != 3 || tokens[1].0 != "-" {
        return Err(RangeError::Syntax { at: tokens[minus_index].1 });
    }
    let (lo_s, lo_at) = tokens[0];
    let (hi_s, hi_at) = tokens[2];
    let lo = partial_or_full(lo_s, lo_at)?;
    let hi = partial_or_full(hi_s, hi_at)?;
    if lo.k == 0 {
        return Err(RangeError::Syntax { at: lo_at });
    }
    if hi.k == 0 {
        return Err(RangeError::Syntax { at: hi_at });
    }
    let mut arm = Arm { bounds: Vec::new(), anchors: Vec::new() };
    let lowerv = endpoint_version(&lo);
    push_bound(&mut arm, lowerv.clone(), Op::Ge, lo.full && !lowerv.pre.is_empty());
    if hi.full {
        let upper = endpoint_version(&hi);
        push_bound(&mut arm, upper.clone(), Op::Le, !upper.pre.is_empty());
    } else {
        let upper = increment_upper(&hi, "bare", hi_at)?;
        push_bound(&mut arm, upper, Op::Lt, false);
    }
    Ok(arm)
}

fn parse_arm(s: &str, base: usize) -> Result<Arm, RangeError> {
    let tokens = split_tokens(s, base);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: base });
    }
    if let Some(i) = tokens.iter().position(|(t, _)| *t == "-") {
        return parse_hyphen_arm(&tokens, i);
    }
    parse_nonhyphen_arm(&tokens)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'|' && bytes.get(i + 1) == Some(&b'|') {
            let sep_start = i;
            let left = &s[start..sep_start];
            if split_tokens(left, start).is_empty() {
                return Err(RangeError::Empty { at: sep_start });
            }
            arms.push(parse_arm(left, start)?);
            i += 2;
            while i < bytes.len() && whitespace(bytes[i]) {
                i += 1;
            }
            start = i;
            continue;
        }
        i += 1;
    }
    let tail = &s[start..];
    if split_tokens(tail, start).is_empty() {
        if arms.is_empty() {
            return Err(RangeError::Empty { at: 0 });
        }
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(tail, start)?);
    Ok(Range { arms })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        if !v.pre.is_empty()
            && !arm.anchors.iter().any(|&(a, b, c)| (a, b, c) == (v.major, v.minor, v.patch))
        {
            return false;
        }
        arm.bounds.iter().all(|bound| {
            let cmp = compare_versions(v, &bound.version);
            match bound.op {
                Op::Eq => cmp == Ordering::Equal,
                Op::Lt => cmp == Ordering::Less,
                Op::Le => cmp != Ordering::Greater,
                Op::Gt => cmp == Ordering::Greater,
                Op::Ge => cmp != Ordering::Less,
            }
        })
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn satisfying_extreme(candidates: &[String], r: &Range, want_max: bool) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        match parse_version(candidate) {
            Ok(version) => parsed.push(version),
            Err(error) => return Err(CandidateError { index, error }),
        }
    }
    let mut best: Option<usize> = None;
    for (i, version) in parsed.iter().enumerate() {
        if !matches(version, r) {
            continue;
        }
        if let Some(current) = best {
            let cmp = compare_versions(version, &parsed[current]);
            if (want_max && cmp == Ordering::Greater) || (!want_max && cmp == Ordering::Less) {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

pub fn max_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    satisfying_extreme(candidates, r, true)
}

pub fn min_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    satisfying_extreme(candidates, r, false)
}

fn main() {}