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

fn is_core_stop(b: Option<u8>) -> bool {
    matches!(b, None | Some(b'.' | b'-' | b'+'))
}

fn parse_core_number(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if *pos == start {
        if is_core_stop(bytes.get(start).copied()) {
            return Err(VersionError::MissingCore { at: start });
        }
        return Err(VersionError::InvalidChar { at: start });
    }
    if *pos - start > 1 && bytes[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    let mut n = 0u64;
    for &b in &bytes[start..*pos] {
        n = n
            .checked_mul(10)
            .and_then(|x| x.checked_add((b - b'0') as u64))
            .ok_or(VersionError::Overflow { at: start })?;
    }
    Ok(n)
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    pre: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut p = start;
    loop {
        let id_start = p;
        while p < end && bytes[p] != b'.' {
            let b = bytes[p];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: p });
            }
            p += 1;
        }
        if id_start == p {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        if pre {
            let id = &bytes[id_start..p];
            if id.len() > 1 && id[0] == b'0' && id.iter().all(u8::is_ascii_digit) {
                return Err(VersionError::LeadingZero { at: id_start });
            }
        }
        out.push(s[id_start..p].to_owned());
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
    let bytes = s.as_bytes();
    let mut p = 0;
    let major = parse_core_number(s, &mut p)?;
    if bytes.get(p) != Some(&b'.') {
        if is_core_stop(bytes.get(p).copied()) {
            return Err(VersionError::MissingCore { at: p });
        }
        return Err(VersionError::InvalidChar { at: p });
    }
    p += 1;
    let minor = parse_core_number(s, &mut p)?;
    if bytes.get(p) != Some(&b'.') {
        if is_core_stop(bytes.get(p).copied()) {
            return Err(VersionError::MissingCore { at: p });
        }
        return Err(VersionError::InvalidChar { at: p });
    }
    p += 1;
    let patch = parse_core_number(s, &mut p)?;

    let mut pre = Vec::new();
    let mut build = Vec::new();
    match bytes.get(p).copied() {
        None => {}
        Some(b'-') => {
            let pre_start = p + 1;
            let build_pos = bytes[pre_start..]
                .iter()
                .position(|&b| b == b'+')
                .map(|x| pre_start + x)
                .unwrap_or(bytes.len());
            pre = parse_identifiers(s, pre_start, build_pos, true)?;
            if build_pos < bytes.len() {
                build = parse_identifiers(s, build_pos + 1, bytes.len(), false)?;
            }
        }
        Some(b'+') => {
            build = parse_identifiers(s, p + 1, bytes.len(), false)?;
        }
        Some(_) => return Err(VersionError::InvalidChar { at: p }),
    }
    Ok(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn all_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn cmp_numeric_ident(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    for (x, y) in a.iter().zip(b) {
        let xd = all_digits(x);
        let yd = all_digits(y);
        let ord = match (xd, yd) {
            (true, true) => cmp_numeric_ident(x, y),
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
    core(a)
        .cmp(&core(b))
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

#[derive(Clone, Copy, Debug)]
struct Token<'a> {
    text: &'a str,
    at: usize,
}

fn ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokenize(s: &str, base: usize) -> Vec<Token<'_>> {
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
        out.push(Token {
            text: &s[start..p],
            at: base + start,
        });
    }
    out
}

fn syntax(at: usize) -> RangeError {
    RangeError::Syntax { at }
}

fn parse_full_at(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

fn version_core(v: &Version) -> (u64, u64, u64) {
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

fn endpoint_zero(major: u64, minor: u64, patch: u64) -> Version {
    Version {
        major,
        minor,
        patch,
        pre: vec!["0".to_owned()],
        build: Vec::new(),
    }
}

#[derive(Clone, Debug)]
struct Partial {
    values: [u64; 3],
    numeric: usize,
    full: Option<Version>,
}

fn partial(s: &str, at: usize) -> Result<Partial, RangeError> {
    if s.is_empty() {
        return Err(syntax(at));
    }
    let fields: Vec<&str> = s.split('.').collect();
    let has_suffix = s.contains('-') || s.contains('+');
    let has_three_no_wild = fields.len() >= 3
        && !fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));
    if has_suffix || has_three_no_wild {
        let v = parse_full_at(s, at)?;
        return Ok(Partial {
            values: [v.major, v.minor, v.patch],
            numeric: 3,
            full: Some(v),
        });
    }
    if fields.len() > 3 {
        return Err(syntax(at));
    }
    let mut values = [0u64; 3];
    let mut numeric = 0;
    let mut wildcard_seen = false;
    for (i, f) in fields.iter().enumerate() {
        if f.is_empty() {
            return Err(syntax(at));
        }
        if matches!(*f, "*" | "x" | "X") {
            wildcard_seen = true;
            continue;
        }
        if wildcard_seen
            || !f.bytes().all(|b| b.is_ascii_digit())
            || (f.len() > 1 && f.as_bytes()[0] == b'0')
        {
            return Err(syntax(at));
        }
        let mut n = 0u64;
        for b in f.bytes() {
            n = n
                .checked_mul(10)
                .and_then(|x| x.checked_add((b - b'0') as u64))
                .ok_or_else(|| syntax(at))?;
        }
        values[i] = n;
        numeric += 1;
    }
    Ok(Partial {
        values,
        numeric,
        full: None,
    })
}

fn full_bound(v: Version, op: Op, anchors: &mut Vec<(u64, u64, u64)>) -> Bound {
    if !v.pre.is_empty() {
        anchors.push(version_core(&v));
    }
    Bound { version: v, op }
}

fn increment_upper(p: &Partial, mode: &str, at: usize) -> Result<Version, RangeError> {
    let [mut a, mut b, mut c] = p.values;
    if mode == "bare" {
        match p.numeric {
            1 => {
                a = a.checked_add(1).ok_or_else(|| syntax(at))?;
                b = 0;
                c = 0;
            }
            2 => {
                b = b.checked_add(1).ok_or_else(|| syntax(at))?;
                c = 0;
            }
            _ => unreachable!(),
        }
    } else if mode == "tilde" {
        if p.numeric == 1 {
            a = a.checked_add(1).ok_or_else(|| syntax(at))?;
            b = 0;
            c = 0;
        } else {
            b = b.checked_add(1).ok_or_else(|| syntax(at))?;
            c = 0;
        }
    } else {
        let bump_major = a > 0 || p.numeric == 1;
        let bump_minor = !bump_major && (b > 0 || p.numeric == 2);
        if bump_major {
            a = a.checked_add(1).ok_or_else(|| syntax(at))?;
            b = 0;
            c = 0;
        } else if bump_minor {
            b = b.checked_add(1).ok_or_else(|| syntax(at))?;
            c = 0;
        } else {
            c = c.checked_add(1).ok_or_else(|| syntax(at))?;
        }
    }
    Ok(endpoint_zero(a, b, c))
}

fn parse_operand_token(token: Token<'_>, arm: &mut Arm) -> Result<(), RangeError> {
    let mut text = token.text;
    let mut at = token.at;
    let mut mode = "bare";
    if let Some(rest) = text.strip_prefix('^') {
        mode = "caret";
        text = rest;
        at += 1;
    } else if let Some(rest) = text.strip_prefix('~') {
        mode = "tilde";
        text = rest;
        at += 1;
    }
    let p = partial(text, at)?;
    if mode != "bare" && p.numeric == 0 {
        return Err(syntax(token.at));
    }
    if let Some(v) = p.full.as_ref() {
        if mode == "bare" {
            arm.bounds.push(full_bound(v.clone(), Op::Eq, &mut arm.anchors));
            return Ok(());
        }
        arm.bounds.push(full_bound(v.clone(), Op::Ge, &mut arm.anchors));
        let upper = increment_upper(&p, mode, at)?;
        arm.bounds.push(Bound {
            version: upper,
            op: Op::Lt,
        });
        return Ok(());
    }
    if mode == "bare" {
        match p.numeric {
            0 => {}
            1 | 2 => {
                arm.bounds.push(Bound {
                    version: endpoint(p.values[0], p.values[1], p.values[2]),
                    op: Op::Ge,
                });
                let upper = increment_upper(&p, "bare", at)?;
                arm.bounds.push(Bound {
                    version: upper,
                    op: Op::Lt,
                });
            }
            _ => {
                arm.bounds.push(Bound {
                    version: endpoint(p.values[0], p.values[1], p.values[2]),
                    op: Op::Eq,
                });
            }
        }
    } else {
        arm.bounds.push(Bound {
            version: endpoint(p.values[0], p.values[1], p.values[2]),
            op: Op::Ge,
        });
        let upper = increment_upper(&p, mode, at)?;
        arm.bounds.push(Bound {
            version: upper,
            op: Op::Lt,
        });
    }
    Ok(())
}

fn parse_comparator(token: Token<'_>, arm: &mut Arm) -> Result<bool, RangeError> {
    let (op, n) = if token.text.starts_with(">=") {
        (Op::Ge, 2)
    } else if token.text.starts_with("<=") {
        (Op::Le, 2)
    } else if token.text.starts_with('>') {
        (Op::Gt, 1)
    } else if token.text.starts_with('<') {
        (Op::Lt, 1)
    } else if token.text.starts_with('=') {
        (Op::Eq, 1)
    } else {
        return Ok(false);
    };
    let operand = &token.text[n..];
    let start = token.at + n;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(syntax(token.at));
    }
    let v = parse_full_at(operand, start)?;
    arm.bounds.push(full_bound(v, op, &mut arm.anchors));
    Ok(true)
}

fn is_wildcard_start(s: &str) -> bool {
    s.starts_with('*') || s.starts_with('x') || s.starts_with('X')
}

fn parse_arm(tokens: &[Token<'_>]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        bounds: Vec::new(),
        anchors: Vec::new(),
    };
    let hyphens: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| if t.text == "-" { Some(i) } else { None })
        .collect();
    if let Some(&i) = hyphens.first() {
        if tokens.len() != 3 || i != 1 {
            return Err(syntax(tokens[i].at));
        }
        let lo = partial(tokens[0].text, tokens[0].at)?;
        let hi = partial(tokens[2].text, tokens[2].at)?;
        if lo.numeric == 0 {
            return Err(syntax(tokens[0].at));
        }
        if hi.numeric == 0 {
            return Err(syntax(tokens[2].at));
        }
        let lower = lo
            .full
            .clone()
            .unwrap_or_else(|| endpoint(lo.values[0], lo.values[1], lo.values[2]));
        arm.bounds
            .push(full_bound(lower, Op::Ge, &mut arm.anchors));
        if let Some(v) = hi.full {
            arm.bounds.push(full_bound(v, Op::Le, &mut arm.anchors));
        } else {
            let upper = match hi.numeric {
                1 => endpoint_zero(
                    hi.values[0]
                        .checked_add(1)
                        .ok_or_else(|| syntax(tokens[2].at))?,
                    0,
                    0,
                ),
                2 => endpoint_zero(
                    hi.values[0],
                    hi.values[1]
                        .checked_add(1)
                        .ok_or_else(|| syntax(tokens[2].at))?,
                    0,
                ),
                _ => endpoint(hi.values[0], hi.values[1], hi.values[2]),
            };
            if hi.numeric == 3 {
                arm.bounds.push(Bound {
                    version: upper,
                    op: Op::Le,
                });
            } else {
                arm.bounds.push(Bound {
                    version: upper,
                    op: Op::Lt,
                });
            }
        }
        return Ok(arm);
    }
    for token in tokens {
        let b = token.text.as_bytes();
        if token.text.is_empty() {
            return Err(syntax(token.at));
        }
        if parse_comparator(*token, &mut arm)? {
            continue;
        }
        if token.text.starts_with('^')
            || token.text.starts_with('~')
            || b[0].is_ascii_digit()
            || is_wildcard_start(token.text)
        {
            parse_operand_token(*token, &mut arm)?;
        } else {
            return Err(syntax(token.at));
        }
    }
    Ok(arm)
}

