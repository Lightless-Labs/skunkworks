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

fn parse_core_number(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && is_digit(bytes[*pos]) {
        *pos += 1;
    }
    if start == *pos {
        if start == bytes.len() || matches!(bytes[start], b'.' | b'-' | b'+') {
            return Err(VersionError::MissingCore { at: start });
        }
        return Err(VersionError::InvalidChar { at: start });
    }
    let raw = &s[start..*pos];
    if raw.len() > 1 && raw.starts_with('0') {
        return Err(VersionError::LeadingZero { at: start });
    }
    raw.parse::<u64>().map_err(|_| VersionError::Overflow { at: start })
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    pre: bool,
) -> Result<Vec<String>, VersionError> {
    if start == end {
        return Err(VersionError::EmptyIdentifier { at: start });
    }
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut p = start;
    loop {
        let ident_start = p;
        while p < end && bytes[p] != b'.' {
            let b = bytes[p];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: p });
            }
            p += 1;
        }
        if p == ident_start {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }
        let ident = &s[ident_start..p];
        if pre && ident.bytes().all(is_digit) && ident.len() > 1 && ident.starts_with('0') {
            return Err(VersionError::LeadingZero { at: ident_start });
        }
        out.push(ident.to_string());
        if p == end {
            break;
        }
        p += 1;
        if p == end {
            return Err(VersionError::EmptyIdentifier { at: p });
        }
    }
    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let mut p = 0;
    let major = parse_core_number(s, &mut p)?;
    if p >= s.len() || s.as_bytes()[p] != b'.' {
        if p >= s.len() || matches!(s.as_bytes()[p], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: p });
        }
        return Err(VersionError::InvalidChar { at: p });
    }
    p += 1;
    let minor = parse_core_number(s, &mut p)?;
    if p >= s.len() || s.as_bytes()[p] != b'.' {
        if p >= s.len() || matches!(s.as_bytes()[p], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: p });
        }
        return Err(VersionError::InvalidChar { at: p });
    }
    p += 1;
    let patch = parse_core_number(s, &mut p)?;
    let mut pre = Vec::new();
    let mut build = Vec::new();
    if p < s.len() {
        match s.as_bytes()[p] {
            b'-' => {
                let pre_start = p + 1;
                let plus = s[pre_start..].find('+').map(|i| pre_start + i);
                let pre_end = plus.unwrap_or(s.len());
                pre = parse_identifiers(s, pre_start, pre_end, true)?;
                if let Some(plus_pos) = plus {
                    build = parse_identifiers(s, plus_pos + 1, s.len(), false)?;
                }
            }
            b'+' => build = parse_identifiers(s, p + 1, s.len(), false)?,
            _ => return Err(VersionError::InvalidChar { at: p }),
        }
    }
    Ok(Version { major, minor, patch, pre, build })
}

