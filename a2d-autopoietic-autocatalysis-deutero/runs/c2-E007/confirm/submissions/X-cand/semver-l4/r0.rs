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

fn missing_start(b: Option<u8>) -> bool {
    matches!(b, None | Some(b'.' | b'-' | b'+'))
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
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if start == pos {
            if missing_start(bytes.get(pos).copied()) {
                return Err(VersionError::MissingCore { at: pos });
            }
            return Err(VersionError::InvalidChar { at: pos });
        }
        if pos - start > 1 && bytes[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }

        let mut n = 0u64;
        for &c in &bytes[start..pos] {
            n = match n.checked_mul(10).and_then(|x| x.checked_add((c - b'0') as u64)) {
                Some(n) => n,
                None => return Err(VersionError::Overflow { at: start }),
            };
        }
        core[i] = n;

        if i < 2 {
            if pos == bytes.len() || matches!(bytes[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            if bytes[pos] != b'.' {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
        } else if pos < bytes.len() && !matches!(bytes[pos], b'-' | b'+') {
            return Err(VersionError::InvalidChar { at: pos });
        }
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] == b'-' {
        pos += 1;
        let plus = bytes[pos..].iter().position(|&b| b == b'+').map(|i| pos + i);
        let end = plus.unwrap_or(bytes.len());
        parse_identifiers(s, pos, end, true, &mut pre)?;
        pos = end;
        if pos < bytes.len() {
            pos += 1;
            parse_identifiers(s, pos, bytes.len(), false, &mut build)?;
            pos = bytes.len();
        }
    } else if pos < bytes.len() && bytes[pos] == b'+' {
        pos += 1;
        parse_identifiers(s, pos, bytes.len(), false, &mut build)?;
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
    check_pre_numeric_zero: bool,
    out: &mut Vec<String>,
) -> Result<(), VersionError> {
    let b = s.as_bytes();
    let mut p = start;
    loop {
        let id_start = p;
        while p < end && b[p] != b'.' {
            if !b[p].is_ascii_alphanumeric() && b[p] != b'-' {
                return Err(VersionError::InvalidChar { at: p });
            }
            p += 1;
        }
        if id_start == p {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }

        let id = &s[id_start..p];
        if check_pre_numeric_zero
            && id.bytes().all(|c| c.is_ascii_digit())
            && id.len() > 1
            && id.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        out.push(id.to_string());

        if p == end {
            break;
        }
        p += 1;
        if p == end {
            return Err(VersionError::EmptyIdentifier { at: p });
        }
    }
    Ok(())
}

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn is_numeric_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }

    for (x, y) in a.iter().zip(b) {
        let xn = is_numeric_id(x);
        let yn = is_numeric_id(y);
        let c = match (xn, yn) {
            (true, true) => x.len().cmp(&y.len()).then_with(|| x.as_bytes().cmp(y.as_bytes())),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => x.as_bytes().cmp(y.as_bytes()),
        };
        if c != Ordering::Equal {
            return c;
        }
    }
    a.len().cmp(&b.len())
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    core(a).cmp(&core(b)).then_with(|| cmp_pre(&a.pre, &b.pre))
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
}

#[derive(Clone, Debug)]
struct Cmp {
    op: CmpOp,
    version: Version,
}

#[derive(Clone, Copy, Debug)]
enum CmpOp {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
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

fn ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn cmp_match(v: &Version, c: &Cmp) -> bool {
    let o = compare_versions(v, &c.version);
    match c.op {
        CmpOp::Eq => o == Ordering::Equal,
        CmpOp::Lt => o == Ordering::Less,
        CmpOp::Le => o != Ordering::Greater,
        CmpOp::Gt => o == Ordering::Greater,
        CmpOp::Ge => o != Ordering::Less,
    }
}

fn parse_full_operand(s: &str, absolute: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at: absolute, error })
}

fn numeric_field(s: &str) -> Option<u64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if s.len() > 1 && s.as_bytes()[0] == b'0' {
        return None;
    }
    s.parse::<u64>().ok()
}

#[derive(Clone, Debug)]
struct Partial {
    padded: Version,
    k: usize,
    full: Option<Version>,
    all_wild: bool,
}

fn is_wild(s: &str) -> bool {
    matches!(s, "*" | "x" | "X")
}

