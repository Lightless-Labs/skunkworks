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

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return Err(VersionError::Empty);
    }

    let mut pos = 0usize;
    let mut core = [0u64; 3];

    for i in 0..3 {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if start == pos {
            if pos == bytes.len() || matches!(bytes[pos], b'.' | b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            return Err(VersionError::InvalidChar { at: pos });
        }

        if pos - start > 1 && bytes[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }
        let mut n = 0u64;
        for &b in &bytes[start..pos] {
            n = n.checked_mul(10)
                .and_then(|v| v.checked_add((b - b'0') as u64))
                .ok_or(VersionError::Overflow { at: start })?;
        }
        core[i] = n;

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

    fn parse_ids(
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
                if !is_ident_byte(bytes[p]) {
                    return Err(VersionError::InvalidChar { at: p });
                }
                p += 1;
            }
            if p == id_start {
                return Err(VersionError::EmptyIdentifier { at: id_start });
            }
            let id = &s[id_start..p];
            if prerelease
                && id.bytes().all(|b| b.is_ascii_digit())
                && id.len() > 1
                && id.as_bytes()[0] == b'0'
            {
                return Err(VersionError::LeadingZero { at: id_start });
            }
            out.push(id.to_string());
            if p == end {
                break;
            }
            id_start = p + 1;
            if id_start == end {
                return Err(VersionError::EmptyIdentifier { at: id_start });
            }
        }
        Ok(out)
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] == b'-' {
        let start = pos + 1;
        let plus = bytes[start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|x| start + x)
            .unwrap_or(bytes.len());
        pre = parse_ids(s, start, plus, true)?;
        pos = plus;
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        let start = pos + 1;
        build = parse_ids(s, start, bytes.len(), false)?;
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

fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    for (x, y) in a.iter().zip(b) {
        let xn = numeric_id(x);
        let yn = numeric_id(y);
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
enum Op {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
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

fn whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
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

fn bump_tuple(t: (u64, u64, u64), field: usize) -> Option<(u64, u64, u64)> {
    match field {
        0 => Some((t.0.checked_add(1)?, 0, 0)),
        1 => Some((t.0, t.1.checked_add(1)?, 0)),
        _ => Some((t.0, t.1, t.2.checked_add(1)?)),
    }
}

#[derive(Clone, Debug)]
struct Partial {
    version: Version,
    numeric: usize,
    full: bool,
}

fn parse_partial(s: &str, at: usize) -> Result<Partial, RangeError> {
    let bytes = s.as_bytes();
    let full = s.contains('-')
        || s.contains('+')
        || (s.split('.').count() >= 3
            && !s.split('.').any(|x| matches!(x, "*" | "x" | "X")));
    if full {
        let v = parse_version(s).map_err(|error| RangeError::Version { at, error })?;
        return Ok(Partial {
            version: v,
            numeric: 3,
            full: true,
        });
    }

    let fields: Vec<&str> = s.split('.').collect();
    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|f| f.is_empty()) {
        return Err(RangeError::Syntax { at });
    }
    let mut vals = [0u64; 3];
    let mut numeric = 0usize;
    let mut wildcard_seen = false;
    for (i, f) in fields.iter().enumerate() {
        if matches!(*f, "*" | "x" | "X") {
            wildcard_seen = true;
        } else {
            if wildcard_seen
                || !f.bytes().all(|b| b.is_ascii_digit())
                || (f.len() > 1 && f.as_bytes()[0] == b'0')
            {
                return Err(RangeError::Syntax { at });
            }
            let mut n = 0u64;
            for b in f.bytes() {
                n = n
                    .checked_mul(10)
                    .and_then(|x| x.checked_add((b - b'0') as u64))
                    .ok_or(RangeError::Syntax { at })?;
            }
            vals[i] = n;
            numeric += 1;
        }
    }
    if numeric > 0 && wildcard_seen {
        for f in fields.iter().skip(numeric) {
            if !matches!(*f, "*" | "x" | "X") {
                return Err(RangeError::Syntax { at });
            }
        }
    }
    Ok(Partial {
        version: tuple_version((vals[0], vals[1], vals[2]), Vec::new()),
        numeric,
        full: false,
    })
}

fn add_cmp(arm: &mut Arm, op: Op, version: Version, explicit: bool) {
    if explicit && !version.pre.is_empty() {
        arm.anchors.push((version.major, version.minor, version.patch));
    }
    arm.cmps.push(Cmp {
        op,
        version,
        anchor: explicit,
    });
}

fn expand_partial(
    arm: &mut Arm,
    partial: Partial,
    at: usize,
    caret: bool,
    tilde: bool,
) -> Result<(), RangeError> {
    let k = partial.numeric;
    if caret || tilde {
        if k == 0 {
            return Err(RangeError::Syntax { at });
        }
        let low = partial.version.clone();
        add_cmp(arm, Op::Ge, low.clone(), !low.pre.is_empty());
        let t = (low.major, low.minor, low.patch);
        let field = if caret {
            if low.major > 0 || k == 1 {
                0
            } else if low.minor > 0 || k == 2 {
                1
            } else {
                2
            }
        } else if k == 1 {
            0
        } else {
            1
        };
        let upper = bump_tuple(t, field).ok_or(RangeError::Syntax { at })?;
        add_cmp(arm, Op::Lt, tuple_version(upper, vec!["0".into()]), false);
    } else if partial.full {
        add_cmp(arm, Op::Eq, partial.version.clone(), !partial.version.pre.is_empty());
    } else if k == 0 {
    } else {
        let low = partial.version.clone();
        add_cmp(arm, Op::Ge, low.clone(), false);
        if k < 3 {
            let upper = bump_tuple(
                (low.major, low.minor, low.patch),
                if k == 1 { 0 } else { 1 },
            )
            .ok_or(RangeError::Syntax { at })?;
            add_cmp(arm, Op::Lt, tuple_version(upper, vec!["0".into()]), false);
        } else {
            add_cmp(arm, Op::Le, low, false);
        }
    }
    Ok(())
}

fn parse_comparator_token(token: &str, start: usize, arm: &mut Arm) -> Result<(), RangeError> {
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
    let operand_at = start + n;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: start });
    }
    let version = parse_version(operand)
        .map_err(|error| RangeError::Version { at: operand_at, error })?;
    add_cmp(arm, op, version.clone(), !version.pre.is_empty());
    Ok(())
}