fn numeric_ident(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn cmp_ident(a: &str, b: &str) -> Ordering {
    match (numeric_ident(a), numeric_ident(b)) {
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
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
                let c = cmp_ident(x, y);
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

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

#[derive(Clone, Debug)]
struct Bound {
    version: Version,
    inclusive: bool,
}

#[derive(Clone, Debug)]
struct Arm {
    lower: Option<Bound>,
    upper: Option<Bound>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

fn version_without_build(mut v: Version) -> Version {
    v.build.clear();
    v
}

fn make_version(major: u64, minor: u64, patch: u64, pre: Vec<String>) -> Version {
    Version { major, minor, patch, pre, build: Vec::new() }
}

fn bump(v: &Version, field: usize) -> Option<Version> {
    let (mut major, mut minor, mut patch) = (v.major, v.minor, v.patch);
    match field {
        0 => { major = major.checked_add(1)?; minor = 0; patch = 0; }
        1 => { minor = minor.checked_add(1)?; patch = 0; }
        _ => { patch = patch.checked_add(1)?; }
    }
    Some(make_version(major, minor, patch, vec!["0".into()]))
}

fn parse_partial(
    s: &str,
    start: usize,
    token: &str,
) -> Result<(Version, usize, bool), RangeError> {
    let special = token.bytes().any(|b| b == b'-' || b == b'+');
    let fields: Vec<&str> = token.split('.').collect();
    let has_wild = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));
    let full = special || (fields.len() >= 3 && !has_wild);
    if full {
        return parse_version(token)
            .map(|v| (v, 3, true))
            .map_err(|error| RangeError::Version { at: start, error });
    }
    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|f| f.is_empty()) {
        return Err(RangeError::Syntax { at: start });
    }
    let mut nums = Vec::new();
    let mut wildcard_seen = false;
    for f in &fields {
        if matches!(*f, "*" | "x" | "X") {
            wildcard_seen = true;
        } else {
            if wildcard_seen || !f.bytes().all(is_digit) || (f.len() > 1 && f.starts_with('0')) {
                return Err(RangeError::Syntax { at: start });
            }
            let n = f.parse::<u64>().map_err(|_| RangeError::Syntax { at: start })?;
            nums.push(n);
        }
    }
    let k = nums.len();
    let (major, minor, patch) = (
        nums.get(0).copied().unwrap_or(0),
        nums.get(1).copied().unwrap_or(0),
        nums.get(2).copied().unwrap_or(0),
    );
    Ok((make_version(major, minor, patch, Vec::new()), k, false))
}

fn add_anchor(arm: &mut Arm, v: &Version) {
    if !v.pre.is_empty() {
        let t = (v.major, v.minor, v.patch);
        if !arm.anchors.contains(&t) {
            arm.anchors.push(t);
        }
    }
}

fn add_lower(arm: &mut Arm, v: Version, inclusive: bool) {
    let v = version_without_build(v);
    match &arm.lower {
        None => arm.lower = Some(Bound { version: v, inclusive }),
        Some(old) => {
            let c = compare_versions(&v, &old.version);
            if c == Ordering::Greater || (c == Ordering::Equal && !inclusive && old.inclusive) {
                arm.lower = Some(Bound { version: v, inclusive });
            }
        }
    }
}

fn add_upper(arm: &mut Arm, v: Version, inclusive: bool) {
    let v = version_without_build(v);
    match &arm.upper {
        None => arm.upper = Some(Bound { version: v, inclusive }),
        Some(old) => {
            let c = compare_versions(&v, &old.version);
            if c == Ordering::Less || (c == Ordering::Equal && !inclusive && old.inclusive) {
                arm.upper = Some(Bound { version: v, inclusive });
            }
        }
    }
}

fn apply_comparator(arm: &mut Arm, op: Op, v: Version) {
    add_anchor(arm, &v);
    match op {
        Op::Eq => {
            add_lower(arm, v.clone(), true);
            add_upper(arm, v, true);
        }
        Op::Lt => add_upper(arm, v, false),
        Op::Le => add_upper(arm, v, true),
        Op::Gt => add_lower(arm, v, false),
        Op::Ge => add_lower(arm, v, true),
    }
}

fn upper_for_partial(v: &Version, k: usize, start: usize) -> Result<Option<Version>, RangeError> {
    let field = if k == 1 { 0 } else { 1 };
    bump(v, field).map(Some).ok_or(RangeError::Syntax { at: start })
}

fn parse_arm(tokens: &[(String, usize)], arm: &mut Arm) -> Result<(), RangeError> {
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    let hyphens: Vec<usize> = tokens.iter().enumerate().filter(|(_, t)| t.0 == "-").map(|(i, _)| i).collect();
    if let Some(&minus) = hyphens.first() {
        if hyphens.len() != 1 || tokens.len() != 3 || minus != 1 {
            return Err(RangeError::Syntax { at: tokens[minus].1 });
        }
        let (lo, lk, lf) = parse_partial(&tokens[0].0, tokens[0].1, &tokens[0].0)?;
        let (hi, hk, hf) = parse_partial(&tokens[2].0, tokens[2].1, &tokens[2].0)?;
        if lk == 0 {
            return Err(RangeError::Syntax { at: tokens[0].1 });
        }
        if hk == 0 {
            return Err(RangeError::Syntax { at: tokens[2].1 });
        }
        add_anchor(arm, &lo);
        add_lower(arm, lo, true);
        add_anchor(arm, &hi);
        if hf {
            add_upper(arm, hi, true);
        } else {
            let up = upper_for_partial(&hi, hk, tokens[2].1)?;
            if let Some(v) = up { add_upper(arm, v, false); }
        }
        return Ok(());
    }

    for (token, start) in tokens {
        let bytes = token.as_bytes();
        if token.is_empty() {
            return Err(RangeError::Syntax { at: *start });
        }
        let op_len;
        let op = if token.starts_with(">=") { op_len = 2; Some(Op::Ge) }
        else if token.starts_with("<=") { op_len = 2; Some(Op::Le) }
        else if token.starts_with('>') { op_len = 1; Some(Op::Gt) }
        else if token.starts_with('<') { op_len = 1; Some(Op::Lt) }
        else if token.starts_with('=') { op_len = 1; Some(Op::Eq) }
        else { op_len = 0; None };

        if let Some(op) = op {
            let operand = &token[op_len..];
            let operand_start = *start + op_len;
            if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
                return Err(RangeError::Syntax { at: *start });
            }
            let v = parse_version(operand)
                .map_err(|error| RangeError::Version { at: operand_start, error })?;
            apply_comparator(arm, op, v);
            continue;
        }

        let first = bytes[0];
        if !first.is_ascii_digit() && !matches!(first, b'*' | b'x' | b'X' | b'^' | b'~') {
            return Err(RangeError::Syntax { at: *start });
        }
        let (kind, operand, operand_start) = if first == b'^' || first == b'~' {
            (first, &token[1..], *start + 1)
        } else {
            (0, token.as_str(), *start)
        };
        let (v, k, full) = parse_partial(operand, operand_start, operand)?;
        if kind == b'^' || kind == b'~' {
            if k == 0 {
                return Err(RangeError::Syntax { at: *start });
            }
            add_anchor(arm, &v);
            add_lower(arm, v.clone(), true);
            let field = if kind == b'^' {
                if v.major > 0 || k == 1 { 0 } else if v.minor > 0 || k == 2 { 1 } else { 2 }
            } else if k == 1 { 0 } else { 1 };
            let up = bump(&v, field).ok_or(RangeError::Syntax { at: operand_start })?;
            add_upper(arm, up, false);
        } else if full {
            apply_comparator(arm, Op::Eq, v);
        } else if k == 0 {
        } else if k == 1 || k == 2 {
            add_lower(arm, v.clone(), true);
            let up = upper_for_partial(&v, k, *start)?;
            if let Some(u) = up { add_upper(arm, u, false); }
        } else {
            add_lower(arm, v.clone(), true);
            add_upper(arm, v, true);
        }
    }
    Ok(())
}

