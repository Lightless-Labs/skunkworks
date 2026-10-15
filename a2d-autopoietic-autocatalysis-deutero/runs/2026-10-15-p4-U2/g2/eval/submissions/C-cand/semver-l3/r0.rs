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

fn core_missing(b: Option<u8>) -> bool {
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
        while pos < end && bytes[pos] != b'.' {
            let b = bytes[pos];
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
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
        out.push(id.to_string());

        if pos == end {
            break;
        }
        pos += 1;
        if pos == end {
            return Err(VersionError::EmptyIdentifier { at: pos });
        }
    }

    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let bytes = s.as_bytes();
    let mut pos = 0;
    let mut nums = [0u64; 3];

    for field in 0..3 {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }

        if pos == start {
            return Err(if core_missing(bytes.get(pos).copied()) {
                VersionError::MissingCore { at: pos }
            } else {
                VersionError::InvalidChar { at: pos }
            });
        }

        let digits = &s[start..pos];
        if digits.len() > 1 && digits.starts_with('0') {
            return Err(VersionError::LeadingZero { at: start });
        }
        nums[field] = digits
            .parse::<u64>()
            .map_err(|_| VersionError::Overflow { at: start })?;

        if field < 2 {
            match bytes.get(pos).copied() {
                Some(b'.') => pos += 1,
                b if core_missing(b) => {
                    return Err(VersionError::MissingCore { at: pos });
                }
                _ => return Err(VersionError::InvalidChar { at: pos }),
            }
        }
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();
    match bytes.get(pos).copied() {
        None => {}
        Some(b'-') => {
            let pre_start = pos + 1;
            let plus = s[pre_start..].find('+').map(|n| pre_start + n);
            let pre_end = plus.unwrap_or(s.len());
            pre = parse_identifiers(s, pre_start, pre_end, true)?;
            if let Some(plus_pos) = plus {
                build = parse_identifiers(s, plus_pos + 1, s.len(), false)?;
            }
        }
        Some(b'+') => {
            build = parse_identifiers(s, pos + 1, s.len(), false)?;
        }
        _ => return Err(VersionError::InvalidChar { at: pos }),
    }

    Ok(Version {
        major: nums[0],
        minor: nums[1],
        patch: nums[2],
        pre,
        build,
    })
}

