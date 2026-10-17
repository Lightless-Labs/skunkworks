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

fn parse_core(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && is_digit(bytes[*pos]) {
        *pos += 1;
    }
    if start == *pos {
        if *pos == bytes.len() || matches!(bytes[*pos], b'.' | b'-' | b'+') {
            return Err(VersionError::MissingCore { at: *pos });
        }
        return Err(VersionError::InvalidChar { at: *pos });
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
    is_pre: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut id_start = start;
    loop {
        let mut p = id_start;
        while p < end && bytes[p] != b'.' {
            if !(bytes[p].is_ascii_alphanumeric() || bytes[p] == b'-') {
                return Err(VersionError::InvalidChar { at: p });
            }
            p += 1;
        }
        if p == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        let id = &s[id_start..p];
        if is_pre && id.len() > 1 && id.bytes().all(is_digit) && id.as_bytes()[0] == b'0' {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        out.push(id.to_string());
        if p == end {
            break;
        }
        id_start = p + 1;
    }
    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let mut pos = 0;
    let major = parse_core(s, &mut pos)?;
    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        if pos >= s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;
    let minor = parse_core(s, &mut pos)?;
    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        if pos >= s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;
    let patch = parse_core(s, &mut pos)?;

    let mut pre = Vec::new();
    let mut build = Vec::new();
    if pos < s.len() && s.as_bytes()[pos] == b'-' {
        let pre_start = pos + 1;
        let plus = s[pre_start..].find('+').map(|n| pre_start + n);
        let pre_end = plus.unwrap_or(s.len());
        pre = parse_identifiers(s, pre_start, pre_end, true)?;
        pos = pre_end;
    }
    if pos < s.len() && s.as_bytes()[pos] == b'+' {
        let build_start = pos + 1;
        build = parse_identifiers(s, build_start, s.len(), false)?;
        pos = s.len();
    }
    if pos != s.len() {
        return Err(VersionError::InvalidChar { at: pos });
    }
    Ok(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn numeric_id_cmp(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

fn is_numeric_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn pre_cmp(a: &[String], b: &[String]) -> Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        let xn = is_numeric_id(x);
        let yn = is_numeric_id(y);
        let ord = match (xn, yn) {
            (true, true) => numeric_id_cmp(x, y),
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
        .then_with(|| match (a.pre.is_empty(), b.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => pre_cmp(&a.pre, &b.pre),
        })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Debug)]
struct Comparator {
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
struct Arm {
    comps: Vec<Comparator>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn version_tuple(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn endpoint(major: u64, minor: u64, patch: u64) -> Version {
    Version {
        major,
        minor,
        patch,
        pre: Vec::new(),
        build: Vec::new(),
    }
}

fn add_cmp(arm: &mut Arm, op: Op, version: Version, grants_admission: bool) {
    if grants_admission && !version.pre.is_empty() {
        arm.anchors.push(version_tuple(&version));
    }
    arm.comps.push(Comparator { op, version });
}

#[derive(Clone)]
struct Tok<'a> {
    text: &'a str,
    start: usize,
}

fn tokens(s: &str) -> Vec<Tok<'_>> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut p = 0;
    while p < bytes.len() {
        while p < bytes.len() && is_ws(bytes[p]) {
            p += 1;
        }
        if p == bytes.len() {
            break;
        }
        let start = p;
        while p < bytes.len() && !is_ws(bytes[p]) {
            p += 1;
        }
        out.push(Tok {
            text: &s[start..p],
            start,
        });
    }
    out
}

fn parse_operand(
    token: &str,
    at: usize,
) -> Result<(Option<Version>, Vec<Option<u64>>), RangeError> {
    if token.contains('-') || token.contains('+') {
        return parse_version(token)
            .map(|v| (Some(v), Vec::new()))
            .map_err(|error| RangeError::Version { at, error });
    }

    let fields: Vec<&str> = token.split('.').collect();
    let has_wild = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));
    if fields.len() >= 3 && !has_wild {
        return parse_version(token)
            .map(|v| (Some(v), Vec::new()))
            .map_err(|error| RangeError::Version { at, error });
    }

    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at });
    }
    let mut vals = Vec::new();
    let mut wildcard_seen = false;
    for f in fields {
        if f.is_empty() {
            return Err(RangeError::Syntax { at });
        }
        if matches!(f, "*" | "x" | "X") {
            wildcard_seen = true;
            vals.push(None);
        } else {
            if wildcard_seen
                || !f.bytes().all(is_digit)
                || (f.len() > 1 && f.as_bytes()[0] == b'0')
            {
                return Err(RangeError::Syntax { at });
            }
            let n = f.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
            vals.push(Some(n));
        }
    }
    Ok((None, vals))
}