fn whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = Vec::new();
    let mut p = 0;
    while p < bytes.len() && whitespace(bytes[p]) { p += 1; }
    if p == bytes.len() {
        return Err(RangeError::Empty { at: 0 });
    }
    loop {
        let arm_start = p;
        let mut tokens = Vec::new();
        while p < bytes.len() {
            while p < bytes.len() && whitespace(bytes[p]) { p += 1; }
            if p >= bytes.len() { break; }
            let start = p;
            while p < bytes.len() && !whitespace(bytes[p]) { p += 1; }
            let token = s[start..p].to_string();
            if token == "||" {
                if tokens.is_empty() {
                    return Err(RangeError::Empty { at: start });
                }
                let mut arm = Arm { lower: None, upper: None, anchors: Vec::new() };
                parse_arm(&tokens, &mut arm)?;
                arms.push(arm);
                tokens.clear();
                while p < bytes.len() && whitespace(bytes[p]) { p += 1; }
                if p == bytes.len() {
                    return Err(RangeError::Empty { at: s.len() });
                }
                break;
            }
            tokens.push((token, start));
        }
        if p >= bytes.len() {
            if tokens.is_empty() {
                return Err(RangeError::Empty { at: arm_start });
            }
            let mut arm = Arm { lower: None, upper: None, anchors: Vec::new() };
            parse_arm(&tokens, &mut arm)?;
            arms.push(arm);
            break;
        }
        // The separator was consumed; continue with the next arm.
    }
    Ok(Range { arms })
}

fn within(v: &Version, arm: &Arm) -> bool {
    if let Some(lo) = &arm.lower {
        let c = compare_versions(v, &lo.version);
        if c == Ordering::Less || (c == Ordering::Equal && !lo.inclusive) { return false; }
    }
    if let Some(hi) = &arm.upper {
        let c = compare_versions(v, &hi.version);
        if c == Ordering::Greater || (c == Ordering::Equal && !hi.inclusive) { return false; }
    }
    true
}

