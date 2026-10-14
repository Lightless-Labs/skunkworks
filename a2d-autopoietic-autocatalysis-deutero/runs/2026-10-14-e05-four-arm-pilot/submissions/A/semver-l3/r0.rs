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

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

fn parse_core_number(s: &str, i: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *i;
    while *i < bytes.len() && is_digit(bytes[*i]) {
        *i += 1;
    }
    if *i == start {
        return Err(match bytes.get(start).copied() {
            None | Some(b'.' | b'-' | b'+') => VersionError::MissingCore { at: start },
            _ => VersionError::InvalidChar { at: start },
        });
    }
    let raw = &s[start..*i];
    if raw.len() > 1 && raw.starts_with('0') {
        return Err(VersionError::LeadingZero { at: start });
    }
    raw.parse::<u64>()
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
    let mut p = start;
    loop {
        let id_start = p;
        while p < end && bytes[p] != b'.' {
            if !is_ident_char(bytes[p]) {
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
            && id.bytes().all(is_digit)
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        result.push(id.to_string());
        if p == end {
            break;
        }
        p += 1;
        if p == end {
            return Err(VersionError::EmptyIdentifier { at: p });
        }
    }
    Ok(result)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let mut i = 0;
    let major = parse_core_number(s, &mut i)?;
    if i >= s.len() || s.as_bytes()[i] != b'.' {
        return Err(match s.as_bytes().get(i).copied() {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: i },
            _ => VersionError::InvalidChar { at: i },
        });
    }
    i += 1;
    let minor = parse_core_number(s, &mut i)?;
    if i >= s.len() || s.as_bytes()[i] != b'.' {
        return Err(match s.as_bytes().get(i).copied() {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: i },
            _ => VersionError::InvalidChar { at: i },
        });
    }
    i += 1;
    let patch = parse_core_number(s, &mut i)?;

    let mut pre = Vec::new();
    let mut build = Vec::new();
    if i < s.len() && s.as_bytes()[i] == b'-' {
        let pre_start = i + 1;
        let build_mark = s.as_bytes()[pre_start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|p| pre_start + p)
            .unwrap_or(s.len());
        pre = parse_identifiers(s, pre_start, build_mark, true)?;
        i = build_mark;
    }
    if i < s.len() && s.as_bytes()[i] == b'+' {
        build = parse_identifiers(s, i + 1, s.len(), false)?;
        i = s.len();
    }
    if i < s.len() {
        return Err(VersionError::InvalidChar { at: i });
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

fn numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn compare_numeric_strings(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

fn compare_pre_identifier(a: &str, b: &str) -> Ordering {
    match (numeric_identifier(a), numeric_identifier(b)) {
        (true, true) => compare_numeric_strings(a, b),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    core(a).cmp(&core(b)).then_with(|| {
        match (a.pre.is_empty(), b.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => {
                for (x, y) in a.pre.iter().zip(&b.pre) {
                    let c = compare_pre_identifier(x, y);
                    if c != Ordering::Equal {
                        return c;
                    }
                }
                a.pre.len().cmp(&b.pre.len())
            }
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

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

fn vcore(major: u64, minor: u64, patch: u64) -> Version {
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

fn version_string(v: &Version, with_build: bool) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    if with_build && !v.build.is_empty() {
        s.push('+');
        s.push_str(&v.build.join("."));
    }
    s
}

fn add_lower(arm: &mut Arm, version: Version, inclusive: bool) {
    let next = Bound { version, inclusive };
    let replace = match &arm.lower {
        None => true,
        Some(old) => match compare_versions(&next.version, &old.version) {
            Ordering::Greater => true,
            Ordering::Less => false,
            Ordering::Equal => !next.inclusive && old.inclusive,
        },
    };
    if replace {
        arm.lower = Some(next);
    }
}

fn add_upper(arm: &mut Arm, version: Version, inclusive: bool) {
    let next = Bound { version, inclusive };
    let replace = match &arm.upper {
        None => true,
        Some(old) => match compare_versions(&next.version, &old.version) {
            Ordering::Less => true,
            Ordering::Greater => false,
            Ordering::Equal => !next.inclusive && old.inclusive,
        },
    };
    if replace {
        arm.upper = Some(next);
    }
}

fn add_comparison(arm: &mut Arm, op: Op, v: Version, anchor: bool) {
    if anchor && !v.pre.is_empty() {
        arm.anchors.push(core(&v));
    }
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

#[derive(Clone, Debug)]
struct Partial {
    values: Vec<u64>,
    count: usize,
    full: Option<Version>,
}

fn parse_partial(token: &str, at: usize) -> Result<Partial, RangeError> {
    let has_suffix = token.contains('-') || token.contains('+');
    let fields: Vec<&str> = token.split('.').collect();
    let has_wild = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));
    if has_suffix || (fields.len() >= 3 && !has_wild) {
        return parse_version(token)
            .map(|v| Partial {
                values: vec![v.major, v.minor, v.patch],
                count: 3,
                full: Some(v),
            })
            .map_err(|error| RangeError::Version { at, error });
    }
    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at });
    }
    let mut values = Vec::new();
    let mut wildcard_seen = false;
    for f in fields {
        if matches!(f, "*" | "x" | "X") {
            wildcard_seen = true;
        } else {
            if wildcard_seen
                || f.is_empty()
                || !f.bytes().all(is_digit)
                || (f.len() > 1 && f.starts_with('0'))
            {
                return Err(RangeError::Syntax { at });
            }
            let n = f.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
            values.push(n);
        }
    }
    if wildcard_seen {
        let first_wild = token
            .split('.')
            .position(|f| matches!(f, "*" | "x" | "X"))
            .unwrap_or(0);
        if token.split('.').skip(first_wild).any(|f| {
            !matches!(f, "*" | "x" | "X")
        }) {
            return Err(RangeError::Syntax { at });
        }
    }
    let count = values.len();
    if count > 3 {
        return Err(RangeError::Syntax { at });
    }
    Ok(Partial {
        values,
        count,
        full: None,
    })
}

fn partial_lower(p: &Partial) -> Version {
    let mut v = vcore(
        *p.values.first().unwrap_or(&0),
        *p.values.get(1).unwrap_or(&0),
        *p.values.get(2).unwrap_or(&0),
    );
    if let Some(full) = &p.full {
        v = full.clone();
    }
    v
}

fn increment_upper(p: &Partial, mode: u8, at: usize) -> Result<Version, RangeError> {
    let mut major = *p.values.first().unwrap_or(&0);
    let mut minor = *p.values.get(1).unwrap_or(&0);
    let mut patch = *p.values.get(2).unwrap_or(&0);
    let index = if mode == 1 {
        if major > 0 || p.count == 1 {
            0
        } else if minor > 0 || p.count == 2 {
            1
        } else {
            2
        }
    } else if mode == 2 {
        if p.count == 1 { 0 } else { 1 }
    } else if p.count == 1 {
        0
    } else {
        1
    };
    match index {
        0 => {
            major = major.checked_add(1).ok_or(RangeError::Syntax { at })?;
            minor = 0;
            patch = 0;
        }
        1 => {
            minor = minor.checked_add(1).ok_or(RangeError::Syntax { at })?;
            patch = 0;
        }
        _ => patch = patch.checked_add(1).ok_or(RangeError::Syntax { at })?,
    }
    Ok(zero_pre(vcore(major, minor, patch)))
}

fn token_starts_operand(token: &str) -> bool {
    token
        .as_bytes()
        .first()
        .map(|b| is_digit(*b) || matches!(*b, b'*' | b'x' | b'X' | b'^' | b'~' | b'<' | b'>' | b'='))
        .unwrap_or(false)
}

fn parse_full_comparator(
    token: &str,
    start: usize,
    arm: &mut Arm,
) -> Result<(), RangeError> {
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
        (Op::Eq, 0)
    };
    let operand = &token[n..];
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: start });
    }
    let v = parse_version(operand).map_err(|error| RangeError::Version {
        at: start + n,
        error,
    })?;
    add_comparison(arm, op, v, true);
    Ok(())
}

