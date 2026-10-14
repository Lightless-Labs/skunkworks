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
    if *pos == start {
        return Err(match bytes.get(start).copied() {
            None | Some(b'.' | b'-' | b'+') => VersionError::MissingCore { at: start },
            Some(_) => VersionError::InvalidChar { at: start },
        });
    }
    let digits = &s[start..*pos];
    if digits.len() > 1 && digits.starts_with('0') {
        return Err(VersionError::LeadingZero { at: start });
    }
    digits
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
    let mut p = start;
    loop {
        let id_start = p;
        while p < end && bytes[p] != b'.' {
            if !valid_identifier_byte(bytes[p]) {
                return Err(VersionError::InvalidChar { at: p });
            }
            p += 1;
        }
        if p == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        let id = &s[id_start..p];
        if prerelease
            && id.len() > 1
            && id.starts_with('0')
            && id.bytes().all(|b| b.is_ascii_digit())
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
    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let mut p = 0;
    let major = parse_core_number(s, &mut p)?;
    if p >= s.len() || s.as_bytes()[p] != b'.' {
        return Err(match s.as_bytes().get(p).copied() {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: p },
            Some(_) => VersionError::InvalidChar { at: p },
        });
    }
    p += 1;
    let minor = parse_core_number(s, &mut p)?;
    if p >= s.len() || s.as_bytes()[p] != b'.' {
        return Err(match s.as_bytes().get(p).copied() {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: p },
            Some(_) => VersionError::InvalidChar { at: p },
        });
    }
    p += 1;
    let patch = parse_core_number(s, &mut p)?;

    if p < s.len() && !matches!(s.as_bytes()[p], b'-' | b'+') {
        return Err(VersionError::InvalidChar { at: p });
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();
    if p < s.len() && s.as_bytes()[p] == b'-' {
        let pre_start = p + 1;
        let build_marker = s.as_bytes()[pre_start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|i| pre_start + i);
        let pre_end = build_marker.unwrap_or(s.len());
        pre = parse_identifiers(s, pre_start, pre_end, true)?;
        p = pre_end;
    }
    if p < s.len() && s.as_bytes()[p] == b'+' {
        let build_start = p + 1;
        build = parse_identifiers(s, build_start, s.len(), false)?;
        p = s.len();
    }
    if p != s.len() {
        return Err(VersionError::InvalidChar { at: p });
    }

    Ok(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn cmp_core(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch))
}

