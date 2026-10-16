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

fn parse_core_field(bytes: &[u8], pos: &mut usize) -> Result<u64, VersionError> {
    let start = *pos;
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if *pos == start {
        if *pos == bytes.len() || matches!(bytes[*pos], b'.' | b'-' | b'+') {
            return Err(VersionError::MissingCore { at: start });
        }
        return Err(VersionError::InvalidChar { at: start });
    }
    if *pos - start > 1 && bytes[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    let mut value = 0u64;
    for &d in &bytes[start..*pos] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((d - b'0') as u64))
            .ok_or(VersionError::Overflow { at: start })?;
    }
    Ok(value)
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    prerelease: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut p = start;
    loop {
        let id_start = p;
        while p < end && bytes[p] != b'.' {
            let b = bytes[p];
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(VersionError::InvalidChar { at: p });
            }
            p += 1;
        }
        if p == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        let id = &s[id_start..p];
        if prerelease
            && id.bytes().all(|b| b.is_ascii_digit())
            && id.len() > 1
            && id.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        result.push(id.to_string());
        if p == end {
            break;
        }
        p += 1;
        if p == end {
            return Err(VersionError::EmptyIdentifier { at: p });
        }
    }
    Ok(result)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let bytes = s.as_bytes();
    let mut p = 0;
    let major = parse_core_field(bytes, &mut p)?;
    if p >= bytes.len() || bytes[p] != b'.' {
        if p >= bytes.len() || matches!(bytes[p], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: p });
        }
        return Err(VersionError::InvalidChar { at: p });
    }
    p += 1;
    let minor = parse_core_field(bytes, &mut p)?;
    if p >= bytes.len() || bytes[p] != b'.' {
        if p >= bytes.len() || matches!(bytes[p], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: p });
        }
        return Err(VersionError::InvalidChar { at: p });
    }
    p += 1;
    let patch = parse_core_field(bytes, &mut p)?;

    let mut pre = Vec::new();
    let mut build = Vec::new();
    if p < bytes.len() && bytes[p] == b'-' {
        let pre_start = p + 1;
        let plus = bytes[pre_start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|n| pre_start + n)
            .unwrap_or(bytes.len());
        pre = parse_identifiers(s, pre_start, plus, true)?;
        p = plus;
    }
    if p < bytes.len() && bytes[p] == b'+' {
        build = parse_identifiers(s, p + 1, bytes.len(), false)?;
        p = bytes.len();
    }
    if p != bytes.len() {
        return Err(VersionError::InvalidChar { at: p });
    }
    Ok(Version { major, minor, patch, pre, build })
}