fn parse_expanded(token: &str, start: usize, arm: &mut Arm) -> Result<(), RangeError> {
    if token.starts_with('<') || token.starts_with('>') || token.starts_with('=') {
        return parse_full_comparator(token, start, arm);
    }
    let mode = match token.as_bytes()[0] {
        b'^' => 1,
        b'~' => 2,
        _ => 0,
    };
    let operand_start = start + usize::from(mode != 0);
    let operand = if mode == 0 { token } else { &token[1..] };
    let p = parse_partial(operand, operand_start)?;
    if mode == 0 {
        if let Some(full) = p.full {
            add_comparison(arm, Op::Eq, full, true);
        } else if p.count == 0 {
            // A wildcard-only operand admits every stable release.
        } else if p.count == 1 || p.count == 2 {
            add_comparison(arm, Op::Ge, partial_lower(&p), false);
            let upper = increment_upper(&p, 0, operand_start)?;
            add_comparison(arm, Op::Lt, upper, false);
        } else {
            let mut lower = partial_lower(&p);
            lower.build.clear();
            add_comparison(arm, Op::Ge, lower, false);
        }
        return Ok(());
    }
    if p.count == 0 {
        return Err(RangeError::Syntax { at: start });
    }
    add_comparison(arm, Op::Ge, partial_lower(&p), p.full.is_some());
    let upper = increment_upper(&p, mode, operand_start)?;
    add_comparison(arm, Op::Lt, upper, false);
    Ok(())
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn split_tokens(s: &str, base: usize) -> Vec<(&str, usize)> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && is_ws(bytes[i]) {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !is_ws(bytes[i]) {
            i += 1;
        }
        out.push((&s[start..i], base + start));
    }
    out
}