fn parse_arm(tokens: &[(String, usize)]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        cmps: Vec::new(),
        anchors: Vec::new(),
    };
    let hyphens: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, (t, _))| if t == "-" { Some(i) } else { None })
        .collect();

    if !hyphens.is_empty() {
        let h = hyphens[0];
        if tokens.len() != 3 || h != 1 {
            return Err(RangeError::Syntax { at: tokens[h].1 });
        }
        let (ls, la) = &tokens[0];
        let (us, ua) = &tokens[2];
        let lower = parse_partial(ls, *la)?;
        let upper = parse_partial(us, *ua)?;
        if lower.numeric == 0 {
            return Err(RangeError::Syntax { at: *la });
        }
        if upper.numeric == 0 {
            return Err(RangeError::Syntax { at: *ua });
        }
        if !lower.version.pre.is_empty() {
            arm.anchors.push((lower.version.major, lower.version.minor, lower.version.patch));
        }
        add_cmp(&mut arm, Op::Ge, lower.version.clone(), false);
        if upper.full {
            if !upper.version.pre.is_empty() {
                arm.anchors.push((upper.version.major, upper.version.minor, upper.version.patch));
            }
            add_cmp(&mut arm, Op::Le, upper.version, false);
        } else if upper.numeric == 3 {
            add_cmp(&mut arm, Op::Le, upper.version, false);
        } else {
            let field = if upper.numeric == 1 { 0 } else { 1 };
            let t = (upper.version.major, upper.version.minor, upper.version.patch);
            let bumped = bump_tuple(t, field).ok_or(RangeError::Syntax { at: *ua })?;
            add_cmp(&mut arm, Op::Lt, tuple_version(bumped, vec!["0".into()]), false);
        }
        return Ok(arm);
    }

    for (token, start) in tokens {
        if token.starts_with('^') || token.starts_with('~') {
            let caret = token.starts_with('^');
            let operand = &token[1..];
            let at = *start + 1;
            let partial = parse_partial(operand, at)?;
            expand_partial(&mut arm, partial, at, caret, !caret)?;
        } else if token.starts_with('<') || token.starts_with('>') || token.starts_with('=') {
            parse_comparator_token(token, *start, &mut arm)?;
        } else {
            let b = token.as_bytes().first().copied();
            if !matches!(b, Some(b'0'..=b'9' | b'*' | b'x' | b'X')) {
                return Err(RangeError::Syntax { at: *start });
            }
            let partial = parse_partial(token, *start)?;
            expand_partial(&mut arm, partial, *start, false, false)?;
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = Vec::new();
    let mut pos = 0usize;
    let mut arm_tokens: Vec<(String, usize)> = Vec::new();
    let mut saw_any = false;
    let mut pending_separator: Option<usize> = None;

    while pos < bytes.len() {
        while pos < bytes.len() && whitespace(bytes[pos]) {
            pos += 1;
        }
        if pos == bytes.len() {
            break;
        }
        if pos + 1 < bytes.len() && bytes[pos] == b'|' && bytes[pos + 1] == b'|' {
            if arm_tokens.is_empty() {
                return Err(RangeError::Empty { at: pos });
            }
            arms.push(parse_arm(&arm_tokens)?);
            arm_tokens.clear();
            pending_separator = Some(pos);
            pos += 2;
            if pos < bytes.len() && !whitespace(bytes[pos]) {
                return Err(RangeError::Syntax { at: pos - 2 });
            }
            continue;
        }
        let start = pos;
        while pos < bytes.len() && !whitespace(bytes[pos]) {
            pos += 1;
        }
        let token = &s[start..pos];
        if token == "||" {
            if arm_tokens.is_empty() {
                return Err(RangeError::Empty { at: start });
            }
            arms.push(parse_arm(&arm_tokens)?);
            arm_tokens.clear();
            pending_separator = Some(start);
        } else {
            arm_tokens.push((token.to_string(), start));
            saw_any = true;
        }
    }

    if !arm_tokens.is_empty() {
        arms.push(parse_arm(&arm_tokens)?);
    } else if let Some(_) = pending_separator {
        return Err(RangeError::Empty { at: s.len() });
    } else if !saw_any {
        return Err(RangeError::Empty { at: 0 });
    }
    Ok(Range { arms })
}

fn cmp_matches(v: &Version, c: &Cmp) -> bool {
    let ord = compare_versions(v, &c.version);
    match c.op {
        Op::Eq => ord == Ordering::Equal,
        Op::Lt => ord == Ordering::Less,
        Op::Le => ord != Ordering::Greater,
        Op::Gt => ord == Ordering::Greater,
        Op::Ge => ord != Ordering::Less,
    }
}

fn matches_arm(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !v.pre.is_empty()
        && !include_prerelease
        && !arm.anchors.contains(&(v.major, v.minor, v.patch))
    {
        return false;
    }
    arm.cmps.iter().all(|c| cmp_matches(v, c))
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|arm| matches_arm(v, arm, include_prerelease))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn satisfying_extreme(
    candidates: &[String],
    r: &Range,
    maximum: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let version = parse_version(candidate).map_err(|error| CandidateError { index, error })?;
        parsed.push(version);
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

pub fn max_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    satisfying_extreme(candidates, r, true)
}

pub fn min_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    satisfying_extreme(candidates, r, false)
}