fn numeric_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn compare_id(a: &str, b: &str) -> Ordering {
    match (numeric_id(a), numeric_id(b)) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a
            .len()
            .cmp(&b.len())
            .then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch)
        .cmp(&(b.major, b.minor, b.patch))
        .then_with(|| match (a.pre.is_empty(), b.pre.is_empty()) {
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
pub struct Range {
    arms: Vec<Arm>,
}

#[derive(Clone, Debug)]
struct Bound {
    version: Version,
    inclusive: bool,
    lower: bool,
}

#[derive(Clone, Debug)]
struct Arm {
    bounds: Vec<Bound>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug)]
struct Partial {
    values: [u64; 3],
    count: usize,
    full: Option<Version>,
}

fn syntax(at: usize) -> RangeError {
    RangeError::Syntax { at }
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn split_tokens(s: &str, offset: usize) -> Vec<(String, usize)> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && is_space(bytes[i]) {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !is_space(bytes[i]) {
            i += 1;
        }
        out.push((s[start..i].to_string(), offset + start));
    }
    out
}

fn is_wild(s: &str) -> bool {
    matches!(s, "*" | "x" | "X")
}

fn parse_partial(s: &str, at: usize) -> Result<Partial, RangeError> {
    let fields: Vec<&str> = s.split('.').collect();
    let fullish = s.contains('-') || s.contains('+');

    if fullish || (fields.len() >= 3 && !fields.iter().any(|f| is_wild(f))) {
        let v = parse_version(s).map_err(|error| RangeError::Version { at, error })?;
        return Ok(Partial {
            values: [v.major, v.minor, v.patch],
            count: 3,
            full: Some(v),
        });
    }

    if fields.is_empty() || fields.len() > 3 {
        return Err(syntax(at));
    }

    let mut values = [0u64; 3];
    let mut count = 0;
    let mut seen_wild = false;

    for field in fields {
        if field.is_empty() {
            return Err(syntax(at));
        }
        if is_wild(field) {
            seen_wild = true;
        } else {
            if seen_wild
                || !field.bytes().all(|b| b.is_ascii_digit())
                || (field.len() > 1 && field.starts_with('0'))
            {
                return Err(syntax(at));
            }
            values[count] = field.parse::<u64>().map_err(|_| syntax(at))?;
            count += 1;
        }
    }

    Ok(Partial {
        values,
        count,
        full: None,
    })
}

fn tuple(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn make_version(t: (u64, u64, u64), pre: Vec<String>) -> Version {
    Version {
        major: t.0,
        minor: t.1,
        patch: t.2,
        pre,
        build: Vec::new(),
    }
}

fn next_tuple(t: (u64, u64, u64), count: usize) -> Option<(u64, u64, u64)> {
    match count {
        1 => Some((t.0.checked_add(1)?, 0, 0)),
        2 => Some((t.0, t.1.checked_add(1)?, 0)),
        _ => Some((t.0, t.1, t.2.checked_add(1)?)),
    }
}

fn add_bound(arm: &mut Arm, version: Version, lower: bool, inclusive: bool) {
    arm.bounds.push(Bound {
        version,
        inclusive,
        lower,
    });
}

fn add_anchor(arm: &mut Arm, v: &Version) {
    if !v.pre.is_empty() {
        arm.anchors.push(tuple(v));
    }
}

fn add_expanded(arm: &mut Arm, p: Partial, kind: u8, at: usize) -> Result<(), RangeError> {
    let t = (p.values[0], p.values[1], p.values[2]);

    if let Some(v) = p.full {
        add_anchor(arm, &v);
        match kind {
            b'=' => {
                add_bound(arm, v.clone(), true, true);
                add_bound(arm, v, false, true);
            }
            b'^' | b'~' => {
                add_bound(arm, v.clone(), true, true);
                let bump = if kind == b'^' {
                    if t.0 > 0 || p.count == 1 {
                        1
                    } else if t.1 > 0 || p.count == 2 {
                        2
                    } else {
                        3
                    }
                } else if p.count == 1 {
                    1
                } else {
                    2
                };
                let upper = next_tuple(t, bump).ok_or_else(|| syntax(at))?;
                add_bound(arm, make_version(upper, vec!["0".into()]), false, false);
            }
            b'-' => {
                add_bound(arm, v, false, true);
            }
            _ => {}
        }
        return Ok(());
    }

    if kind == b'^' || kind == b'~' {
        if p.count == 0 {
            return Err(syntax(at));
        }
        add_bound(arm, make_version(t, Vec::new()), true, true);
        let bump = if kind == b'^' {
            if t.0 > 0 || p.count == 1 {
                1
            } else if t.1 > 0 || p.count == 2 {
                2
            } else {
                3
            }
        } else if p.count == 1 {
            1
        } else {
            2
        };
        let upper = next_tuple(t, bump).ok_or_else(|| syntax(at))?;
        add_bound(arm, make_version(upper, vec!["0".into()]), false, false);
    } else if kind == b'-' {
        if p.count == 0 {
            return Err(syntax(at));
        }
        add_bound(arm, make_version(t, Vec::new()), true, true);
    } else if p.count == 1 || p.count == 2 {
        add_bound(arm, make_version(t, Vec::new()), true, true);
        let upper = next_tuple(t, p.count).ok_or_else(|| syntax(at))?;
        add_bound(arm, make_version(upper, vec!["0".into()]), false, false);
    }

    Ok(())
}

fn comparator(token: &str) -> Option<(Op, usize)> {
    if token.starts_with(">=") {
        Some((Op::Ge, 2))
    } else if token.starts_with("<=") {
        Some((Op::Le, 2))
    } else if token.starts_with('>') {
        Some((Op::Gt, 1))
    } else if token.starts_with('<') {
        Some((Op::Lt, 1))
    } else if token.starts_with('=') {
        Some((Op::Eq, 1))
    } else {
        None
    }
}

fn parse_comparator(arm: &mut Arm, token: &str, at: usize) -> Result<(), RangeError> {
    let (op, n) = comparator(token).unwrap();
    let operand = &token[n..];
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(syntax(at));
    }

    let v = parse_version(operand)
        .map_err(|error| RangeError::Version { at: at + n, error })?;
    add_anchor(arm, &v);

    match op {
        Op::Eq => {
            add_bound(arm, v.clone(), true, true);
            add_bound(arm, v, false, true);
        }
        Op::Lt => add_bound(arm, v, false, false),
        Op::Le => add_bound(arm, v, false, true),
        Op::Gt => add_bound(arm, v, true, false),
        Op::Ge => add_bound(arm, v, true, true),
    }
    Ok(())
}

fn parse_arm(tokens: &[(String, usize)]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        bounds: Vec::new(),
        anchors: Vec::new(),
    };

    let hyphens: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, (token, _))| (token == "-").then_some(i))
        .collect();

    if !hyphens.is_empty() {
        let i = hyphens[0];
        if hyphens.len() != 1 || tokens.len() != 3 || i != 1 {
            return Err(syntax(tokens[i].1));
        }

        let (lo, lo_at) = &tokens[0];
        let (hi, hi_at) = &tokens[2];
        let lp = parse_partial(lo, *lo_at)?;
        let hp = parse_partial(hi, *hi_at)?;

        if lp.count == 0 {
            return Err(syntax(*lo_at));
        }
        if hp.count == 0 {
            return Err(syntax(*hi_at));
        }

        add_expanded(&mut arm, lp, b'-', *lo_at)?;

        if let Some(v) = hp.full.clone() {
            add_anchor(&mut arm, &v);
            add_bound(&mut arm, v, false, true);
        } else {
            let t = (hp.values[0], hp.values[1], hp.values[2]);
            if hp.count == 1 || hp.count == 2 {
                let next = next_tuple(t, hp.count).ok_or_else(|| syntax(*hi_at))?;
                add_bound(
                    &mut arm,
                    make_version(next, vec!["0".into()]),
                    false,
                    false,
                );
            } else {
                add_bound(&mut arm, make_version(t, Vec::new()), false, true);
            }
        }
        return Ok(arm);
    }

    for (token, at) in tokens {
        let first = token.as_bytes()[0];
        if comparator(token).is_some() {
            parse_comparator(&mut arm, token, *at)?;
        } else if first == b'^' || first == b'~' {
            if token.len() == 1 {
                return Err(syntax(*at));
            }
            let p = parse_partial(&token[1..], *at + 1)?;
            if p.count == 0 {
                return Err(syntax(*at));
            }
            add_expanded(&mut arm, p, first, *at + 1)?;
        } else if first.is_ascii_digit() || is_wild(token) {
            let p = parse_partial(token, *at)?;
            add_expanded(&mut arm, p, b'=', *at)?;
        } else {
            return Err(syntax(*at));
        }
    }

    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = Vec::new();
    let mut arm_start = 0;
    let mut i = 0;
    let mut saw_separator = false;

    while i + 1 < bytes.len() {
        if bytes[i] == b'|' && bytes[i + 1] == b'|' {
            let left = split_tokens(&s[arm_start..i], arm_start);
            if left.is_empty() {
                return Err(RangeError::Empty { at: i });
            }
            arms.push(parse_arm(&left)?);

            let mut next = i + 2;
            while next < bytes.len() && is_space(bytes[next]) {
                next += 1;
            }
            if next == bytes.len() {
                return Err(RangeError::Empty { at: bytes.len() });
            }

            arm_start = next;
            i = next;
            saw_separator = true;
        } else {
            i += 1;
        }
    }

    let tail = split_tokens(&s[arm_start..], arm_start);
    if tail.is_empty() {
        return if saw_separator {
            Err(RangeError::Empty { at: bytes.len() })
        } else {
            Err(RangeError::Empty { at: 0 })
        };
    }

    arms.push(parse_arm(&tail)?);
    Ok(Range { arms })
}