fn parse_arm(tokens: &[(&str, usize)]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        lower: None,
        upper: None,
        anchors: Vec::new(),
    };
    let hyphen_positions: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, (t, _))| if *t == "-" { Some(i) } else { None })
        .collect();
    if !hyphen_positions.is_empty() {
        if hyphen_positions[0] != 1 || hyphen_positions.len() != 1 || tokens.len() != 3 {
            return Err(RangeError::Syntax {
                at: tokens[hyphen_positions[0]].1,
            });
        }
        let (lo_s, lo_at) = tokens[0];
        let (hi_s, hi_at) = tokens[2];
        let lo = parse_partial(lo_s, lo_at)?;
        let hi = parse_partial(hi_s, hi_at)?;
        if lo.count == 0 {
            return Err(RangeError::Syntax { at: lo_at });
        }
        if hi.count == 0 {
            return Err(RangeError::Syntax { at: hi_at });
        }
        if let Some(full) = lo.full {
            add_comparison(&mut arm, Op::Ge, full, true);
        } else {
            add_comparison(&mut arm, Op::Ge, partial_lower(&lo), false);
        }
        if let Some(full) = hi.full {
            add_comparison(&mut arm, Op::Le, full, true);
        } else {
            let upper = increment_upper(&hi, 0, hi_at)?;
            add_comparison(&mut arm, Op::Lt, upper, false);
        }
        return Ok(arm);
    }
    for &(token, start) in tokens {
        if !token_starts_operand(token) {
            return Err(RangeError::Syntax { at: start });
        }
        parse_expanded(token, start, &mut arm)?;
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut nonwhite = false;
    for &b in bytes {
        if !is_ws(b) {
            nonwhite = true;
            break;
        }
    }
    if !nonwhite {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms = Vec::new();
    let mut segment_start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'|' && bytes[i + 1] == b'|' {
            let before = &s[segment_start..i];
            let tokens = split_tokens(before, segment_start);
            if tokens.is_empty() {
                return Err(RangeError::Empty { at: i });
            }
            arms.push(parse_arm(&tokens)?);
            i += 2;
            let mut next = i;
            while next < bytes.len() && is_ws(bytes[next]) {
                next += 1;
            }
            if next == bytes.len() {
                return Err(RangeError::Empty { at: bytes.len() });
            }
            if next + 1 < bytes.len() && bytes[next] == b'|' && bytes[next + 1] == b'|' {
                return Err(RangeError::Empty { at: next });
            }
            segment_start = i;
        } else {
            i += 1;
        }
    }
    let tokens = split_tokens(&s[segment_start..], segment_start);
    if tokens.is_empty() {
        if !arms.is_empty() {
            return Err(RangeError::Empty { at: bytes.len() });
        }
        return Err(RangeError::Empty { at: 0 });
    }
    arms.push(parse_arm(&tokens)?);
    Ok(Range { arms })
}

