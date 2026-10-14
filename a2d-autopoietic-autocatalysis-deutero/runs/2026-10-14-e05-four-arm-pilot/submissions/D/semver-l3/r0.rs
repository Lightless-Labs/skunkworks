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

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    prerelease: bool,
) -> Result<Vec<String>, VersionError> {
    if start == end {
        return Err(VersionError::EmptyIdentifier { at: start });
    }

    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut pos = start;

    loop {
        let ident_start = pos;
        while pos < end && bytes[pos] != b'.' {
            if !is_ident_byte(bytes[pos]) {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
        }

        if ident_start == pos {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }

        let ident = &s[ident_start..pos];
        if prerelease
            && ident.bytes().all(is_digit)
            && ident.len() > 1
            && ident.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: ident_start });
        }

        out.push(ident.to_owned());
        if pos == end {
            break;
        }
        pos += 1;
        if pos == end {
            return Err(VersionError::EmptyIdentifier { at: pos });
        }
    }

    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let bytes = s.as_bytes();
    let mut pos = 0usize;
    let mut core = [0u64; 3];

    for i in 0..3 {
        let start = pos;
        while pos < bytes.len() && is_digit(bytes[pos]) {
            pos += 1;
        }

        if start == pos {
            if pos == bytes.len() || matches!(bytes[pos], b'.' | b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            return Err(VersionError::InvalidChar { at: pos });
        }

        let digits = &s[start..pos];
        if digits.len() > 1 && digits.as_bytes()[0] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }
        core[i] = digits
            .parse::<u64>()
            .map_err(|_| VersionError::Overflow { at: start })?;

        if i < 2 {
            if pos < bytes.len() && bytes[pos] == b'.' {
                pos += 1;
            } else if pos == bytes.len() || matches!(bytes[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            } else {
                return Err(VersionError::InvalidChar { at: pos });
            }
        }
    }

    if pos < bytes.len() && !matches!(bytes[pos], b'-' | b'+') {
        return Err(VersionError::InvalidChar { at: pos });
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] == b'-' {
        let pre_start = pos + 1;
        let plus = bytes[pre_start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|n| pre_start + n);
        let pre_end = plus.unwrap_or(bytes.len());
        pre = parse_identifiers(s, pre_start, pre_end, true)?;
        pos = pre_end;
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        let build_start = pos + 1;
        build = parse_identifiers(s, build_start, bytes.len(), false)?;
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

fn numeric_id(s: &str) -> bool {
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
        let xn = numeric_id(x);
        let yn = numeric_id(y);
        let order = match (xn, yn) {
            (true, true) => x.len().cmp(&y.len()).then_with(|| x.as_bytes().cmp(y.as_bytes())),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => x.as_bytes().cmp(y.as_bytes()),
        };
        if order != Ordering::Equal {
            return order;
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum Op {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
}

#[derive(Clone, Debug)]
struct Cmp {
    op: Op,
    version: Version,
    anchor: bool,
}

#[derive(Clone, Debug)]
struct Arm {
    cmps: Vec<Cmp>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokens(s: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut p = start;
    while p < end {
        while p < end && is_ws(b[p]) {
            p += 1;
        }
        if p == end {
            break;
        }
        let a = p;
        while p < end && !is_ws(b[p]) {
            p += 1;
        }
        out.push((a, p));
    }
    out
}

#[derive(Clone, Debug)]
struct Partial {
    fields: Vec<Option<u64>>,
    full: Option<Version>,
    numeric_count: usize,
}

fn syntax(at: usize) -> RangeError {
    RangeError::Syntax { at }
}

fn parse_partial(operand: &str, start: usize) -> Result<Partial, RangeError> {
    let has_suffix = operand.contains('-') || operand.contains('+');
    let fields: Vec<&str> = operand.split('.').collect();
    let no_wild = fields
        .iter()
        .all(|f| !matches!(*f, "*" | "x" | "X"));

    if has_suffix || (fields.len() >= 3 && no_wild) {
        let v =
            parse_version(operand).map_err(|error| RangeError::Version { at: start, error })?;
        return Ok(Partial {
            fields: vec![Some(v.major), Some(v.minor), Some(v.patch)],
            full: Some(v),
            numeric_count: 3,
        });
    }

    if fields.is_empty() || fields.len() > 3 {
        return Err(syntax(start));
    }

    let mut out = Vec::new();
    let mut wild_seen = false;
    let mut numeric_count = 0usize;

    for field in fields {
        if field.is_empty() {
            return Err(syntax(start));
        }
        if matches!(field, "*" | "x" | "X") {
            wild_seen = true;
            out.push(None);
            continue;
        }
        if wild_seen || !field.bytes().all(is_digit) {
            return Err(syntax(start));
        }
        if field.len() > 1 && field.as_bytes()[0] == b'0' {
            return Err(syntax(start));
        }
        let n = field.parse::<u64>().map_err(|_| syntax(start))?;
        out.push(Some(n));
        numeric_count += 1;
    }

    if out.is_empty() {
        return Err(syntax(start));
    }
    Ok(Partial {
        fields: out,
        full: None,
        numeric_count,
    })
}

fn padded(p: &Partial) -> (u64, u64, u64) {
    let mut v = [0u64; 3];
    for (i, field) in p.fields.iter().enumerate() {
        if let Some(n) = field {
            v[i] = *n;
        } else {
            break;
        }
    }
    (v[0], v[1], v[2])
}

fn endpoint(core: (u64, u64, u64), pre: Vec<String>) -> Version {
    Version {
        major: core.0,
        minor: core.1,
        patch: core.2,
        pre,
        build: Vec::new(),
    }
}

fn bound_cmp(op: Op, version: Version, anchor: bool) -> Cmp {
    Cmp {
        op,
        version,
        anchor,
    }
}

fn increment_upper(
    core: (u64, u64, u64),
    field: usize,
    at: usize,
) -> Result<Version, RangeError> {
    let mut c = [core.0, core.1, core.2];
    c[field] = c[field].checked_add(1).ok_or_else(|| syntax(at))?;
    for x in c.iter_mut().skip(field + 1) {
        *x = 0;
    }
    Ok(endpoint((c[0], c[1], c[2]), vec!["0".to_owned()]))
}

fn add_partial_bare(arm: &mut Arm, p: Partial, at: usize) -> Result<(), RangeError> {
    if let Some(v) = p.full {
        arm.anchors.push((v.major, v.minor, v.patch));
        arm.cmps.push(bound_cmp(Op::Eq, v, true));
        return Ok(());
    }
    if p.numeric_count == 0 {
        return Ok(());
    }
    let core = padded(&p);
    arm.cmps
        .push(bound_cmp(Op::Ge, endpoint(core, Vec::new()), false));
    if p.numeric_count == 1 {
        arm.cmps
            .push(bound_cmp(Op::Lt, increment_upper(core, 0, at)?, false));
    } else if p.numeric_count == 2 {
        arm.cmps
            .push(bound_cmp(Op::Lt, increment_upper(core, 1, at)?, false));
    }
    Ok(())
}

fn add_caret_tilde(
    arm: &mut Arm,
    p: Partial,
    at: usize,
    caret: bool,
) -> Result<(), RangeError> {
    if p.numeric_count == 0 {
        return Err(syntax(at));
    }
    let lower = if let Some(v) = p.full.clone() {
        v
    } else {
        endpoint(padded(&p), Vec::new())
    };
    if !lower.pre.is_empty() {
        arm.anchors.push((lower.major, lower.minor, lower.patch));
    }
    arm.cmps.push(bound_cmp(
        Op::Ge,
        lower.clone(),
        !lower.pre.is_empty(),
    ));

    let core = (lower.major, lower.minor, lower.patch);
    let field = if caret {
        if core.0 > 0 || p.numeric_count == 1 {
            0
        } else if core.1 > 0 || p.numeric_count == 2 {
            1
        } else {
            2
        }
    } else if p.numeric_count == 1 {
        0
    } else {
        1
    };

    arm.cmps
        .push(bound_cmp(Op::Lt, increment_upper(core, field, at)?, false));
    Ok(())
}

fn full_comparator(token: &str, start: usize) -> Result<Cmp, RangeError> {
    let (op, n) = if token.starts_with(">=") {
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
        (Op::Eq, 0)
    };

    let operand = &token[n..];
    let operand_start = start + n;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(syntax(start));
    }
    let v = parse_version(operand)
        .map_err(|error| RangeError::Version { at: operand_start, error })?;
    let anchor = !v.pre.is_empty();
    Ok(Cmp {
        op,
        version: v,
        anchor,
    })
}

fn parse_arm(s: &str, start: usize, end: usize) -> Result<Arm, RangeError> {
    let ts = tokens(s, start, end);
    if ts.is_empty() {
        return Err(RangeError::Empty { at: start });
    }

    let minus_positions: Vec<usize> = ts
        .iter()
        .filter(|&&(a, b)| &s[a..b] == "-")
        .map(|&(a, _)| a)
        .collect();

    if let Some(&minus) = minus_positions.first() {
        if ts.len() != 3 || &s[ts[1].0..ts[1].1] != "-" {
            return Err(syntax(minus));
        }

        let (la, lb) = ts[0];
        let (ua, ub) = ts[2];
        let mut lower = parse_partial(&s[la..lb], la)?;
        let mut upper = parse_partial(&s[ua..ub], ua)?;
        if lower.numeric_count == 0 {
            return Err(syntax(la));
        }
        if upper.numeric_count == 0 {
            return Err(syntax(ua));
        }

        let mut arm = Arm {
            cmps: Vec::new(),
            anchors: Vec::new(),
        };
        let lv = lower
            .full
            .take()
            .unwrap_or_else(|| endpoint(padded(&lower), Vec::new()));
        if !lv.pre.is_empty() {
            arm.anchors.push((lv.major, lv.minor, lv.patch));
        }
        arm.cmps
            .push(bound_cmp(Op::Ge, lv.clone(), !lv.pre.is_empty()));

        let upper_is_full = upper.full.is_some();
        let upper_is_three = upper.numeric_count == 3;
        let uv = if let Some(v) = upper.full.take() {
            if !v.pre.is_empty() {
                arm.anchors.push((v.major, v.minor, v.patch));
            }
            v
        } else {
            let core = padded(&upper);
            if upper.numeric_count == 1 {
                increment_upper(core, 0, ua)?
            } else if upper.numeric_count == 2 {
                increment_upper(core, 1, ua)?
            } else {
                endpoint(core, Vec::new())
            }
        };
        if upper_is_full || upper_is_three {
            let anchor = !uv.pre.is_empty();
            arm.cmps.push(bound_cmp(Op::Le, uv, anchor));
        } else {
            arm.cmps.push(bound_cmp(Op::Lt, uv, false));
        }
        return Ok(arm);
    }

    let mut arm = Arm {
        cmps: Vec::new(),
        anchors: Vec::new(),
    };

    for &(a, b) in &ts {
        let token = &s[a..b];
        if token.starts_with('^') || token.starts_with('~') {
            let p = parse_partial(&token[1..], a + 1)?;
            add_caret_tilde(&mut arm, p, a + 1, token.starts_with('^'))?;
        } else if token.starts_with('<') || token.starts_with('>') || token.starts_with('=') {
            let cmp = full_comparator(token, a)?;
            if cmp.anchor {
                arm.anchors
                    .push((cmp.version.major, cmp.version.minor, cmp.version.patch));
            }
            arm.cmps.push(cmp);
        } else if token.starts_with(',')
            || token.starts_with('(')
            || token.starts_with(')')
            || token == "&"
            || !(token.as_bytes()[0].is_ascii_digit()
                || matches!(token.as_bytes()[0], b'*' | b'x' | b'X'))
        {
            return Err(syntax(a));
        } else {
            let p = parse_partial(token, a)?;
            add_partial_bare(&mut arm, p, a)?;
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = Vec::new();
    let mut arm_start = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i..].starts_with(b"||") {
            let separator_start = i;
            let separator_end = i + 2;
            let left = tokens(s, arm_start, separator_start);
            if left.is_empty() {
                return Err(RangeError::Empty { at: separator_start });
            }
            arms.push(parse_arm(s, arm_start, separator_start)?);

            let mut next = separator_end;
            while next < bytes.len() && is_ws(bytes[next]) {
                next += 1;
            }
            if next == bytes.len() {
                return Err(RangeError::Empty { at: bytes.len() });
            }
            if next == separator_end {
                return Err(syntax(separator_start));
            }
            arm_start = next;
            i = next;
        } else {
            i += 1;
        }
    }

    if tokens(s, arm_start, bytes.len()).is_empty() {
        if arms.is_empty() {
            return Err(RangeError::Empty { at: 0 });
        }
        return Err(RangeError::Empty { at: bytes.len() });
    }
    arms.push(parse_arm(s, arm_start, bytes.len())?);
    Ok(Range { arms })
}

fn cmp_matches(v: &Version, c: &Cmp) -> bool {
    let ord = compare_versions(v, &c.version);
    match c.op {
        Op::Lt => ord == Ordering::Less,
        Op::Le => ord != Ordering::Greater,
        Op::Eq => ord == Ordering::Equal,
        Op::Ge => ord != Ordering::Less,
        Op::Gt => ord == Ordering::Greater,
    }
}

fn tuple(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn arm_matches(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !arm.cmps.iter().all(|c| cmp_matches(v, c)) {
        return false;
    }
    v.pre.is_empty()
        || include_prerelease
        || arm.anchors.iter().any(|&a| a == tuple(v))
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms
        .iter()
        .any(|arm| arm_matches(v, arm, include_prerelease))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn satisfying(
    candidates: &[String],
    r: &Range,
    want_max: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, s) in candidates.iter().enumerate() {
        let v = parse_version(s).map_err(|error| CandidateError { index, error })?;
        parsed.push((index, v));
    }

    let mut best: Option<(usize, Version)> = None;
    for (index, v) in parsed {
        if !matches(&v, r) {
            continue;
        }
        let replace = match &best {
            None => true,
            Some((best_index, best_v)) => {
                let ord = compare_versions(&v, best_v);
                (if want_max {
                    ord == Ordering::Greater
                } else {
                    ord == Ordering::Less
                }) || (ord == Ordering::Equal && index < *best_index)
            }
        };
        if replace {
            best = Some((index, v));
        }
    }

    Ok(best.map(|(index, _)| candidates[index].clone()))
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

#[derive(Clone)]
struct Bound {
    v: Version,
    inclusive: bool,
}

fn arm_bounds(arm: &Arm) -> (Option<Bound>, Option<Bound>, Option<Version>) {
    let mut lower: Option<Bound> = None;
    let mut upper: Option<Bound> = None;
    let mut equal: Option<Version> = None;

    for c in &arm.cmps {
        match c.op {
            Op::Eq => equal = Some(c.version.clone()),
            Op::Ge | Op::Gt => {
                let candidate = Bound {
                    v: c.version.clone(),
                    inclusive: c.op == Op::Ge,
                };
                let replace = match &lower {
                    None => true,
                    Some(old) => {
                        let ord = compare_versions(&candidate.v, &old.v);
                        ord == Ordering::Greater
                            || (ord == Ordering::Equal && !candidate.inclusive && old.inclusive)
                    }
                };
                if replace {
                    lower = Some(candidate);
                }
            }
            Op::Le | Op::Lt => {
                let candidate = Bound {
                    v: c.version.clone(),
                    inclusive: c.op == Op::Le,
                };
                let replace = match &upper {
                    None => true,
                    Some(old) => {
                        let ord = compare_versions(&candidate.v, &old.v);
                        ord == Ordering::Less
                            || (ord == Ordering::Equal && !candidate.inclusive && old.inclusive)
                    }
                };
                if replace {
                    upper = Some(candidate);
                }
            }
        }
    }

    if let Some(v) = equal.clone() {
        lower = Some(Bound {
            v: v.clone(),
            inclusive: true,
        });
        upper = Some(Bound {
            v,
            inclusive: true,
        });
    }
    (lower, upper, equal)
}

fn within_bounds(v: &Version, lower: &Option<Bound>, upper: &Option<Bound>) -> bool {
    if let Some(l) = lower {
        let o = compare_versions(v, &l.v);
        if o == Ordering::Less || (o == Ordering::Equal && !l.inclusive) {
            return false;
        }
    }
    if let Some(u) = upper {
        let o = compare_versions(v, &u.v);
        if o == Ordering::Greater || (o == Ordering::Equal && !u.inclusive) {
            return false;
        }
    }
    true
}

fn release(core: (u64, u64, u64)) -> Version {
    endpoint(core, Vec::new())
}

fn next_core(c: (u64, u64, u64)) -> Option<(u64, u64, u64)> {
    if c.2 < u64::MAX {
        Some((c.0, c.1, c.2 + 1))
    } else if c.1 < u64::MAX {
        Some((c.0, c.1 + 1, 0))
    } else if c.0 < u64::MAX {
        Some((c.0 + 1, 0, 0))
    } else {
        None
    }
}

fn stable_witness(lower: &Option<Bound>, upper: &Option<Bound>) -> bool {
    if let (Some(l), Some(u)) = (lower, upper) {
        let o = compare_versions(&l.v, &u.v);
        if o == Ordering::Greater || (o == Ordering::Equal && (!l.inclusive || !u.inclusive)) {
            return false;
        }
    }

    let mut candidates = Vec::new();
    if let Some(l) = lower {
        candidates.push(release(tuple(&l.v)));
        if !l.inclusive || !l.v.pre.is_empty() {
            if let Some(c) = next_core(tuple(&l.v)) {
                candidates.push(release(c));
            }
        }
    } else {
        candidates.push(release((0, 0, 0)));
    }
    if let Some(u) = upper {
        candidates.push(release(tuple(&u.v)));
    }

    candidates
        .iter()
        .any(|v| v.pre.is_empty() && within_bounds(v, lower, upper))
}

fn anchor_witness(
    anchor: (u64, u64, u64),
    lower: &Option<Bound>,
    upper: &Option<Bound>,
) -> bool {
    let least = endpoint(anchor, vec!["0".to_owned()]);
    if !within_bounds(&least, lower, upper) {
        return false;
    }

    let mut candidates = vec![least.clone()];
    let mut successor = least;
    successor.pre.push("0".to_owned());
    candidates.push(successor);

    candidates
        .iter()
        .any(|v| !v.pre.is_empty() && within_bounds(v, lower, upper))
}

fn bounds_obviously_empty(lower: &Option<Bound>, upper: &Option<Bound>) -> bool {
    if let (Some(l), Some(u)) = (lower, upper) {
        let o = compare_versions(&l.v, &u.v);
        o == Ordering::Greater || (o == Ordering::Equal && (!l.inclusive || !u.inclusive))
    } else {
        false
    }
}

fn arms_intersect(a: &Arm, b: &Arm, include_prerelease: bool) -> bool {
    let (al, au, _) = arm_bounds(a);
    let (bl, bu, _) = arm_bounds(b);

    let lower = match (al, bl) {
        (Some(x), Some(y)) => {
            let o = compare_versions(&x.v, &y.v);
            if o == Ordering::Greater || (o == Ordering::Equal && !x.inclusive) {
                Some(x)
            } else if o == Ordering::Less || (o == Ordering::Equal && !y.inclusive) {
                Some(y)
            } else {
                Some(x)
            }
        }
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    };
    let upper = match (au, bu) {
        (Some(x), Some(y)) => {
            let o = compare_versions(&x.v, &y.v);
            if o == Ordering::Less || (o == Ordering::Equal && !x.inclusive) {
                Some(x)
            } else if o == Ordering::Greater || (o == Ordering::Equal && !y.inclusive) {
                Some(y)
            } else {
                Some(x)
            }
        }
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    };

    if stable_witness(&lower, &upper) {
        return true;
    }
    if include_prerelease {
        return !bounds_obviously_empty(&lower, &upper);
    }

    a.anchors
        .iter()
        .filter(|anchor| b.anchors.contains(anchor))
        .copied()
        .any(|anchor| anchor_witness(anchor, &lower, &upper))
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    a.arms
        .iter()
        .any(|x| b.arms.iter().any(|y| arms_intersect(x, y, include_prerelease)))
}

fn render_version(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

fn render_bound(bound: &Option<Bound>, lower: bool) -> String {
    match bound {
        None => "*".to_owned(),
        Some(b) => {
            let op = match (lower, b.inclusive) {
                (true, true) => ">=",
                (true, false) => ">",
                (false, true) => "<=",
                (false, false) => "<",
            };
            format!("{}{}", op, render_version(&b.v))
        }
    }
}

fn canonical_arm(arm: &Arm) -> Option<String> {
    let (lower, upper, _) = arm_bounds(arm);
    if !stable_witness(&lower, &upper)
        && !arm
            .anchors
            .iter()
            .copied()
            .any(|a| anchor_witness(a, &lower, &upper))
    {
        return None;
    }

    let mut anchors = arm.anchors.clone();
    anchors.sort_unstable();
    anchors.dedup();
    let anchor_text = if anchors.is_empty() {
        "-".to_owned()
    } else {
        anchors
            .iter()
            .map(|(a, b, c)| format!("{}.{}.{}", a, b, c))
            .collect::<Vec<_>>()
            .join(",")
    };

    Some(format!(
        "[{};{};pre={}]",
        render_bound(&lower, true),
        render_bound(&upper, false),
        anchor_text
    ))
}

pub fn canonical(r: &Range) -> String {
    let mut arms = r
        .arms
        .iter()
        .filter_map(canonical_arm)
        .collect::<Vec<_>>();
    arms.sort();
    arms.dedup();
    if arms.is_empty() {
        "EMPTY".to_owned()
    } else {
        arms.join(" || ")
    }
}