fn parse_partial(s: &str, absolute: usize) -> Result<Partial, RangeError> {
    let full_like = s.contains(['-', '+']);
    let fields: Vec<&str> = s.split('.').collect();
    let has_three_without_wild = fields.len() >= 3 && !fields.iter().any(|x| is_wild(x));
    if full_like || has_three_without_wild {
        let v = parse_full_operand(s, absolute)?;
        return Ok(Partial {
            padded: v.clone(),
            k: 3,
            full: Some(v),
            all_wild: false,
        });
    }

    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|x| x.is_empty()) {
        return Err(RangeError::Syntax { at: absolute });
    }

    let mut vals = [0u64; 3];
    let mut wild_seen = false;
    let mut k = 0usize;

    for (i, f) in fields.iter().enumerate() {
        if is_wild(f) {
            wild_seen = true;
        } else if let Some(n) = numeric_field(f) {
            if wild_seen {
                return Err(RangeError::Syntax { at: absolute });
            }
            vals[i] = n;
            k += 1;
        } else {
            return Err(RangeError::Syntax { at: absolute });
        }
    }

    Ok(Partial {
        padded: Version {
            major: vals[0],
            minor: vals[1],
            patch: vals[2],
            pre: Vec::new(),
            build: Vec::new(),
        },
        k,
        full: None,
        all_wild: k == 0,
    })
}

fn upper_bump(v: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let mut out = Version {
        major: v.major,
        minor: v.minor,
        patch: v.patch,
        pre: vec!["0".to_string()],
        build: Vec::new(),
    };

    let result = match field {
        0 => out.major.checked_add(1).map(|x| {
            out.major = x;
            out.minor = 0;
            out.patch = 0;
        }),
        1 => out.minor.checked_add(1).map(|x| {
            out.minor = x;
            out.patch = 0;
        }),
        _ => out.patch.checked_add(1).map(|x| out.patch = x),
    };

    if result.is_none() {
        return Err(RangeError::Syntax { at });
    }
    Ok(out)
}

fn add_cmp(arm: &mut Arm, op: CmpOp, version: Version, anchor: bool) {
    if anchor && !version.pre.is_empty() {
        arm.anchors.push(core(&version));
    }
    arm.cmps.push(Cmp { op, version });
}

fn parse_comparator(token: &str, start: usize, arm: &mut Arm) -> Result<(), RangeError> {
    let (op, n) = if token.starts_with(">=") {
        (CmpOp::Ge, 2)
    } else if token.starts_with("<=") {
        (CmpOp::Le, 2)
    } else if token.starts_with('>') {
        (CmpOp::Gt, 1)
    } else if token.starts_with('<') {
        (CmpOp::Lt, 1)
    } else if token.starts_with('=') {
        (CmpOp::Eq, 1)
    } else {
        return Err(RangeError::Syntax { at: start });
    };

    let operand = &token[n..];
    let operand_at = start + n;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: start });
    }

    let v = parse_full_operand(operand, operand_at)?;
    add_cmp(arm, op, v, true);
    Ok(())
}

fn token_parts<'a>(s: &'a str) -> Vec<(usize, &'a str)> {
    let b = s.as_bytes();
    let mut result = Vec::new();
    let mut i = 0usize;

    while i < b.len() {
        while i < b.len() && ws(b[i]) {
            i += 1;
        }
        if i == b.len() {
            break;
        }
        let start = i;
        while i < b.len() && !ws(b[i]) {
            i += 1;
        }
        result.push((start, &s[start..i]));
    }
    result
}

fn parse_hyphen_arm(tokens: &[(usize, &str)], base: usize) -> Result<Arm, RangeError> {
    let dash = tokens.iter().find(|(_, t)| *t == "-").unwrap().0;
    if tokens.len() != 3 || tokens[1].1 != "-" {
        return Err(RangeError::Syntax { at: base + dash });
    }

    let (ls, lt) = tokens[0];
    let (us, ut) = tokens[2];
    let ls = base + ls;
    let us = base + us;

    let lower = parse_partial(lt, ls)?;
    let upper = parse_partial(ut, us)?;
    if lower.k == 0 {
        return Err(RangeError::Syntax { at: ls });
    }
    if upper.k == 0 {
        return Err(RangeError::Syntax { at: us });
    }

    let mut arm = Arm { cmps: Vec::new(), anchors: Vec::new() };
    add_cmp(&mut arm, CmpOp::Ge, lower.padded.clone(), true);

    if let Some(full) = upper.full {
        add_cmp(&mut arm, CmpOp::Le, full, true);
    } else {
        let upper_bound = if upper.k == 1 {
            upper_bump(&upper.padded, 0, us)?
        } else {
            upper_bump(&upper.padded, 1, us)?
        };
        add_cmp(&mut arm, CmpOp::Lt, upper_bound, false);
    }
    Ok(arm)
}