#[derive(Clone, Debug)]
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
            Op::Eq => {
                equal = Some(c.version.clone());
                let b = Bound {
                    v: c.version.clone(),
                    inclusive: true,
                };
                lower = Some(b.clone());
                upper = Some(b);
            }
            Op::Gt | Op::Ge => {
                let b = Bound {
                    v: c.version.clone(),
                    inclusive: matches!(c.op, Op::Ge),
                };
                if lower.as_ref().map_or(true, |old| {
                    compare_versions(&b.v, &old.v) == Ordering::Greater
                        || (compare_versions(&b.v, &old.v) == Ordering::Equal
                            && !b.inclusive
                            && old.inclusive)
                }) {
                    lower = Some(b);
                }
            }
            Op::Lt | Op::Le => {
                let b = Bound {
                    v: c.version.clone(),
                    inclusive: matches!(c.op, Op::Le),
                };
                if upper.as_ref().map_or(true, |old| {
                    compare_versions(&b.v, &old.v) == Ordering::Less
                        || (compare_versions(&b.v, &old.v) == Ordering::Equal
                            && !b.inclusive
                            && old.inclusive)
                }) {
                    upper = Some(b);
                }
            }
        }
    }
    (lower, upper, equal)
}

fn bound_string(b: &Option<Bound>, lower: bool) -> String {
    let Some(b) = b else { return "*".into() };
    let mut v = b.v.clone();
    v.build.clear();
    let text = version_string(&v);
    match (lower, b.inclusive) {
        (true, true) => format!(">={text}"),
        (true, false) => format!(">{text}"),
        (false, true) => format!("<={text}"),
        (false, false) => format!("<{text}"),
    }
}

fn version_string(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    if !v.build.is_empty() {
        s.push('+');
        s.push_str(&v.build.join("."));
    }
    s
}

