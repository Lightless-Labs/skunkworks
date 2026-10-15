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

fn numeric(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn valid_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let bytes = s.as_bytes();
    let mut pos = 0usize;
    let mut nums = [0u64; 3];

    for field in 0..3 {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos == start {
            if pos == bytes.len() || matches!(bytes[pos], b'.' | b'-' | b'+') {
                return Err(VersionError::MissingCore { at: start });
            }
            return Err(VersionError::InvalidChar { at: pos });
        }
        if pos - start > 1 && bytes[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }
        nums[field] = s[start..pos]
            .parse::<u64>()
            .map_err(|_| VersionError::Overflow { at: start })?;

        if field < 2 {
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

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] == b'-' {
        pos += 1;
        let plus = bytes[pos..]
            .iter()
            .position(|&b| b == b'+')
            .map(|i| pos + i)
            .unwrap_or(bytes.len());
        parse_identifiers(s, pos, plus, true, &mut pre)?;
        pos = plus;
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
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
    check_pre_numeric_zero: bool,
    out: &mut Vec<String>,
) -> Result<(), VersionError> {
    let bytes = s.as_bytes();
    let mut part = start;
    loop {
        let mut stop = part;
        while stop < end && bytes[stop] != b'.' {
            if !valid_ident_char(bytes[stop]) {
                return Err(VersionError::InvalidChar { at: stop });
            }
            stop += 1;
        }
        if stop == part {
            return Err(VersionError::EmptyIdentifier { at: part });
        }
        let text = &s[part..stop];
        if check_pre_numeric_zero
            && numeric(text)
            && text.len() > 1
            && text.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: part });
        }
        out.push(text.to_string());
        if stop == end {
            break;
        }
        part = stop + 1;
    }
    Ok(())
}

fn cmp_ident(a: &str, b: &str) -> Ordering {
    let an = numeric(a);
    let bn = numeric(b);
    match (an, bn) {
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
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

#[derive(Clone, Debug)]
struct Token {
    text: String,
    at: usize,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokens(s: &str, start: usize, end: usize) -> Vec<Token> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut p = start;
    while p < end {
        while p < end && is_ws(bytes[p]) {
            p += 1;
        }
        if p == end {
            break;
        }
        let q0 = p;
        while p < end && !is_ws(bytes[p]) {
            p += 1;
        }
        out.push(Token {
            text: s[q0..p].to_string(),
            at: q0,
        });
    }
    out
}

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn version_at_core(c: (u64, u64, u64), pre: Vec<String>) -> Version {
    Version {
        major: c.0,
        minor: c.1,
        patch: c.2,
        pre,
        build: Vec::new(),
    }
}

fn parse_operand(text: &str, at: usize) -> Result<(Version, usize, bool), RangeError> {
    let fields: Vec<&str> = text.split('.').collect();
    let full = text.contains(['-', '+'])
        || (fields.len() >= 3 && !fields.iter().any(|x| matches!(*x, "*" | "x" | "X")));
    if full {
        let v = parse_version(text).map_err(|error| RangeError::Version { at, error })?;
        return Ok((v, 3, true));
    }

    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at });
    }
    let mut nums = [0u64; 3];
    let mut wild = false;
    let mut numeric_count = 0;
    for (i, f) in fields.iter().enumerate() {
        if f.is_empty() {
            return Err(RangeError::Syntax { at });
        }
        if matches!(*f, "*" | "x" | "X") {
            wild = true;
        } else {
            if wild
                || !f.bytes().all(|b| b.is_ascii_digit())
                || (f.len() > 1 && f.starts_with('0'))
            {
                return Err(RangeError::Syntax { at });
            }
            nums[i] = f.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
            numeric_count += 1;
        }
    }
    Ok((
        version_at_core((nums[0], nums[1], nums[2]), Vec::new()),
        numeric_count,
        false,
    ))
}

