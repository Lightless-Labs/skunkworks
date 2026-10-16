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

fn missing_byte(b: Option<u8>) -> bool {
    matches!(b, None | Some(b'.') | Some(b'-') | Some(b'+'))
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return Err(VersionError::Empty);
    }
    let mut pos = 0usize;
    let mut nums = [0u64; 3];
    for i in 0..3 {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if start == pos {
            return Err(if missing_byte(bytes.get(pos).copied()) {
                VersionError::MissingCore { at: pos }
            } else {
                VersionError::InvalidChar { at: pos }
            });
        }
        if pos - start > 1 && bytes[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }
        nums[i] = s[start..pos]
            .parse::<u64>()
            .map_err(|_| VersionError::Overflow { at: start })?;
        if i < 2 {
            if bytes.get(pos) != Some(&b'.') {
                return Err(if missing_byte(bytes.get(pos).copied()) {
                    VersionError::MissingCore { at: pos }
                } else {
                    VersionError::InvalidChar { at: pos }
                });
            }
            pos += 1;
        }
    }
    let mut pre = Vec::new();
    let mut build = Vec::new();
    if pos < bytes.len() && bytes[pos] != b'-' && bytes[pos] != b'+' {
        return Err(VersionError::InvalidChar { at: pos });
    }
    if bytes.get(pos) == Some(&b'-') {
        pos += 1;
        let start = pos;
        while pos < bytes.len() && bytes[pos] != b'+' {
            pos += 1;
        }
        parse_identifiers(s, start, pos, true, &mut pre)?;
    }
    if bytes.get(pos) == Some(&b'+') {
        pos += 1;
        parse_identifiers(s, pos, bytes.len(), false, &mut build)?;
        pos = bytes.len();
    }
    if pos != bytes.len() {
        return Err(VersionError::InvalidChar { at: pos });
    }
    Ok(Version {
        major: nums[0],
        minor: nums[1],
        patch: nums[2],
        pre,
        build,
    })
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    is_pre: bool,
    out: &mut Vec<String>,
) -> Result<(), VersionError> {
    let bytes = s.as_bytes();
    let mut id_start = start;
    loop {
        let mut p = id_start;
        while p < end && bytes[p] != b'.' {
            let c = bytes[p];
            if !(c.is_ascii_alphanumeric() || c == b'-') {
                return Err(VersionError::InvalidChar { at: p });
            }
            p += 1;
        }
        if p == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        if is_pre
            && bytes[id_start..p].iter().all(u8::is_ascii_digit)
            && p - id_start > 1
            && bytes[id_start] == b'0'
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        out.push(s[id_start..p].to_string());
        if p == end {
            break;
        }
        id_start = p + 1;
        if id_start > end {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
    }
    Ok(())
}

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn numeric_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn cmp_numeric_id(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    let c = core(a).cmp(&core(b));
    if c != Ordering::Equal {
        return c;
    }
    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (x, y) in a.pre.iter().zip(&b.pre) {
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
enum Op {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug)]
struct Comparator {
    op: Op,
    version: Version,
}

#[derive(Clone, Debug)]
struct Arm {
    comparisons: Vec<Comparator>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

fn ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokens(s: &str, base: usize) -> Vec<(String, usize)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut p = 0;
    while p < b.len() {
        while p < b.len() && ws(b[p]) {
            p += 1;
        }
        if p == b.len() {
            break;
        }
        let start = p;
        while p < b.len() && !ws(b[p]) {
            p += 1;
        }
        out.push((s[start..p].to_string(), base + start));
    }
    out
}

fn add_comparison(arm: &mut Arm, op: Op, version: Version, anchor: bool) {
    if anchor && !version.pre.is_empty() {
        arm.anchors.push(core(&version));
    }
    arm.comparisons.push(Comparator { op, version });
}

fn test_comparator(v: &Version, c: &Comparator) -> bool {
    match c.op {
        Op::Eq => compare_versions(v, &c.version) == Ordering::Equal,
        Op::Lt => compare_versions(v, &c.version) == Ordering::Less,
        Op::Le => compare_versions(v, &c.version) != Ordering::Greater,
        Op::Gt => compare_versions(v, &c.version) == Ordering::Greater,
        Op::Ge => compare_versions(v, &c.version) != Ordering::Less,
    }
}

fn partial_fields(s: &str) -> Option<(Vec<Option<u64>>, usize)> {
    if s.is_empty() {
        return None;
    }
    let fields: Vec<&str> = s.split('.').collect();
    if fields.len() > 3 {
        return None;
    }
    let mut out = Vec::new();
    let mut wild = false;
    let mut k = 0;
    for f in fields {
        if f == "*" || f == "x" || f == "X" {
            wild = true;
            out.push(None);
        } else {
            if wild
                || f.is_empty()
                || !f.bytes().all(|b| b.is_ascii_digit())
                || (f.len() > 1 && f.starts_with('0'))
            {
                return None;
            }
            let n = f.parse::<u64>().ok()?;
            k += 1;
            out.push(Some(n));
        }
    }
    Some((out, k))
}

fn full_or_partial(
    operand: &str,
    at: usize,
) -> Result<Result<Version, (Vec<Option<u64>>, usize)>, RangeError> {
    let fields: Vec<&str> = operand.split('.').collect();
    let has_suffix = operand.contains('-') || operand.contains('+');
    let has_wild = fields.iter().any(|f| *f == "*" || *f == "x" || *f == "X");
    if has_suffix || (fields.len() >= 3 && !has_wild) {
        return parse_version(operand)
            .map(Ok)
            .map_err(|error| RangeError::Version { at, error });
    }
    partial_fields(operand)
        .map(Err)
        .ok_or(RangeError::Syntax { at })
}

fn tuple_version(t: (u64, u64, u64), pre: Vec<String>) -> Version {
    Version {
        major: t.0,
        minor: t.1,
        patch: t.2,
        pre,
        build: Vec::new(),
    }
}

fn bump(t: (u64, u64, u64), field: usize, at: usize) -> Result<(u64, u64, u64), RangeError> {
    let mut a = [t.0, t.1, t.2];
    a[field] = a[field]
        .checked_add(1)
        .ok_or(RangeError::Syntax { at })?;
    for x in a.iter_mut().skip(field + 1) {
        *x = 0;
    }
    Ok((a[0], a[1], a[2]))
}

fn padded(p: &[Option<u64>]) -> (u64, u64, u64) {
    (
        p.first().and_then(|x| *x).unwrap_or(0),
        p.get(1).and_then(|x| *x).unwrap_or(0),
        p.get(2).and_then(|x| *x).unwrap_or(0),
    )
}

fn is_all_numeric(p: &[Option<u64>]) -> usize {
    p.iter().take_while(|x| x.is_some()).count()
}

fn parse_expanded(
    arm: &mut Arm,
    token: &str,
    at: usize,
    kind: u8,
) -> Result<(), RangeError> {
    let operand = &token[1..];
    let oat = at + 1;
    let parsed = full_or_partial(operand, oat)?;
    let (lower, k, full) = match parsed {
        Ok(v) => (v.clone(), 3, Some(v)),
        Err((p, k)) => (tuple_version(padded(&p), Vec::new()), k, None),
    };
    if k == 0 {
        return Err(RangeError::Syntax { at });
    }
    add_comparison(arm, Op::Ge, lower.clone(), true);
    let t = core(&lower);
    let field = if kind == b'^' {
        if t.0 > 0 || k == 1 { 0 } else if t.1 > 0 || k == 2 { 1 } else { 2 }
    } else if k == 1 {
        0
    } else {
        1
    };
    let upper = bump(t, field, oat)?;
    add_comparison(arm, Op::Lt, tuple_version(upper, vec!["0".into()]), false);
    let _ = full;
    Ok(())
}

fn parse_hyphen(arm: &mut Arm, ts: &[(String, usize)]) -> Result<(), RangeError> {
    let minus = ts.iter().find(|x| x.0 == "-").unwrap().1;
    if ts.len() != 3 || ts[1].0 != "-" {
        return Err(RangeError::Syntax { at: minus });
    }
    let (lo_v, lo_k) = match full_or_partial(&ts[0].0, ts[0].1)? {
        Ok(v) => (v, 3),
        Err((p, k)) => (tuple_version(padded(&p), Vec::new()), k),
    };
    let upper_parsed = full_or_partial(&ts[2].0, ts[2].1)?;
    let (hi_v, hi_k, upper_full) = match upper_parsed {
        Ok(v) => (v, 3, true),
        Err((p, k)) => (tuple_version(padded(&p), Vec::new()), k, false),
    };
    if lo_k == 0 {
        return Err(RangeError::Syntax { at: ts[0].1 });
    }
    if hi_k == 0 {
        return Err(RangeError::Syntax { at: ts[2].1 });
    }
    add_comparison(arm, Op::Ge, lo_v, true);
    if upper_full {
        add_comparison(arm, Op::Le, hi_v, true);
    } else {
        let t = core(&hi_v);
        let field = if hi_k == 1 { 0 } else { 1 };
        let upper = bump(t, field, ts[2].1)?;
        add_comparison(arm, Op::Lt, tuple_version(upper, vec!["0".into()]), false);
    }
    Ok(())
}

fn parse_bare(arm: &mut Arm, token: &str, at: usize) -> Result<(), RangeError> {
    match full_or_partial(token, at)? {
        Ok(v) => add_comparison(arm, Op::Eq, v, true),
        Err((p, k)) => {
            if k == 0 {
                return Ok(());
            }
            let lo = tuple_version(padded(&p), Vec::new());
            add_comparison(arm, Op::Ge, lo.clone(), false);
            if k < 3 {
                let upper = bump(core(&lo), if k == 1 { 0 } else { 1 }, at)?;
                add_comparison(arm, Op::Lt, tuple_version(upper, vec!["0".into()]), false);
            }
        }
    }
    Ok(())
}

fn parse_comparator(arm: &mut Arm, token: &str, at: usize) -> Result<(), RangeError> {
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
        return Err(RangeError::Syntax { at });
    };
    let operand = &token[n..];
    let oat = at + n;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at });
    }
    let v = parse_version(operand).map_err(|error| RangeError::Version { at: oat, error })?;
    add_comparison(arm, op, v, true);
    Ok(())
}