fn parse_bare(token: &str, start: usize, arm: &mut Arm) -> Result<(), RangeError> {
    let p = parse_partial(token, start)?;
    if let Some(v) = p.full {
        add_cmp(arm, CmpOp::Eq, v, true);
        return Ok(());
    }
    if p.k == 0 {
        return Ok(());
    }

    add_cmp(arm, CmpOp::Ge, p.padded.clone(), true);
    if p.k == 1 {
        add_cmp(arm, CmpOp::Lt, upper_bump(&p.padded, 0, start)?, false);
    } else if p.k == 2 {
        add_cmp(arm, CmpOp::Lt, upper_bump(&p.padded, 1, start)?, false);
    } else {
        return Err(RangeError::Syntax { at: start });
    }
    Ok(())
}

fn parse_caret_tilde(
    token: &str,
    start: usize,
    caret: bool,
    arm: &mut Arm,
) -> Result<(), RangeError> {
    let operand = &token[1..];
    let operand_at = start + 1;
    let p = parse_partial(operand, operand_at)?;
    if p.k == 0 {
        return Err(RangeError::Syntax { at: start });
    }

    add_cmp(arm, CmpOp::Ge, p.padded.clone(), true);
    let field = if caret {
        if p.padded.major > 0 || p.k == 1 {
            0
        } else if p.padded.minor > 0 || p.k == 2 {
            1
        } else {
            2
        }
    } else if p.k == 1 {
        0
    } else {
        1
    };
    add_cmp(arm, CmpOp::Lt, upper_bump(&p.padded, field, operand_at)?, false);
    Ok(())
}

fn parse_arm(s: &str, base: usize, tokens: &[(usize, &str)]) -> Result<Arm, RangeError> {
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: base });
    }
    if tokens.iter().any(|(_, t)| *t == "-") {
        return parse_hyphen_arm(tokens, base);
    }

    let mut arm = Arm { cmps: Vec::new(), anchors: Vec::new() };
    for &(local, token) in tokens {
        let start = base + local;
        match token.as_bytes()[0] {
            b'<' | b'>' | b'=' => parse_comparator(token, start, &mut arm)?,
            b'^' => parse_caret_tilde(token, start, true, &mut arm)?,
            b'~' => parse_caret_tilde(token, start, false, &mut arm)?,
            b'*' | b'x' | b'X' => {
                let p = parse_partial(token, start)?;
                if !p.all_wild {
                    return Err(RangeError::Syntax { at: start });
                }
            }
            b'0'..=b'9' => parse_bare(token, start, &mut arm)?,
            _ => return Err(RangeError::Syntax { at: start }),
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let tokens = token_parts(s);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms = Vec::new();
    let mut arm_tokens = Vec::new();
    let mut arm_start = tokens[0].0;
    let mut i = 0usize;

    while i < tokens.len() {
        if tokens[i].1 == "||" {
            if arm_tokens.is_empty() {
                return Err(RangeError::Empty { at: tokens[i].0 });
            }
            arms.push(parse_arm(s, arm_start, &arm_tokens)?);
            arm_tokens.clear();
            i += 1;
            if i == tokens.len() {
                return Err(RangeError::Empty { at: s.len() });
            }
            arm_start = tokens[i].0;
        } else {
            arm_tokens.push(tokens[i]);
            i += 1;
        }
    }

    if arm_tokens.is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(s, arm_start, &arm_tokens)?);
    Ok(Range { arms })
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|arm| {
        arm.cmps.iter().all(|c| cmp_match(v, c))
            && (include_prerelease || v.pre.is_empty() || arm.anchors.contains(&core(v)))
    })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn extrema(candidates: &[String], r: &Range, max: bool) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, s) in candidates.iter().enumerate() {
        parsed.push(parse_version(s).map_err(|error| CandidateError { index, error })?);
    }

    let mut best: Option<usize> = None;
    for i in 0..parsed.len() {
        if !matches(&parsed[i], r) {
            continue;
        }
        if let Some(j) = best {
            let ord = compare_versions(&parsed[i], &parsed[j]);
            if (max && ord == Ordering::Greater) || (!max && ord == Ordering::Less) {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

pub fn max_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, true)
}

pub fn min_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, false)
}