fn core_cmp(a: &(u64, u64, u64), b: &(u64, u64, u64)) -> Ordering {
    a.cmp(b)
}

fn arm_has_witness(arm: &Arm) -> bool {
    if let Some((_, _, eq)) = {
        let (_, _, e) = arm_bounds(arm);
        e.map(|v| (v.major, v.minor, v.patch, v))
            .map(|(a, b, c, v)| (a, b, Some(v)))
            .or(Some((0, 0, None)))
    } {
        if let Some(eq) = eq {
            return matches_arm(&eq, arm, false);
        }
    }

    let (lower, upper, _) = arm_bounds(arm);
    if let (Some(l), Some(u)) = (&lower, &upper) {
        match compare_versions(&l.v, &u.v) {
            Ordering::Greater => return false,
            Ordering::Equal if !l.inclusive || !u.inclusive => return false,
            _ => {}
        }
    }
    let candidates = witness_candidates(&lower, &upper, &arm.anchors);
    candidates.into_iter().any(|v| matches_arm(&v, arm, false))
}

fn witness_candidates(
    lower: &Option<Bound>,
    upper: &Option<Bound>,
    anchors: &[(u64, u64, u64)],
) -> Vec<Version> {
    let mut out = Vec::new();
    if let Some(l) = lower {
        out.push(l.v.clone());
        if !l.inclusive && !l.v.pre.is_empty() {
            let mut seq = l.v.pre.clone();
            seq.push("0".into());
            out.push(tuple_version(
                (l.v.major, l.v.minor, l.v.patch),
                seq,
            ));
        }
        if l.v.pre.is_empty() {
            if let Some(t) = next_core((l.v.major, l.v.minor, l.v.patch)) {
                out.push(tuple_version(t, vec!["0".into()]));
                out.push(tuple_version(t, Vec::new()));
            }
        }
    } else {
        out.push(tuple_version((0, 0, 0), vec!["0".into()]));
        out.push(tuple_version((0, 0, 0), Vec::new()));
    }
    for &(a, b, c) in anchors {
        out.push(tuple_version((a, b, c), vec!["0".into()]));
        out.push(tuple_version((a, b, c), Vec::new()));
    }
    if let Some(u) = upper {
        out.push(u.v.clone());
        if u.v.pre.is_empty() {
            out.push(tuple_version(
                (u.v.major, u.v.minor, u.v.patch),
                vec!["0".into()],
            ));
        }
    }
    out
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

pub fn canonical(r: &Range) -> String {
    let mut texts = Vec::new();
    for arm in &r.arms {
        if !arm_has_witness(arm) {
            continue;
        }
        let (lower, upper, _) = arm_bounds(arm);
        let mut anchors = arm.anchors.clone();
        anchors.sort_by(core_cmp);
        anchors.dedup();
        let anchor_text = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors
                .iter()
                .map(|(a, b, c)| format!("{a}.{b}.{c}"))
                .collect::<Vec<_>>()
                .join(",")
        };
        texts.push(format!(
            "[{};{};pre={}]",
            bound_string(&lower, true),
            bound_string(&upper, false),
            anchor_text
        ));
    }
    texts.sort();
    texts.dedup();
    if texts.is_empty() {
        "EMPTY".into()
    } else {
        texts.join(" || ")
    }
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for aa in &a.arms {
        for bb in &b.arms {
            let anchors: Vec<_> = if include_prerelease {
                Vec::new()
            } else {
                aa.anchors
                    .iter()
                    .copied()
                    .filter(|x| bb.anchors.contains(x))
                    .collect()
            };
            let mut combined = Arm {
                cmps: aa.cmps.clone(),
                anchors,
            };
            combined.cmps.extend(bb.cmps.clone());
            if arm_has_witness_with_policy(&combined, include_prerelease) {
                return true;
            }
        }
    }
    false
}

fn arm_has_witness_with_policy(arm: &Arm, include: bool) -> bool {
    if include {
        let (lower, upper, eq) = arm_bounds(arm);
        if let Some(eq) = eq {
            return matches_arm(&eq, arm, true);
        }
        if let (Some(l), Some(u)) = (&lower, &upper) {
            match compare_versions(&l.v, &u.v) {
                Ordering::Greater => return false,
                Ordering::Equal if !l.inclusive || !u.inclusive => return false,
                _ => {}
            }
        }
        return witness_candidates(&lower, &upper, &[]).into_iter().any(|v| matches_arm(&v, arm, true));
    }
    arm_has_witness(arm)
}