fn bound_allows_lower(v: &Version, bound: &Bound) -> bool {
    match compare_versions(v, &bound.version) {
        Ordering::Greater => true,
        Ordering::Equal => bound.inclusive,
        Ordering::Less => false,
    }
}

fn bound_allows_upper(v: &Version, bound: &Bound) -> bool {
    match compare_versions(v, &bound.version) {
        Ordering::Less => true,
        Ordering::Equal => bound.inclusive,
        Ordering::Greater => false,
    }
}

fn arm_allows(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if let Some(lower) = &arm.lower {
        if !bound_allows_lower(v, lower) {
            return false;
        }
    }
    if let Some(upper) = &arm.upper {
        if !bound_allows_upper(v, upper) {
            return false;
        }
    }
    v.pre.is_empty()
        || include_prerelease
        || arm.anchors.iter().any(|&t| t == core(v))
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|arm| arm_allows(v, arm, include_prerelease))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

fn next_core(t: (u64, u64, u64)) -> Option<(u64, u64, u64)> {
    let (a, b, c) = t;
    if c < u64::MAX {
        Some((a, b, c + 1))
    } else if b < u64::MAX {
        Some((a, b + 1, 0))
    } else if a < u64::MAX {
        Some((a + 1, 0, 0))
    } else {
        None
    }
}

fn prev_core(t: (u64, u64, u64)) -> Option<(u64, u64, u64)> {
    let (a, b, c) = t;
    if c > 0 {
        Some((a, b, c - 1))
    } else if b > 0 {
        Some((a, b - 1, u64::MAX))
    } else if a > 0 {
        Some((a - 1, u64::MAX, u64::MAX))
    } else {
        None
    }
}

fn stable_witness(arm: &Arm) -> bool {
    let mut low = (0, 0, 0);
    let mut high = (u64::MAX, u64::MAX, u64::MAX);
    if let Some(b) = &arm.lower {
        let t = core(&b.version);
        low = if b.version.pre.is_empty() {
            if b.inclusive { t } else if let Some(n) = next_core(t) { n } else { return false }
        } else {
            t
        };
    }
    if let Some(b) = &arm.upper {
        let t = core(&b.version);
        high = if b.version.pre.is_empty() {
            if b.inclusive { t } else if let Some(p) = prev_core(t) { p } else { return false }
        } else if let Some(p) = prev_core(t) {
            p
        } else {
            return false;
        };
    }
    low <= high
}