fn padded(vals: &[Option<u64>]) -> (u64, u64, u64, usize) {
    let mut nums = Vec::new();
    for v in vals {
        match v {
            Some(n) => nums.push(*n),
            None => break,
        }
    }
    let k = nums.len();
    (
        *nums.first().unwrap_or(&0),
        *nums.get(1).unwrap_or(&0),
        *nums.get(2).unwrap_or(&0),
        k,
    )
}

fn full_from_vals(vals: &[Option<u64>]) -> Version {
    let (a, b, c, _) = padded(vals);
    endpoint(a, b, c)
}

fn next_bound(vals: &[Option<u64>], mode: u8, at: usize) -> Result<Version, RangeError> {
    let (mut major, mut minor, mut patch, k) = padded(vals);
    if mode == 1 {
        if major > 0 || k == 1 {
            major = major.checked_add(1).ok_or(RangeError::Syntax { at })?;
            minor = 0;
            patch = 0;
        } else if minor > 0 || k == 2 {
            minor = minor.checked_add(1).ok_or(RangeError::Syntax { at })?;
            patch = 0;
        } else {
            patch = patch.checked_add(1).ok_or(RangeError::Syntax { at })?;
        }
    } else if k == 1 {
        major = major.checked_add(1).ok_or(RangeError::Syntax { at })?;
        minor = 0;
        patch = 0;
    } else {
        minor = minor.checked_add(1).ok_or(RangeError::Syntax { at })?;
        patch = 0;
    }
    let mut v = endpoint(major, minor, patch);
    v.pre.push("0".to_string());
    Ok(v)
}

fn parse_comparator(token: &str, start: usize, arm: &mut Arm) -> Result<(), RangeError> {
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
        return Err(RangeError::Syntax { at: start });
    };
    let operand = &token[n..];
    let operand_start = start + n;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: start });
    }
    let version = parse_version(operand)
        .map_err(|error| RangeError::Version { at: operand_start, error })?;
    add_cmp(arm, op, version, true);
    Ok(())
}

