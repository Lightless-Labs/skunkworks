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
        return Err(match bytes.get(*pos) {
            None | Some(b'.' | b'-' | b'+') => VersionError::MissingCore { at: *pos },
            _ => VersionError::InvalidChar { at: *pos },
        });
    }
    if *pos - start > 1 && bytes[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    s[start..*pos]
        .parse::<u64>()
        .map_err(|_| VersionError::Overflow { at: start })
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    prerelease: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut id_start = start;

    loop {
        let mut id_end = id_start;
        while id_end < end && bytes[id_end] != b'.' {
            let b = bytes[id_end];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: id_end });
            }
            id_end += 1;
        }

        if id_start == id_end {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }

        let id = &s[id_start..id_end];
        if prerelease
            && id.len() > 1
            && id.as_bytes()[0] == b'0'
            && id.bytes().all(is_digit)
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        result.push(id.to_string());

        if id_end == end {
            break;
        }
        id_start = id_end + 1;
        if id_start == end {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
    }

    Ok(result)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let bytes = s.as_bytes();
    let mut pos = 0;
    let major = parse_core(s, &mut pos)?;
    if pos >= bytes.len() || bytes[pos] != b'.' {
        return Err(match bytes.get(pos) {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: pos },
            _ => VersionError::InvalidChar { at: pos },
        });
    }
    pos += 1;

    let minor = parse_core(s, &mut pos)?;
    if pos >= bytes.len() || bytes[pos] != b'.' {
        return Err(match bytes.get(pos) {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: pos },
            _ => VersionError::InvalidChar { at: pos },
        });
    }
    pos += 1;

    let patch = parse_core(s, &mut pos)?;
    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() {
        match bytes[pos] {
            b'-' => {
                let pre_start = pos + 1;
                let plus = bytes[pre_start..]
                    .iter()
                    .position(|&b| b == b'+')
                    .map(|i| pre_start + i);
                let pre_end = plus.unwrap_or(bytes.len());
                pre = parse_identifiers(s, pre_start, pre_end, true)?;
                pos = plus.unwrap_or(bytes.len());
            }
            b'+' => {}
            _ => return Err(VersionError::InvalidChar { at: pos }),
        }
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        build = parse_identifiers(s, pos + 1, bytes.len(), false)?;
    }

    Ok(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn numeric_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn cmp_identifier(a: &str, b: &str) -> Ordering {
    match (numeric_id(a), numeric_id(b)) {
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
                let ordering = cmp_identifier(x, y);
                if ordering != Ordering::Equal {
                    return ordering;
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
struct Arm {
    tests: Vec<Test>,
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

#[derive(Clone, Debug)]
struct Test {
    op: Op,
    version: Version,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokens(s: &str) -> Vec<(usize, &str)> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        while pos < bytes.len() && is_ws(bytes[pos]) {
            pos += 1;
        }
        if pos == bytes.len() {
            break;
        }
        let start = pos;
        while pos < bytes.len() && !is_ws(bytes[pos]) {
            pos += 1;
        }
        result.push((start, &s[start..pos]));
    }
    result
}

#[derive(Clone, Debug)]
struct Operand {
    version: Version,
    full: bool,
    numeric: usize,
}

fn parse_full_operand(s: &str, at: usize) -> Result<Operand, RangeError> {
    let version = parse_version(s).map_err(|error| RangeError::Version { at, error })?;
    Ok(Operand {
        version,
        full: true,
        numeric: 3,
    })
}

fn parse_partial(s: &str, at: usize) -> Result<Operand, RangeError> {
    let fields: Vec<&str> = s.split('.').collect();
    let has_wildcard = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));

    if s.contains('-') || s.contains('+') || (fields.len() >= 3 && !has_wildcard) {
        return parse_full_operand(s, at);
    }
    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at });
    }

    let mut values = Vec::new();
    let mut seen_wildcard = false;
    for field in &fields {
        if field.is_empty() {
            return Err(RangeError::Syntax { at });
        }
        if matches!(*field, "*" | "x" | "X") {
            seen_wildcard = true;
        } else {
            if seen_wildcard
                || !field.bytes().all(is_digit)
                || (field.len() > 1 && field.as_bytes()[0] == b'0')
            {
                return Err(RangeError::Syntax { at });
            }
            values.push(
                field
                    .parse::<u64>()
                    .map_err(|_| RangeError::Syntax { at })?,
            );
        }
    }

    let numeric = values.len();
    let (major, minor, patch) = match values.as_slice() {
        [] => (0, 0, 0),
        [a] => (*a, 0, 0),
        [a, b] => (*a, *b, 0),
        [a, b, c] => (*a, *b, *c),
        _ => return Err(RangeError::Syntax { at }),
    };

    Ok(Operand {
        version: Version {
            major,
            minor,
            patch,
            pre: Vec::new(),
            build: Vec::new(),
        },
        full: false,
        numeric,
    })
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