#[derive(Clone, Debug)]
struct Interval {
    lower: Option<Bound>,
    upper: Option<Bound>,
}

fn interval_for(arm: &Arm) -> Interval {
    let mut out = Interval { lower: None, upper: None };
    for c in &arm.cmps {
        match c.op {
            CmpOp::Eq => {
                update_lower(&mut out.lower, c.version.clone(), true);
                update_upper(&mut out.upper, c.version.clone(), true);
            }
            CmpOp::Gt => update_lower(&mut out.lower, c.version.clone(), false),
            CmpOp::Ge => update_lower(&mut out.lower, c.version.clone(), true),
            CmpOp::Lt => update_upper(&mut out.upper, c.version.clone(), false),
            CmpOp::Le => update_upper(&mut out.upper, c.version.clone(), true),
        }
    }
    out
}

fn update_lower(slot: &mut Option<Bound>, v: Version, inclusive: bool) {
    let new = Bound { version: v, inclusive };
    let replace = match slot {
        None => true,
        Some(old) => match compare_versions(&new.version, &old.version) {
            Ordering::Greater => true,
            Ordering::Less => false,
            Ordering::Equal => !new.inclusive && old.inclusive,
        },
    };
    if replace {
        *slot = Some(new);
    }
}

fn update_upper(slot: &mut Option<Bound>, v: Version, inclusive: bool) {
    let new = Bound { version: v, inclusive };
    let replace = match slot {
        None => true,
        Some(old) => match compare_versions(&new.version, &old.version) {
            Ordering::Less => true,
            Ordering::Greater => false,
            Ordering::Equal => !new.inclusive && old.inclusive,
        },
    };
    if replace {
        *slot = Some(new);
    }
}

fn version_string(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

fn bound_string(b: &Option<Bound>, lower: bool) -> String {
    match b {
        None => "*".to_string(),
        Some(b) => {
            let op = match (lower, b.inclusive) {
                (true, true) => ">=",
                (true, false) => ">",
                (false, true) => "<=",
                (false, false) => "<",
            };
            format!("{}{}", op, version_string(&b.version))
        }
    }
}

pub fn canonical(r: &Range) -> String {
    let mut strings = Vec::new();
    for arm in &r.arms {
        if !arm_has_witness(arm, false) {
            continue;
        }
        let i = interval_for(arm);
        let mut anchors = arm.anchors.clone();
        anchors.sort();
        anchors.dedup();
        let anchor_s = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors.iter()
                .map(|(a, b, c)| format!("{a}.{b}.{c}"))
                .collect::<Vec<_>>()
                .join(",")
        };

        strings.push(format!(
            "[{};{};pre={}]",
            bound_string(&i.lower, true),
            bound_string(&i.upper, false),
            anchor_s
        ));
    }

    strings.sort();
    strings.dedup();
    if strings.is_empty() {
        "EMPTY".to_string()
    } else {
        strings.join(" || ")
    }
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for aa in &a.arms {
        for bb in &b.arms {
            let mut cmps = aa.cmps.clone();
            cmps.extend(bb.cmps.clone());
            let mut anchors = aa.anchors.clone();
            anchors.retain(|x| bb.anchors.contains(x));
            let arm = Arm { cmps, anchors };
            if arm_has_witness(&arm, include_prerelease) {
                return true;
            }
        }
    }
    false
}

fn interval_contains(i: &Interval, v: &Version) -> bool {
    if let Some(l) = &i.lower {
        let c = compare_versions(v, &l.version);
        if c == Ordering::Less || (c == Ordering::Equal && !l.inclusive) {
            return false;
        }
    }
    if let Some(u) = &i.upper {
        let c = compare_versions(v, &u.version);
        if c == Ordering::Greater || (c == Ordering::Equal && !u.inclusive) {
            return false;
        }
    }
    true
}