fn is_numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn compare_identifier(a: &str, b: &str) -> Ordering {
    let an = is_numeric_identifier(a);
    let bn = is_numeric_identifier(b);
    match (an, bn) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
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
                let c = compare_identifier(x, y);
                if c != Ordering::Equal {
                    return c;
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

#[derive(Clone, Debug)]
enum Op {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
}

#[derive(Clone, Debug)]
struct Comparator {
    op: Op,
    version: Version,
    explicit_anchor: bool,
}

#[derive(Clone, Debug)]
struct Arm {
    comparators: Vec<Comparator>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

fn tuple(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn release(major: u64, minor: u64, patch: u64) -> Version {
    Version { major, minor, patch, pre: Vec::new(), build: Vec::new() }
}

fn endpoint(major: u64, minor: u64, patch: u64) -> Version {
    Version { major, minor, patch, pre: vec!["0".to_string()], build: Vec::new() }
}

fn push_cmp(arm: &mut Arm, op: Op, version: Version, anchor: bool) {
    if anchor && !version.pre.is_empty() {
        arm.anchors.push(tuple(&version));
    }
    arm.comparators.push(Comparator { op, version, explicit_anchor: anchor });
}

fn whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokens_with_offsets(s: &str, base: usize) -> Vec<(&str, usize)> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && whitespace(bytes[i]) { i += 1; }
        if i == bytes.len() { break; }
        let start = i;
        while i < bytes.len() && !whitespace(bytes[i]) { i += 1; }
        out.push((&s[start..i], base + start));
    }
    out
}

fn version_at(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

#[derive(Clone, Debug)]
enum Partial {
    Full(Version),
    Parts(Vec<Option<u64>>),
}

impl Partial {
    fn numeric_count(&self) -> usize {
        match self {
            Partial::Full(_) => 3,
            Partial::Parts(v) => v.iter().take_while(|x| x.is_some()).count(),
        }
    }
    fn lower(&self) -> Version {
        match self {
            Partial::Full(v) => v.clone(),
            Partial::Parts(v) => release(
                v.get(0).and_then(|x| *x).unwrap_or(0),
                v.get(1).and_then(|x| *x).unwrap_or(0),
                v.get(2).and_then(|x| *x).unwrap_or(0),
            ),
        }
    }
}

fn parse_partial_operand(s: &str, at: usize) -> Result<Partial, RangeError> {
    let fullish = s.contains('-') || s.contains('+') ||
        (s.split('.').count() >= 3 && !s.split('.').any(|x| matches!(x, "*" | "x" | "X")));
    if fullish {
        return version_at(s, at).map(Partial::Full);
    }
    let fields: Vec<&str> = s.split('.').collect();
    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|x| x.is_empty()) {
        return Err(RangeError::Syntax { at });
    }
    let mut out = Vec::new();
    let mut wildcard = false;
    for field in fields {
        if matches!(field, "*" | "x" | "X") {
            wildcard = true;
            out.push(None);
        } else {
            if wildcard || !field.bytes().all(|b| b.is_ascii_digit())
                || (field.len() > 1 && field.as_bytes()[0] == b'0')
            {
                return Err(RangeError::Syntax { at });
            }
            let Some(n) = field.parse::<u64>().ok() else {
                return Err(RangeError::Syntax { at });
            };
            out.push(Some(n));
        }
    }
    Ok(Partial::Parts(out))
}

fn bump_upper(v: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let (mut major, mut minor, mut patch) = tuple(v);
    match field {
        0 => major = major.checked_add(1).ok_or(RangeError::Syntax { at })?,
        1 => minor = minor.checked_add(1).ok_or(RangeError::Syntax { at })?,
        _ => patch = patch.checked_add(1).ok_or(RangeError::Syntax { at })?,
    }
    Ok(endpoint(major, minor, patch))
}

fn partial_upper(p: &Partial, at: usize) -> Result<Option<Version>, RangeError> {
    match p {
        Partial::Full(_) => Ok(None),
        Partial::Parts(v) => {
            let k = p.numeric_count();
            if k == 0 { return Ok(None); }
            let lo = p.lower();
            Ok(Some(bump_upper(&lo, if k == 1 { 0 } else { 1 }, at)?))
        }
    }
}

fn parse_operand(s: &str, at: usize) -> Result<Partial, RangeError> {
    parse_partial_operand(s, at)
}

fn parse_normal_arm(tokens: &[(&str, usize)], arm: &mut Arm) -> Result<(), RangeError> {
    for &(tok, at) in tokens {
        let (op, operand, operand_at) = if tok.starts_with(">=") {
            (Some(Op::Ge), &tok[2..], at + 2)
        } else if tok.starts_with("<=") {
            (Some(Op::Le), &tok[2..], at + 2)
        } else if tok.starts_with('>') {
            (Some(Op::Gt), &tok[1..], at + 1)
        } else if tok.starts_with('<') {
            (Some(Op::Lt), &tok[1..], at + 1)
        } else if tok.starts_with('=') {
            (Some(Op::Eq), &tok[1..], at + 1)
        } else if tok.starts_with('^') || tok.starts_with('~') {
            let caret = tok.starts_with('^');
            let operand = &tok[1..];
            let op_at = at + 1;
            let p = parse_operand(operand, op_at)?;
            let k = p.numeric_count();
            if k == 0 { return Err(RangeError::Syntax { at }); }
            let lo = p.lower();
            push_cmp(arm, Op::Ge, lo.clone(), matches!(p, Partial::Full(_)));
            let field = if caret {
                if lo.major > 0 || k == 1 { 0 }
                else if lo.minor > 0 || k == 2 { 1 }
                else { 2 }
            } else if k == 1 { 0 } else { 1 };
            push_cmp(arm, Op::Lt, bump_upper(&lo, field, op_at)?, false);
            continue;
        } else {
            if !(tok.as_bytes().first().is_some_and(|b| b.is_ascii_digit()) ||
                matches!(tok, "*" | "x" | "X")) {
                return Err(RangeError::Syntax { at });
            }
            let p = parse_operand(tok, at)?;
            match p {
                Partial::Full(v) => push_cmp(arm, Op::Eq, v, true),
                Partial::Parts(_) => {
                    let k = p.numeric_count();
                    if k > 0 {
                        let lo = p.lower();
                        push_cmp(arm, Op::Ge, lo.clone(), false);
                        if let Some(hi) = partial_upper(&p, at)? {
                            push_cmp(arm, Op::Lt, hi, false);
                        }
                    }
                }
            }
            continue;
        };
        if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
            return Err(RangeError::Syntax { at });
        }
        let v = version_at(operand, operand_at)?;
        push_cmp(arm, op.unwrap(), v, true);
    }
    Ok(())
}

fn parse_hyphen_arm(tokens: &[(&str, usize)], arm: &mut Arm, dash_at: usize) -> Result<(), RangeError> {
    if tokens.len() != 3 || tokens[1].0 != "-" {
        return Err(RangeError::Syntax { at: dash_at });
    }
    let (lower_s, lower_at) = tokens[0];
    let (upper_s, upper_at) = tokens[2];
    let lower = parse_operand(lower_s, lower_at)?;
    let upper = parse_operand(upper_s, upper_at)?;
    if lower.numeric_count() == 0 { return Err(RangeError::Syntax { at: lower_at }); }
    if upper.numeric_count() == 0 { return Err(RangeError::Syntax { at: upper_at }); }
    let lo = lower.lower();
    push_cmp(arm, Op::Ge, lo, matches!(lower, Partial::Full(ref v) if !v.pre.is_empty()));
    match upper {
        Partial::Full(v) => push_cmp(arm, Op::Le, v.clone(), !v.pre.is_empty()),
        p => {
            if let Some(hi) = partial_upper(&p, upper_at)? {
                push_cmp(arm, Op::Lt, hi, false);
            } else {
                return Err(RangeError::Syntax { at: upper_at });
            }
        }
    }
    Ok(())
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = Vec::new();
    let mut arm_start = 0;
    let mut i = 0;
    let mut saw_any = false;
    while i < bytes.len() {
        while i < bytes.len() && whitespace(bytes[i]) { i += 1; }
        if i == bytes.len() { break; }
        if i + 1 < bytes.len() && bytes[i] == b'|' && bytes[i + 1] == b'|' {
            if !saw_any { return Err(RangeError::Empty { at: i }); }
            let seg = &s[arm_start..i];
            let ts = tokens_with_offsets(seg, arm_start);
            let mut arm = Arm { comparators: Vec::new(), anchors: Vec::new() };
            let d = ts.iter().find(|t| t.0 == "-").map(|t| t.1);
            if let Some(dash) = d { parse_hyphen_arm(&ts, &mut arm, dash)?; }
            else { parse_normal_arm(&ts, &mut arm)?; }
            arms.push(arm);
            i += 2;
            while i < bytes.len() && whitespace(bytes[i]) { i += 1; }
            arm_start = i;
            saw_any = false;
        } else {
            let start = i;
            while i < bytes.len() && !whitespace(bytes[i]) { i += 1; }
            if i > start { saw_any = true; }
        }
    }
    if !saw_any && arms.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    if !saw_any && !arms.is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    let seg = &s[arm_start..];
    let ts = tokens_with_offsets(seg, arm_start);
    let mut arm = Arm { comparators: Vec::new(), anchors: Vec::new() };
    if let Some((_, dash)) = ts.iter().find(|t| t.0 == "-") {
        parse_hyphen_arm(&ts, &mut arm, *dash)?;
    } else {
        parse_normal_arm(&ts, &mut arm)?;
    }
    arms.push(arm);
    Ok(Range { arms })
}

fn comparator_matches(v: &Version, c: &Comparator) -> bool {
    let ord = compare_versions(v, &c.version);
    match c.op {
        Op::Lt => ord == Ordering::Less,
        Op::Le => ord != Ordering::Greater,
        Op::Eq => ord == Ordering::Equal,
        Op::Ge => ord != Ordering::Less,
        Op::Gt => ord == Ordering::Greater,
    }
}

fn arm_matches(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !arm.comparators.iter().all(|c| comparator_matches(v, c)) {
        return false;
    }
    if include_prerelease || v.pre.is_empty() {
        return true;
    }
    arm.anchors.iter().any(|&t| t == tuple(v))
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|arm| arm_matches(v, arm, include_prerelease))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

pub fn max_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, true)
}