fn arm_comparisons(v: &Version, arm: &Arm) -> bool {
    arm.bounds.iter().all(|b| {
        let c = compare_versions(v, &b.version);
        if b.lower {
            c == Ordering::Greater || (b.inclusive && c == Ordering::Equal)
        } else {
            c == Ordering::Less || (b.inclusive && c == Ordering::Equal)
        }
    })
}

fn admitted(v: &Version, arm: &Arm) -> bool {
    arm.anchors.contains(&tuple(v))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|arm| {
        arm_comparisons(v, arm)
            && (include_prerelease || v.pre.is_empty() || admitted(v, arm))
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn extrema(
    candidates: &[String],
    r: &Range,
    want_max: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        parsed.push(
            parse_version(candidate)
                .map_err(|error| CandidateError { index, error })?,
        );
    }

    let mut best: Option<usize> = None;
    for i in 0..parsed.len() {
        if !matches(&parsed[i], r) {
            continue;
        }
        if let Some(j) = best {
            let order = compare_versions(&parsed[i], &parsed[j]);
            if (want_max && order == Ordering::Greater)
                || (!want_max && order == Ordering::Less)
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

fn lower_bound(arm: &Arm) -> Option<&Bound> {
    arm.bounds
        .iter()
        .filter(|b| b.lower)
        .max_by(|a, b| {
            compare_versions(&a.version, &b.version)
                .then_with(|| a.inclusive.cmp(&b.inclusive))
        })
}

fn upper_bound(arm: &Arm) -> Option<&Bound> {
    arm.bounds
        .iter()
        .filter(|b| !b.lower)
        .min_by(|a, b| {
            compare_versions(&a.version, &b.version)
                .then_with(|| b.inclusive.cmp(&a.inclusive))
        })
}

fn boundary_ok(v: &Version, lower: Option<&Bound>, upper: Option<&Bound>) -> bool {
    if let Some(b) = lower {
        let c = compare_versions(v, &b.version);
        if c == Ordering::Less || (c == Ordering::Equal && !b.inclusive) {
            return false;
        }
    }
    if let Some(b) = upper {
        let c = compare_versions(v, &b.version);
        if c == Ordering::Greater || (c == Ordering::Equal && !b.inclusive) {
            return false;
        }
    }
    true
}

fn successor_core(t: (u64, u64, u64)) -> Option<(u64, u64, u64)> {
    if let Some(p) = t.2.checked_add(1) {
        Some((t.0, t.1, p))
    } else if let Some(m) = t.1.checked_add(1) {
        Some((t.0, m, 0))
    } else {
        Some((t.0.checked_add(1)?, 0, 0))
    }
}

fn possible_witness(a: &Arm, b: Option<&Arm>, include_pre: bool) -> bool {
    let mut bounds = a.bounds.clone();
    if let Some(other) = b {
        bounds.extend(other.bounds.clone());
    }
    let combined = Arm {
        bounds,
        anchors: Vec::new(),
    };
    let lo = lower_bound(&combined);
    let hi = upper_bound(&combined);

    if let (Some(l), Some(u)) = (lo, hi) {
        let c = compare_versions(&l.version, &u.version);
        if c == Ordering::Greater || (c == Ordering::Equal && !(l.inclusive && u.inclusive)) {
            return false;
        }
    }

    let mut candidates = vec![make_version((0, 0, 0), Vec::new())];
    for bound in &combined.bounds {
        let t = tuple(&bound.version);
        candidates.push(make_version(t, Vec::new()));
        if let Some(next) = successor_core(t) {
            candidates.push(make_version(next, Vec::new()));
        }
    }

    if !include_pre {
        for anchor in &a.anchors {
            if b.is_none_or(|other| other.anchors.contains(anchor)) {
                candidates.push(make_version(*anchor, vec!["0".into()]));
            }
        }
    }

    candidates.into_iter().any(|v| {
        boundary_ok(&v, lower_bound(&combined), upper_bound(&combined))
            && (include_pre
                || v.pre.is_empty()
                || (admitted(&v, a) && b.is_none_or(|other| admitted(&v, other))))
    })
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    a.arms.iter().any(|x| {
        b.arms
            .iter()
            .any(|y| possible_witness(x, Some(y), include_prerelease))
    })
}

fn version_string(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

fn arm_string(arm: &Arm) -> String {
    let lower = lower_bound(arm).map_or_else(
        || "*".to_string(),
        |b| {
            format!(
                "{}{}",
                if b.inclusive { ">=" } else { ">" },
                version_string(&b.version)
            )
        },
    );
    let upper = upper_bound(arm).map_or_else(
        || "*".to_string(),
        |b| {
            format!(
                "{}{}",
                if b.inclusive { "<=" } else { "<" },
                version_string(&b.version)
            )
        },
    );

    let mut anchors = arm.anchors.clone();
    anchors.sort_unstable();
    anchors.dedup();
    let anchor_text = if anchors.is_empty() {
        "-".to_string()
    } else {
        anchors
            .iter()
            .map(|(a, m, p)| format!("{a}.{m}.{p}"))
            .collect::<Vec<_>>()
            .join(",")
    };

    format!("[{lower};{upper};pre={anchor_text}]")
}

fn arm_has_witness(arm: &Arm) -> bool {
    possible_witness(arm, None, false)
}

pub fn canonical(r: &Range) -> String {
    let mut strings: Vec<String> = r
        .arms
        .iter()
        .filter(|arm| arm_has_witness(arm))
        .map(arm_string)
        .collect();
    strings.sort();
    strings.dedup();
    if strings.is_empty() {
        "EMPTY".to_string()
    } else {
        strings.join(" || ")
    }
}

fn main() {}