fn increment(v: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let core = match field {
        0 => (
            v.major.checked_add(1).ok_or(RangeError::Syntax { at })?,
            0,
            0,
        ),
        1 => (
            v.major,
            v.minor.checked_add(1).ok_or(RangeError::Syntax { at })?,
            0,
        ),
        _ => (
            v.major,
            v.minor,
            v.patch.checked_add(1).ok_or(RangeError::Syntax { at })?,
        ),
    };
    Ok(endpoint(core, vec!["0".to_string()]))
}

fn add_test(arm: &mut Arm, op: Op, version: Version) {
    if !version.pre.is_empty() {
        arm.anchors
            .push((version.major, version.minor, version.patch));
    }
    arm.tests.push(Test { op, version });
}

fn parse_token(token: &str, start: usize, arm: &mut Arm) -> Result<(), RangeError> {
    if token.starts_with('^') || token.starts_with('~') {
        let caret = token.starts_with('^');
        let operand_at = start + 1;
        let operand = parse_partial(&token[1..], operand_at)?;
        if operand.numeric == 0 {
            return Err(RangeError::Syntax { at: start });
        }
        add_test(arm, Op::Ge, operand.version.clone());

        let v = &operand.version;
        let field = if caret {
            if v.major > 0 || operand.numeric == 1 {
                0
            } else if v.minor > 0 || operand.numeric == 2 {
                1
            } else {
                2
            }
        } else if operand.numeric == 1 {
            0
        } else {
            1
        };
        add_test(arm, Op::Lt, increment(v, field, operand_at)?);
        return Ok(());
    }

    let (op, body, body_at) = if token.starts_with(">=") {
        (Some(Op::Ge), &token[2..], start + 2)
    } else if token.starts_with("<=") {
        (Some(Op::Le), &token[2..], start + 2)
    } else if token.starts_with('>') {
        (Some(Op::Gt), &token[1..], start + 1)
    } else if token.starts_with('<') {
        (Some(Op::Lt), &token[1..], start + 1)
    } else if token.starts_with('=') {
        (Some(Op::Eq), &token[1..], start + 1)
    } else {
        (None, token, start)
    };

    if let Some(op) = op {
        if body.is_empty() || !body.as_bytes()[0].is_ascii_digit() {
            return Err(RangeError::Syntax { at: start });
        }
        let operand = parse_full_operand(body, body_at)?;
        add_test(arm, op, operand.version);
        return Ok(());
    }

    let operand = parse_partial(body, body_at)?;
    if operand.full {
        add_test(arm, Op::Eq, operand.version);
    } else if operand.numeric == 1 {
        add_test(arm, Op::Ge, operand.version.clone());
        add_test(arm, Op::Lt, increment(&operand.version, 0, body_at)?);
    } else if operand.numeric == 2 {
        add_test(arm, Op::Ge, operand.version.clone());
        add_test(arm, Op::Lt, increment(&operand.version, 1, body_at)?);
    }
    Ok(())
}

