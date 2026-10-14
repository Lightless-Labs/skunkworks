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

fn is_digit(b: u8) -> bool {
    b.is_ascii_digit()
}

fn parse_core_field(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && is_digit(bytes[*pos]) {
        *pos += 1;
    }
    if *pos == start {
        if *pos == bytes.len() || matches!(bytes[*pos], b'.' | b'-' | b'+') {
            return Err(VersionError::MissingCore { at: start });
        }
        return Err(VersionError::InvalidChar { at: start });
    }
    let field = &s[start..*pos];
    if field.len() > 1 && field.starts_with('0') {
        return Err(VersionError::LeadingZero { at: start });
    }
    field.parse::<u64>().map_err(|_| VersionError::Overflow { at: start })
}

fn valid_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
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
        let mut p = id_start;
        while p < end && bytes[p] != b'.' {
            if !valid_ident_char(bytes[p]) {
                return Err(VersionError::InvalidChar { at: p });
            }
            p += 1;
        }
        if p == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        let ident = &s[id_start..p];
        if prerelease
            && ident.len() > 1
            && ident.starts_with('0')
            && ident.bytes().all(is_digit)
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        out.push(ident.to_owned());
        if p == end {
            break;
        }
        id_start = p + 1;
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
    let major = parse_core_field(s, &mut pos)?;
    for expected in 0..2 {
        if pos == s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: pos });
        }
        if s.as_bytes()[pos] != b'.' {
            return Err(VersionError::InvalidChar { at: pos });
        }
        pos += 1;
        let number = parse_core_field(s, &mut pos)?;
        if expected == 0 {
            let minor = number;
            if pos == s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            if s.as_bytes()[pos] != b'.' {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
            let patch = parse_core_field(s, &mut pos)?;
            return parse_tail(s, pos, major, minor, patch);
        }
    }
    unreachable!()
}

fn parse_tail(
    s: &str,
    pos: usize,
    major: u64,
    minor: u64,
    patch: u64,
) -> Result<Version, VersionError> {
    let bytes = s.as_bytes();
    if pos == bytes.len() {
        return Ok(Version { major, minor, patch, pre: Vec::new(), build: Vec::new() });
    }
    if bytes[pos] != b'-' && bytes[pos] != b'+' {
        return Err(VersionError::InvalidChar { at: pos });
    }
    let mut pre = Vec::new();
    let mut build = Vec::new();
    let mut cursor = pos;
    if bytes[cursor] == b'-' {
        let start = cursor + 1;
        let plus = bytes[start..].iter().position(|&b| b == b'+').map(|i| start + i);
        let end = plus.unwrap_or(bytes.len());
        pre = parse_identifiers(s, start, end, true)?;
        cursor = end;
    }
    if cursor < bytes.len() {
        if bytes[cursor] != b'+' {
            return Err(VersionError::InvalidChar { at: cursor });
        }
        build = parse_identifiers(s, cursor + 1, bytes.len(), false)?;
        cursor = bytes.len();
    }
    if cursor != bytes.len() {
        return Err(VersionError::InvalidChar { at: cursor });
    }
    Ok(Version { major, minor, patch, pre, build })
}

fn numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    for (x, y) in a.iter().zip(b.iter()) {
        let xn = numeric_identifier(x);
        let yn = numeric_identifier(y);
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
        .then_with(|| cmp_pre(&a.pre, &b.pre))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Debug)]
struct Bound {
    version: Version,
    inclusive: bool,
    lower: bool,
}