fn parse_arm(ts: &[(String, usize)]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        comparisons: Vec::new(),
        anchors: Vec::new(),
    };
    if ts.iter().any(|x| x.0 == "-") {
        parse_hyphen(&mut arm, ts)?;
        return Ok(arm);
    }
    for (token, at) in ts {
        let b = token.as_bytes()[0];
        match b {
            b'^' | b'~' => parse_expanded(&mut arm, token, *at, b)?,
            b'<' | b'>' | b'=' => parse_comparator(&mut arm, token, *at)?,
            b'0'..=b'9' | b'*' | b'x' | b'X' => parse_bare(&mut arm, token, *at)?,
            _ => return Err(RangeError::Syntax { at: *at }),
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let b = s.as_bytes();
    let mut arms = Vec::new();
    let mut start = 0usize;
    let mut p = 0usize;
    let mut any = false;
    while p < b.len() {
        while p < b.len() && ws(b[p]) {
            p += 1;
        }
        if p == b.len() {
            break;
        }
        if p + 1 < b.len() && b[p] == b'|' && b[p + 1] == b'|' {
            return Err(RangeError::Empty { at: p });
        }
        let arm_start = p;
        while p < b.len() {
            if p + 1 < b.len() && b[p] == b'|' && b[p + 1] == b'|' {
                break;
            }
            p += 1;
        }
        let arm_end = p;
        let arm_s = &s[arm_start..arm_end];
        let ts = tokens(arm_s, arm_start);
        if ts.is_empty() {
            return Err(RangeError::Empty { at: arm_start });
        }
        arms.push(parse_arm(&ts)?);
        any = true;
        if p < b.len() {
            let sep = p;
            if p > 0 && !ws(b[p - 1]) || p + 2 < b.len() && !ws(b[p + 2]) {
                // A glued || remains part of an operand.
                if p + 1 < b.len() {
                    let end = (p + 2).min(b.len());
                    let t = &s[arm_start..end];
                    let ts = tokens(t, arm_start);
                    return Err(parse_arm(&ts).err().unwrap_or(RangeError::Syntax { at: sep }));
                }
            }
            p += 2;
            let mut q = p;
            while q < b.len() && ws(b[q]) {
                q += 1;
            }
            if q == b.len() {
                return Err(RangeError::Empty { at: b.len() });
            }
            if q + 1 < b.len() && b[q] == b'|' && b[q + 1] == b'|' {
                return Err(RangeError::Empty { at: q });
            }
            p = q;
            start = p;
        }
    }
    let _ = start;
    if !any {
        return Err(RangeError::Empty { at: 0 });
    }
    Ok(Range { arms })
}

fn arm_match(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !include_prerelease
        && !v.pre.is_empty()
        && !arm.anchors.iter().any(|t| *t == core(v))
    {
        return false;
    }
    arm.comparisons.iter().all(|c| test_comparator(v, c))
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|a| arm_match(v, a, include_prerelease))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn extrema(candidates: &[String], r: &Range, greatest: bool) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, s) in candidates.iter().enumerate() {
        let v = parse_version(s).map_err(|error| CandidateError { index, error })?;
        parsed.push(v);
    }
    let mut best: Option<usize> = None;
    for i in 0..parsed.len() {
        if !matches(&parsed[i], r) {
            continue;
        }
        best = match best {
            None => Some(i),
            Some(j) => {
                let ord = compare_versions(&parsed[i], &parsed[j]);
                if (greatest && ord == Ordering::Greater) || (!greatest && ord == Ordering::Less) {
                    Some(i)
                } else {
                    Some(j)
                }
            }
        };
    }
    Ok(best.map(|i| candidates[i].clone()))
}