fn admitted(v: &Version, arm: &Arm) -> bool {
    v.pre.is_empty() || arm.anchors.contains(&(v.major, v.minor, v.patch))
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|arm| within(v, arm) && (include_prerelease || admitted(v, arm)))
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

fn core_string(t: (u64, u64, u64)) -> String {
    format!("{}.{}.{}", t.0, t.1, t.2)
}

fn version_string(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

pub fn canonical(r: &Range) -> String {
    let mut result = Vec::new();
    for arm in &r.arms {
        let mut anchors = arm.anchors.clone();
        anchors.sort();
        let anchor_text = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors.iter().map(|t| core_string(*t)).collect::<Vec<_>>().join(",")
        };
        let lower = arm.lower.as_ref().map(|b| format!("{}{}", if b.inclusive { ">=" } else { ">" }, version_string(&b.version))).unwrap_or_else(|| "*".into());
        let upper = arm.upper.as_ref().map(|b| format!("{}{}", if b.inclusive { "<=" } else { "<" }, version_string(&b.version))).unwrap_or_else(|| "*".into());
        result.push(format!("[{};{};pre={}]", lower, upper, anchor_text));
    }
    result.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    result.dedup();
    if result.is_empty() { "EMPTY".into() } else { result.join(" || ") }
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    a.arms.iter().any(|aa| b.arms.iter().any(|bb| {
        let mut arm = Arm { lower: aa.lower.clone(), upper: aa.upper.clone(), anchors: Vec::new() };
        if let Some(lo) = &bb.lower { add_lower(&mut arm, lo.version.clone(), lo.inclusive); }
        if let Some(hi) = &bb.upper { add_upper(&mut arm, hi.version.clone(), hi.inclusive); }
        let anchors_ok = include_prerelease
            || aa.anchors.iter().any(|x| bb.anchors.contains(x));
        if !include_prerelease && !anchors_ok {
            return has_stable_witness(&arm);
        }
        has_any_witness(&arm, include_prerelease || anchors_ok)
    }))
}

fn has_stable_witness(arm: &Arm) -> bool {
    let mut v = make_version(0, 0, 0, Vec::new());
    if let Some(lo) = &arm.lower {
        v = lo.version.clone();
        if !lo.inclusive && !v.pre.is_empty() {
            v.pre.clear();
        } else if !lo.inclusive {
            if let Some(n) = next_stable(&v) { v = n; } else { return false; }
        } else if !v.pre.is_empty() {
            v.pre.clear();
        }
    }
    if v.pre.is_empty() && within(&v, arm) { return true; }
    if let Some(lo) = &arm.lower {
        let mut c = lo.version.clone();
        if !c.pre.is_empty() { c.pre.clear(); }
        else if !lo.inclusive {
            if let Some(n) = next_stable(&c) { c = n; } else { return false; }
        }
        return c.pre.is_empty() && within(&c, arm);
    }
    within(&v, arm)
}

fn next_stable(v: &Version) -> Option<Version> {
    if v.patch < u64::MAX {
        Some(make_version(v.major, v.minor, v.patch + 1, Vec::new()))
    } else if v.minor < u64::MAX {
        Some(make_version(v.major, v.minor + 1, 0, Vec::new()))
    } else if v.major < u64::MAX {
        Some(make_version(v.major + 1, 0, 0, Vec::new()))
    } else {
        None
    }
}

fn has_any_witness(arm: &Arm, allow_pre: bool) -> bool {
    if has_stable_witness(arm) { return true; }
    if !allow_pre { return false; }
    let mut candidates = Vec::new();
    if let Some(lo) = &arm.lower {
        if !lo.version.pre.is_empty() {
            candidates.push(lo.version.clone());
            let mut succ = lo.version.clone();
            succ.pre.push("0".into());
            candidates.push(succ);
        } else {
            if let Some(n) = next_stable(&lo.version) {
                let mut pre = n.clone();
                pre.pre = vec!["0".into()];
                candidates.push(pre);
            }
        }
    } else {
        candidates.push(make_version(0, 0, 0, vec!["0".into()]));
    }
    candidates.into_iter().any(|v| !v.pre.is_empty() && within(&v, arm))
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
        SetExpr::Union(xs) | SetExpr::Intersection(xs) => {
            for (i, child) in xs.iter().enumerate() {
                path.push(i);
                validate_expr(child, package, label, path)?;
                path.pop();
            }
            Ok(())
        }
    }
}