fn tuple_successor(t: (u64, u64, u64)) -> Option<(u64, u64, u64)> {
    if let Some(patch) = t.2.checked_add(1) {
        Some((t.0, t.1, patch))
    } else if let Some(minor) = t.1.checked_add(1) {
        Some((t.0, minor, 0))
    } else {
        t.0.checked_add(1).map(|major| (major, 0, 0))
    }
}

fn stable_witness(i: &Interval) -> bool {
    let first = match &i.lower {
        None => (0, 0, 0),
        Some(l) => {
            let t = core(&l.version);
            if l.version.pre.is_empty() && !l.inclusive {
                match tuple_successor(t) {
                    Some(next) => next,
                    None => return false,
                }
            } else {
                t
            }
        }
    };

    let candidate = Version {
        major: first.0,
        minor: first.1,
        patch: first.2,
        pre: Vec::new(),
        build: Vec::new(),
    };
    interval_contains(i, &candidate)
}

fn least_pre_at_tuple(i: &Interval, t: (u64, u64, u64)) -> bool {
    let least = Version {
        major: t.0,
        minor: t.1,
        patch: t.2,
        pre: vec!["0".to_string()],
        build: Vec::new(),
    };
    if interval_contains(i, &least) {
        return true;
    }

    let mut candidate = least;
    if let Some(lower) = &i.lower {
        if core(&lower.version) == t && !lower.version.pre.is_empty() {
            candidate = lower.version.clone();
            candidate.build.clear();
            if !lower.inclusive {
                candidate.pre.push("0".to_string());
            }
        }
    }
    interval_contains(i, &candidate)
}

fn interval_has_any(i: &Interval) -> bool {
    if stable_witness(i) {
        return true;
    }

    let mut tuples = Vec::new();
    if let Some(l) = &i.lower {
        tuples.push(core(&l.version));
    } else {
        tuples.push((0, 0, 0));
    }
    if let Some(u) = &i.upper {
        tuples.push(core(&u.version));
    }
    if let Some(lower) = &i.lower {
        let t = core(&lower.version);
        tuples.push(t);
        if let Some(next) = tuple_successor(t) {
            tuples.push(next);
        }
    }

    tuples.sort();
    tuples.dedup();
    tuples.into_iter().any(|t| least_pre_at_tuple(i, t))
}

fn arm_has_witness(arm: &Arm, include_pre: bool) -> bool {
    let interval = interval_for(arm);
    if include_pre {
        return interval_has_any(&interval);
    }
    if stable_witness(&interval) {
        return true;
    }

    arm.anchors.iter().copied().any(|t| {
        let least = Version {
            major: t.0,
            minor: t.1,
            patch: t.2,
            pre: vec!["0".to_string()],
            build: Vec::new(),
        };
        interval_contains(&interval, &least)
    })
}

#[derive(Clone, Debug)]
pub enum SetExpr {
    Range(String),
    Union(Vec<SetExpr>),
    Intersection(Vec<SetExpr>),
}

#[derive(Clone, Debug)]
pub struct Constraint {
    pub label: String,
    pub expr: SetExpr,
}

#[derive(Clone, Debug)]
pub struct Package {
    pub name: String,
    pub candidates: Vec<String>,
    pub constraints: Vec<Constraint>,
}

#[derive(Clone, Copy, Debug)]
pub enum Preference {
    Highest,
    Lowest,
    StableFirst,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub package: String,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectError {
    DuplicatePackage { name: String },
    DuplicateLabel { package: String, label: String },
    InvalidCandidate { package: String, index: usize, error: VersionError },
    InvalidRange { package: String, label: String, path: Vec<usize>, error: RangeError },
    NoSolution { package: String, labels: Vec<String> },
}

fn validate_expr(
    expr: &SetExpr,
    package: &str,
    label: &str,
    path: &mut Vec<usize>,
) -> Result<(), SelectError> {
    match expr {
        SetExpr::Range(s) => parse_range(s).map(|_| ()).map_err(|error| SelectError::InvalidRange {
            package: package.to_string(),
            label: label.to_string(),
            path: path.clone(),
            error,
        }),
        SetExpr::Union(children) | SetExpr::Intersection(children) => {
            for (i, child) in children.iter().enumerate() {
                path.push(i);
                validate_expr(child, package, label, path)?;
                path.pop();
            }
            Ok(())
        }
    }
}

fn expr_matches(expr: &SetExpr, v: &Version, include: bool) -> bool {
    match expr {
        SetExpr::Range(s) => parse_range(s)
            .map(|r| matches_with(v, &r, include))
            .unwrap_or(false),
        SetExpr::Union(xs) => xs.iter().any(|x| expr_matches(x, v, include)),
        SetExpr::Intersection(xs) => xs.iter().all(|x| expr_matches(x, v, include)),
    }
}

fn lex_label_vectors(a: &[String], b: &[String]) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        let c = x.as_bytes().cmp(y.as_bytes());
        if c != Ordering::Equal {
            return c;
        }
    }
    a.len().cmp(&b.len())
}