fn is_numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn cmp_pre_identifier(a: &str, b: &str) -> Ordering {
    let an = is_numeric_identifier(a);
    let bn = is_numeric_identifier(b);
    match (an, bn) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    cmp_core(a, b).then_with(|| match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (x, y) in a.pre.iter().zip(&b.pre) {
                let c = cmp_pre_identifier(x, y);
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
enum Op {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
}

#[derive(Clone, Debug)]
struct Bound {
    version: Version,
    op: Op,
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

fn core_tuple(v: &Version) -> (u64, u64, u64) {
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

fn zero_pre(mut v: Version) -> Version {
    v.pre = vec!["0".to_string()];
    v.build.clear();
    v
}

fn strip_build(mut v: Version) -> Version {
    v.build.clear();
    v
}

fn ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokens_with_offsets(s: &str) -> Vec<(usize, &str)> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut p = 0;
    while p < bytes.len() {
        while p < bytes.len() && ws(bytes[p]) {
            p += 1;
        }
        if p == bytes.len() {
            break;
        }
        let start = p;
        while p < bytes.len() && !ws(bytes[p]) {
            p += 1;
        }
        out.push((start, &s[start..p]));
    }
    out
}

fn parse_full_operand(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

fn parse_partial(s: &str) -> Result<Vec<Option<u64>>, ()> {
    if s.is_empty() {
        return Err(());
    }
    let fields: Vec<&str> = s.split('.').collect();
    if fields.len() > 3 || fields.iter().any(|x| x.is_empty()) {
        return Err(());
    }
    let mut out = Vec::new();
    let mut wild = false;
    for f in fields {
        if f == "*" || f == "x" || f == "X" {
            wild = true;
            out.push(None);
        } else {
            if wild
                || !f.bytes().all(|b| b.is_ascii_digit())
                || (f.len() > 1 && f.starts_with('0'))
            {
                return Err(());
            }
            let n = f.parse::<u64>().map_err(|_| ())?;
            out.push(Some(n));
        }
    }
    Ok(out)
}

fn looks_full_operand(s: &str) -> bool {
    s.contains('-')
        || s.contains('+')
        || (s.split('.').count() >= 3
            && !s.split('.').any(|f| matches!(f, "*" | "x" | "X")))
}

fn partial_key(fields: &[Option<u64>]) -> (u64, u64, u64, usize) {
    let k = fields.iter().take_while(|x| x.is_some()).count();
    let n = |i: usize| fields.get(i).and_then(|x| *x).unwrap_or(0);
    (n(0), n(1), n(2), k)
}

fn bump_upper(
    fields: &[Option<u64>],
    mode: u8,
    at: usize,
) -> Result<Version, RangeError> {
    let (mut major, mut minor, mut patch, k) = partial_key(fields);
    let result = if mode == 1 {
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
        endpoint(major, minor, patch)
    } else if mode == 2 {
        if k == 1 {
            major = major.checked_add(1).ok_or(RangeError::Syntax { at })?;
            minor = 0;
            patch = 0;
        } else {
            minor = minor.checked_add(1).ok_or(RangeError::Syntax { at })?;
            patch = 0;
        }
        endpoint(major, minor, patch)
    } else if k == 1 {
        major = major.checked_add(1).ok_or(RangeError::Syntax { at })?;
        endpoint(major, 0, 0)
    } else {
        minor = minor.checked_add(1).ok_or(RangeError::Syntax { at })?;
        endpoint(major, minor, 0)
    };
    Ok(zero_pre(result))
}

fn add_anchor(arm: &mut Arm, v: &Version) {
    if !v.pre.is_empty() {
        arm.anchors.push(core_tuple(v));
    }
}

fn add_partial_operand(
    arm: &mut Arm,
    text: &str,
    at: usize,
    kind: u8,
) -> Result<(), RangeError> {
    if looks_full_operand(text) {
        let v = parse_full_operand(text, at)?;
        if kind == 0 {
            arm.bounds.push(Bound {
                version: v.clone(),
                op: Op::Eq,
            });
        } else {
            add_anchor(arm, &v);
            arm.bounds.push(Bound {
                version: strip_build(v.clone()),
                op: Op::Ge,
            });
            let fields = vec![Some(v.major), Some(v.minor), Some(v.patch)];
            let upper = bump_upper(&fields, kind, at)?;
            arm.bounds.push(Bound {
                version: upper,
                op: Op::Lt,
            });
        }
        add_anchor(arm, &v);
        return Ok(());
    }

    let fields = parse_partial(text).map_err(|_| RangeError::Syntax { at })?;
    let (major, minor, patch, k) = partial_key(&fields);
    if kind != 0 && k == 0 {
        return Err(RangeError::Syntax { at });
    }
    if k == 0 {
        return Ok(());
    }
    let lower = endpoint(major, minor, patch);
    match kind {
        0 if k == 1 || k == 2 => {
            arm.bounds.push(Bound {
                version: lower,
                op: Op::Ge,
            });
            arm.bounds.push(Bound {
                version: bump_upper(&fields, 3, at)?,
                op: Op::Lt,
            });
        }
        0 => {
            arm.bounds.push(Bound {
                version: lower,
                op: Op::Eq,
            });
        }
        1 | 2 => {
            arm.bounds.push(Bound {
                version: lower,
                op: Op::Ge,
            });
            arm.bounds.push(Bound {
                version: bump_upper(&fields, kind, at)?,
                op: Op::Lt,
            });
        }
        _ => {}
    }
    Ok(())
}

fn add_comparator(arm: &mut Arm, text: &str, at: usize) -> Result<(), RangeError> {
    let (op, n) = if text.starts_with(">=") {
        (Op::Ge, 2)
    } else if text.starts_with("<=") {
        (Op::Le, 2)
    } else if text.starts_with('>') {
        (Op::Gt, 1)
    } else if text.starts_with('<') {
        (Op::Lt, 1)
    } else if text.starts_with('=') {
        (Op::Eq, 1)
    } else {
        return Err(RangeError::Syntax { at });
    };
    let operand = &text[n..];
    let operand_at = at + n;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at });
    }
    let v = parse_full_operand(operand, operand_at)?;
    add_anchor(arm, &v);
    arm.bounds.push(Bound {
        version: v,
        op,
    });
    Ok(())
}

fn parse_arm_tokens(tokens: &[(usize, &str)]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        bounds: Vec::new(),
        anchors: Vec::new(),
    };

    if let Some((idx, _)) = tokens.iter().find(|(_, t)| *t == "-") {
        if tokens.len() != 3 || tokens[1].1 != "-" {
            return Err(RangeError::Syntax { at: *idx });
        }
        let (la, lower_text) = tokens[0];
        let (ua, upper_text) = tokens[2];
        let lower_fields = if looks_full_operand(lower_text) {
            let v = parse_full_operand(lower_text, la)?;
            add_anchor(&mut arm, &v);
            vec![Some(v.major), Some(v.minor), Some(v.patch)]
        } else {
            parse_partial(lower_text).map_err(|_| RangeError::Syntax { at: la })?
        };
        let upper_fields = if looks_full_operand(upper_text) {
            let v = parse_full_operand(upper_text, ua)?;
            add_anchor(&mut arm, &v);
            vec![Some(v.major), Some(v.minor), Some(v.patch)]
        } else {
            parse_partial(upper_text).map_err(|_| RangeError::Syntax { at: ua })?
        };
        let (lm, ln, lp, lk) = partial_key(&lower_fields);
        let (um, un, up, uk) = partial_key(&upper_fields);
        if lk == 0 || uk == 0 {
            return Err(RangeError::Syntax {
                at: if lk == 0 { la } else { ua },
            });
        }
        arm.bounds.push(Bound {
            version: endpoint(lm, ln, lp),
            op: Op::Ge,
        });
        let upper = if looks_full_operand(upper_text) {
            parse_full_operand(upper_text, ua)?
        } else if uk == 1 || uk == 2 {
            bump_upper(&upper_fields, 3, ua)?
        } else {
            endpoint(um, un, up)
        };
        arm.bounds.push(Bound {
            version: upper,
            op: if uk == 3 && looks_full_operand(upper_text) {
                Op::Le
            } else if uk == 3 {
                Op::Le
            } else {
                Op::Lt
            },
        });
        return Ok(arm);
    }

    for &(at, token) in tokens {
        if token.starts_with('^') || token.starts_with('~') {
            let kind = if token.starts_with('^') { 1 } else { 2 };
            let operand = &token[1..];
            add_partial_operand(&mut arm, operand, at + 1, kind)?;
        } else if token.starts_with('<') || token.starts_with('>') || token.starts_with('=') {
            add_comparator(&mut arm, token, at)?;
        } else {
            let first = token.as_bytes().first().copied();
            if !matches!(first, Some(b'0'..=b'9' | b'*' | b'x' | b'X')) {
                return Err(RangeError::Syntax { at });
            }
            add_partial_operand(&mut arm, token, at, 0)?;
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let tokens = tokens_with_offsets(s);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i <= tokens.len() {
        if i == tokens.len() || tokens[i].1 == "||" {
            if start == i {
                let at = if i == tokens.len() {
                    s.len()
                } else {
                    tokens[i].0
                };
                return Err(RangeError::Empty { at });
            }
            arms.push(parse_arm_tokens(&tokens[start..i])?);
            start = i + 1;
        }
        i += 1;
    }
    Ok(Range { arms })
}

fn bound_accepts(v: &Version, b: &Bound) -> bool {
    let c = compare_versions(v, &b.version);
    match b.op {
        Op::Lt => c == Ordering::Less,
        Op::Le => c != Ordering::Greater,
        Op::Eq => c == Ordering::Equal,
        Op::Ge => c != Ordering::Less,
        Op::Gt => c == Ordering::Greater,
    }
}

fn arm_admits(arm: &Arm, v: &Version) -> bool {
    arm.anchors.iter().any(|&c| c == core_tuple(v))
}

fn arm_matches(arm: &Arm, v: &Version, include_prerelease: bool) -> bool {
    if arm.bounds.iter().any(|b| !bound_accepts(v, b)) {
        return false;
    }
    v.pre.is_empty() || include_prerelease || arm_admits(arm, v)
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|arm| arm_matches(arm, v, include_prerelease))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn extrema(candidates: &[String], r: &Range, maximum: bool) -> Result<Option<String>, CandidateError> {
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
            let c = compare_versions(&parsed[i], &parsed[j]);
            if (maximum && c == Ordering::Greater) || (!maximum && c == Ordering::Less) {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

fn increment_core(v: &Version) -> Option<Version> {
    if v.patch < u64::MAX {
        Some(endpoint(v.major, v.minor, v.patch + 1))
    } else if v.minor < u64::MAX {
        Some(endpoint(v.major, v.minor + 1, 0))
    } else if v.major < u64::MAX {
        Some(endpoint(v.major + 1, 0, 0))
    } else {
        None
    }
}

fn default_arm_has_witness(arm: &Arm) -> bool {
    if arm.bounds.iter().any(|b| {
        arm.bounds.iter().any(|c| {
            if matches!((&b.op, &c.op), (Op::Eq, _) | (_, Op::Eq)) {
                let eq = if matches!(b.op, Op::Eq) { b } else { c };
                arm.bounds.iter().all(|x| bound_accepts(&eq.version, x))
                    .then_some(false)
                    .is_none()
            } else {
                false
            }
        })
    }) {
        // The exact test below handles equality; this branch is intentionally a no-op.
    }

    if let Some(eq) = arm.bounds.iter().find(|b| matches!(b.op, Op::Eq)) {
        return arm.bounds.iter().all(|b| bound_accepts(&eq.version, b))
            && (eq.version.pre.is_empty() || arm_admits(arm, &eq.version));
    }

    let mut lower: Option<&Bound> = None;
    let mut upper: Option<&Bound> = None;
    for b in &arm.bounds {
        match b.op {
            Op::Gt | Op::Ge => {
                let replace = lower.map_or(true, |old| {
                    let c = compare_versions(&b.version, &old.version);
                    c == Ordering::Greater
                        || (c == Ordering::Equal && matches!(b.op, Op::Gt))
                });
                if replace {
                    lower = Some(b);
                }
            }
            Op::Lt | Op::Le => {
                let replace = upper.map_or(true, |old| {
                    let c = compare_versions(&b.version, &old.version);
                    c == Ordering::Less
                        || (c == Ordering::Equal && matches!(b.op, Op::Lt))
                });
                if replace {
                    upper = Some(b);
                }
            }
            Op::Eq => {}
        }
    }

    // Stable releases are the first witness class to consider.
    let mut probe = endpoint(0, 0, 0);
    if let Some(l) = lower {
        probe = l.version.clone();
        if matches!(l.op, Op::Gt) {
            if probe.pre.is_empty() {
                if let Some(next) = increment_core(&probe) {
                    probe = next;
                } else {
                    return false;
                }
            } else {
                probe.pre.push("0".to_string());
            }
        } else {
            probe.pre.clear();
            probe.build.clear();
        }
    }
    if let Some(u) = upper {
        if compare_versions(&probe, &u.version) == Ordering::Greater
            || (compare_versions(&probe, &u.version) == Ordering::Equal
                && matches!(u.op, Op::Lt))
        {
            return false;
        }
    }
    if probe.pre.is_empty() && arm.bounds.iter().all(|b| bound_accepts(&probe, b)) {
        return true;
    }

    // A prerelease can witness the interval only at an explicitly admitted tuple.
    for &tuple in &arm.anchors {
        let mut p = Version {
            major: tuple.0,
            minor: tuple.1,
            patch: tuple.2,
            pre: vec!["0".to_string()],
            build: Vec::new(),
        };
        if arm.bounds.iter().all(|b| bound_accepts(&p, b)) {
            return true;
        }
        p.pre.clear();
        if arm.bounds.iter().all(|b| bound_accepts(&p, b)) {
            return true;
        }
    }
    false
}

fn canonical_core(c: (u64, u64, u64)) -> String {
    format!("{}.{}.{}", c.0, c.1, c.2)
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
    let mut output = Vec::new();
    for arm in &r.arms {
        if !default_arm_has_witness(arm) {
            continue;
        }
        let mut equality: Option<&Version> = None;
        let mut lower: Option<&Bound> = None;
        let mut upper: Option<&Bound> = None;
        for b in &arm.bounds {
            match b.op {
                Op::Eq => equality = Some(&b.version),
                Op::Gt | Op::Ge => {
                    let replace = lower.map_or(true, |old| {
                        let c = compare_versions(&b.version, &old.version);
                        c == Ordering::Greater
                            || (c == Ordering::Equal && matches!(b.op, Op::Gt))
                    });
                    if replace {
                        lower = Some(b);
                    }
                }
                Op::Lt | Op::Le => {
                    let replace = upper.map_or(true, |old| {
                        let c = compare_versions(&b.version, &old.version);
                        c == Ordering::Less
                            || (c == Ordering::Equal && matches!(b.op, Op::Lt))
                    });
                    if replace {
                        upper = Some(b);
                    }
                }
            }
        }
        let (lo, hi) = if let Some(eq) = equality {
            (Some((eq, false)), Some((eq, false)))
        } else {
            (
                lower.map(|b| (&b.version, matches!(b.op, Op::Gt))),
                upper.map(|b| (&b.version, matches!(b.op, Op::Lt))),
            )
        };
        let lower_text = lo.map_or("*".to_string(), |(v, strict)| {
            format!("{}{}", if strict { ">" } else { ">=" }, version_string(v))
        });
        let upper_text = hi.map_or("*".to_string(), |(v, strict)| {
            format!("{}{}", if strict { "<" } else { "<=" }, version_string(v))
        });
        let mut anchors = arm.anchors.clone();
        anchors.sort();
        anchors.dedup();
        let anchor_text = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors
                .iter()
                .map(|&c| canonical_core(c))
                .collect::<Vec<_>>()
                .join(",")
        };
        output.push(format!(
            "[{};{};pre={}]",
            lower_text, upper_text, anchor_text
        ));
    }
    output.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    output.dedup();
    if output.is_empty() {
        "EMPTY".to_string()
    } else {
        output.join(" || ")
    }
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for aa in &a.arms {
        for bb in &b.arms {
            let mut combined = Arm {
                bounds: aa.bounds.iter().chain(&bb.bounds).cloned().collect(),
                anchors: aa
                    .anchors
                    .iter()
                    .chain(&bb.anchors)
                    .copied()
                    .collect(),
            };
            combined.anchors.sort();
            combined.anchors.dedup();
            if include_prerelease {
                if default_arm_has_witness(&combined) || interval_may_have_stable(&combined) {
                    return true;
                }
            } else if default_arm_has_witness(&combined) {
                return true;
            }
        }
    }
    false
}

fn interval_may_have_stable(arm: &Arm) -> bool {
    let stable_only = Arm {
        bounds: arm.bounds.clone(),
        anchors: Vec::new(),
    };
    default_arm_has_witness(&stable_only)
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

fn validate_expr(
    expr: &SetExpr,
    package: &str,
    label: &str,
    path: &mut Vec<usize>,
    parsed: &mut Vec<Range>,
) -> Result<(), SelectError> {
    match expr {
        SetExpr::Range(s) => {
            let range = parse_range(s).map_err(|error| SelectError::InvalidRange {
                package: package.to_string(),
                label: label.to_string(),
                path: path.clone(),
                error,
            })?;
            parsed.push(range);
        }
        SetExpr::Union(children) | SetExpr::Intersection(children) => {
            for (i, child) in children.iter().enumerate() {
                path.push(i);
                validate_expr(child, package, label, path, parsed)?;
                path.pop();
            }
        }
    }
    Ok(())
}

fn expr_matches(
    expr: &SetExpr,
    ranges: &mut std::slice::Iter<'_, Range>,
    v: &Version,
    include_prerelease: bool,
) -> bool {
    match expr {
        SetExpr::Range(_) => ranges
            .next()
            .map_or(false, |r| matches_with(v, r, include_prerelease)),
        SetExpr::Union(children) => children
            .iter()
            .any(|e| expr_matches(e, ranges, v, include_prerelease)),
        SetExpr::Intersection(children) => children
            .iter()
            .all(|e| expr_matches(e, ranges, v, include_prerelease)),
    }
}

fn constraint_accepts(
    constraint: &Constraint,
    ranges: &[Range],
    v: &Version,
    include_prerelease: bool,
) -> bool {
    let mut iter = ranges.iter();
    expr_matches(&constraint.expr, &mut iter, v, include_prerelease)
}

fn conflict_labels(
    package: &Package,
    parsed_versions: &[Version],
    parsed_ranges: &[Vec<Range>],
    include_prerelease: bool,
) -> Vec<String> {
    if package.candidates.is_empty() {
        return Vec::new();
    }
    let n = package.constraints.len();
    for size in 1..=n {
        let mut chosen = Vec::new();
        if let Some(indices) = find_conflict(
            package,
            parsed_versions,
            parsed_ranges,
            include_prerelease,
            size,
            0,
            &mut chosen,
        ) {
            let mut labels = indices
                .into_iter()
                .map(|i| package.constraints[i].label.clone())
                .collect::<Vec<_>>();
            labels.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            return labels;
        }
    }
    Vec::new()
}

fn find_conflict(
    package: &Package,
    versions: &[Version],
    ranges: &[Vec<Range>],
    include_prerelease: bool,
    remaining: usize,
    start: usize,
    chosen: &mut Vec<usize>,
) -> Option<Vec<usize>> {
    if remaining == 0 {
        let any = versions.iter().any(|v| {
            chosen.iter().all(|&i| {
                constraint_accepts(
                    &package.constraints[i],
                    &ranges[i],
                    v,
                    include_prerelease,
                )
            })
        });
        return if any { None } else { Some(chosen.clone()) };
    }
    if package.constraints.len().saturating_sub(start) < remaining {
        return None;
    }
    for i in start..=package.constraints.len() - remaining {
        chosen.push(i);
        if let Some(found) = find_conflict(
            package,
            versions,
            ranges,
            include_prerelease,
            remaining - 1,
            i + 1,
            chosen,
        ) {
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

    let mut parsed_versions = Vec::with_capacity(packages.len());
    for p in packages {
        let mut versions = Vec::with_capacity(p.candidates.len());
        for (index, candidate) in p.candidates.iter().enumerate() {
            versions.push(parse_version(candidate).map_err(|error| {
                SelectError::InvalidCandidate {
                    package: p.name.clone(),
                    index,
                    error,
                }
            })?);
        }
        parsed_versions.push(versions);
    }

    let mut parsed_ranges: Vec<Vec<Vec<Range>>> = Vec::with_capacity(packages.len());
    for p in packages {
        let mut per_constraint = Vec::with_capacity(p.constraints.len());
        for c in &p.constraints {
            let mut ranges = Vec::new();
            validate_expr(
                &c.expr,
                &p.name,
                &c.label,
                &mut Vec::new(),
                &mut ranges,
            )?;
            per_constraint.push(ranges);
        }
        parsed_ranges.push(per_constraint);
    }

    let mut result = Vec::with_capacity(packages.len());
    for (pi, package) in packages.iter().enumerate() {
        let versions = &parsed_versions[pi];
        let mut qualifying: Vec<usize> = Vec::new();
        for (i, version) in versions.iter().enumerate() {
            let accepted = package.constraints.iter().enumerate().all(|(ci, c)| {
                constraint_accepts(
                    c,
                    &parsed_ranges[pi][ci],
                    version,
                    include_prerelease,
                )
            });
            if accepted {
                qualifying.push(i);
            }
        }

        if qualifying.is_empty() {
            return Err(SelectError::NoSolution {
                package: package.name.clone(),
                labels: conflict_labels(
                    package,
                    versions,
                    &parsed_ranges[pi],
                    include_prerelease,
                ),
            });
        }

        let stable_exists = matches!(preference, Preference::StableFirst)
            && qualifying.iter().any(|&i| versions[i].pre.is_empty());
        let mut best = qualifying[0];
        for &i in qualifying.iter().skip(1) {
            let a = &versions[i];
            let b = &versions[best];
            if stable_exists && a.pre.is_empty() != b.pre.is_empty() {
                if a.pre.is_empty() {
                    best = i;
                }
                continue;
            }
            let cmp = compare_versions(a, b);
            let better = match preference {
                Preference::Highest | Preference::StableFirst => cmp == Ordering::Greater,
                Preference::Lowest => cmp == Ordering::Less,
            };
            if better {
                best = i;
            }
        }
        result.push(Selection {
            package: package.name.clone(),
            version: package.candidates[best].clone(),
        });
    }
    Ok(result)
}