fn eval_expr(expr: &SetExpr, v: &Version, include: bool) -> bool {
    match expr {
        SetExpr::Range(s) => parse_range(s).map(|r| matches_with(v, &r, include)).unwrap_or(false),
        SetExpr::Union(xs) => xs.iter().any(|x| eval_expr(x, v, include)),
        SetExpr::Intersection(xs) => xs.iter().all(|x| eval_expr(x, v, include)),
    }
}

fn conflict_labels(
    constraints: &[Constraint],
    parsed: &[Version],
    include: bool,
) -> Vec<String> {
    if parsed.is_empty() { return Vec::new(); }
    let n = constraints.len();
    for size in 1..=n {
        let mut chosen = Vec::new();
        if let Some(indices) = find_conflict(constraints, parsed, include, size, 0, &mut chosen) {
            let mut labels: Vec<String> = indices.iter().map(|&i| constraints[i].label.clone()).collect();
            labels.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            return labels;
        }
    }
    Vec::new()
}

fn find_conflict(
    constraints: &[Constraint],
    parsed: &[Version],
    include: bool,
    remaining: usize,
    start: usize,
    chosen: &mut Vec<usize>,
) -> Option<Vec<usize>> {
    if remaining == 0 {
        let any = parsed.iter().any(|v| chosen.iter().all(|&i| eval_expr(&constraints[i].expr, v, include)));
        return if any { None } else { Some(chosen.clone()) };
    }
    if constraints.len().saturating_sub(start) < remaining { return None; }
    for i in start..=constraints.len() - remaining {
        chosen.push(i);
        if let Some(found) = find_conflict(constraints, parsed, include, remaining - 1, i + 1, chosen) {
            return Some(found);
        }
        chosen.pop();
    }
    None
}

pub fn select_packages(
    packages: &[Package],
    preference: Preference,
    include_prerelease: bool,
) -> Result<Vec<Selection>, SelectError> {
    let mut names = Vec::new();
    for p in packages {
        if names.iter().any(|n: &String| n == &p.name) {
            return Err(SelectError::DuplicatePackage { name: p.name.clone() });
        }
        names.push(p.name.clone());
    }
    for p in packages {
        let mut labels = Vec::new();
        for c in &p.constraints {
            if labels.iter().any(|x: &String| x == &c.label) {
                return Err(SelectError::DuplicateLabel { package: p.name.clone(), label: c.label.clone() });
            }
            labels.push(c.label.clone());
        }
    }
    let mut parsed_all = Vec::new();
    for p in packages {
        let mut versions = Vec::new();
        for (index, s) in p.candidates.iter().enumerate() {
            versions.push(parse_version(s).map_err(|error| SelectError::InvalidCandidate {
                package: p.name.clone(), index, error,
            })?);
        }
        parsed_all.push(versions);
    }
    for p in packages {
        for c in &p.constraints {
            validate_expr(&c.expr, &p.name, &c.label, &mut Vec::new())?;
        }
    }
    let mut selections = Vec::new();
    for (pi, p) in packages.iter().enumerate() {
        let parsed = &parsed_all[pi];
        let mut qualifying: Vec<usize> = (0..parsed.len()).filter(|&i| {
            p.constraints.iter().all(|c| eval_expr(&c.expr, &parsed[i], include_prerelease))
        }).collect();
        if qualifying.is_empty() {
            return Err(SelectError::NoSolution {
                package: p.name.clone(),
                labels: conflict_labels(&p.constraints, parsed, include_prerelease),
            });
        }
        match preference {
            Preference::Highest => qualifying.sort_by(|&a, &b| compare_versions(&parsed[b], &parsed[a])),
            Preference::Lowest => qualifying.sort_by(|&a, &b| compare_versions(&parsed[a], &parsed[b])),
            Preference::StableFirst => {
                qualifying.sort_by(|&a, &b| {
                    parsed[a].pre.is_empty().cmp(&parsed[b].pre.is_empty()).reverse()
                        .then_with(|| compare_versions(&parsed[b], &parsed[a]))
                });
            }
        }
        selections.push(Selection { package: p.name.clone(), version: p.candidates[qualifying[0]].clone() });
    }
    Ok(selections)
}

fn main() {}