#[derive(Clone, Debug)]
struct Comparator {
    op: Op,
    version: Version,
    explicit: bool,
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

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

fn tuple(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn has_admission(v: &Version, arm: &Arm) -> bool {
    v.pre.is_empty() || arm.anchors.contains(&tuple(v))
}

fn matches_arm(v: &Version, arm: &Arm, include_pre: bool) -> bool {
    if !include_pre && !has_admission(v, arm) {
        return false;
    }
    arm.comparators.iter().all(|c| {
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
    matches_with(v, r, false)
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|a| matches_arm(v, a, include_prerelease))
}

fn whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokens(s: &str, base: usize) -> Vec<(String, usize)> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut p = 0;
    while p < bytes.len() {
        while p < bytes.len() && whitespace(bytes[p]) {
            p += 1;
        }
        if p == bytes.len() {
            break;
        }
        let start = p;
        while p < bytes.len() && !whitespace(bytes[p]) {
            p += 1;
        }
        out.push((s[start..p].to_owned(), base + start));
    }
    out
}

fn parse_full_at(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

fn version_core(major: u64, minor: u64, patch: u64) -> Version {
    Version { major, minor, patch, pre: Vec::new(), build: Vec::new() }
}

fn pre_zero(major: u64, minor: u64, patch: u64) -> Version {
    Version {
        major, minor, patch,
        pre: vec!["0".to_owned()],
        build: Vec::new(),
    }
}

fn parse_partial(token: &str, at: usize) -> Result<(Version, usize, bool), RangeError> {
    let fields: Vec<&str> = token.split('.').collect();
    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|f| f.is_empty()) {
        return Err(RangeError::Syntax { at });
    }
    let mut nums = Vec::new();
    let mut wild = false;
    for f in &fields {
        if *f == "*" || *f == "x" || *f == "X" {
            wild = true;
        } else {
            if wild || !f.bytes().all(is_digit) || (f.len() > 1 && f.starts_with('0')) {
                return Err(RangeError::Syntax { at });
            }
            match f.parse::<u64>() {
                Ok(n) => nums.push(n),
                Err(_) => return Err(RangeError::Syntax { at }),
            }
        }
    }
    let k = nums.len();
    let mut padded = [0u64; 3];
    for (i, n) in nums.iter().enumerate() {
        padded[i] = *n;
    }
    Ok((version_core(padded[0], padded[1], padded[2]), k, wild))
}

fn full_or_partial(token: &str, at: usize) -> Result<(Version, usize, bool, bool), RangeError> {
    let fields: Vec<&str> = token.split('.').collect();
    let wildcard = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));
    let full = token.contains(['-', '+']) || (fields.len() >= 3 && !wildcard);
    if full {
        let v = parse_full_at(token, at)?;
        Ok((v, 3, false, true))
    } else {
        let (v, k, w) = parse_partial(token, at)?;
        Ok((v, k, w, false))
    }
}

fn bump(v: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let mut out = version_core(v.major, v.minor, v.patch);
    match field {
        0 => {
            out.major = out.major.checked_add(1).ok_or(RangeError::Syntax { at })?;
        }
        1 => {
            out.major = v.major;
            out.minor = v.minor.checked_add(1).ok_or(RangeError::Syntax { at })?;
        }
        _ => {
            out.major = v.major;
            out.minor = v.minor;
            out.patch = v.patch.checked_add(1).ok_or(RangeError::Syntax { at })?;
        }
    }
    Ok(out)
}

fn add(arm: &mut Arm, op: Op, version: Version, explicit: bool) {
    if explicit && !version.pre.is_empty() {
        arm.anchors.push(tuple(&version));
    }
    arm.comparators.push(Comparator { op, version, explicit });
}

fn upper_op(version: Version) -> Comparator {
    Comparator { op: Op::Lt, version, explicit: false }
}

fn parse_expanded(token: &str, at: usize, arm: &mut Arm) -> Result<(), RangeError> {
    let bytes = token.as_bytes();
    if bytes[0] == b'^' || bytes[0] == b'~' {
        let operand_at = at + 1;
        let operand = &token[1..];
        if operand.is_empty() {
            return Err(RangeError::Syntax { at });
        }
        let (mut lower, k, _, full) = full_or_partial(operand, operand_at)?;
        if k == 0 {
            return Err(RangeError::Syntax { at });
        }
        if !full {
            lower.pre.clear();
            lower.build.clear();
        }
        let upper = if bytes[0] == b'^' {
            if lower.major > 0 || k == 1 {
                bump(&lower, 0, at)?
            } else if lower.minor > 0 || k == 2 {
                bump(&lower, 1, at)?
            } else {
                bump(&lower, 2, at)?
            }
        } else if k == 1 {
            bump(&lower, 0, at)?
        } else {
            bump(&lower, 1, at)?
        };
        add(arm, Op::Ge, lower.clone(), true);
        arm.comparators.push(upper_op(upper));
        return Ok(());
    }

    let mut op_len = 0;
    let mut op = Op::Eq;
    if bytes.starts_with(b">=") { op_len = 2; op = Op::Ge; }
    else if bytes.starts_with(b"<=") { op_len = 2; op = Op::Le; }
    else if bytes.starts_with(b">") { op_len = 1; op = Op::Gt; }
    else if bytes.starts_with(b"<") { op_len = 1; op = Op::Lt; }
    else if bytes.starts_with(b"=") { op_len = 1; op = Op::Eq; }

    if op_len > 0 {
        let operand = &token[op_len..];
        if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
            return Err(RangeError::Syntax { at });
        }
        let v = parse_full_at(operand, at + op_len)?;
        add(arm, op, v, true);
        return Ok(());
    }

    if !bytes[0].is_ascii_digit() && !matches!(bytes[0], b'*' | b'x' | b'X') {
        return Err(RangeError::Syntax { at });
    }
    let (base, k, _, full) = full_or_partial(token, at)?;
    if full {
        add(arm, Op::Eq, base, true);
    } else if k == 0 {
        // An all-wildcard operand contributes no constraint.
    } else if k == 1 {
        add(arm, Op::Ge, base.clone(), false);
        arm.comparators.push(upper_op(bump(&base, 0, at)?));
    } else if k == 2 {
        add(arm, Op::Ge, base.clone(), false);
        arm.comparators.push(upper_op(bump(&base, 1, at)?));
    } else {
        add(arm, Op::Eq, base, false);
    }
    Ok(())
}