fn bump(
    c: (u64, u64, u64),
    field: usize,
    at: usize,
) -> Result<(u64, u64, u64), RangeError> {
    let mut x = [c.0, c.1, c.2];
    x[field] = x[field]
        .checked_add(1)
        .ok_or(RangeError::Syntax { at })?;
    for i in field + 1..3 {
        x[i] = 0;
    }
    Ok((x[0], x[1], x[2]))
}

fn add_cmp(arm: &mut Arm, op: Op, v: Version, anchor: bool) {
    if anchor && !v.pre.is_empty() {
        arm.anchors.push(core(&v));
    }
    arm.cmps.push(Cmp { op, version: v });
}

fn parse_arm(ts: &[Token]) -> Result<Arm, RangeError> {
    if ts.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    if let Some(minus) = ts.iter().find(|t| t.text == "-") {
        if ts.len() != 3 || ts[1].text != "-" {
            return Err(RangeError::Syntax { at: minus.at });
        }
        let (lo, lk, _) = parse_operand(&ts[0].text, ts[0].at)?;
        let (hi, hk, hf) = parse_operand(&ts[2].text, ts[2].at)?;
        if lk == 0 {
            return Err(RangeError::Syntax { at: ts[0].at });
        }
        if hk == 0 {
            return Err(RangeError::Syntax { at: ts[2].at });
        }
        let mut arm = Arm {
            cmps: vec![],
            anchors: vec![],
        };
        add_cmp(&mut arm, Op::Ge, lo, true);
        if hf {
            add_cmp(&mut arm, Op::Le, hi, true);
        } else if hk == 1 {
            let upper = bump(core(&hi), 0, ts[2].at)?;
            add_cmp(
                &mut arm,
                Op::Lt,
                version_at_core(upper, vec!["0".into()]),
                false,
            );
        } else {
            let upper = bump(core(&hi), 1, ts[2].at)?;
            add_cmp(
                &mut arm,
                Op::Lt,
                version_at_core(upper, vec!["0".into()]),
                false,
            );
        }
        return Ok(arm);
    }

    let mut arm = Arm {
        cmps: vec![],
        anchors: vec![],
    };
    for t in ts {
        let s = t.text.as_str();
        if s.is_empty() {
            return Err(RangeError::Syntax { at: t.at });
        }
        let first = s.as_bytes()[0];
        if matches!(first, b'<' | b'>' | b'=') {
            let (op, n) = if s.starts_with("<=") {
                (Op::Le, 2)
            } else if s.starts_with(">=") {
                (Op::Ge, 2)
            } else if s.starts_with('<') {
                (Op::Lt, 1)
            } else if s.starts_with('>') {
                (Op::Gt, 1)
            } else {
                (Op::Eq, 1)
            };
            let operand = &s[n..];
            if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
                return Err(RangeError::Syntax { at: t.at });
            }
            let v = parse_version(operand)
                .map_err(|error| RangeError::Version { at: t.at + n, error })?;
            add_cmp(&mut arm, op, v, true);
            continue;
        }

        let (kind, off) = if first == b'^' {
            ('^', 1)
        } else if first == b'~' {
            ('~', 1)
        } else {
            (' ', 0)
        };
        let operand = &s[off..];
        if operand.is_empty() {
            return Err(RangeError::Syntax { at: t.at });
        }
        if kind == ' ' && !first.is_ascii_digit() && !matches!(first, b'*' | b'x' | b'X') {
            return Err(RangeError::Syntax { at: t.at });
        }
        let (v, k, full) = parse_operand(operand, t.at + off)?;
        if kind == '^' || kind == '~' {
            if k == 0 {
                return Err(RangeError::Syntax { at: t.at });
            }
            add_cmp(&mut arm, Op::Ge, v.clone(), true);
            let c = core(&v);
            let field = if kind == '^' {
                if c.0 > 0 || k == 1 {
                    0
                } else if c.1 > 0 || k == 2 {
                    1
                } else {
                    2
                }
            } else if k == 1 {
                0
            } else {
                1
            };
            let upper = bump(c, field, t.at + off)?;
            add_cmp(
                &mut arm,
                Op::Lt,
                version_at_core(upper, vec!["0".into()]),
                false,
            );
        } else if full {
            add_cmp(&mut arm, Op::Eq, v, true);
        } else if k == 0 {
            // An all-wildcard operand adds no comparison.
        } else if k == 1 {
            add_cmp(&mut arm, Op::Ge, v.clone(), true);
            add_cmp(
                &mut arm,
                Op::Lt,
                version_at_core(bump(core(&v), 0, t.at)?, vec!["0".into()]),
                false,
            );
        } else if k == 2 {
            add_cmp(&mut arm, Op::Ge, v.clone(), true);
            add_cmp(
                &mut arm,
                Op::Lt,
                version_at_core(bump(core(&v), 1, t.at)?, vec!["0".into()]),
                false,
            );
        } else {
            add_cmp(&mut arm, Op::Eq, v, true);
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = Vec::new();
    let mut arm_start = 0usize;
    let mut i = 0usize;
    let mut saw_non_ws = false;

    while i < bytes.len() {
        if bytes[i..].starts_with(b"||") {
            let left = tokens(s, arm_start, i);
            if left.is_empty() {
                return Err(RangeError::Empty { at: i });
            }
            arms.push(parse_arm(&left)?);
            let sep = i;
            i += 2;
            if i < bytes.len() && !is_ws(bytes[i]) {
                let start = sep;
                while i < bytes.len() && !is_ws(bytes[i]) {
                    i += 1;
                }
                let t = Token {
                    text: s[start..i].to_string(),
                    at: start,
                };
                return Err(parse_arm(&[t]).unwrap_err());
            }
            while i < bytes.len() && is_ws(bytes[i]) {
                i += 1;
            }
            arm_start = i;
            saw_non_ws = true;
            if i == bytes.len() {
                return Err(RangeError::Empty { at: bytes.len() });
            }
        } else {
            if !is_ws(bytes[i]) {
                saw_non_ws = true;
            }
            i += 1;
        }
    }

    let last = tokens(s, arm_start, bytes.len());
    if last.is_empty() {
        if arms.is_empty() && !saw_non_ws {
            return Err(RangeError::Empty { at: 0 });
        }
        return Err(RangeError::Empty { at: bytes.len() });
    }
    arms.push(parse_arm(&last)?);
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

fn arm_matches(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !arm.cmps.iter().all(|c| cmp_matches(v, c)) {
        return false;
    }
    v.pre.is_empty()
        || include_prerelease
        || arm.anchors.iter().any(|&a| a == core(v))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms
        .iter()
        .any(|a| arm_matches(v, a, include_prerelease))
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
    max: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (i, s) in candidates.iter().enumerate() {
        parsed.push(parse_version(s).map_err(|error| CandidateError { index: i, error })?);
    }
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

#[derive(Clone)]
struct Bound {
    v: Version,
    inclusive: bool,
}

fn bounds(arm: &Arm) -> (Option<Bound>, Option<Bound>) {
    let mut lower: Option<Bound> = None;
    let mut upper: Option<Bound> = None;
    for c in &arm.cmps {
        match c.op {
            Op::Eq => {
                let b = Bound {
                    v: c.version.clone(),
                    inclusive: true,
                };
                lower = Some(b.clone());
                upper = Some(b);
            }
            Op::Gt | Op::Ge => {
                let candidate = Bound {
                    v: c.version.clone(),
                    inclusive: matches!(c.op, Op::Ge),
                };
                let replace = lower
                    .as_ref()
                    .map(|old| {
                        let ord = compare_versions(&candidate.v, &old.v);
                        ord == Ordering::Greater
                            || (ord == Ordering::Equal && !candidate.inclusive && old.inclusive)
                    })
                    .unwrap_or(true);
                if replace {
                    lower = Some(candidate);
                }
            }
            Op::Lt | Op::Le => {
                let candidate = Bound {
                    v: c.version.clone(),
                    inclusive: matches!(c.op, Op::Le),
                };
                let replace = upper
                    .as_ref()
                    .map(|old| {
                        let ord = compare_versions(&candidate.v, &old.v);
                        ord == Ordering::Less
                            || (ord == Ordering::Equal && !candidate.inclusive && old.inclusive)
                    })
                    .unwrap_or(true);
                if replace {
                    upper = Some(candidate);
                }
            }
        }
    }
    (lower, upper)
}

fn arms_intersect(a: &Arm, b: &Arm, include: bool) -> bool {
    if include {
        return intervals_have_witness(a, b);
    }

    let (al, au) = bounds(a);
    let (bl, bu) = bounds(b);

    // Stable versions are witnesses whenever the combined interval contains a stable point.
    if intervals_have_stable_witness(al.as_ref(), au.as_ref(), bl.as_ref(), bu.as_ref()) {
        return true;
    }

    // Otherwise, a prerelease witness must lie at a tuple admitted by both arms.
    let mut anchors = a.anchors.clone();
    anchors.extend_from_slice(&b.anchors);
    anchors.sort();
    anchors.dedup();

    anchors.into_iter().any(|tuple| {
        if !a.anchors.contains(&tuple) || !b.anchors.contains(&tuple) {
            return false;
        }
        let candidate = version_at_core(tuple, vec!["0".into()]);
        arm_matches(&candidate, a, false) && arm_matches(&candidate, b, false)
    })
}

fn interval_bounds(
    a_lower: Option<&Bound>,
    a_upper: Option<&Bound>,
    b_lower: Option<&Bound>,
    b_upper: Option<&Bound>,
) -> (Option<Bound>, Option<Bound>) {
    let mut lower = a_lower.cloned();
    if let Some(candidate) = b_lower {
        let replace = lower
            .as_ref()
            .map(|old| {
                let ord = compare_versions(&candidate.v, &old.v);
                ord == Ordering::Greater
                    || (ord == Ordering::Equal && !candidate.inclusive && old.inclusive)
            })
            .unwrap_or(true);
        if replace {
            lower = Some(candidate.clone());
        }
    }

    let mut upper = a_upper.cloned();
    if let Some(candidate) = b_upper {
        let replace = upper
            .as_ref()
            .map(|old| {
                let ord = compare_versions(&candidate.v, &old.v);
                ord == Ordering::Less
                    || (ord == Ordering::Equal && !candidate.inclusive && old.inclusive)
            })
            .unwrap_or(true);
        if replace {
            upper = Some(candidate.clone());
        }
    }
    (lower, upper)
}

fn interval_has_candidate(lower: Option<&Bound>, upper: Option<&Bound>, v: &Version) -> bool {
    if let Some(l) = lower {
        let ord = compare_versions(v, &l.v);
        if ord == Ordering::Less || (ord == Ordering::Equal && !l.inclusive) {
            return false;
        }
    }
    if let Some(u) = upper {
        let ord = compare_versions(v, &u.v);
        if ord == Ordering::Greater || (ord == Ordering::Equal && !u.inclusive) {
            return false;
        }
    }
    true
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

fn intervals_have_stable_witness(
    al: Option<&Bound>,
    au: Option<&Bound>,
    bl: Option<&Bound>,
    bu: Option<&Bound>,
) -> bool {
    let (lower, upper) = interval_bounds(al, au, bl, bu);
    if let (Some(l), Some(u)) = (&lower, &upper) {
        let ord = compare_versions(&l.v, &u.v);
        if ord == Ordering::Greater || (ord == Ordering::Equal && (!l.inclusive || !u.inclusive)) {
            return false;
        }
    }

    let candidate = match &lower {
        None => version_at_core((0, 0, 0), Vec::new()),
        Some(l) if l.v.pre.is_empty() => {
            if l.inclusive {
                l.v.clone()
            } else if let Some(c) = next_core(core(&l.v)) {
                version_at_core(c, Vec::new())
            } else {
                return false;
            }
        }
        Some(l) => {
            let stable = version_at_core(core(&l.v), Vec::new());
            if compare_versions(&stable, &l.v) == Ordering::Greater
                && (l.inclusive || compare_versions(&stable, &l.v) == Ordering::Greater)
            {
                stable
            } else if let Some(c) = next_core(core(&l.v)) {
                version_at_core(c, Vec::new())
            } else {
                return false;
            }
        }
    };

    interval_has_candidate(lower.as_ref(), upper.as_ref(), &candidate)
}

fn intervals_have_witness(a: &Arm, b: &Arm) -> bool {
    let (al, au) = bounds(a);
    let (bl, bu) = bounds(b);
    let (lower, upper) = interval_bounds(al.as_ref(), au.as_ref(), bl.as_ref(), bu.as_ref());
    if let (Some(l), Some(u)) = (&lower, &upper) {
        let ord = compare_versions(&l.v, &u.v);
        if ord == Ordering::Greater || (ord == Ordering::Equal && (!l.inclusive || !u.inclusive)) {
            return false;
        }
    }

    let candidate = match &lower {
        None => version_at_core((0, 0, 0), Vec::new()),
        Some(l) if l.v.pre.is_empty() => {
            if l.inclusive {
                l.v.clone()
            } else if let Some(c) = next_core(core(&l.v)) {
                version_at_core(c, Vec::new())
            } else {
                return false;
            }
        }
        Some(l) => {
            let stable = version_at_core(core(&l.v), Vec::new());
            if compare_versions(&stable, &l.v) == Ordering::Greater {
                stable
            } else {
                l.v.clone()
            }
        }
    };
    interval_has_candidate(lower.as_ref(), upper.as_ref(), &candidate)
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    a.arms
        .iter()
        .any(|aa| b.arms.iter().any(|bb| arms_intersect(aa, bb, include_prerelease)))
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
    let (lower, upper) = bounds(arm);
    if !intervals_have_witness(
        arm,
        &Arm {
            cmps: Vec::new(),
            anchors: Vec::new(),
        },
    ) {
        return None;
    }
    if !arm.anchors.is_empty()
        && !intervals_have_stable_witness(
            lower.as_ref(),
            upper.as_ref(),
            None,
            None,
        )
        && !arm.anchors.iter().any(|tuple| {
            let candidate = version_at_core(*tuple, vec!["0".into()]);
            arm_matches(&candidate, arm, false)
        })
    {
        return None;
    }

    let mut anchors = arm.anchors.clone();
    anchors.sort();
    anchors.dedup();

    let low = lower
        .map(|b| {
            format!(
                "{}{}",
                if b.inclusive { ">=" } else { ">" },
                version_string(&b.v)
            )
        })
        .unwrap_or_else(|| "*".into());
    let high = upper
        .map(|b| {
            format!(
                "{}{}",
                if b.inclusive { "<=" } else { "<" },
                version_string(&b.v)
            )
        })
        .unwrap_or_else(|| "*".into());
    let anc = if anchors.is_empty() {
        "-".to_string()
    } else {
        anchors
            .iter()
            .map(|c| format!("{}.{}.{}", c.0, c.1, c.2))
            .collect::<Vec<_>>()
            .join(",")
    };
    Some(format!("[{};{};pre={}]", low, high, anc))
}

pub fn canonical(r: &Range) -> String {
    let mut strings: Vec<String> = r.arms.iter().filter_map(canonical_arm).collect();
    strings.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    strings.dedup();
    if strings.is_empty() {
        "EMPTY".to_string()
    } else {
        strings.join(" || ")
    }
}

fn main() {}