pub fn min_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, false)
}

fn extrema(candidates: &[String], r: &Range, maximum: bool) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, s) in candidates.iter().enumerate() {
        parsed.push(parse_version(s).map_err(|error| CandidateError { index, error })?);
    }
    let mut best: Option<usize> = None;
    for (i, v) in parsed.iter().enumerate() {
        if !matches(v, r) { continue; }
        if let Some(j) = best {
            let ord = compare_versions(v, &parsed[j]);
            if (maximum && ord == Ordering::Greater) || (!maximum && ord == Ordering::Less) {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

#[derive(Clone, Debug)]
struct Bound {
    version: Version,
    inclusive: bool,
}

fn apply_bound(slot: &mut Option<Bound>, candidate: Bound, lower: bool) {
    let replace = match slot {
        None => true,
        Some(old) => {
            let ord = compare_versions(&candidate.version, &old.version);
            if lower {
                ord == Ordering::Greater || (ord == Ordering::Equal && !candidate.inclusive && old.inclusive)
            } else {
                ord == Ordering::Less || (ord == Ordering::Equal && !candidate.inclusive && old.inclusive)
            }
        }
    };
    if replace { *slot = Some(candidate); }
}

fn arm_bounds(arm: &Arm) -> (Option<Bound>, Option<Bound>, Option<Version>) {
    let mut lower = None;
    let mut upper = None;
    let mut equality = None;
    for c in &arm.comparators {
        let b = Bound { version: c.version.clone(), inclusive: matches!(c.op, Op::Le | Op::Ge | Op::Eq) };
        match c.op {
            Op::Gt | Op::Ge => apply_bound(&mut lower, b, true),
            Op::Lt | Op::Le => apply_bound(&mut upper, b, false),
            Op::Eq => {
                equality = Some(c.version.clone());
                apply_bound(&mut lower, b.clone(), true);
                apply_bound(&mut upper, b, false);
            }
        }
    }
    (lower, upper, equality)
}

fn version_string(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

fn core_string(t: (u64, u64, u64)) -> String {
    format!("{}.{}.{}", t.0, t.1, t.2)
}

fn valid_stable_in_bounds(lower: &Option<Bound>, upper: &Option<Bound>) -> bool {
    let zero = release(0, 0, 0);
    if let Some(l) = lower {
        if compare_versions(&zero, &l.version) == Ordering::Less ||
            (compare_versions(&zero, &l.version) == Ordering::Equal && !l.inclusive) {
            // The domain has no negative versions; continue with interval logic.
        }
    }
    match (lower, upper) {
        (Some(l), Some(u)) => {
            let o = compare_versions(&l.version, &u.version);
            o == Ordering::Less || (o == Ordering::Equal && l.inclusive && u.inclusive)
        }
        _ => true,
    }
}

fn canonical_arm(arm: &Arm) -> Option<String> {
    let (lower, upper, equality) = arm_bounds(arm);
    if let Some(eq) = equality {
        if arm.comparators.iter().all(|c| comparator_matches(&eq, c)) &&
            arm_matches(&eq, arm, false) {
            // equality itself is an exact witness
        } else {
            return None;
        }
    }
    if !valid_stable_in_bounds(&lower, &upper) && arm.anchors.is_empty() {
        return None;
    }
    let mut anchors = arm.anchors.clone();
    anchors.sort();
    anchors.dedup();
    let a = anchors.iter().map(|t| core_string(*t)).collect::<Vec<_>>().join(",");
    let l = match lower {
        None => "*".to_string(),
        Some(b) => format!("{}{}", if b.inclusive { ">=" } else { ">" }, version_string(&b.version)),
    };
    let u = match upper {
        None => "*".to_string(),
        Some(b) => format!("{}{}", if b.inclusive { "<=" } else { "<" }, version_string(&b.version)),
    };
    Some(format!("[{};{};pre={}]", l, u, if a.is_empty() { "-".to_string() } else { a }))
}

pub fn canonical(r: &Range) -> String {
    let mut arms: Vec<String> = r.arms.iter().filter_map(canonical_arm).collect();
    arms.sort();
    arms.dedup();
    if arms.is_empty() { "EMPTY".to_string() } else { arms.join(" || ") }
}

fn interval_witness(a: &Arm, b: &Arm, include_prerelease: bool) -> bool {
    let (la, ua, _) = arm_bounds(a);
    let (lb, ub, _) = arm_bounds(b);
    let mut lower = la;
    if let Some(x) = lb { apply_bound(&mut lower, x, true); }
    let mut upper = ua;
    if let Some(x) = ub { apply_bound(&mut upper, x, false); }

    if let (Some(l), Some(u)) = (&lower, &upper) {
        let ord = compare_versions(&l.version, &u.version);
        if ord == Ordering::Greater || (ord == Ordering::Equal && !(l.inclusive && u.inclusive)) {
            return false;
        }
    }

    if include_prerelease {
        return true;
    }

    // Stable releases are witnesses whenever the interval contains a stable point.
    let stable_zero = release(0, 0, 0);
    let stable_ok = |v: &Version| {
        let lower_ok = lower.as_ref().is_none_or(|l| {
            let o = compare_versions(v, &l.version);
            o == Ordering::Greater || (o == Ordering::Equal && l.inclusive)
        });
        let upper_ok = upper.as_ref().is_none_or(|u| {
            let o = compare_versions(v, &u.version);
            o == Ordering::Less || (o == Ordering::Equal && u.inclusive)
        });
        lower_ok && upper_ok
    };
    if stable_ok(&stable_zero) { return true; }
    if let Some(l) = &lower {
        if l.version.pre.is_empty() && l.inclusive && stable_ok(&l.version) {
            return true;
        }
        let (m, n, p) = tuple(&l.version);
        let next = if p < u64::MAX {
            Some(release(m, n, p + 1))
        } else if n < u64::MAX {
            Some(release(m, n + 1, 0))
        } else if m < u64::MAX {
            Some(release(m + 1, 0, 0))
        } else { None };
        if let Some(v) = next {
            if stable_ok(&v) { return true; }
        }
    } else {
        return true;
    }

    // A prerelease can witness only on a tuple admitted by both arms.
    for t in a.anchors.iter().chain(&b.anchors) {
        if !a.anchors.contains(t) || !b.anchors.contains(t) { continue; }
        let least = endpoint(t.0, t.1, t.2);
        let lower_ok = lower.as_ref().is_none_or(|l| {
            let o = compare_versions(&least, &l.version);
            o == Ordering::Greater || (o == Ordering::Equal && l.inclusive)
        });
        let upper_ok = upper.as_ref().is_none_or(|u| {
            let o = compare_versions(&least, &u.version);
            o == Ordering::Less || (o == Ordering::Equal && u.inclusive)
        });
        if lower_ok && upper_ok { return true; }
    }
    false
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    a.arms.iter().any(|x| b.arms.iter().any(|y| interval_witness(x, y, include_prerelease)))
}