pub fn max_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, true)
}

pub fn min_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, false)
}

fn next_core(t: (u64, u64, u64)) -> Option<(u64, u64, u64)> {
    if t.2 < u64::MAX {
        Some((t.0, t.1, t.2 + 1))
    } else if t.1 < u64::MAX {
        Some((t.0, t.1 + 1, 0))
    } else if t.0 < u64::MAX {
        Some((t.0 + 1, 0, 0))
    } else {
        None
    }
}

fn bounds(arm: &Arm) -> (Option<(Version, bool)>, Option<(Version, bool)>) {
    let mut lower: Option<(Version, bool)> = None;
    let mut upper: Option<(Version, bool)> = None;
    for c in &arm.comparisons {
        match c.op {
            Op::Eq => {
                lower = Some((c.version.clone(), true));
                upper = Some((c.version.clone(), true));
            }
            Op::Ge | Op::Gt => {
                let inc = matches!(c.op, Op::Ge);
                let replace = lower.as_ref().map_or(true, |(v, old_inc)| {
                    let ord = compare_versions(&c.version, v);
                    ord == Ordering::Greater || (ord == Ordering::Equal && !inc && *old_inc)
                });
                if replace {
                    lower = Some((c.version.clone(), inc));
                }
            }
            Op::Le | Op::Lt => {
                let inc = matches!(c.op, Op::Le);
                let replace = upper.as_ref().map_or(true, |(v, old_inc)| {
                    let ord = compare_versions(&c.version, v);
                    ord == Ordering::Less || (ord == Ordering::Equal && !inc && *old_inc)
                });
                if replace {
                    upper = Some((c.version.clone(), inc));
                }
            }
        }
    }
    (lower, upper)
}

