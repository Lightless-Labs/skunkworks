use std::cmp::Ordering;
use std::collections::HashSet;

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

fn is_sep_or_end(b: Option<u8>) -> bool {
    matches!(b, None | Some(b'.' | b'-' | b'+'))
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    is_pre: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut pos = start;
    loop {
        let id_start = pos;
        while pos < end {
            let b = bytes[pos];
            if b.is_ascii_alphanumeric() || b == b'-' {
                pos += 1;
            } else {
                return Err(VersionError::InvalidChar { at: pos });
            }
        }
        if id_start == pos {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        let id = &s[id_start..pos];
        if is_pre
            && id.len() > 1
            && id.starts_with('0')
            && id.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        out.push(id.to_owned());
        if pos == end {
            break;
        }
        // `end` is the suffix boundary; separators are processed by the caller.
        break;
    }
    Ok(out)
}

fn parse_id_list(s: &str, start: usize, end: usize, is_pre: bool) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut ids = Vec::new();
    let mut p = start;
    loop {
        let q = (p..end).find(|&i| bytes[i] == b'.').unwrap_or(end);
        if q == p {
            return Err(VersionError::EmptyIdentifier { at: p });
        }
        for i in p..q {
            let b = bytes[i];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: i });
            }
        }
        let id = &s[p..q];
        if is_pre
            && id.len() > 1
            && id.starts_with('0')
            && id.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(VersionError::LeadingZero { at: p });
        }
        ids.push(id.to_owned());
        if q == end {
            break;
        }
        p = q + 1;
        if p == end {
            return Err(VersionError::EmptyIdentifier { at: p });
        }
    }
    Ok(ids)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let bytes = s.as_bytes();
    let mut p = 0usize;
    let mut core = [0u64; 3];

    for field in 0..3 {
        let start = p;
        while p < bytes.len() && bytes[p].is_ascii_digit() {
            p += 1;
        }
        if start == p {
            return Err(if is_sep_or_end(bytes.get(p).copied()) {
                VersionError::MissingCore { at: p }
            } else {
                VersionError::InvalidChar { at: p }
            });
        }
        let digits = &s[start..p];
        if digits.len() > 1 && digits.starts_with('0') {
            return Err(VersionError::LeadingZero { at: start });
        }
        core[field] = digits
            .parse::<u64>()
            .map_err(|_| VersionError::Overflow { at: start })?;
        if field < 2 {
            if bytes.get(p) != Some(&b'.') {
                return Err(if is_sep_or_end(bytes.get(p).copied()) {
                    VersionError::MissingCore { at: p }
                } else {
                    VersionError::InvalidChar { at: p }
                });
            }
            p += 1;
        }
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();
    if p < bytes.len() {
        match bytes[p] {
            b'-' => {
                let pre_start = p + 1;
                let plus = (pre_start..bytes.len()).find(|&i| bytes[i] == b'+');
                let pre_end = plus.unwrap_or(bytes.len());
                pre = parse_id_list(s, pre_start, pre_end, true)?;
                p = pre_end;
                if p < bytes.len() {
                    let build_start = p + 1;
                    build = parse_id_list(s, build_start, bytes.len(), false)?;
                    p = bytes.len();
                }
            }
            b'+' => {
                let build_start = p + 1;
                build = parse_id_list(s, build_start, bytes.len(), false)?;
                p = bytes.len();
            }
            _ => return Err(VersionError::InvalidChar { at: p }),
        }
    }
    if p != bytes.len() {
        return Err(VersionError::InvalidChar { at: p });
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
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn cmp_numeric_id(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    if a.is_empty() || b.is_empty() {
        return match (a.is_empty(), b.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            _ => Ordering::Equal,
        };
    }
    for (x, y) in a.iter().zip(b) {
        let xn = numeric_id(x);
        let yn = numeric_id(y);
        let c = match (xn, yn) {
            (true, true) => cmp_numeric_id(x, y),
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
struct Cmp {
    op: u8, // 0 =, 1 <, 2 <=, 3 >, 4 >=
    v: Version,
    explicit_pre: bool,
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

fn version_core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn cmp_pass(v: &Version, c: &Cmp) -> bool {
    let ord = compare_versions(v, &c.v);
    match c.op {
        0 => ord == Ordering::Equal,
        1 => ord == Ordering::Less,
        2 => ord != Ordering::Greater,
        3 => ord == Ordering::Greater,
        _ => ord != Ordering::Less,
    }
}

fn arm_matches(v: &Version, arm: &Arm, include_pre: bool) -> bool {
    if !v.pre.is_empty()
        && !include_pre
        && !arm.anchors.contains(&version_core(v))
    {
        return false;
    }
    arm.cmps.iter().all(|c| cmp_pass(v, c))
}

fn next_core(core: (u64, u64, u64), field: usize) -> Option<(u64, u64, u64)> {
    match field {
        0 => Some((core.0.checked_add(1)?, 0, 0)),
        1 => Some((core.0, core.1.checked_add(1)?, 0)),
        _ => Some((core.0, core.1, core.2.checked_add(1)?)),
    }
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

fn parse_full_operand(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

fn partial_fields(s: &str, at: usize) -> Result<(Vec<Option<u64>>, Option<Version>), RangeError> {
    let fullish = s.contains('-')
        || s.contains('+')
        || (s.split('.').count() >= 3
            && !s.split('.').any(|x| matches!(x, "*" | "x" | "X")));
    if fullish {
        return parse_full_operand(s, at).map(|v| (Vec::new(), Some(v)));
    }
    let parts: Vec<&str> = s.split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return Err(RangeError::Syntax { at });
    }
    let mut vals = Vec::new();
    let mut saw_wild = false;
    for part in parts {
        if part.is_empty() {
            return Err(RangeError::Syntax { at });
        }
        if matches!(part, "*" | "x" | "X") {
            saw_wild = true;
            vals.push(None);
        } else if saw_wild
            || !part.bytes().all(|b| b.is_ascii_digit())
            || (part.len() > 1 && part.starts_with('0'))
        {
            return Err(RangeError::Syntax { at });
        } else {
            let n = part.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
            vals.push(Some(n));
        }
    }
    Ok((vals, None))
}

fn padded(vals: &[Option<u64>]) -> (u64, u64, u64) {
    let mut out = [0u64; 3];
    for (i, x) in vals.iter().enumerate() {
        if let Some(n) = x {
            out[i] = *n;
        } else {
            break;
        }
    }
    (out[0], out[1], out[2])
}

fn add_cmp(arm: &mut Arm, op: u8, v: Version, anchor: bool) {
    if anchor && !v.pre.is_empty() {
        let core = version_core(&v);
        if !arm.anchors.contains(&core) {
            arm.anchors.push(core);
        }
    }
    arm.cmps.push(Cmp {
        op,
        explicit_pre: !v.pre.is_empty(),
        v,
    });
}

fn expand_bare(arm: &mut Arm, operand: &str, at: usize) -> Result<(), RangeError> {
    let (vals, full) = partial_fields(operand, at)?;
    if let Some(v) = full {
        add_cmp(arm, 0, v, true);
        return Ok(());
    }
    let k = vals.iter().take_while(|x| x.is_some()).count();
    if k == 0 {
        return Ok(());
    }
    let core = padded(&vals);
    add_cmp(arm, 4, endpoint(core, Vec::new()), true);
    if k < 3 {
        let upper = next_core(core, k - 1).ok_or(RangeError::Syntax { at })?;
        add_cmp(arm, 1, endpoint(upper, vec!["0".to_owned()]), false);
    }
    Ok(())
}

fn expand_caret_tilde(
    arm: &mut Arm,
    operand: &str,
    at: usize,
    caret: bool,
) -> Result<(), RangeError> {
    let (vals, full) = partial_fields(operand, at)?;
    let (core, lower, k) = if let Some(v) = full {
        let core = version_core(&v);
        let k = 3;
        (core, v, k)
    } else {
        let k = vals.iter().take_while(|x| x.is_some()).count();
        if k == 0 {
            return Err(RangeError::Syntax { at: at - 1 });
        }
        let core = padded(&vals);
        (core, endpoint(core, Vec::new()), k)
    };
    add_cmp(arm, 4, lower, true);
    let field = if caret {
        if core.0 > 0 || k == 1 {
            0
        } else if core.1 > 0 || k == 2 {
            1
        } else {
            2
        }
    } else if k == 1 {
        0
    } else {
        1
    };
    let upper = next_core(core, field).ok_or(RangeError::Syntax { at: at - 1 })?;
    add_cmp(arm, 1, endpoint(upper, vec!["0".to_owned()]), false);
    Ok(())
}

fn tokenize(s: &str) -> Vec<(usize, usize)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
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
        out.push((start, i));
    }
    out
}

fn parse_arm(s: &str, toks: &[(usize, usize)]) -> Result<Arm, RangeError> {
    let mut arm = Arm { cmps: Vec::new(), anchors: Vec::new() };
    let minus_positions: Vec<usize> = toks
        .iter()
        .filter(|&&(a, b)| &s[a..b] == "-")
        .map(|&(a, _)| a)
        .collect();
    if !minus_positions.is_empty() {
        if toks.len() != 3 || &s[toks[1].0..toks[1].1] != "-" {
            return Err(RangeError::Syntax { at: minus_positions[0] });
        }
        let lo_s = &s[toks[0].0..toks[0].1];
        let hi_s = &s[toks[2].0..toks[2].1];
        let (lv, lf) = partial_fields(lo_s, toks[0].0)?;
        let (hv, hf) = partial_fields(hi_s, toks[2].0)?;
        if lv.is_empty() && lf.is_none() {
            return Err(RangeError::Syntax { at: toks[0].0 });
        }
        if hv.is_empty() && hf.is_none() {
            return Err(RangeError::Syntax { at: toks[2].0 });
        }
        if let Some(v) = lf {
            add_cmp(&mut arm, 4, v, true);
        } else {
            add_cmp(&mut arm, 4, endpoint(padded(&lv), Vec::new()), true);
        }
        if let Some(v) = hf {
            add_cmp(&mut arm, 2, v, true);
        } else {
            let k = hv.iter().take_while(|x| x.is_some()).count();
            let core = padded(&hv);
            if k == 3 {
                add_cmp(&mut arm, 2, endpoint(core, Vec::new()), true);
            } else {
                let upper = next_core(core, k - 1).ok_or(RangeError::Syntax { at: toks[2].0 })?;
                add_cmp(&mut arm, 1, endpoint(upper, vec!["0".to_owned()]), false);
            }
        }
        return Ok(arm);
    }

    for &(start, end) in toks {
        let t = &s[start..end];
        let bytes = t.as_bytes();
        if t == "||" || t.contains(',') || t.contains('(') || t.contains(')') || t == "&" {
            return Err(RangeError::Syntax { at: start });
        }
        if t.starts_with('^') || t.starts_with('~') {
            if t.len() == 1 {
                return Err(RangeError::Syntax { at: start });
            }
            expand_caret_tilde(&mut arm, &t[1..], start + 1, t.starts_with('^'))?;
            continue;
        }
        let op_len = if t.starts_with("<=") || t.starts_with(">=") {
            2
        } else if t.starts_with('<') || t.starts_with('>') || t.starts_with('=') {
            1
        } else {
            0
        };
        if op_len > 0 {
            let operand = &t[op_len..];
            let operand_start = start + op_len;
            if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
                return Err(RangeError::Syntax { at: start });
            }
            let v = parse_full_operand(operand, operand_start)?;
            let op = match &t[..op_len] {
                "=" => 0, "<" => 1, "<=" => 2, ">" => 3, _ => 4,
            };
            add_cmp(&mut arm, op, v, true);
            continue;
        }
        if bytes.is_empty()
            || !(bytes[0].is_ascii_digit() || matches!(bytes[0], b'*' | b'x' | b'X'))
        {
            return Err(RangeError::Syntax { at: start });
        }
        expand_bare(&mut arm, t, start)?;
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let toks = tokenize(s);
    if toks.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    let mut arms = Vec::new();
    let mut begin = 0usize;
    let mut i = 0usize;
    while i < toks.len() {
        if &s[toks[i].0..toks[i].1] == "||" {
            if begin == i {
                return Err(RangeError::Empty { at: toks[i].0 });
            }
            arms.push(parse_arm(s, &toks[begin..i])?);
            begin = i + 1;
        }
        i += 1;
    }
    if begin == toks.len() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(s, &toks[begin..])?);
    Ok(Range { arms })
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|arm| arm_matches(v, arm, include_prerelease))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

fn arm_is_satisfiable(arm: &Arm, include_pre: bool) -> bool {
    // Exact finite-bound checks are performed by probing all explicit endpoints and
    // their immediate neighbors; this also covers ordinary stable intervals.
    let mut probes = Vec::new();
    for c in &arm.cmps {
        probes.push(c.v.clone());
        let core = version_core(&c.v);
        probes.push(endpoint(core, Vec::new()));
        probes.push(endpoint(core, vec!["0".into()]));
        if let Some(n) = next_core(core, 2) {
            probes.push(endpoint(n, Vec::new()));
        }
    }
    probes.push(endpoint((0, 0, 0), Vec::new()));
    probes.into_iter().any(|v| arm_matches(&v, arm, include_pre))
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for aa in &a.arms {
        for bb in &b.arms {
            let mut both = aa.clone();
            both.cmps.extend(bb.cmps.clone());
            both.anchors.retain(|x| bb.anchors.contains(x));
            if include_prerelease {
                if arm_is_satisfiable(&both, true) {
                    return true;
                }
            } else if arm_is_satisfiable(&both, false) {
                return true;
            }
        }
    }
    false
}

fn render_version(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

pub fn canonical(r: &Range) -> String {
    let mut rendered = Vec::new();
    for arm in &r.arms {
        if !arm_is_satisfiable(arm, false) {
            continue;
        }
        let mut lower: Option<(Version, bool)> = None;
        let mut upper: Option<(Version, bool)> = None;
        let mut impossible = false;
        for c in &arm.cmps {
            if c.op == 0 {
                let l = (c.v.clone(), true);
                let u = (c.v.clone(), true);
                if lower.as_ref().map_or(true, |x| compare_versions(&l.0, &x.0) == Ordering::Greater) {
                    lower = Some(l);
                }
                if upper.as_ref().map_or(true, |x| compare_versions(&u.0, &x.0) == Ordering::Less) {
                    upper = Some(u);
                }
            } else if c.op == 3 || c.op == 4 {
                let v = (c.v.clone(), c.op == 4);
                if lower.as_ref().map_or(true, |x| {
                    compare_versions(&v.0, &x.0) == Ordering::Greater
                        || (compare_versions(&v.0, &x.0) == Ordering::Equal && !v.1 && x.1)
                }) {
                    lower = Some(v);
                }
            } else {
                let v = (c.v.clone(), c.op == 2);
                if upper.as_ref().map_or(true, |x| {
                    compare_versions(&v.0, &x.0) == Ordering::Less
                        || (compare_versions(&v.0, &x.0) == Ordering::Equal && !v.1 && x.1)
                }) {
                    upper = Some(v);
                }
            }
        }
        if let (Some((l, li)), Some((u, ui))) = (&lower, &upper) {
            let c = compare_versions(l, u);
            if c == Ordering::Greater || (c == Ordering::Equal && (!li || !ui)) {
                impossible = true;
            }
        }
        if impossible {
            continue;
        }
        let l = lower.map_or_else(|| "*".to_owned(), |(v, inc)| {
            format!("{}{}", if inc { ">=" } else { ">" }, render_version(&v))
        });
        let u = upper.map_or_else(|| "*".to_owned(), |(v, inc)| {
            format!("{}{}", if inc { "<=" } else { "<" }, render_version(&v))
        });
        let mut anchors = arm.anchors.clone();
        anchors.sort();
        anchors.dedup();
        let a = if anchors.is_empty() {
            "-".to_owned()
        } else {
            anchors
                .iter()
                .map(|(x, y, z)| format!("{x}.{y}.{z}"))
                .collect::<Vec<_>>()
                .join(",")
        };
        rendered.push(format!("[{l};{u};pre={a}]"));
    }
    rendered.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    rendered.dedup();
    if rendered.is_empty() {
        "EMPTY".to_owned()
    } else {
        rendered.join(" || ")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn extrema(candidates: &[String], r: &Range, want_max: bool) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, s) in candidates.iter().enumerate() {
        match parse_version(s) {
            Ok(v) => parsed.push(v),
            Err(error) => return Err(CandidateError { index, error }),
        }
    }
    let mut best: Option<usize> = None;
    for (i, v) in parsed.iter().enumerate() {
        if !matches(v, r) {
            continue;
        }
        if let Some(j) = best {
            let c = compare_versions(v, &parsed[j]);
            if (want_max && c == Ordering::Greater) || (!want_max && c == Ordering::Less) {
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
        SetExpr::Range(s) => {
            parse_range(s).map(|_| ()).map_err(|error| SelectError::InvalidRange {
                package: package.to_owned(),
                label: label.to_owned(),
                path: path.clone(),
                error,
            })
        }
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

fn expr_matches(expr: &SetExpr, v: &Version, include: bool) -> bool {
    match expr {
        SetExpr::Range(s) => parse_range(s).map_or(false, |r| matches_with(v, &r, include)),
        SetExpr::Union(xs) => xs.iter().any(|x| expr_matches(x, v, include)),
        SetExpr::Intersection(xs) => xs.iter().all(|x| expr_matches(x, v, include)),
    }
}

fn constraint_subset_satisfiable(
    package: &Package,
    parsed: &[Version],
    indices: &[usize],
    include: bool,
) -> bool {
    parsed.iter().any(|v| {
        indices
            .iter()
            .all(|&i| expr_matches(&package.constraints[i].expr, v, include))
    })
}

fn lex_labels(package: &Package, idxs: &[usize]) -> Vec<String> {
    let mut labels: Vec<String> = idxs.iter().map(|&i| package.constraints[i].label.clone()).collect();
    labels.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    labels
}

fn find_conflict(package: &Package, parsed: &[Version], include: bool) -> Vec<String> {
    if package.candidates.is_empty() {
        return Vec::new();
    }
    let n = package.constraints.len();
    for size in 1..=n {
        let mut best: Option<Vec<String>> = None;
        fn visit(
            package: &Package,
            parsed: &[Version],
            include: bool,
            n: usize,
            size: usize,
            next: usize,
            chosen: &mut Vec<usize>,
            best: &mut Option<Vec<String>>,
        ) {
            if chosen.len() == size {
                if !constraint_subset_satisfiable(package, parsed, chosen, include) {
                    let labels = lex_labels(package, chosen);
                    if best.as_ref().map_or(true, |b| {
                        labels.iter().map(|s| s.as_bytes()).cmp(b.iter().map(|s| s.as_bytes())) == Ordering::Less
                    }) {
                        *best = Some(labels);
                    }
                }
                return;
            }
            let remaining = size - chosen.len();
            if next + remaining > n {
                return;
            }
            for i in next..n {
                chosen.push(i);
                visit(package, parsed, include, n, size, i + 1, chosen, best);
                chosen.pop();
            }
        }
        visit(package, parsed, include, n, size, 0, &mut Vec::new(), &mut best);
        if let Some(v) = best {
            return v;
        }
    }
    Vec::new()
}

pub fn select_packages(
    packages: &[Package],
    preference: Preference,
    include_prerelease: bool,
) -> Result<Vec<Selection>, SelectError> {
    let mut names = HashSet::new();
    for p in packages {
        if !names.insert(p.name.clone()) {
            return Err(SelectError::DuplicatePackage { name: p.name.clone() });
        }
    }
    for p in packages {
        let mut labels = HashSet::new();
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
        for (i, s) in p.candidates.iter().enumerate() {
            match parse_version(s) {
                Ok(v) => parsed.push(v),
                Err(error) => {
                    return Err(SelectError::InvalidCandidate {
                        package: p.name.clone(),
                        index: i,
                        error,
                    })
                }
            }
        }
        parsed_all.push(parsed);
    }
    for p in packages {
        for c in &p.constraints {
            validate_expr(&c.expr, &p.name, &c.label, &mut Vec::new())?;
        }
    }

    let mut result = Vec::new();
    for (pi, p) in packages.iter().enumerate() {
        let parsed = &parsed_all[pi];
        let qualifying: Vec<usize> = parsed
            .iter()
            .enumerate()
            .filter(|(_, v)| {
                p.constraints.iter().all(|c| expr_matches(&c.expr, v, include_prerelease))
            })
            .map(|(i, _)| i)
            .collect();
        if qualifying.is_empty() {
            return Err(SelectError::NoSolution {
                package: p.name.clone(),
                labels: find_conflict(p, parsed, include_prerelease),
            });
        }
        let stable_first = matches!(preference, Preference::StableFirst)
            && qualifying.iter().any(|&i| parsed[i].pre.is_empty());
        let choose_max = !matches!(preference, Preference::Lowest);
        let pool = qualifying.into_iter().filter(|&i| {
            if stable_first {
                parsed[i].pre.is_empty()
            } else {
                true
            }
        });
        let mut best = None;
        for i in pool {
            if let Some(j) = best {
                let cmp = compare_versions(&parsed[i], &parsed[j]);
                if (choose_max && cmp == Ordering::Greater) || (!choose_max && cmp == Ordering::Less) {
                    best = Some(i);
                }
            } else {
                best = Some(i);
            }
        }
        let i = best.unwrap();
        result.push(Selection {
            package: p.name.clone(),
            version: p.candidates[i].clone(),
        });
    }
    Ok(result)
}