fn anchor_witness(arm: &Arm, tuple: (u64, u64, u64)) -> bool {
    let mut v = vcore(tuple.0, tuple.1, tuple.2);
    if let Some(l) = &arm.lower {
        if core(&l.version) > tuple {
            return false;
        }
        if core(&l.version) == tuple {
            if l.version.pre.is_empty() {
                if !l.inclusive {
                    return false;
                }
            } else if l.version.pre.as_slice() > ["0"] {
                // There may be a prerelease above the lower bound.
                v.pre = l.version.pre.clone();
                if !l.inclusive {
                    v.pre.push("0".to_string());
                }
            } else if !l.inclusive {
                v.pre = vec!["0".to_string(), "0".to_string()];
            } else {
                v.pre = vec!["0".to_string()];
            }
        }
    }
    if v.pre.is_empty() {
        v.pre = vec!["0".to_string()];
    }
    if let Some(u) = &arm.upper {
        if core(&u.version) < tuple {
            return false;
        }
        if core(&u.version) == tuple {
            if u.version.pre.is_empty() {
                return true;
            }
            if compare_versions(&v, &u.version) == Ordering::Greater
                || (!u.inclusive && compare_versions(&v, &u.version) == Ordering::Equal)
            {
                return false;
            }
        }
    }
    if let Some(l) = &arm.lower {
        if !bound_allows_lower(&v, l) {
            // Try the immediate successor of a same-tuple prerelease.
            if core(&l.version) == tuple && !l.version.pre.is_empty() {
                let mut successor = l.version.clone();
                successor.pre.push("0".to_string());
                if l.inclusive {
                    v = successor;
                } else {
                    successor.pre.push("0".to_string());
                    v = successor;
                }
            } else {
                return false;
            }
        }
    }
    arm.upper.as_ref().map_or(true, |u| bound_allows_upper(&v, u))
}

fn arm_witness(arm: &Arm) -> bool {
    if stable_witness(arm) {
        return true;
    }
    arm.anchors
        .iter()
        .copied()
        .any(|tuple| anchor_witness(arm, tuple))
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for aa in &a.arms {
        for bb in &b.arms {
            let mut combined = Arm {
                lower: None,
                upper: None,
                anchors: Vec::new(),
            };
            if let Some(x) = &aa.lower {
                add_lower(&mut combined, x.version.clone(), x.inclusive);
            }
            if let Some(x) = &bb.lower {
                add_lower(&mut combined, x.version.clone(), x.inclusive);
            }
            if let Some(x) = &aa.upper {
                add_upper(&mut combined, x.version.clone(), x.inclusive);
            }
            if let Some(x) = &bb.upper {
                add_upper(&mut combined, x.version.clone(), x.inclusive);
            }
            if include_prerelease {
                if stable_witness(&combined) || anchor_witness(&combined, (0, 0, 0)) {
                    return true;
                }
            } else {
                for &x in &aa.anchors {
                    if bb.anchors.contains(&x) && anchor_witness(&combined, x) {
                        return true;
                    }
                }
                if stable_witness(&combined) {
                    return true;
                }
            }
        }
    }
    false
}

fn arm_string(arm: &Arm) -> Option<String> {
    if !arm_witness(arm) {
        return None;
    }
    let lower = arm.lower.as_ref().map_or_else(
        || "*".to_string(),
        |b| {
            format!(
                "{}{}",
                if b.inclusive { ">=" } else { ">" },
                version_string(&b.version, false)
            )
        },
    );
    let upper = arm.upper.as_ref().map_or_else(
        || "*".to_string(),
        |b| {
            format!(
                "{}{}",
                if b.inclusive { "<=" } else { "<" },
                version_string(&b.version, false)
            )
        },
    );
    let mut anchors = arm.anchors.clone();
    anchors.sort_unstable();
    anchors.dedup();
    let anchors = if anchors.is_empty() {
        "-".to_string()
    } else {
        anchors
            .iter()
            .map(|(a, b, c)| format!("{}.{}.{}", a, b, c))
            .collect::<Vec<_>>()
            .join(",")
    };
    Some(format!("[{};{};pre={}]", lower, upper, anchors))
}

pub fn canonical(r: &Range) -> String {
    let mut arms: Vec<String> = r.arms.iter().filter_map(arm_string).collect();
    arms.sort();
    arms.dedup();
    if arms.is_empty() {
        "EMPTY".to_string()
    } else {
        arms.join(" || ")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn satisfying_extreme(candidates: &[String], r: &Range, greatest: bool) -> Result<Option<String>, CandidateError> {
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
            let ord = compare_versions(v, &parsed[j]);
            if (greatest && ord == Ordering::Greater) || (!greatest && ord == Ordering::Less) {
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