fn parse_hyphen(tokens: &[(String, usize)], arm: &mut Arm) -> Result<(), RangeError> {
    let minus = tokens.iter().position(|(t, _)| t == "-").unwrap();
    if tokens.len() != 3 || minus != 1 {
        return Err(RangeError::Syntax { at: tokens[minus].1 });
    }
    let (lo, lk, _, lf) = full_or_partial(&tokens[0].0, tokens[0].1)?;
    let (hi, hk, _, hf) = full_or_partial(&tokens[2].0, tokens[2].1)?;
    if lk == 0 {
        return Err(RangeError::Syntax { at: tokens[0].1 });
    }
    if hk == 0 {
        return Err(RangeError::Syntax { at: tokens[2].1 });
    }
    add(arm, Op::Ge, lo, true);
    if hf {
        add(arm, Op::Le, hi, true);
    } else if hk == 1 {
        arm.comparators.push(upper_op(bump(&hi, 0, tokens[2].1)?));
    } else if hk == 2 {
        arm.comparators.push(upper_op(bump(&hi, 1, tokens[2].1)?));
    } else {
        // A three-field partial has an exclusive next patch bound.
        arm.comparators.push(upper_op(bump(&hi, 2, tokens[2].1)?));
    }
    let _ = lf;
    Ok(())
}

