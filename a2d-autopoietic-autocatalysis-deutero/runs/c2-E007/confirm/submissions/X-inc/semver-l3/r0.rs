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

fn parse_core_number(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let start = *pos;
    let bytes = s.as_bytes();
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if start == *pos {
        let at = *pos;
        if at == bytes.len() || matches!(bytes[at], b'.' | b'-' | b'+') {
            return Err(VersionError::MissingCore { at });
        }
        return Err(VersionError::InvalidChar { at });
    }
    if *pos - start > 1 && bytes[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    s[start..*pos]
        .parse::<u64>()
        .map_err(|_| VersionError::Overflow { at: start })
}

fn valid_identifier_byte(b: u8) -> bool {
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
        let mut i = id_start;
        while i < end && bytes[i] != b'.' {
            if !valid_identifier_byte(bytes[i]) {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }
        if i == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        if prerelease
            && bytes[id_start..i].iter().all(u8::is_ascii_digit)
            && i - id_start > 1
            && bytes[id_start] == b'0'
        {
            return Err(VersionError::LeadingZero { at: id_start });
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
    let mut pos = 0;
    let major = parse_core_number(s, &mut pos)?;
    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        if pos >= s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;
    let minor = parse_core_number(s, &mut pos)?;
    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        if pos >= s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;
    let patch = parse_core_number(s, &mut pos)?;

    let bytes = s.as_bytes();
    let mut pre = Vec::new();
    let mut build = Vec::new();
    if pos < bytes.len() && bytes[pos] == b'-' {
        let pre_start = pos + 1;
        let plus = bytes[pre_start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|i| pre_start + i)
            .unwrap_or(bytes.len());
        pre = parse_identifiers(s, pre_start, plus, true)?;
        pos = plus;
    }
    if pos < bytes.len() && bytes[pos] == b'+' {
        let build_start = pos + 1;
        build = parse_identifiers(s, build_start, bytes.len(), false)?;
        pos = bytes.len();
    }
    if pos < bytes.len() {
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

fn is_numeric_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    for (x, y) in a.iter().zip(b.iter()) {
        let xn = is_numeric_id(x);
        let yn = is_numeric_id(y);
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
        .then_with(|| compare_pre(&a.pre, &b.pre))
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
    anchor: bool,
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

fn whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokens(s: &str, start: usize, end: usize) -> Vec<(usize, &str)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = start;
    while i < end {
        while i < end && whitespace(b[i]) {
            i += 1;
        }
        if i == end {
            break;
        }
        let st = i;
        while i < end && !whitespace(b[i]) {
            i += 1;
        }
        out.push((st, &s[st..i]));
    }
    out
}

fn version_error(at: usize, e: VersionError) -> RangeError {
    RangeError::Version { at, error: e }
}

fn syntax(at: usize) -> RangeError {
    RangeError::Syntax { at }
}

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn vcore(c: (u64, u64, u64), pre: Vec<String>) -> Version {
    Version {
        major: c.0,
        minor: c.1,
        patch: c.2,
        pre,
        build: Vec::new(),
    }
}

fn parse_partial(
    operand: &str,
    start: usize,
) -> Result<(Vec<u64>, usize, Option<Version>), RangeError> {
    let fullish = operand.contains(['-', '+'])
        || (operand.split('.').count() >= 3
            && !operand.split('.').any(|x| matches!(x, "*" | "x" | "X")));
    if fullish {
        let v = parse_version(operand).map_err(|e| version_error(start, e))?;
        return Ok((vec![v.major, v.minor, v.patch], 3, Some(v)));
    }
    let fields: Vec<&str> = operand.split('.').collect();
    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|f| f.is_empty()) {
        return Err(syntax(start));
    }
    let mut nums = Vec::new();
    let mut wildcard_seen = false;
    for f in &fields {
        if matches!(*f, "*" | "x" | "X") {
            wildcard_seen = true;
        } else {
            if wildcard_seen
                || !f.bytes().all(|b| b.is_ascii_digit())
                || (f.len() > 1 && f.starts_with('0'))
            {
                return Err(syntax(start));
            }
            let n = f.parse::<u64>().map_err(|_| syntax(start))?;
            nums.push(n);
        }
    }
    Ok((nums.clone(), nums.len(), None))
}

fn padded(nums: &[u64]) -> (u64, u64, u64) {
    (
        *nums.first().unwrap_or(&0),
        *nums.get(1).unwrap_or(&0),
        *nums.get(2).unwrap_or(&0),
    )
}

fn increment(c: (u64, u64, u64), index: usize, at: usize) -> Result<(u64, u64, u64), RangeError> {
    match index {
        0 => Ok((c.0.checked_add(1).ok_or_else(|| syntax(at))?, 0, 0)),
        1 => Ok((c.0, c.1.checked_add(1).ok_or_else(|| syntax(at))?, 0)),
        _ => Ok((c.0, c.1, c.2.checked_add(1).ok_or_else(|| syntax(at))?)),
    }
}

fn add_comparator(arm: &mut Arm, op: Op, v: Version, anchor: bool) {
    if anchor && !v.pre.is_empty() {
        arm.anchors.push(core(&v));
    }
    arm.comparators.push(Comparator {
        op,
        version: v,
        anchor,
    });
}

fn parse_explicit_comparator(token: &str, at: usize) -> Result<(Op, Version), RangeError> {
    let bytes = token.as_bytes();
    let (op, n) = if token.starts_with("<=") {
        (Op::Le, 2)
    } else if token.starts_with(">=") {
        (Op::Ge, 2)
    } else if token.starts_with('<') {
        (Op::Lt, 1)
    } else if token.starts_with('>') {
        (Op::Gt, 1)
    } else {
        (Op::Eq, 1)
    };
    if n >= bytes.len() || !bytes[n].is_ascii_digit() {
        return Err(syntax(at));
    }
    let v = parse_version(&token[n..]).map_err(|e| version_error(at + n, e))?;
    Ok((op, v))
}

fn parse_expanded(token: &str, at: usize, arm: &mut Arm) -> Result<(), RangeError> {
    let (kind, operand, op_at) = if token.starts_with('^') || token.starts_with('~') {
        (token.as_bytes()[0], &token[1..], at + 1)
    } else {
        (0, token, at)
    };
    let (nums, k, full) = parse_partial(operand, op_at)?;
    let base = full
        .as_ref()
        .map(|v| core(v))
        .unwrap_or_else(|| padded(&nums));

    if kind == b'^' || kind == b'~' {
        if k == 0 {
            return Err(syntax(at));
        }
        let lower = full.unwrap_or_else(|| vcore(base, Vec::new()));
        add_comparator(arm, Op::Ge, lower, true);
        let bump = if kind == b'^' {
            if base.0 > 0 || k == 1 {
                0
            } else if base.1 > 0 || k == 2 {
                1
            } else {
                2
            }
        } else if k == 1 {
            0
        } else {
            1
        };
        let upper = increment(base, bump, op_at)?;
        add_comparator(arm, Op::Lt, vcore(upper, vec!["0".into()]), false);
        return Ok(());
    }

    if let Some(v) = full {
        add_comparator(arm, Op::Eq, v, true);
    } else if k == 0 {
        // all wildcard: no constraints
    } else {
        add_comparator(arm, Op::Ge, vcore(base, Vec::new()), false);
        if k < 3 {
            let upper = increment(base, k - 1, at)?;
            add_comparator(arm, Op::Lt, vcore(upper, vec!["0".into()]), false);
        } else {
            add_comparator(arm, Op::Eq, vcore(base, Vec::new()), false);
        }
    }
    Ok(())
}

fn parse_hyphen(tokens: &[(usize, &str)], arm: &mut Arm) -> Result<(), RangeError> {
    let hyphens: Vec<usize> = tokens
        .iter()
        .filter(|(_, t)| *t == "-")
        .map(|(at, _)| *at)
        .collect();
    if hyphens.is_empty() {
        return Ok(());
    }
    if tokens.len() != 3 || tokens[1].1 != "-" {
        return Err(syntax(hyphens[0]));
    }
    let (la, lt) = tokens[0];
    let (ua, ut) = tokens[2];
    let (ln, lk, lf) = parse_partial(lt, la)?;
    let (un, uk, uf) = parse_partial(ut, ua)?;
    if lk == 0 {
        return Err(syntax(la));
    }
    if uk == 0 {
        return Err(syntax(ua));
    }
    let lower = lf.unwrap_or_else(|| vcore(padded(&ln), Vec::new()));
    add_comparator(arm, Op::Ge, lower, true);
    if let Some(upper) = uf {
        add_comparator(arm, Op::Le, upper, true);
    } else if uk == 3 {
        add_comparator(arm, Op::Le, vcore(padded(&un), Vec::new()), false);
    } else {
        let base = padded(&un);
        let upper = increment(base, uk - 1, ua)?;
        add_comparator(arm, Op::Lt, vcore(upper, vec!["0".into()]), false);
    }
    Ok(())
}

fn parse_arm(s: &str, ts: &[(usize, &str)]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        comparators: Vec::new(),
        anchors: Vec::new(),
    };
    if ts.iter().any(|(_, t)| *t == "-") {
        parse_hyphen(ts, &mut arm)?;
        return Ok(arm);
    }
    for &(at, token) in ts {
        if token.starts_with('<')
            || token.starts_with('>')
            || token.starts_with('=')
        {
            let (op, v) = parse_explicit_comparator(token, at)?;
            add_comparator(&mut arm, op, v, true);
        } else if token.starts_with('^') || token.starts_with('~') {
            parse_expanded(token, at, &mut arm)?;
        } else if token.starts_with(|c: char| c.is_ascii_digit())
            || matches!(token, "*" | "x" | "X")
        {
            parse_expanded(token, at, &mut arm)?;
        } else {
            return Err(syntax(at));
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let b = s.as_bytes();
    let mut arms = Vec::new();
    let mut arm_start = 0;
    let mut i = 0;
    let mut saw_any = false;
    while i < b.len() {
        if b[i] == b'|' && i + 1 < b.len() && b[i + 1] == b'|' {
            let left = tokens(s, arm_start, i);
            if left.is_empty() {
                return Err(RangeError::Empty { at: i });
            }
            arms.push(parse_arm(s, &left)?);
            saw_any = true;
            i += 2;
            while i < b.len() && whitespace(b[i]) {
                i += 1;
            }
            arm_start = i;
            continue;
        }
        i += 1;
    }
    let right = tokens(s, arm_start, b.len());
    if right.is_empty() {
        if saw_any {
            return Err(RangeError::Empty { at: b.len() });
        }
        return Err(RangeError::Empty { at: 0 });
    }
    arms.push(parse_arm(s, &right)?);
    Ok(Range { arms })
}

fn cmp_one(v: &Version, c: &Comparator) -> bool {
    let ord = compare_versions(v, &c.version);
    match c.op {
        Op::Eq => ord == Ordering::Equal,
        Op::Lt => ord == Ordering::Less,
        Op::Le => ord != Ordering::Greater,
        Op::Gt => ord == Ordering::Greater,
        Op::Ge => ord != Ordering::Less,
    }
}

fn arm_matches(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !arm.comparators.iter().all(|c| cmp_one(v, c)) {
        return false;
    }
    if include_prerelease || v.pre.is_empty() {
        return true;
    }
    arm.anchors.iter().any(|&a| a == core(v))
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

fn extrema(
    candidates: &[String],
    r: &Range,
    maximum: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, s) in candidates.iter().enumerate() {
        parsed.push(
            parse_version(s).map_err(|error| CandidateError { index, error })?,
        );
    }
    let mut best: Option<usize> = None;
    for i in 0..candidates.len() {
        if !matches(&parsed[i], r) {
            continue;
        }
        if let Some(j) = best {
            let ord = compare_versions(&parsed[i], &parsed[j]);
            if (maximum && ord == Ordering::Greater) || (!maximum && ord == Ordering::Less) {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

fn arm_has_witness(arm: &Arm, include_prerelease: bool) -> bool {
    // Exact equality and contradictory bounds are checked using finite bound candidates.
    let mut lower: Option<(Version, bool)> = None;
    let mut upper: Option<(Version, bool)> = None;
    let mut equal: Option<Version> = None;
    for c in &arm.comparators {
        match c.op {
            Op::Eq => equal = Some(c.version.clone()),
            Op::Ge | Op::Gt => {
                let strict = matches!(c.op, Op::Gt);
                if lower.as_ref().map_or(true, |(v, old_strict)| {
                    compare_versions(&c.version, v) == Ordering::Greater
                        || (compare_versions(&c.version, v) == Ordering::Equal
                            && strict
                            && !*old_strict)
                }) {
                    lower = Some((c.version.clone(), strict));
                }
            }
            Op::Le | Op::Lt => {
                let strict = matches!(c.op, Op::Lt);
                if upper.as_ref().map_or(true, |(v, old_strict)| {
                    compare_versions(&c.version, v) == Ordering::Less
                        || (compare_versions(&c.version, v) == Ordering::Equal
                            && strict
                            && !*old_strict)
                }) {
                    upper = Some((c.version.clone(), strict));
                }
            }
        }
    }
    if let Some(eq) = equal {
        return arm_matches(&eq, arm, include_prerelease);
    }
    let mut candidates = Vec::new();
    if let Some((v, strict)) = &lower {
        candidates.push(v.clone());
        if *strict {
            if v.pre.is_empty() {
                if let Some(next) = next_core(core(v)) {
                    candidates.push(vcore(next, vec!["0".into()]));
                    candidates.push(vcore(next, Vec::new()));
                }
            } else {
                let mut p = v.pre.clone();
                p.push("0".into());
                candidates.push(vcore(core(v), p));
            }
        }
    }
    if let Some((v, _)) = &upper {
        candidates.push(v.clone());
    }
    if lower.is_none() {
        candidates.push(vcore((0, 0, 0), vec!["0".into()]));
        candidates.push(vcore((0, 0, 0), Vec::new()));
    }
    if upper.is_none() {
        candidates.push(vcore((u64::MAX, u64::MAX, u64::MAX), Vec::new()));
    }
    candidates.into_iter().any(|v| arm_matches(&v, arm, include_prerelease))
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

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    a.arms.iter().any(|aa| {
        b.arms.iter().any(|bb| {
            let mut both = aa.clone();
            both.comparators.extend(bb.comparators.clone());
            both.anchors.extend(bb.anchors.clone());
            arm_has_witness(&both, include_prerelease)
        })
    })
}

fn display_version(v: &Version) -> String {
    let mut out = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        out.push('-');
        out.push_str(&v.pre.join("."));
    }
    out
}

fn canonical_arm(arm: &Arm) -> Option<String> {
    if !arm_has_witness(arm, false) {
        return None;
    }
    let mut lower: Option<(Version, bool)> = None;
    let mut upper: Option<(Version, bool)> = None;
    for c in &arm.comparators {
        match c.op {
            Op::Eq => {
                let v = c.version.clone();
                lower = Some((v.clone(), false));
                upper = Some((v, false));
            }
            Op::Ge | Op::Gt => {
                let strict = matches!(c.op, Op::Gt);
                if lower.as_ref().map_or(true, |(v, old_strict)| {
                    compare_versions(&c.version, v) == Ordering::Greater
                        || (compare_versions(&c.version, v) == Ordering::Equal
                            && strict
                            && !*old_strict)
                }) {
                    lower = Some((c.version.clone(), strict));
                }
            }
            Op::Le | Op::Lt => {
                let strict = matches!(c.op, Op::Lt);
                if upper.as_ref().map_or(true, |(v, old_strict)| {
                    compare_versions(&c.version, v) == Ordering::Less
                        || (compare_versions(&c.version, v) == Ordering::Equal
                            && strict
                            && !*old_strict)
                }) {
                    upper = Some((c.version.clone(), strict));
                }
            }
        }
    }
    let bound = |b: Option<(Version, bool)>, lower_side: bool| -> String {
        match b {
            None => "*".into(),
            Some((mut v, strict)) => {
                v.build.clear();
                let sym = match (lower_side, strict) {
                    (true, true) => ">",
                    (true, false) => ">=",
                    (false, true) => "<",
                    (false, false) => "<=",
                };
                format!("{}{}", sym, display_version(&v))
            }
        }
    };
    let mut anchors = arm.anchors.clone();
    anchors.sort();
    anchors.dedup();
    let pre = if anchors.is_empty() {
        "-".to_string()
    } else {
        anchors
            .iter()
            .map(|(a, b, c)| format!("{}.{}.{}", a, b, c))
            .collect::<Vec<_>>()
            .join(",")
    };
    Some(format!(
        "[{};{};pre={}]",
        bound(lower, true),
        bound(upper, false),
        pre
    ))
}

pub fn canonical(r: &Range) -> String {
    let mut arms: Vec<String> = r.arms.iter().filter_map(canonical_arm).collect();
    arms.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    arms.dedup();
    if arms.is_empty() {
        "EMPTY".into()
    } else {
        arms.join(" || ")
    }
}