fn split_or_arms(s: &str) -> Result<Vec<Vec<Token<'_>>>, RangeError> {
    let tokens = tokenize(s, 0);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    let mut arms = Vec::new();
    let mut current = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        if token.text == "||" {
            if current.is_empty() {
                return Err(RangeError::Empty { at: token.at });
            }
            arms.push(std::mem::take(&mut current));
            if i + 1 == tokens.len() {
                return Err(RangeError::Empty { at: s.len() });
            }
        } else {
            current.push(*token);
        }
    }
    if current.is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(current);
    Ok(arms)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let split = split_or_arms(s)?;
    let mut arms = Vec::new();
    for tokens in split {
        arms.push(parse_arm(&tokens)?);
    }
    Ok(Range { arms })
}

fn bound_pass(v: &Version, b: &Bound) -> bool {
    let c = compare_versions(v, &b.version);
    match b.op {
        Op::Eq => c == Ordering::Equal,
        Op::Lt => c == Ordering::Less,
        Op::Le => c != Ordering::Greater,
        Op::Gt => c == Ordering::Greater,
        Op::Ge => c != Ordering::Less,
    }
}

fn arm_matches(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !arm.bounds.iter().all(|b| bound_pass(v, b)) {
        return false;
    }
    v.pre.is_empty() || include_prerelease || arm.anchors.contains(&version_core(v))
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

fn extrema(
    candidates: &[String],
    r: &Range,
    greatest: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, s) in candidates.iter().enumerate() {
        match parse_version(s) {
            Ok(v) => parsed.push(v),
            Err(error) => return Err(CandidateError { index, error }),
        }
    }
    let mut best: Option<usize> = None;
    for i in 0..parsed.len() {
        if !matches(&parsed[i], r) {
            continue;
        }
        match best {
            None => best = Some(i),
            Some(j) => {
                let ord = compare_versions(&parsed[i], &parsed[j]);
                if (greatest && ord == Ordering::Greater)
                    || (!greatest && ord == Ordering::Less)
                {
                    best = Some(i);
                }
            }
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

#[derive(Clone, Debug)]
struct Interval {
    lower: Option<(Version, bool)>,
    upper: Option<(Version, bool)>,
}

fn interval(arm: &Arm) -> Interval {
    let mut out = Interval {
        lower: None,
        upper: None,
    };
    for b in &arm.bounds {
        match b.op {
            Op::Eq => {
                out.lower = Some((b.version.clone(), true));
                out.upper = Some((b.version.clone(), true));
            }
            Op::Gt | Op::Ge => {
                let inc = matches!(b.op, Op::Ge);
                match &out.lower {
                    None => out.lower = Some((b.version.clone(), inc)),
                    Some((old, old_inc)) => match compare_versions(&b.version, old) {
                        Ordering::Greater => out.lower = Some((b.version.clone(), inc)),
                        Ordering::Equal if !inc && *old_inc => {
                            out.lower = Some((b.version.clone(), false))
                        }
                        _ => {}
                    },
                }
            }
            Op::Lt | Op::Le => {
                let inc = matches!(b.op, Op::Le);
                match &out.upper {
                    None => out.upper = Some((b.version.clone(), inc)),
                    Some((old, old_inc)) => match compare_versions(&b.version, old) {
                        Ordering::Less => out.upper = Some((b.version.clone(), inc)),
                        Ordering::Equal if !inc && *old_inc => {
                            out.upper = Some((b.version.clone(), false))
                        }
                        _ => {}
                    },
                }
            }
        }
    }
    out
}

fn max_core() -> Version {
    endpoint(u64::MAX, u64::MAX, u64::MAX)
}

fn tuple_successor(v: &Version) -> Option<Version> {
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

fn stable_witness(i: &Interval) -> bool {
    let min = endpoint(0, 0, 0);
    let max = max_core();
    if let Some((l, inc)) = &i.lower {
        if compare_versions(l, &max) == Ordering::Greater {
            return false;
        }
        if !inc && l.pre.is_empty() {
            let Some(next) = tuple_successor(l) else {
                return false;
            };
            if let Some((u, uinc)) = &i.upper {
                let c = compare_versions(&next, u);
                return c == Ordering::Less || (c == Ordering::Equal && *uinc);
            }
        }
    }
    if let Some((u, _)) = &i.upper {
        if compare_versions(u, &min) == Ordering::Less {
            return false;
        }
    }
    match (&i.lower, &i.upper) {
        (Some((l, li)), Some((u, ui))) => {
            let c = compare_versions(l, u);
            c == Ordering::Less || (c == Ordering::Equal && *li && *ui)
        }
        _ => true,
    }
}

fn anchors_allow(arm: &Arm, tuple: (u64, u64, u64), include: bool) -> bool {
    include || arm.anchors.contains(&tuple)
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for aa in &a.arms {
        for bb in &b.arms {
            let ia = interval(aa);
            let ib = interval(bb);
            let lower = match (&ia.lower, &ib.lower) {
                (Some(x), Some(y)) => {
                    if compare_versions(&x.0, &y.0) == Ordering::Greater {
                        Some(x.clone())
                    } else if compare_versions(&x.0, &y.0) == Ordering::Less {
                        Some(y.clone())
                    } else {
                        Some((x.0.clone(), x.1 && y.1))
                    }
                }
                (Some(x), None) | (None, Some(x)) => Some(x.clone()),
                _ => None,
            };
            let upper = match (&ia.upper, &ib.upper) {
                (Some(x), Some(y)) => {
                    if compare_versions(&x.0, &y.0) == Ordering::Less {
                        Some(x.clone())
                    } else if compare_versions(&x.0, &y.0) == Ordering::Greater {
                        Some(y.clone())
                    } else {
                        Some((x.0.clone(), x.1 && y.1))
                    }
                }
                (Some(x), None) | (None, Some(x)) => Some(x.clone()),
                _ => None,
            };
            let combined = Interval { lower, upper };
            if stable_witness(&combined) {
                return true;
            }
            if include_prerelease {
                continue;
            }
            let mut candidates = aa.anchors.clone();
            candidates.extend(bb.anchors.iter().copied());
            candidates.sort_unstable();
            candidates.dedup();
            for t in candidates {
                let v = Version {
                    major: t.0,
                    minor: t.1,
                    patch: t.2,
                    pre: vec!["0".into()],
                    build: Vec::new(),
                };
                if anchors_allow(aa, t, false)
                    && anchors_allow(bb, t, false)
                    && ia.bounds_pass(&v)
                    && ib.bounds_pass(&v)
                {
                    return true;
                }
            }
        }
    }
    false
}

impl Interval {
    fn bounds_pass(&self, v: &Version) -> bool {
        if let Some((l, inc)) = &self.lower {
            let c = compare_versions(v, l);
            if c == Ordering::Less || (c == Ordering::Equal && !inc) {
                return false;
            }
        }
        if let Some((u, inc)) = &self.upper {
            let c = compare_versions(v, u);
            if c == Ordering::Greater || (c == Ordering::Equal && !inc) {
                return false;
            }
        }
        true
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
    let i = interval(arm);
    if !stable_witness(&i) {
        let admitted = arm.anchors.iter().any(|t| {
            let v = Version {
                major: t.0,
                minor: t.1,
                patch: t.2,
                pre: vec!["0".into()],
                build: Vec::new(),
            };
            i.bounds_pass(&v)
        });
        if !admitted {
            return None;
        }
    }
    let lower = i
        .lower
        .as_ref()
        .map(|(v, inc)| format!("{}{}", if *inc { ">=" } else { ">" }, version_string(v)))
        .unwrap_or_else(|| "*".to_owned());
    let upper = i
        .upper
        .as_ref()
        .map(|(v, inc)| format!("{}{}", if *inc { "<=" } else { "<" }, version_string(v)))
        .unwrap_or_else(|| "*".to_owned());
    let mut anchors = arm.anchors.clone();
    anchors.sort_unstable();
    anchors.dedup();
    let anchors = if anchors.is_empty() {
        "-".to_owned()
    } else {
        anchors
            .iter()
            .map(|(a, b, c)| format!("{a}.{b}.{c}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    Some(format!("[{lower};{upper};pre={anchors}]"))
}

pub fn canonical(r: &Range) -> String {
    let mut arms: Vec<String> = r.arms.iter().filter_map(canonical_arm).collect();
    arms.sort();
    arms.dedup();
    if arms.is_empty() {
        "EMPTY".to_owned()
    } else {
        arms.join(" || ")
    }
}

fn main() {}