fn parse_arm(text: &str, base: usize) -> Result<Arm, RangeError> {
    let ts = tokens(text);
    if ts.is_empty() {
        return Err(RangeError::Empty { at: base });
    }

    let mut arm = Arm {
        tests: Vec::new(),
        anchors: Vec::new(),
    };

    if let Some((minus_at, _)) = ts.iter().find(|(_, t)| *t == "-") {
        if ts.len() != 3 || ts[1].1 != "-" {
            return Err(RangeError::Syntax {
                at: base + *minus_at,
            });
        }

        let (lo_off, lo_text) = ts[0];
        let (hi_off, hi_text) = ts[2];
        let lo_at = base + lo_off;
        let hi_at = base + hi_off;
        let lo = parse_partial(lo_text, lo_at)?;
        let hi = parse_partial(hi_text, hi_at)?;
        if lo.numeric == 0 {
            return Err(RangeError::Syntax { at: lo_at });
        }
        if hi.numeric == 0 {
            return Err(RangeError::Syntax { at: hi_at });
        }

        add_test(&mut arm, Op::Ge, lo.version);
        if hi.full {
            add_test(&mut arm, Op::Le, hi.version);
        } else {
            let field = if hi.numeric == 1 { 0 } else { 1 };
            add_test(&mut arm, Op::Lt, increment(&hi.version, field, hi_at)?);
        }
        return Ok(arm);
    }

    for (offset, token) in ts {
        let at = base + offset;
        let first = token.as_bytes()[0];
        if !first.is_ascii_digit()
            && !matches!(first, b'*' | b'x' | b'X' | b'^' | b'~' | b'<' | b'>' | b'=')
        {
            return Err(RangeError::Syntax { at });
        }
        parse_token(token, at, &mut arm)?;
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let ts = tokens(s);
    if ts.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms = Vec::new();
    let mut arm_start: Option<usize> = None;
    let mut arm_tokens = Vec::new();

    for (at, token) in ts {
        if token == "||" {
            if arm_tokens.is_empty() {
                return Err(RangeError::Empty { at });
            }
            let start = arm_start.unwrap();
            let end = at;
            arms.push(parse_arm(&s[start..end], start)?);
            arm_start = None;
            arm_tokens.clear();
        } else {
            if arm_start.is_none() {
                arm_start = Some(at);
            }
            arm_tokens.push((at, token));
        }
    }

    if arm_tokens.is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    let start = arm_start.unwrap();
    arms.push(parse_arm(&s[start..], start)?);
    Ok(Range { arms })
}

fn test_matches(v: &Version, test: &Test) -> bool {
    match (compare_versions(v, &test.version), test.op) {
        (Ordering::Equal, Op::Eq) => true,
        (Ordering::Less, Op::Lt | Op::Le | Op::Ge) => test.op != Op::Ge,
        (Ordering::Greater, Op::Gt | Op::Ge | Op::Le) => test.op != Op::Le,
        (Ordering::Equal, Op::Lt | Op::Le | Op::Gt | Op::Ge) => {
            matches!(test.op, Op::Le | Op::Ge)
        }
        _ => false,
    }
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|arm| {
        arm.tests.iter().all(|t| test_matches(v, t))
            && (include_prerelease
                || v.pre.is_empty()
                || arm.anchors.contains(&(v.major, v.minor, v.patch)))
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

fn extrema(
    candidates: &[String],
    range: &Range,
    greatest: bool,
) -> Result<Option<String>, CandidateError> {
    let parsed: Vec<Version> = candidates
        .iter()
        .enumerate()
        .map(|(index, s)| {
            parse_version(s).map_err(|error| CandidateError { index, error })
        })
        .collect::<Result<_, _>>()?;

    let mut best: Option<usize> = None;
    for (i, version) in parsed.iter().enumerate() {
        if !matches(version, range) {
            continue;
        }
        if let Some(j) = best {
            let ordering = compare_versions(version, &parsed[j]);
            if (greatest && ordering == Ordering::Greater)
                || (!greatest && ordering == Ordering::Less)
            {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
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

fn version_string(v: &Version, include_build: bool) -> String {
    let mut result = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        result.push('-');
        result.push_str(&v.pre.join("."));
    }
    if include_build && !v.build.is_empty() {
        result.push('+');
        result.push_str(&v.build.join("."));
    }
    result
}

type Bound = (Version, bool);

fn stronger_lower(old: Option<Bound>, new: Bound) -> Option<Bound> {
    match old {
        None => Some(new),
        Some(o) => {
            let ordering = compare_versions(&new.0, &o.0);
            if ordering == Ordering::Greater
                || (ordering == Ordering::Equal && !new.1 && o.1)
            {
                Some(new)
            } else {
                Some(o)
            }
        }
    }
}

fn stronger_upper(old: Option<Bound>, new: Bound) -> Option<Bound> {
    match old {
        None => Some(new),
        Some(o) => {
            let ordering = compare_versions(&new.0, &o.0);
            if ordering == Ordering::Less
                || (ordering == Ordering::Equal && !new.1 && o.1)
            {
                Some(new)
            } else {
                Some(o)
            }
        }
    }
}

fn arm_bounds(arm: &Arm) -> (Option<Bound>, Option<Bound>) {
    let mut lower = None;
    let mut upper = None;
    for test in &arm.tests {
        match test.op {
            Op::Eq => {
                lower = stronger_lower(lower, (test.version.clone(), true));
                upper = stronger_upper(upper, (test.version.clone(), true));
            }
            Op::Gt | Op::Ge => {
                lower = stronger_lower(
                    lower,
                    (test.version.clone(), test.op == Op::Ge),
                );
            }
            Op::Lt | Op::Le => {
                upper = stronger_upper(
                    upper,
                    (test.version.clone(), test.op == Op::Le),
                );
            }
        }
    }
    (lower, upper)
}

fn next_core(core: (u64, u64, u64)) -> Option<(u64, u64, u64)> {
    if core.2 < u64::MAX {
        Some((core.0, core.1, core.2 + 1))
    } else if core.1 < u64::MAX {
        Some((core.0, core.1 + 1, 0))
    } else if core.0 < u64::MAX {
        Some((core.0 + 1, 0, 0))
    } else {
        None
    }
}

fn stable(core: (u64, u64, u64)) -> Version {
    endpoint(core, Vec::new())
}

fn pre_zero(core: (u64, u64, u64)) -> Version {
    endpoint(core, vec!["0".to_string()])
}

fn lower_candidate(lower: &Option<Bound>) -> Option<Version> {
    match lower {
        None => Some(stable((0, 0, 0))),
        Some((version, true)) => Some(version.clone()),
        Some((version, false)) if version.pre.is_empty() => {
            next_core((version.major, version.minor, version.patch)).map(stable)
        }
        Some((version, false)) => {
            let mut pre = version.pre.clone();
            pre.push("0".to_string());
            Some(endpoint(
                (version.major, version.minor, version.patch),
                pre,
            ))
        }
    }
}

fn within_upper(candidate: &Version, upper: &Option<Bound>) -> bool {
    match upper {
        None => true,
        Some((version, inclusive)) => {
            let ordering = compare_versions(candidate, version);
            ordering == Ordering::Less || (*inclusive && ordering == Ordering::Equal)
        }
    }
}

fn interval_has_stable(lower: &Option<Bound>, upper: &Option<Bound>) -> bool {
    let candidate = match lower {
        None => stable((0, 0, 0)),
        Some((v, inclusive)) if v.pre.is_empty() && *inclusive => v.clone(),
        Some((v, _)) if v.pre.is_empty() => {
            match next_core((v.major, v.minor, v.patch)) {
                Some(core) => stable(core),
                None => return false,
            }
        }
        Some((v, _)) => stable((v.major, v.minor, v.patch)),
    };
    within_upper(&candidate, upper)
        && match lower {
            None => true,
            Some((v, inclusive)) => {
                let c = compare_versions(&candidate, v);
                c == Ordering::Greater || (*inclusive && c == Ordering::Equal)
            }
        }
}

fn interval_has_prerelease(
    lower: &Option<Bound>,
    upper: &Option<Bound>,
    anchors: &[(u64, u64, u64)],
) -> bool {
    for &core in anchors {
        let mut candidate = pre_zero(core);
        if let Some((low, inclusive)) = lower {
            let c = compare_versions(&candidate, low);
            if c == Ordering::Less || (c == Ordering::Equal && !inclusive) {
                if (low.major, low.minor, low.patch) == core {
                    if low.pre.is_empty() {
                        candidate = pre_zero(core);
                    } else {
                        candidate = low.clone();
                        if !inclusive {
                            let mut ids = low.pre.clone();
                            ids.push("0".to_string());
                            candidate = endpoint(core, ids);
                        }
                    }
                }
            }
        }

        let low_ok = match lower {
            None => true,
            Some((low, inclusive)) => {
                let c = compare_versions(&candidate, low);
                c == Ordering::Greater || (*inclusive && c == Ordering::Equal)
            }
        };
        if low_ok && within_upper(&candidate, upper) {
            return true;
        }

        let stable_at_core = stable(core);
        let after_lower = match lower {
            None => true,
            Some((low, inclusive)) => {
                let c = compare_versions(&stable_at_core, low);
                c == Ordering::Greater || (*inclusive && c == Ordering::Equal)
            }
        };
        if after_lower && within_upper(&pre_zero(core), upper) {
            return true;
        }
    }
    false
}

fn arm_has_default_witness(arm: &Arm) -> bool {
    let (lower, upper) = arm_bounds(arm);
    let anchors = &arm.anchors;

    if let (Some((l, li)), Some((u, ui))) = (&lower, &upper) {
        let c = compare_versions(l, u);
        if c == Ordering::Greater || (c == Ordering::Equal && !(*li && *ui)) {
            return false;
        }
    }

    interval_has_stable(&lower, &upper)
        || interval_has_prerelease(&lower, &upper, anchors)
}

pub fn canonical(r: &Range) -> String {
    let mut result = Vec::new();
    for arm in &r.arms {
        if !arm_has_default_witness(arm) {
            continue;
        }
        let (lower, upper) = arm_bounds(arm);
        let mut anchors = arm.anchors.clone();
        anchors.sort();
        anchors.dedup();

        let low = lower
            .map(|(v, inclusive)| {
                format!(
                    "{}{}",
                    if inclusive { ">=" } else { ">" },
                    version_string(&v, false)
                )
            })
            .unwrap_or_else(|| "*".to_string());
        let high = upper
            .map(|(v, inclusive)| {
                format!(
                    "{}{}",
                    if inclusive { "<=" } else { "<" },
                    version_string(&v, false)
                )
            })
            .unwrap_or_else(|| "*".to_string());
        let pre = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors
                .iter()
                .map(|(a, b, c)| format!("{a}.{b}.{c}"))
                .collect::<Vec<_>>()
                .join(",")
        };
        result.push(format!("[{low};{high};pre={pre}]"));
    }

    result.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    result.dedup();
    if result.is_empty() {
        "EMPTY".to_string()
    } else {
        result.join(" || ")
    }
}

fn arm_intersects(a: &Arm, b: &Arm, include_prerelease: bool) -> bool {
    let (al, au) = arm_bounds(a);
    let (bl, bu) = arm_bounds(b);

    let lower = match (al, bl) {
        (None, other) | (other, None) => other,
        (Some(x), Some(y)) => stronger_lower(Some(x), y),
    };
    let upper = match (au, bu) {
        (None, other) | (other, None) => other,
        (Some(x), Some(y)) => stronger_upper(Some(x), y),
    };

    if let (Some((l, li)), Some((u, ui))) = (&lower, &upper) {
        let c = compare_versions(l, u);
        if c == Ordering::Greater || (c == Ordering::Equal && !(*li && *ui)) {
            return false;
        }
    }

    if include_prerelease {
        return lower_candidate(&lower)
            .is_some_and(|v| within_upper(&v, &upper))
            || interval_has_stable(&lower, &upper);
    }

    let common: Vec<_> = a
        .anchors
        .iter()
        .copied()
        .filter(|core| b.anchors.contains(core))
        .collect();
    interval_has_stable(&lower, &upper)
        || interval_has_prerelease(&lower, &upper, &common)
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    a.arms
        .iter()
        .any(|x| b.arms.iter().any(|y| arm_intersects(x, y, include_prerelease)))
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

fn collect_ranges(
    expr: &SetExpr,
    package: &str,
    label: &str,
    path: &mut Vec<usize>,
    out: &mut Vec<Range>,
) -> Result<(), SelectError> {
    match expr {
        SetExpr::Range(s) => {
            let range = parse_range(s).map_err(|error| SelectError::InvalidRange {
                package: package.to_string(),
                label: label.to_string(),
                path: path.clone(),
                error,
            })?;
            out.push(range);
        }
        SetExpr::Union(children) | SetExpr::Intersection(children) => {
            for (i, child) in children.iter().enumerate() {
                path.push(i);
                collect_ranges(child, package, label, path, out)?;
                path.pop();
            }
        }
    }
    Ok(())
}

fn expression_matches(
    expr: &SetExpr,
    version: &Version,
    include: bool,
    ranges: &[Range],
    cursor: &mut usize,
) -> bool {
    match expr {
        SetExpr::Range(_) => {
            let result = matches_with(version, &ranges[*cursor], include);
            *cursor += 1;
            result
        }
        SetExpr::Union(children) => children.iter().any(|child| {
            expression_matches(child, version, include, ranges, cursor)
        }),
        SetExpr::Intersection(children) => children.iter().all(|child| {
            expression_matches(child, version, include, ranges, cursor)
        }),
    }
}

fn constraint_matches(
    constraint: &Constraint,
    version: &Version,
    include: bool,
    ranges: &[Range],
) -> bool {
    expression_matches(&constraint.expr, version, include, ranges, &mut 0)
}

fn diagnostic_labels(
    package: &Package,
    versions: &[Version],
    include: bool,
    parsed: &[Vec<Range>],
) -> Vec<String> {
    if versions.is_empty() || package.constraints.is_empty() {
        return Vec::new();
    }

    fn search(
        package: &Package,
        versions: &[Version],
        include: bool,
        parsed: &[Vec<Range>],
        target_size: usize,
        start: usize,
        chosen: &mut Vec<usize>,
    ) -> Option<Vec<String>> {
        if chosen.len() == target_size {
            let has_candidate = versions.iter().any(|version| {
                chosen.iter().all(|&i| {
                    constraint_matches(
                        &package.constraints[i],
                        version,
                        include,
                        &parsed[i],
                    )
                })
            });
            if !has_candidate {
                let mut labels: Vec<String> = chosen
                    .iter()
                    .map(|&i| package.constraints[i].label.clone())
                    .collect();
                labels.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
                return Some(labels);
            }
            return None;
        }

        let remaining = target_size - chosen.len();
        if package.constraints.len().saturating_sub(start) < remaining {
            return None;
        }
        for i in start..=package.constraints.len() - remaining {
            chosen.push(i);
            if let Some(found) = search(
                package,
                versions,
                include,
                parsed,
                target_size,
                i + 1,
                chosen,
            ) {
                return Some(found);
            }
            chosen.pop();
        }
        None
    }

    for size in 1..=package.constraints.len() {
        if let Some(labels) = search(
            package,
            versions,
            include,
            parsed,
            size,
            0,
            &mut Vec::new(),
        ) {
            return labels;
        }
    }
    Vec::new()
}

pub fn select_packages(
    packages: &[Package],
    preference: Preference,
    include_prerelease: bool,
) -> Result<Vec<Selection>, SelectError> {
    let mut package_names = HashSet::new();
    for package in packages {
        if !package_names.insert(package.name.clone()) {
            return Err(SelectError::DuplicatePackage {
                name: package.name.clone(),
            });
        }
    }

    for package in packages {
        let mut labels = HashSet::new();
        for constraint in &package.constraints {
            if !labels.insert(constraint.label.clone()) {
                return Err(SelectError::DuplicateLabel {
                    package: package.name.clone(),
                    label: constraint.label.clone(),
                });
            }
        }
    }

    let mut all_versions = Vec::new();
    for package in packages {
        let mut versions = Vec::new();
        for (index, candidate) in package.candidates.iter().enumerate() {
            versions.push(parse_version(candidate).map_err(|error| {
                SelectError::InvalidCandidate {
                    package: package.name.clone(),
                    index,
                    error,
                }
            })?);
        }
        all_versions.push(versions);
    }

    let mut all_ranges: Vec<Vec<Vec<Range>>> = Vec::new();
    for package in packages {
        let mut package_ranges = Vec::new();
        for constraint in &package.constraints {
            let mut ranges = Vec::new();
            collect_ranges(
                &constraint.expr,
                &package.name,
                &constraint.label,
                &mut Vec::new(),
                &mut ranges,
            )?;
            package_ranges.push(ranges);
        }
        all_ranges.push(package_ranges);
    }

    let mut selections = Vec::new();
    for (pi, package) in packages.iter().enumerate() {
        let versions = &all_versions[pi];
        let mut chosen: Option<usize> = None;

        for (i, version) in versions.iter().enumerate() {
            let accepted = package.constraints.iter().enumerate().all(|(ci, constraint)| {
                constraint_matches(
                    constraint,
                    version,
                    include_prerelease,
                    &all_ranges[pi][ci],
                )
            });
            if !accepted {
                continue;
            }

            if let Some(j) = chosen {
                let ordering = compare_versions(version, &versions[j]);
                let better = match preference {
                    Preference::Highest => ordering == Ordering::Greater,
                    Preference::Lowest => ordering == Ordering::Less,
                    Preference::StableFirst => {
                        (version.pre.is_empty() && !versions[j].pre.is_empty())
                            || (version.pre.is_empty() == versions[j].pre.is_empty()
                                && ordering == Ordering::Greater)
                    }
                };
                if better {
                    chosen = Some(i);
                }
            } else {
                chosen = Some(i);
            }
        }

        if let Some(i) = chosen {
            selections.push(Selection {
                package: package.name.clone(),
                version: package.candidates[i].clone(),
            });
        } else {
            let labels = diagnostic_labels(
                package,
                versions,
                include_prerelease,
                &all_ranges[pi],
            );
            return Err(SelectError::NoSolution {
                package: package.name.clone(),
                labels,
            });
        }
    }

    Ok(selections)
}

fn main() {}