fn parse_arm(s: &str, base: usize) -> Result<Arm, RangeError> {
    let ts = tokens(s, base);
    if ts.is_empty() {
        return Err(RangeError::Empty { at: base });
    }
    let mut arm = Arm { comparators: Vec::new(), anchors: Vec::new() };
    if ts.iter().any(|(t, _)| t == "-") {
        parse_hyphen(&ts, &mut arm)?;
        return Ok(arm);
    }
    for (t, at) in ts {
        parse_expanded(&t, at, &mut arm)?;
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    if s.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    let mut separators = Vec::new();
    let mut p = 0;
    while p + 1 < bytes.len() {
        if bytes[p] == b'|' && bytes[p + 1] == b'|' {
            let left_ok = p == 0 || whitespace(bytes[p - 1]);
            let right_ok = p + 2 == bytes.len() || whitespace(bytes[p + 2]);
            if left_ok && right_ok {
                separators.push((p, p + 2));
                p += 2;
                continue;
            }
        }
        p += 1;
    }
    if separators.is_empty() {
        let ts = tokens(s, 0);
        if ts.is_empty() {
            return Err(RangeError::Empty { at: 0 });
        }
        return Ok(Range { arms: vec![parse_arm(s, 0)?] });
    }
    let mut arms = Vec::new();
    let mut start = 0;
    for (sep_start, sep_end) in separators {
        let part = &s[start..sep_start];
        let t = tokens(part, start);
        if t.is_empty() {
            return Err(RangeError::Empty { at: sep_start });
        }
        arms.push(parse_arm(part, start)?);
        start = sep_end;
    }
    let part = &s[start..];
    if tokens(part, start).is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(part, start)?);
    Ok(Range { arms })
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn extrema(candidates: &[String], r: &Range, maximum: bool) -> Result<Option<String>, CandidateError> {
    let parsed: Vec<Version> = candidates.iter().enumerate().map(|(index, s)| {
        parse_version(s).map_err(|error| CandidateError { index, error })
    }).collect::<Result<_, _>>()?;
    let mut best: Option<usize> = None;
    for (i, v) in parsed.iter().enumerate() {
        if !matches(v, r) {
            continue;
        }
        best = match best {
            None => Some(i),
            Some(j) => {
                let ord = compare_versions(v, &parsed[j]);
                if (maximum && ord == Ordering::Greater) || (!maximum && ord == Ordering::Less) {
                    Some(i)
                } else {
                    Some(j)
                }
            }
        };
    }
    Ok(best.map(|i| candidates[i].clone()))
}

fn min_lower(a: Option<Bound>, b: Bound) -> Option<Bound> {
    match a {
        None => Some(b),
        Some(old) => {
            let ord = compare_versions(&b.version, &old.version);
            if ord == Ordering::Greater || (ord == Ordering::Equal && !b.inclusive && old.inclusive) {
                Some(b)
            } else {
                Some(old)
            }
        }
    }
}

fn max_upper(a: Option<Bound>, b: Bound) -> Option<Bound> {
    match a {
        None => Some(b),
        Some(old) => {
            let ord = compare_versions(&b.version, &old.version);
            if ord == Ordering::Less || (ord == Ordering::Equal && !b.inclusive && old.inclusive) {
                Some(b)
            } else {
                Some(old)
            }
        }
    }
}

fn bounds(arm: &Arm) -> (Option<Bound>, Option<Bound>) {
    let mut lower = None;
    let mut upper = None;
    for c in &arm.comparators {
        match c.op {
            Op::Eq => {
                lower = min_lower(lower, Bound { version: c.version.clone(), inclusive: true, lower: true });
                upper = max_upper(upper, Bound { version: c.version.clone(), inclusive: true, lower: false });
            }
            Op::Ge | Op::Gt => {
                lower = min_lower(lower, Bound { version: c.version.clone(), inclusive: matches!(c.op, Op::Ge), lower: true });
            }
            Op::Le | Op::Lt => {
                upper = max_upper(upper, Bound { version: c.version.clone(), inclusive: matches!(c.op, Op::Le), lower: false });
            }
        }
    }
    (lower, upper)
}

fn bounds_overlap(l: &Option<Bound>, u: &Option<Bound>) -> bool {
    match (l, u) {
        (Some(l), Some(u)) => {
            let c = compare_versions(&l.version, &u.version);
            c == Ordering::Less || (c == Ordering::Equal && l.inclusive && u.inclusive)
        }
        _ => true,
    }
}

fn core_successor(v: &Version) -> Option<Version> {
    if v.patch < u64::MAX {
        Some(pre_zero(v.major, v.minor, v.patch + 1))
    } else if v.minor < u64::MAX {
        Some(pre_zero(v.major, v.minor + 1, 0))
    } else if v.major < u64::MAX {
        Some(pre_zero(v.major + 1, 0, 0))
    } else {
        None
    }
}

fn interval_has_stable(lower: &Option<Bound>, upper: &Option<Bound>) -> bool {
    if !bounds_overlap(lower, upper) {
        return false;
    }
    let candidate = match lower {
        None => version_core(0, 0, 0),
        Some(l) if l.version.pre.is_empty() => {
            if l.inclusive { l.version.clone() } else {
                match core_successor(&l.version) { Some(v) => v, None => return false }
            }
        }
        Some(l) => version_core(l.version.major, l.version.minor, l.version.patch),
    };
    match upper {
        None => true,
        Some(u) => {
            let ord = compare_versions(&candidate, &u.version);
            ord == Ordering::Less || (ord == Ordering::Equal && u.inclusive)
        }
    }
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for aa in &a.arms {
        for bb in &b.arms {
            let (la, ua) = bounds(aa);
            let (lb, ub) = bounds(bb);
            let lower = match (la, lb) {
                (Some(x), Some(y)) => {
                    let c = compare_versions(&x.version, &y.version);
                    Some(if c == Ordering::Greater || (c == Ordering::Equal && !x.inclusive) { x } else { y })
                }
                (x, y) => x.or(y),
            };
            let upper = match (ua, ub) {
                (Some(x), Some(y)) => {
                    let c = compare_versions(&x.version, &y.version);
                    Some(if c == Ordering::Less || (c == Ordering::Equal && !x.inclusive) { x } else { y })
                }
                (x, y) => x.or(y),
            };
            if !bounds_overlap(&lower, &upper) {
                continue;
            }
            if interval_has_stable(&lower, &upper) {
                return true;
            }
            if include_prerelease {
                // A prerelease witness at a finite interval endpoint is sufficient when
                // either range explicitly describes that same precedence tuple.
                for anchor in aa.anchors.iter().chain(bb.anchors.iter()) {
                    let v = version_core(anchor.0, anchor.1, anchor.2);
                    let lo_ok = lower.as_ref().map_or(true, |l| {
                        let o = compare_versions(&v, &l.version);
                        o == Ordering::Greater || (o == Ordering::Equal && l.inclusive)
                    });
                    let hi_ok = upper.as_ref().map_or(true, |u| {
                        let o = compare_versions(&v, &u.version);
                        o == Ordering::Less || (o == Ordering::Equal && u.inclusive)
                    });
                    if lo_ok && hi_ok { return true; }
                }
            } else {
                for anchor in aa.anchors.iter().filter(|x| bb.anchors.contains(x)) {
                    let mut v = pre_zero(anchor.0, anchor.1, anchor.2);
                    let lo_ok = lower.as_ref().map_or(true, |l| {
                        compare_versions(&v, &l.version) == Ordering::Greater
                            || (l.inclusive && compare_versions(&v, &l.version) == Ordering::Equal)
                    });
                    let hi_ok = upper.as_ref().map_or(true, |u| {
                        compare_versions(&v, &u.version) == Ordering::Less
                            || (u.inclusive && compare_versions(&v, &u.version) == Ordering::Equal)
                    });
                    if lo_ok && hi_ok { return true; }
                    v.pre.clear();
                }
            }
        }
    }
    false
}

fn core_text(t: (u64, u64, u64)) -> String {
    format!("{}.{}.{}", t.0, t.1, t.2)
}

fn version_text(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

fn canonical_arm(arm: &Arm) -> Option<String> {
    let (lower, upper) = bounds(arm);
    if !bounds_overlap(&lower, &upper) || !interval_has_stable(&lower, &upper) {
        return None;
    }
    let mut anchors = arm.anchors.clone();
    anchors.sort();
    anchors.dedup();
    let anchor_text = if anchors.is_empty() {
        "-".to_owned()
    } else {
        anchors.iter().map(|t| core_text(*t)).collect::<Vec<_>>().join(",")
    };
    let lo = lower.map_or("*".to_owned(), |b| {
        format!("{}{}", if b.inclusive { ">=" } else { ">" }, version_text(&b.version))
    });
    let hi = upper.map_or("*".to_owned(), |b| {
        format!("{}{}", if b.inclusive { "<=" } else { "<" }, version_text(&b.version))
    });
    Some(format!("[{};{};pre={}]", lo, hi, anchor_text))
}

pub fn canonical(r: &Range) -> String {
    let mut arms: Vec<String> = r.arms.iter().filter_map(canonical_arm).collect();
    arms.sort();
    arms.dedup();
    if arms.is_empty() { "EMPTY".to_owned() } else { arms.join(" || ") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pv(s: &str) -> Version { parse_version(s).unwrap() }
    fn pr(s: &str) -> Range { parse_range(s).unwrap() }

    #[test]
    fn public_version_comparators() {
        let v = pv("2.1.3-rc.1+build.7");
        assert_eq!((v.major, v.minor, v.patch), (2, 1, 3));
        assert_eq!(v.pre, vec!["rc", "1"]);
        assert_eq!(v.build, vec!["build", "7"]);
        assert_eq!(compare_versions(&pv("2.1.3-alpha"), &pv("2.1.3-beta")), Ordering::Less);
        let r = pr(">=2.1.0 <3.0.0");
        assert!(matches(&pv("2.5.0"), &r));
        assert!(!matches(&pv("3.0.0"), &r));
        assert!(matches(&pv("2.1.3-rc"), &pr(">=2.1.3-beta <2.1.3")));
    }

    #[test]
    fn public_expanded_extrema() {
        for s in ["^2.1.0", "~2.5.0", "2.5.x", "2.1 - 2.8"] {
            assert!(matches(&pv("2.5.1"), &pr(s)));
        }
        assert!(matches(&pv("4.0.0"), &pr("2.x || 4.0.0")));
        let cs = vec!["2.2.0".to_string(), "2.8.0".to_string(), "3.0.0".to_string()];
        let r = pr("2.x");
        assert_eq!(max_satisfying(&cs, &r), Ok(Some("2.8.0".into())));
        assert_eq!(min_satisfying(&cs, &r), Ok(Some("2.2.0".into())));
    }

    #[test]
    fn public_algebra() {
        let r = pr(">=2.1.0 <2.8.0");
        assert!(intersects(&r, &pr("2.5.x"), false));
        assert_eq!(canonical(&r), "[>=2.1.0;<2.8.0;pre=-]");
        assert!(matches_with(&pv("2.5.0-rc"), &r, true));
    }

    #[test]
    fn public_canonical_empty() {
        assert_eq!(canonical(&pr(">=2.0.0 <2.0.0")), "EMPTY");
        assert!(!intersects(&pr("2.x"), &pr("3.x"), true));
    }
}