fn parse_arm(ts: &[Tok<'_>]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        comps: Vec::new(),
        anchors: Vec::new(),
    };

    let standalone_minus: Vec<usize> = ts
        .iter()
        .enumerate()
        .filter_map(|(i, t)| if t.text == "-" { Some(i) } else { None })
        .collect();

    if !standalone_minus.is_empty() {
        if ts.len() != 3 || standalone_minus.as_slice() != [1] {
            return Err(RangeError::Syntax {
                at: ts[standalone_minus[0]].start,
            });
        }

        let (lo_full, lo_vals) = parse_operand(ts[0].text, ts[0].start)?;
        let (hi_full, hi_vals) = parse_operand(ts[2].text, ts[2].start)?;

        let lo_k = lo_full.as_ref().map(|_| 3).unwrap_or_else(|| padded(&lo_vals).3);
        let hi_k = hi_full.as_ref().map(|_| 3).unwrap_or_else(|| padded(&hi_vals).3);
        if lo_k == 0 {
            return Err(RangeError::Syntax { at: ts[0].start });
        }
        if hi_k == 0 {
            return Err(RangeError::Syntax { at: ts[2].start });
        }

        let lo = lo_full.unwrap_or_else(|| full_from_vals(&lo_vals));
        add_cmp(&mut arm, Op::Ge, lo, true);

        if let Some(hi) = hi_full {
            add_cmp(&mut arm, Op::Le, hi, true);
        } else if hi_k == 1 || hi_k == 2 {
            let upper = next_bound(&hi_vals, 2, ts[2].start)?;
            add_cmp(&mut arm, Op::Lt, upper, false);
        } else {
            add_cmp(
                &mut arm,
                Op::Le,
                full_from_vals(&hi_vals),
                true,
            );
        }
        return Ok(arm);
    }

    for t in ts {
        let s = t.text;
        if s.starts_with('^') || s.starts_with('~') {
            let mode = if s.as_bytes()[0] == b'^' { 1 } else { 2 };
            let operand_start = t.start + 1;
            let (full, vals) = parse_operand(&s[1..], operand_start)?;
            let k = full.as_ref().map(|_| 3).unwrap_or_else(|| padded(&vals).3);
            if k == 0 {
                return Err(RangeError::Syntax { at: t.start });
            }
            let lower = full.unwrap_or_else(|| full_from_vals(&vals));
            add_cmp(&mut arm, Op::Ge, lower, true);
            let upper = next_bound(&vals, mode, operand_start)?;
            add_cmp(&mut arm, Op::Lt, upper, false);
        } else if s.starts_with('<') || s.starts_with('>') || s.starts_with('=') {
            parse_comparator(s, t.start, &mut arm)?;
        } else if s.starts_with(|c: char| c.is_ascii_digit())
            || matches!(s.as_bytes().first(), Some(b'*' | b'x' | b'X'))
        {
            let (full, vals) = parse_operand(s, t.start)?;
            if let Some(v) = full {
                add_cmp(&mut arm, Op::Eq, v, true);
            } else {
                let k = padded(&vals).3;
                if k == 1 || k == 2 {
                    let lower = full_from_vals(&vals);
                    add_cmp(&mut arm, Op::Ge, lower, true);
                    let upper = next_bound(&vals, 2, t.start)?;
                    add_cmp(&mut arm, Op::Lt, upper, false);
                }
            }
        } else {
            return Err(RangeError::Syntax { at: t.start });
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
    let mut cur = Vec::new();
    let mut previous_or = None;
    for t in ts {
        if t.text == "||" {
            if cur.is_empty() {
                return Err(RangeError::Empty {
                    at: previous_or.unwrap_or(t.start),
                });
            }
            arms.push(parse_arm(&cur)?);
            cur.clear();
            previous_or = Some(t.start);
        } else {
            cur.push(t);
        }
    }
    if cur.is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(&cur)?);
    Ok(Range { arms })
}

fn compare_one(v: &Version, c: &Comparator) -> bool {
    match c.op {
        Op::Eq => compare_versions(v, &c.version) == Ordering::Equal,
        Op::Lt => compare_versions(v, &c.version) == Ordering::Less,
        Op::Le => compare_versions(v, &c.version) != Ordering::Greater,
        Op::Gt => compare_versions(v, &c.version) == Ordering::Greater,
        Op::Ge => compare_versions(v, &c.version) != Ordering::Less,
    }
}

fn arm_match(v: &Version, arm: &Arm, include: bool) -> bool {
    if !v.pre.is_empty() && !include && !arm.anchors.contains(&version_tuple(v)) {
        return false;
    }
    arm.comps.iter().all(|c| compare_one(v, c))
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms
        .iter()
        .any(|a| arm_match(v, a, include_prerelease))
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
    let parsed: Vec<Version> = candidates
        .iter()
        .enumerate()
        .map(|(index, s)| {
            parse_version(s).map_err(|error| CandidateError { index, error })
        })
        .collect::<Result<_, _>>()?;

    let mut best: Option<usize> = None;
    for (i, v) in parsed.iter().enumerate() {
        if !matches(v, r) {
            continue;
        }
        if let Some(j) = best {
            let ord = compare_versions(v, &parsed[j]);
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

#[derive(Clone, Copy)]
enum BoundKind {
    Lower,
    Upper,
}

fn choose_bound(
    slot: &mut Option<(Version, bool)>,
    version: Version,
    inclusive: bool,
    kind: BoundKind,
) {
    let replace = if let Some((old, old_inc)) = slot {
        let cmp = compare_versions(&version, old);
        match kind {
            BoundKind::Lower => {
                cmp == Ordering::Greater || (cmp == Ordering::Equal && !inclusive && *old_inc)
            }
            BoundKind::Upper => {
                cmp == Ordering::Less || (cmp == Ordering::Equal && !inclusive && *old_inc)
            }
        }
    } else {
        true
    };
    if replace {
        *slot = Some((version, inclusive));
    }
}

fn effective_bounds(
    arm: &Arm,
) -> (
    Option<(Version, bool)>,
    Option<(Version, bool)>,
    Vec<(u64, u64, u64)>,
) {
    let mut lower = None;
    let mut upper = None;
    for c in &arm.comps {
        match c.op {
            Op::Eq => {
                choose_bound(&mut lower, c.version.clone(), true, BoundKind::Lower);
                choose_bound(&mut upper, c.version.clone(), true, BoundKind::Upper);
            }
            Op::Ge | Op::Gt => choose_bound(
                &mut lower,
                c.version.clone(),
                matches!(c.op, Op::Ge),
                BoundKind::Lower,
            ),
            Op::Le | Op::Lt => choose_bound(
                &mut upper,
                c.version.clone(),
                matches!(c.op, Op::Le),
                BoundKind::Upper,
            ),
        }
    }
    (lower, upper, arm.anchors.clone())
}

fn version_without_build(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

fn fmt_bound(bound: &Option<(Version, bool)>, lower: bool) -> String {
    match bound {
        None => "*".to_string(),
        Some((v, inc)) => {
            let op = match (lower, inc) {
                (true, true) => ">=",
                (true, false) => ">",
                (false, true) => "<=",
                (false, false) => "<",
            };
            format!("{}{}", op, version_without_build(v))
        }
    }
}

fn arm_has_witness(arm: &Arm) -> bool {
    if arm.anchors.iter().any(|tuple| {
        let probe = Version {
            major: tuple.0,
            minor: tuple.1,
            patch: tuple.2,
            pre: vec!["0".into()],
            build: Vec::new(),
        };
        arm.comps.iter().all(|c| compare_one(&probe, c))
    }) {
        return true;
    }

    let (lower, upper, _) = effective_bounds(arm);
    let lo = lower.as_ref().map(|x| &x.0);
    let hi = upper.as_ref().map(|x| &x.0);

    if let (Some(l), Some(u)) = (lo, hi) {
        let cmp = compare_versions(l, u);
        if cmp == Ordering::Greater {
            return false;
        }
        if cmp == Ordering::Equal {
            let li = lower.as_ref().unwrap().1;
            let ui = upper.as_ref().unwrap().1;
            if !li || !ui {
                return false;
            }
            let stable = Version {
                major: l.major,
                minor: l.minor,
                patch: l.patch,
                pre: Vec::new(),
                build: Vec::new(),
            };
            return arm.comps.iter().all(|c| compare_one(&stable, c));
        }
    }

    if let Some((l, inclusive)) = &lower {
        let mut candidate = l.clone();
        candidate.build.clear();
        if !inclusive {
            if !candidate.pre.is_empty() {
                candidate.pre.push("0".into());
            } else if let Some(next) = next_core(candidate.major, candidate.minor, candidate.patch) {
                candidate = Version {
                    major: next.0,
                    minor: next.1,
                    patch: next.2,
                    pre: vec!["0".into()],
                    build: Vec::new(),
                };
            } else {
                return false;
            }
        }
        if upper.as_ref().is_some_and(|(u, inc)| {
            let ord = compare_versions(&candidate, u);
            ord == Ordering::Greater || (ord == Ordering::Equal && !*inc)
        }) {
            return false;
        }
        return arm.comps.iter().all(|c| compare_one(&candidate, c));
    }

    if let Some((u, inclusive)) = &upper {
        let candidate = if *inclusive {
            u.clone()
        } else if !u.pre.is_empty() {
            let mut v = u.clone();
            v.pre.push("0".into());
            v
        } else {
            let mut v = u.clone();
            v.pre = vec!["0".into()];
            v
        };
        return arm.comps.iter().all(|c| compare_one(&candidate, c));
    }

    true
}

fn next_core(major: u64, minor: u64, patch: u64) -> Option<(u64, u64, u64)> {
    if let Some(p) = patch.checked_add(1) {
        Some((major, minor, p))
    } else if let Some(m) = minor.checked_add(1) {
        Some((major, m, 0))
    } else {
        major.checked_add(1).map(|a| (a, 0, 0))
    }
}

pub fn canonical(r: &Range) -> String {
    let mut out = Vec::new();
    for arm in &r.arms {
        if !arm_has_witness(arm) {
            continue;
        }
        let (lo, hi, mut anchors) = effective_bounds(arm);
        anchors.sort_unstable();
        anchors.dedup();
        let anchor_text = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors
                .iter()
                .map(|(a, b, c)| format!("{}.{}.{}", a, b, c))
                .collect::<Vec<_>>()
                .join(",")
        };
        out.push(format!(
            "[{};{};pre={}]",
            fmt_bound(&lo, true),
            fmt_bound(&hi, false),
            anchor_text
        ));
    }
    out.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    out.dedup();
    if out.is_empty() {
        "EMPTY".to_string()
    } else {
        out.join(" || ")
    }
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    if include_prerelease {
        for aa in &a.arms {
            for bb in &b.arms {
                let mut combined = aa.clone();
                combined.comps.extend(bb.comps.clone());
                combined.anchors.clear();
                if arm_has_witness(&combined) {
                    return true;
                }
            }
        }
        return false;
    }

    for aa in &a.arms {
        for bb in &b.arms {
            let mut combined = aa.clone();
            combined.comps.extend(bb.comps.clone());
            combined.anchors = aa
                .anchors
                .iter()
                .copied()
                .filter(|x| bb.anchors.contains(x))
                .collect();
            if arm_has_witness(&combined) {
                return true;
            }
        }
    }
    false
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
    InvalidCandidate {
        package: String,
        index: usize,
        error: VersionError,
    },
    InvalidRange {
        package: String,
        label: String,
        path: Vec<usize>,
        error: RangeError,
    },
    NoSolution { package: String, labels: Vec<String> },
}

fn eval_expr(expr: &SetExpr, v: &Version, include: bool) -> bool {
    match expr {
        SetExpr::Range(s) => parse_range(s)
            .map(|r| matches_with(v, &r, include))
            .unwrap_or(false),
        SetExpr::Union(xs) => xs.iter().any(|x| eval_expr(x, v, include)),
        SetExpr::Intersection(xs) => xs.iter().all(|x| eval_expr(x, v, include)),
    }
}

fn validate_expr(
    expr: &SetExpr,
    package: &str,
    label: &str,
    path: &mut Vec<usize>,
) -> Result<(), SelectError> {
    match expr {
        SetExpr::Range(s) => parse_range(s)
            .map(|_| ())
            .map_err(|error| SelectError::InvalidRange {
                package: package.to_string(),
                label: label.to_string(),
                path: path.clone(),
                error,
            }),
        SetExpr::Union(xs) | SetExpr::Intersection(xs) => {
            for (i, x) in xs.iter().enumerate() {
                path.push(i);
                validate_expr(x, package, label, path)?;
                path.pop();
            }
            Ok(())
        }
    }
}

fn minimum_conflict(p: &Package, candidates: &[Version], include: bool) -> Vec<String> {
    let n = p.constraints.len();
    for size in 1..=n {
        let mut found: Vec<Vec<String>> = Vec::new();

        fn search(
            p: &Package,
            candidates: &[Version],
            include: bool,
            size: usize,
            start: usize,
            chosen: &mut Vec<usize>,
            found: &mut Vec<Vec<String>>,
        ) {
            if chosen.len() == size {
                let any = candidates.iter().any(|v| {
                    chosen
                        .iter()
                        .all(|&i| eval_expr(&p.constraints[i].expr, v, include))
                });
                if !any {
                    let mut labels: Vec<String> = chosen
                        .iter()
                        .map(|&i| p.constraints[i].label.clone())
                        .collect();
                    labels.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
                    found.push(labels);
                }
                return;
            }
            for i in start..p.constraints.len() {
                chosen.push(i);
                search(p, candidates, include, size, i + 1, chosen, found);
                chosen.pop();
            }
        }

        search(
            p,
            candidates,
            include,
            size,
            0,
            &mut Vec::new(),
            &mut found,
        );
        if !found.is_empty() {
            found.sort_by(|a, b| {
                for (x, y) in a.iter().zip(b.iter()) {
                    let c = x.as_bytes().cmp(y.as_bytes());
                    if c != Ordering::Equal {
                        return c;
                    }
                }
                a.len().cmp(&b.len())
            });
            return found.remove(0);
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
            return Err(SelectError::DuplicatePackage {
                name: p.name.clone(),
            });
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

    let mut parsed_all = Vec::new();
    for p in packages {
        let mut parsed = Vec::new();
        for (index, s) in p.candidates.iter().enumerate() {
            parsed.push(parse_version(s).map_err(|error| SelectError::InvalidCandidate {
                package: p.name.clone(),
                index,
                error,
            })?);
        }
        parsed_all.push(parsed);
    }

    for p in packages {
        for c in &p.constraints {
            validate_expr(&c.expr, &p.name, &c.label, &mut Vec::new())?;
        }
    }

    let mut selections = Vec::new();
    for (pi, p) in packages.iter().enumerate() {
        let parsed = &parsed_all[pi];
        let good: Vec<usize> = parsed
            .iter()
            .enumerate()
            .filter_map(|(i, v)| {
                if p.constraints
                    .iter()
                    .all(|c| eval_expr(&c.expr, v, include_prerelease))
                {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        if good.is_empty() {
            let labels = if p.candidates.is_empty() {
                Vec::new()
            } else {
                minimum_conflict(p, parsed, include_prerelease)
            };
            return Err(SelectError::NoSolution {
                package: p.name.clone(),
                labels,
            });
        }

        let mut best = good[0];
        for &i in &good[1..] {
            let better = match preference {
                Preference::Highest => {
                    compare_versions(&parsed[i], &parsed[best]) == Ordering::Greater
                }
                Preference::Lowest => {
                    compare_versions(&parsed[i], &parsed[best]) == Ordering::Less
                }
                Preference::StableFirst => {
                    let si = parsed[i].pre.is_empty();
                    let sb = parsed[best].pre.is_empty();
                    if si != sb {
                        si
                    } else {
                        compare_versions(&parsed[i], &parsed[best]) == Ordering::Greater
                    }
                }
            };
            if better {
                best = i;
            }
        }

        selections.push(Selection {
            package: p.name.clone(),
            version: p.candidates[best].clone(),
        });
    }
    Ok(selections)
}