fn conflict_labels(candidates: &[Version], constraints: &[Constraint], include: bool) -> Vec<String> {
    if candidates.is_empty() {
        return Vec::new();
    }

    let n = constraints.len();
    for size in 1..=n {
        let mut indices: Vec<usize> = (0..size).collect();
        let mut best: Option<Vec<String>> = None;

        loop {
            let all_rejected = candidates.iter().all(|v| {
                !indices.iter().all(|&i| expr_matches(&constraints[i].expr, v, include))
            });

            if all_rejected {
                let mut labels: Vec<String> = indices.iter()
                    .map(|&i| constraints[i].label.clone())
                    .collect();
                labels.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

                if best.as_ref().map_or(true, |old| lex_label_vectors(&labels, old) == Ordering::Less) {
                    best = Some(labels);
                }
            }

            let mut p = size;
            while p > 0 {
                p -= 1;
                if indices[p] != p + n - size {
                    break;
                }
            }
            if p == 0 && indices[0] == n - size {
                break;
            }
            indices[p] += 1;
            for j in p + 1..size {
                indices[j] = indices[j - 1] + 1;
            }
        }

        if let Some(best) = best {
            return best;
        }
    }

    Vec::new()
}

pub fn select_packages(
    packages: &[Package],
    preference: Preference,
    include_prerelease: bool,
) -> Result<Vec<Selection>, SelectError> {
    let mut names = std::collections::HashSet::new();
    for p in packages {
        if !names.insert(p.name.clone()) {
            return Err(SelectError::DuplicatePackage { name: p.name.clone() });
        }
    }

    for p in packages {
        let mut labels = std::collections::HashSet::new();
        for c in &p.constraints {
            if !labels.insert(c.label.clone()) {
                return Err(SelectError::DuplicateLabel {
                    package: p.name.clone(),
                    label: c.label.clone(),
                });
            }
        }
    }

    let mut parsed_candidates: Vec<Vec<Version>> = Vec::with_capacity(packages.len());
    for p in packages {
        let mut parsed = Vec::with_capacity(p.candidates.len());
        for (index, s) in p.candidates.iter().enumerate() {
            parsed.push(parse_version(s).map_err(|error| SelectError::InvalidCandidate {
                package: p.name.clone(),
                index,
                error,
            })?);
        }
        parsed_candidates.push(parsed);
    }

    for p in packages {
        for c in &p.constraints {
            validate_expr(&c.expr, &p.name, &c.label, &mut Vec::new())?;
        }
    }

    let mut selections = Vec::new();
    for (pi, p) in packages.iter().enumerate() {
        let parsed = &parsed_candidates[pi];
        let qualifying: Vec<usize> = parsed.iter().enumerate()
            .filter_map(|(i, v)| {
                if p.constraints.iter().all(|c| expr_matches(&c.expr, v, include_prerelease)) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        if qualifying.is_empty() {
            return Err(SelectError::NoSolution {
                package: p.name.clone(),
                labels: conflict_labels(parsed, &p.constraints, include_prerelease),
            });
        }

        let mut best = qualifying[0];
        for &i in qualifying.iter().skip(1) {
            let i_stable = parsed[i].pre.is_empty();
            let best_stable = parsed[best].pre.is_empty();

            if matches!(preference, Preference::StableFirst) && i_stable != best_stable {
                if i_stable {
                    best = i;
                }
                continue;
            }

            let ord = compare_versions(&parsed[i], &parsed[best]);
            match preference {
                Preference::Lowest if ord == Ordering::Less => best = i,
                Preference::Highest | Preference::StableFirst if ord == Ordering::Greater => best = i,
                _ => {}
            }
        }

        selections.push(Selection {
            package: p.name.clone(),
            version: p.candidates[best].clone(),
        });
    }

    Ok(selections)
}