fn clean_version(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

fn arm_satisfiable(arm: &Arm) -> bool {
    let (lo, hi) = bounds(arm);
    if let (Some((l, li)), Some((h, hi))) = (&lo, &hi) {
        match compare_versions(l, h) {
            Ordering::Greater => return false,
            Ordering::Equal => {
                if !li || !hi {
                    return false;
                }
            }
            Ordering::Less => {}
        }
    }
    true
}

pub fn canonical(r: &Range) -> String {
    let mut out = Vec::new();
    for arm in &r.arms {
        if !arm_satisfiable(arm) {
            continue;
        }
        let (lo, hi) = bounds(arm);
        let lower = lo.map_or("*".to_string(), |(v, inc)| {
            format!("{}{}", if inc { ">=" } else { ">" }, clean_version(&v))
        });
        let upper = hi.map_or("*".to_string(), |(v, inc)| {
            format!("{}{}", if inc { "<=" } else { "<" }, clean_version(&v))
        });
        let mut anchors = arm.anchors.clone();
        anchors.sort();
        anchors.dedup();
        let anchor_text = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors
                .iter()
                .map(|t| format!("{}.{}.{}", t.0, t.1, t.2))
                .collect::<Vec<_>>()
                .join(",")
        };
        out.push(format!("[{};{};pre={}]", lower, upper, anchor_text));
    }
    out.sort();
    out.dedup();
    if out.is_empty() {
        "EMPTY".to_string()
    } else {
        out.join(" || ")
    }
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for aa in &a.arms {
        for bb in &b.arms {
            let (al, au) = bounds(aa);
            let (bl, bu) = bounds(bb);
            let mut candidates = Vec::new();
            for t in [
                al.as_ref().map(|x| core(&x.0)),
                au.as_ref().map(|x| core(&x.0)),
                bl.as_ref().map(|x| core(&x.0)),
                bu.as_ref().map(|x| core(&x.0)),
                Some((0, 0, 0)),
            ]
            .into_iter()
            .flatten()
            {
                candidates.push(tuple_version(t, Vec::new()));
                candidates.push(tuple_version(t, vec!["0".into()]));
            }
            if let Some((l, _)) = &al {
                if let Some(t) = next_core(core(l)) {
                    candidates.push(tuple_version(t, Vec::new()));
                    candidates.push(tuple_version(t, vec!["0".into()]));
                }
            }
            if let Some((l, _)) = &bl {
                if let Some(t) = next_core(core(l)) {
                    candidates.push(tuple_version(t, Vec::new()));
                    candidates.push(tuple_version(t, vec!["0".into()]));
                }
            }
            for v in candidates {
                if arm_match(&v, aa, include_prerelease)
                    && arm_match(&v, bb, include_prerelease)
                {
                    return true;
                }
            }
        }
    }
    false
}