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
    if *pos == start {
        return Err(if *pos == bytes.len() || matches!(bytes[*pos], b'.' | b'-' | b'+') {
            VersionError::MissingCore { at: start }
        } else {
            VersionError::InvalidChar { at: start }
        });
    }
    let digits = &s[start..*pos];
    if digits.len() > 1 && digits.as_bytes()[0] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    digits.parse::<u64>().map_err(|_| VersionError::Overflow { at: start })
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    is_pre: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut id_start = start;
    loop {
        let mut i = id_start;
        while i < end && bytes[i] != b'.' {
            if !(bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }
        if i == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        let id = &s[id_start..i];
        if is_pre && id.bytes().all(is_digit) && id.len() > 1 && id.as_bytes()[0] == b'0' {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        result.push(id.to_owned());
        if i == end {
            break;
        }
        id_start = i + 1;
    }
    Ok(result)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let mut pos = 0;
    let major = parse_core(s, &mut pos)?;
    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        return Err(if pos == s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            VersionError::MissingCore { at: pos }
        } else {
            VersionError::InvalidChar { at: pos }
        });
    }
    pos += 1;
    let minor = parse_core(s, &mut pos)?;
    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        return Err(if pos == s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            VersionError::MissingCore { at: pos }
        } else {
            VersionError::InvalidChar { at: pos }
        });
    }
    pos += 1;
    let patch = parse_core(s, &mut pos)?;

    let bytes = s.as_bytes();
    if pos < bytes.len() && !matches!(bytes[pos], b'-' | b'+') {
        return Err(VersionError::InvalidChar { at: pos });
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();
    if pos < bytes.len() && bytes[pos] == b'-' {
        let start = pos + 1;
        let plus = bytes[start..].iter().position(|&b| b == b'+').map(|n| start + n);
        let end = plus.unwrap_or(bytes.len());
        pre = parse_identifiers(s, start, end, true)?;
        pos = end;
    }
    if pos < bytes.len() && bytes[pos] == b'+' {
        let start = pos + 1;
        build = parse_identifiers(s, start, bytes.len(), false)?;
        pos = bytes.len();
    }
    if pos != bytes.len() {
        return Err(VersionError::InvalidChar { at: pos });
    }
    Ok(Version { major, minor, patch, pre, build })
}

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn numeric_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn compare_id(a: &str, b: &str) -> Ordering {
    match (numeric_id(a), numeric_id(b)) {
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    core(a).cmp(&core(b)).then_with(|| match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (x, y) in a.pre.iter().zip(&b.pre) {
                let c = compare_id(x, y);
                if c != Ordering::Equal {
                    return c;
                }
            }
            a.pre.len().cmp(&b.pre.len())
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
enum Op { Eq, Lt, Le, Gt, Ge }

#[derive(Clone, Debug)]
struct Bound {
    op: Op,
    version: Version,
}

#[derive(Clone, Debug)]
struct Arm {
    bounds: Vec<Bound>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn vcore(m: u64, n: u64, p: u64, pre: Vec<String>) -> Version {
    Version { major: m, minor: n, patch: p, pre, build: Vec::new() }
}

fn zero_pre() -> Vec<String> {
    vec!["0".to_owned()]
}

fn next_core(m: u64, n: u64, p: u64, bump: usize, at: usize) -> Result<Version, RangeError> {
    let (major, minor, patch) = match bump {
        0 => (m.checked_add(1).ok_or(RangeError::Syntax { at })?, 0, 0),
        1 => (m, n.checked_add(1).ok_or(RangeError::Syntax { at })?, 0),
        _ => (m, n, p.checked_add(1).ok_or(RangeError::Syntax { at })?,)
    };
    Ok(vcore(major, minor, patch, zero_pre()))
}

fn partial_fields(s: &str, at: usize) -> Result<(Vec<Option<u64>>, usize), RangeError> {
    if s.is_empty() {
        return Err(RangeError::Syntax { at });
    }
    let fields: Vec<&str> = s.split('.').collect();
    if fields.len() > 3 || fields.iter().any(|f| f.is_empty()) {
        return Err(RangeError::Syntax { at });
    }
    let mut seen_wild = false;
    let mut nums = Vec::new();
    let mut numeric_count = 0;
    for field in fields {
        if matches!(field, "*" | "x" | "X") {
            seen_wild = true;
            nums.push(None);
        } else {
            if seen_wild || !field.bytes().all(is_digit) || (field.len() > 1 && field.starts_with('0')) {
                return Err(RangeError::Syntax { at });
            }
            let n = field.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
            nums.push(Some(n));
            numeric_count += 1;
        }
    }
    Ok((nums, numeric_count))
}

fn full_or_partial(s: &str, at: usize) -> Result<(Version, usize, bool), RangeError> {
    let fields_count = s.split('.').count();
    let full = s.contains('-') || s.contains('+') ||
        (fields_count >= 3 && !s.split('.').any(|x| matches!(x, "*" | "x" | "X")));
    if full {
        let v = parse_version(s).map_err(|error| RangeError::Version { at, error })?;
        return Ok((v, 3, true));
    }
    let (fields, k) = partial_fields(s, at)?;
    let vals = [
        fields.get(0).and_then(|x| *x).unwrap_or(0),
        fields.get(1).and_then(|x| *x).unwrap_or(0),
        fields.get(2).and_then(|x| *x).unwrap_or(0),
    ];
    Ok((vcore(vals[0], vals[1], vals[2], Vec::new()), k, false))
}

fn add_anchor(arm: &mut Arm, v: &Version) {
    if !v.pre.is_empty() {
        arm.anchors.push(core(v));
    }
}

fn parse_operand_token(token: &str, at: usize, arm: &mut Arm) -> Result<(), RangeError> {
    let bytes = token.as_bytes();
    if let Some(opbyte) = bytes.first().copied() {
        if matches!(opbyte, b'=' | b'<' | b'>') {
            let (op, n) = match opbyte {
                b'=' => (Op::Eq, 1),
                b'<' if bytes.get(1) == Some(&b'=') => (Op::Le, 2),
                b'<' => (Op::Lt, 1),
                b'>' if bytes.get(1) == Some(&b'=') => (Op::Ge, 2),
                _ => (Op::Gt, 1),
            };
            let operand_at = at + n;
            let operand = &token[n..];
            if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
                return Err(RangeError::Syntax { at });
            }
            let v = parse_version(operand).map_err(|error| RangeError::Version { at: operand_at, error })?;
            add_anchor(arm, &v);
            arm.bounds.push(Bound { op, version: v });
            return Ok(());
        }
    }

    if token.starts_with('^') || token.starts_with('~') {
        let caret = token.starts_with('^');
        let operand_at = at + 1;
        let operand = &token[1..];
        let (lower, k, _full) = full_or_partial(operand, operand_at)?;
        if k == 0 { return Err(RangeError::Syntax { at }); }
        add_anchor(arm, &lower);
        arm.bounds.push(Bound { op: Op::Ge, version: lower.clone() });
        let (m, n, p) = core(&lower);
        let bump = if caret {
            if m > 0 || k == 1 { 0 } else if n > 0 || k == 2 { 1 } else { 2 }
        } else if k == 1 { 0 } else { 1 };
        let upper = next_core(m, n, p, bump, operand_at)?;
        arm.bounds.push(Bound { op: Op::Lt, version: upper });
        return Ok(());
    }

    if token.starts_with('*') || token.starts_with('x') || token.starts_with('X') ||
        token.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        let (v, k, full) = full_or_partial(token, at)?;
        if full {
            add_anchor(arm, &v);
            arm.bounds.push(Bound { op: Op::Eq, version: v });
        } else if k == 0 {
        } else if k == 1 || k == 2 {
            arm.bounds.push(Bound { op: Op::Ge, version: v.clone() });
            let upper = next_core(v.major, v.minor, v.patch, if k == 1 { 0 } else { 1 }, at)?;
            arm.bounds.push(Bound { op: Op::Lt, version: upper });
        } else {
            arm.bounds.push(Bound { op: Op::Eq, version: v });
        }
        return Ok(());
    }
    Err(RangeError::Syntax { at })
}

fn token_spans(s: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = start;
    while i < end {
        while i < end && is_ws(b[i]) { i += 1; }
        if i == end { break; }
        let st = i;
        while i < end && !is_ws(b[i]) { i += 1; }
        out.push((st, i));
    }
    out
}

fn parse_arm(s: &str, spans: &[(usize, usize)]) -> Result<Arm, RangeError> {
    let mut arm = Arm { bounds: Vec::new(), anchors: Vec::new() };
    let minus_positions: Vec<usize> = spans.iter().filter_map(|&(a, b)| {
        if &s[a..b] == "-" { Some(a) } else { None }
    }).collect();
    if !minus_positions.is_empty() {
        if spans.len() != 3 || &s[spans[1].0..spans[1].1] != "-" {
            return Err(RangeError::Syntax { at: minus_positions[0] });
        }
        let (lo, lk, _lf) = full_or_partial(&s[spans[0].0..spans[0].1], spans[0].0)?;
        let (hi, hk, hf) = full_or_partial(&s[spans[2].0..spans[2].1], spans[2].0)?;
        if lk == 0 { return Err(RangeError::Syntax { at: spans[0].0 }); }
        if hk == 0 { return Err(RangeError::Syntax { at: spans[2].0 }); }
        add_anchor(&mut arm, &lo);
        add_anchor(&mut arm, &hi);
        arm.bounds.push(Bound { op: Op::Ge, version: lo });
        if hf {
            arm.bounds.push(Bound { op: Op::Le, version: hi });
        } else {
            let up = next_core(hi.major, hi.minor, hi.patch, if hk == 1 { 0 } else { 1 }, spans[2].0)?;
            arm.bounds.push(Bound { op: Op::Lt, version: up });
        }
        return Ok(arm);
    }
    for &(a, b) in spans {
        parse_operand_token(&s[a..b], a, &mut arm)?;
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let b = s.as_bytes();
    let mut separators = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(b"||") {
            let left_ok = i == 0 || is_ws(b[i - 1]);
            let right_ok = i + 2 == b.len() || is_ws(b[i + 2]);
            if left_ok && right_ok {
                separators.push(i);
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    let mut arms = Vec::new();
    let mut start = 0;
    for sep in separators.iter().copied() {
        let spans = token_spans(s, start, sep);
        if spans.is_empty() {
            return Err(RangeError::Empty { at: sep });
        }
        arms.push(parse_arm(s, &spans)?);
        start = sep + 2;
    }
    let spans = token_spans(s, start, s.len());
    if spans.is_empty() {
        if separators.is_empty() {
            return Err(RangeError::Empty { at: 0 });
        }
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(s, &spans)?);
    Ok(Range { arms })
}

fn pre_admitted(v: &Version, arm: &Arm) -> bool {
    !v.pre.is_empty() && arm.anchors.iter().any(|&c| c == core(v))
}

fn satisfies_bound(v: &Version, bound: &Bound) -> bool {
    let c = compare_versions(v, &bound.version);
    match bound.op {
        Op::Eq => c == Ordering::Equal,
        Op::Lt => c == Ordering::Less,
        Op::Le => c != Ordering::Greater,
        Op::Gt => c == Ordering::Greater,
        Op::Ge => c != Ordering::Less,
    }
}

fn arm_matches(v: &Version, arm: &Arm, include: bool) -> bool {
    (v.pre.is_empty() || include || pre_admitted(v, arm))
        && arm.bounds.iter().all(|b| satisfies_bound(v, b))
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|a| arm_matches(v, a, include_prerelease))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
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
            let c = compare_versions(v, &parsed[j]);
            if (maximum && c == Ordering::Greater) || (!maximum && c == Ordering::Less) {
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct Interval {
    lower: Option<(Version, bool)>,
    upper: Option<(Version, bool)>,
}

fn bound_interval(bounds: &[Bound]) -> Interval {
    let mut interval = Interval { lower: None, upper: None };
    for b in bounds {
        match b.op {
            Op::Eq => {
                set_lower(&mut interval, b.version.clone(), true);
                set_upper(&mut interval, b.version.clone(), true);
            }
            Op::Gt => set_lower(&mut interval, b.version.clone(), false),
            Op::Ge => set_lower(&mut interval, b.version.clone(), true),
            Op::Lt => set_upper(&mut interval, b.version.clone(), false),
            Op::Le => set_upper(&mut interval, b.version.clone(), true),
        }
    }
    interval
}

fn set_lower(i: &mut Interval, v: Version, inclusive: bool) {
    let replace = match &i.lower {
        None => true,
        Some((old, old_inc)) => {
            let c = compare_versions(&v, old);
            c == Ordering::Greater || (c == Ordering::Equal && !inclusive && *old_inc)
        }
    };
    if replace { i.lower = Some((v, inclusive)); }
}

fn set_upper(i: &mut Interval, v: Version, inclusive: bool) {
    let replace = match &i.upper {
        None => true,
        Some((old, old_inc)) => {
            let c = compare_versions(&v, old);
            c == Ordering::Less || (c == Ordering::Equal && !inclusive && *old_inc)
        }
    };
    if replace { i.upper = Some((v, inclusive)); }
}

fn interval_consistent(i: &Interval) -> bool {
    match (&i.lower, &i.upper) {
        (Some((l, li)), Some((u, ui))) => match compare_versions(l, u) {
            Ordering::Less => true,
            Ordering::Greater => false,
            Ordering::Equal => *li && *ui,
        },
        _ => true,
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

fn canonical_arm(arm: &Arm) -> Option<String> {
    let i = bound_interval(&arm.bounds);
    if !interval_consistent(&i) { return None; }
    let mut anchors = arm.anchors.clone();
    anchors.sort();
    anchors.dedup();
    let a = anchors.iter().map(|(m, n, p)| format!("{m}.{n}.{p}")).collect::<Vec<_>>().join(",");
    let lower = i.lower.as_ref().map_or("*".to_owned(), |(v, inc)| {
        format!("{}{}", if *inc { ">=" } else { ">" }, version_string(v))
    });
    let upper = i.upper.as_ref().map_or("*".to_owned(), |(v, inc)| {
        format!("{}{}", if *inc { "<=" } else { "<" }, version_string(v))
    });
    Some(format!("[{lower};{upper};pre={}]", if a.is_empty() { "-" } else { &a }))
}

pub fn canonical(r: &Range) -> String {
    let mut arms: Vec<String> = r.arms.iter().filter_map(canonical_arm).collect();
    arms.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    arms.dedup();
    if arms.is_empty() { "EMPTY".to_owned() } else { arms.join(" || ") }
}

fn arm_has_stable_witness(i: &Interval) -> bool {
    match (&i.lower, &i.upper) {
        (Some((l, li)), Some((u, ui))) => match compare_versions(l, u) {
            Ordering::Less => true,
            Ordering::Greater => false,
            Ordering::Equal => *li && *ui && l.pre.is_empty(),
        },
        _ => true,
    }
}

fn intersects_arms(a: &Arm, b: &Arm, include: bool) -> bool {
    let mut i = bound_interval(&a.bounds);
    let j = bound_interval(&b.bounds);
    if let Some((v, inc)) = j.lower { set_lower(&mut i, v, inc); }
    if let Some((v, inc)) = j.upper { set_upper(&mut i, v, inc); }
    if !interval_consistent(&i) { return false; }
    if include { return true; }
    if arm_has_stable_witness(&i) { return true; }
    let mut anchors = a.anchors.clone();
    anchors.extend(b.anchors.iter().copied());
    anchors.sort();
    anchors.dedup();
    anchors.into_iter().any(|c| {
        let va = vcore(c.0, c.1, c.2, zero_pre());
        let in_bounds = |x: &Version| {
            i.lower.as_ref().map_or(true, |(l, inc)| {
                let ord = compare_versions(x, l);
                ord == Ordering::Greater || (ord == Ordering::Equal && *inc)
            }) && i.upper.as_ref().map_or(true, |(u, inc)| {
                let ord = compare_versions(x, u);
                ord == Ordering::Less || (ord == Ordering::Equal && *inc)
            })
        };
        in_bounds(&va) && pre_admitted(&va, a) && pre_admitted(&va, b)
    })
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    a.arms.iter().any(|x| b.arms.iter().any(|y| intersects_arms(x, y, include_prerelease)))
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
    out: &mut Vec<(Vec<usize>, Range)>,
) -> Result<(), SelectError> {
    match expr {
        SetExpr::Range(s) => {
            let r = parse_range(s).map_err(|error| SelectError::InvalidRange {
                package: package.to_owned(),
                label: label.to_owned(),
                path: path.clone(),
                error,
            })?;
            out.push((path.clone(), r));
        }
        SetExpr::Union(xs) | SetExpr::Intersection(xs) => {
            for (i, x) in xs.iter().enumerate() {
                path.push(i);
                validate_expr(x, package, label, path, out)?;
                path.pop();
            }
        }
    }
    Ok(())
}

fn eval_expr(
    expr: &SetExpr,
    v: &Version,
    include: bool,
    leaves: &[(Vec<usize>, Range)],
    path: &mut Vec<usize>,
) -> bool {
    match expr {
        SetExpr::Range(_) => leaves.iter().find(|(p, _)| p == path)
            .is_some_and(|(_, r)| matches_with(v, r, include)),
        SetExpr::Union(xs) => xs.iter().enumerate().any(|(i, x)| {
            path.push(i);
            let ok = eval_expr(x, v, include, leaves, path);
            path.pop();
            ok
        }),
        SetExpr::Intersection(xs) => xs.iter().enumerate().all(|(i, x)| {
            path.push(i);
            let ok = eval_expr(x, v, include, leaves, path);
            path.pop();
            ok
        }),
    }
}

fn subset_conflict(
    constraints: &[Constraint],
    parsed: &[Vec<(Vec<usize>, Range)>],
    candidates: &[Version],
    include: bool,
) -> Vec<String> {
    for size in 1..=constraints.len() {
        let mut chosen = Vec::new();
        if let Some(labels) = search_conflict(
            0, size, &mut chosen, constraints, parsed, candidates, include,
        ) {
            return labels;
        }
    }
    Vec::new()
}

fn search_conflict(
    start: usize,
    need: usize,
    chosen: &mut Vec<usize>,
    constraints: &[Constraint],
    parsed: &[Vec<(Vec<usize>, Range)>],
    candidates: &[Version],
    include: bool,
) -> Option<Vec<String>> {
    if need == 0 {
        let any = candidates.iter().any(|v| chosen.iter().all(|&idx| {
            let mut path = Vec::new();
            eval_expr(&constraints[idx].expr, v, include, &parsed[idx], &mut path)
        }));
        if any { return None; }
        let mut labels: Vec<String> = chosen.iter().map(|&i| constraints[i].label.clone()).collect();
        labels.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        return Some(labels);
    }
    if constraints.len().saturating_sub(start) < need { return None; }
    let mut best: Option<Vec<String>> = None;
    for i in start..=constraints.len() - need {
        chosen.push(i);
        if let Some(candidate) = search_conflict(
            i + 1, need - 1, chosen, constraints, parsed, candidates, include,
        ) {
            if best.as_ref().map_or(true, |b| {
                candidate.iter().map(String::as_bytes).cmp(b.iter().map(String::as_bytes))
                    == Ordering::Less
            }) {
                best = Some(candidate);
            }
        }
        chosen.pop();
    }
    best
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

    let mut versions = Vec::new();
    for p in packages {
        let mut vs = Vec::new();
        for (index, s) in p.candidates.iter().enumerate() {
            vs.push(parse_version(s).map_err(|error| SelectError::InvalidCandidate {
                package: p.name.clone(),
                index,
                error,
            })?);
        }
        versions.push(vs);
    }

    let mut parsed: Vec<Vec<Vec<(Vec<usize>, Range)>>> = Vec::new();
    for p in packages {
        let mut package_ranges = Vec::new();
        for c in &p.constraints {
            let mut leaves = Vec::new();
            validate_expr(&c.expr, &p.name, &c.label, &mut Vec::new(), &mut leaves)?;
            package_ranges.push(leaves);
        }
        parsed.push(package_ranges);
    }

    let mut selections = Vec::new();
    for (pi, p) in packages.iter().enumerate() {
        let mut qualifying = Vec::new();
        'candidate: for (i, v) in versions[pi].iter().enumerate() {
            for (ci, c) in p.constraints.iter().enumerate() {
                if !eval_expr(&c.expr, v, include_prerelease, &parsed[pi][ci], &mut Vec::new()) {
                    continue 'candidate;
                }
            }
            qualifying.push(i);
        }

        if qualifying.is_empty() {
            let labels = if p.candidates.is_empty() {
                Vec::new()
            } else {
                subset_conflict(&p.constraints, &parsed[pi], &versions[pi], include_prerelease)
            };
            return Err(SelectError::NoSolution { package: p.name.clone(), labels });
        }

        let stable_first = matches!(preference, Preference::StableFirst)
            && qualifying.iter().any(|&i| versions[pi][i].pre.is_empty());
        let mut best = qualifying[0];
        for &i in qualifying.iter().skip(1) {
            let candidate_stable = versions[pi][i].pre.is_empty();
            let best_stable = versions[pi][best].pre.is_empty();
            if stable_first && candidate_stable != best_stable {
                if candidate_stable { best = i; }
                continue;
            }
            let ord = compare_versions(&versions[pi][i], &versions[pi][best]);
            let prefer_high = !matches!(preference, Preference::Lowest);
            if (prefer_high && ord == Ordering::Greater) || (!prefer_high && ord == Ordering::Less) {
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