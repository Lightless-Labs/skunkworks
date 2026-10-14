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

fn is_id_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

fn parse_core_field(bytes: &[u8], pos: &mut usize) -> Result<u64, VersionError> {
    let start = *pos;
    while *pos < bytes.len() && is_digit(bytes[*pos]) {
        *pos += 1;
    }
    if start == *pos {
        return Err(if *pos == bytes.len()
            || matches!(bytes[*pos], b'.' | b'-' | b'+')
        {
            VersionError::MissingCore { at: start }
        } else {
            VersionError::InvalidChar { at: start }
        });
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
    base: usize,
    numeric_zero_check: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    loop {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'.')
            .map(|i| start + i)
            .unwrap_or(bytes.len());
        if start == end {
            return Err(VersionError::EmptyIdentifier { at: base + start });
        }
        for i in start..end {
            if !is_id_char(bytes[i]) {
                return Err(VersionError::InvalidChar { at: base + i });
            }
        }
        if numeric_zero_check
            && bytes[start..end].iter().all(|b| is_digit(*b))
            && end - start > 1
            && bytes[start] == b'0'
        {
            return Err(VersionError::LeadingZero { at: base + start });
        }
        out.push(s[start..end].to_string());
        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(VersionError::EmptyIdentifier { at: base + start });
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
    let major = parse_core_field(bytes, &mut pos)?;
    for expected in 0..2 {
        if pos >= bytes.len() || bytes[pos] != b'.' {
            return Err(if pos == bytes.len() || matches!(bytes[pos], b'-' | b'+') {
                VersionError::MissingCore { at: pos }
            } else {
                VersionError::InvalidChar { at: pos }
            });
        }
        pos += 1;
        let n = parse_core_field(bytes, &mut pos)?;
        if expected == 0 {
            let minor = n;
            if pos >= bytes.len() || bytes[pos] != b'.' {
                return Err(if pos == bytes.len() || matches!(bytes[pos], b'-' | b'+') {
                    VersionError::MissingCore { at: pos }
                } else {
                    VersionError::InvalidChar { at: pos }
                });
            }
            pos += 1;
            let patch = parse_core_field(bytes, &mut pos)?;
            return parse_version_tail(s, pos, major, minor, patch);
        }
    }
    unreachable!()
}

fn parse_version_tail(
    s: &str,
    mut pos: usize,
    major: u64,
    minor: u64,
    patch: u64,
) -> Result<Version, VersionError> {
    let bytes = s.as_bytes();
    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] == b'-' {
        pos += 1;
        let pre_start = pos;
        let plus = bytes[pos..]
            .iter()
            .position(|&b| b == b'+')
            .map(|i| pos + i)
            .unwrap_or(bytes.len());
        pre = parse_identifiers(&s[pos..plus], pos, true)?;
        pos = plus;
        if pos == pre_start {
            return Err(VersionError::EmptyIdentifier { at: pos });
        }
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        pos += 1;
        build = parse_identifiers(&s[pos..], pos, false)?;
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

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn all_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn cmp_numeric_id(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    core(a)
        .cmp(&core(b))
        .then_with(|| match (a.pre.is_empty(), b.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => {
                for (x, y) in a.pre.iter().zip(&b.pre) {
                    let xd = all_digits(x);
                    let yd = all_digits(y);
                    let ord = match (xd, yd) {
                        (true, true) => cmp_numeric_id(x, y),
                        (true, false) => Ordering::Less,
                        (false, true) => Ordering::Greater,
                        (false, false) => x.as_bytes().cmp(y.as_bytes()),
                    };
                    if ord != Ordering::Equal {
                        return ord;
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

#[derive(Clone, Copy)]
struct Tok<'a> {
    text: &'a str,
    at: usize,
}

fn whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokens(s: &str, base: usize) -> Vec<Tok<'_>> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && whitespace(b[i]) {
            i += 1;
        }
        if i == b.len() {
            break;
        }
        let start = i;
        while i < b.len() && !whitespace(b[i]) {
            i += 1;
        }
        out.push(Tok {
            text: &s[start..i],
            at: base + start,
        });
    }
    out
}

fn zero_version(major: u64, minor: u64, patch: u64) -> Version {
    Version {
        major,
        minor,
        patch,
        pre: Vec::new(),
        build: Vec::new(),
    }
}

#[derive(Clone, Debug)]
enum Partial {
    Full(Version),
    Partial {
        fields: Vec<Option<u64>>,
        k: usize,
    },
}

fn parse_partial(token: Tok<'_>) -> Result<Partial, RangeError> {
    let s = token.text;
    if s.contains('-')
        || s.contains('+')
        || (s.split('.').count() >= 3
            && !s.split('.').any(|f| matches!(f, "*" | "x" | "X")))
    {
        return parse_version(s)
            .map(Partial::Full)
            .map_err(|error| RangeError::Version { at: token.at, error });
    }

    let fields: Vec<&str> = s.split('.').collect();
    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|f| f.is_empty()) {
        return Err(RangeError::Syntax { at: token.at });
    }

    let mut values = Vec::new();
    let mut wildcard_seen = false;
    let mut k = 0;
    for f in fields {
        if matches!(f, "*" | "x" | "X") {
            wildcard_seen = true;
            values.push(None);
        } else {
            if wildcard_seen || !f.bytes().all(is_digit) || (f.len() > 1 && f.starts_with('0')) {
                return Err(RangeError::Syntax { at: token.at });
            }
            let n = f.parse::<u64>().map_err(|_| RangeError::Syntax { at: token.at })?;
            values.push(Some(n));
            k += 1;
        }
    }
    if values.iter().skip(k).any(Option::is_some) {
        return Err(RangeError::Syntax { at: token.at });
    }
    Ok(Partial::Partial { fields: values, k })
}

fn padded(fields: &[Option<u64>]) -> (u64, u64, u64) {
    (
        fields.first().and_then(|x| *x).unwrap_or(0),
        fields.get(1).and_then(|x| *x).unwrap_or(0),
        fields.get(2).and_then(|x| *x).unwrap_or(0),
    )
}

fn upper_bump(
    tuple: (u64, u64, u64),
    field: usize,
    at: usize,
) -> Result<Version, RangeError> {
    let (mut a, mut b, mut c) = tuple;
    match field {
        0 => a = a.checked_add(1).ok_or(RangeError::Syntax { at })?,
        1 => b = b.checked_add(1).ok_or(RangeError::Syntax { at })?,
        _ => c = c.checked_add(1).ok_or(RangeError::Syntax { at })?,
    }
    if field <= 0 {
        b = 0;
        c = 0;
    } else if field == 1 {
        c = 0;
    }
    let mut v = zero_version(a, b, c);
    v.pre.push("0".into());
    Ok(v)
}

fn add_bound(arm: &mut Arm, version: Version, inclusive: bool, lower: bool, anchor: bool) {
    if anchor && !version.pre.is_empty() {
        arm.anchors.push(core(&version));
    }
    arm.bounds.push(Bound {
        version,
        inclusive,
        lower,
    });
}

fn parse_comparator(tok: Tok<'_>, arm: &mut Arm) -> Result<(), RangeError> {
    let (op, skip) = if tok.text.starts_with(">=") {
        (">=", 2)
    } else if tok.text.starts_with("<=") {
        ("<=", 2)
    } else if tok.text.starts_with('>') {
        (">", 1)
    } else if tok.text.starts_with('<') {
        ("<", 1)
    } else if tok.text.starts_with('=') {
        ("=", 1)
    } else {
        ("=", 0)
    };
    let operand = &tok.text[skip..];
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: tok.at });
    }
    let v = parse_version(operand).map_err(|error| RangeError::Version {
        at: tok.at + skip,
        error,
    })?;
    match op {
        ">" => add_bound(arm, v, false, true, true),
        ">=" => add_bound(arm, v, true, true, true),
        "<" => add_bound(arm, v, false, false, true),
        "<=" => add_bound(arm, v, true, false, true),
        _ => {
            add_bound(arm, v.clone(), true, true, true);
            add_bound(arm, v, true, false, true);
        }
    }
    Ok(())
}

fn parse_bare(tok: Tok<'_>, arm: &mut Arm) -> Result<(), RangeError> {
    match parse_partial(tok)? {
        Partial::Full(v) => {
            add_bound(arm, v.clone(), true, true, true);
            add_bound(arm, v, true, false, true);
        }
        Partial::Partial { fields, k } => match k {
            0 => {}
            1 | 2 => {
                let tuple = padded(&fields);
                add_bound(arm, zero_version(tuple.0, tuple.1, tuple.2), true, true, false);
                let upper = upper_bump(tuple, if k == 1 { 0 } else { 1 }, tok.at)?;
                add_bound(arm, upper, false, false, false);
            }
            _ => {
                let tuple = padded(&fields);
                let v = zero_version(tuple.0, tuple.1, tuple.2);
                add_bound(arm, v.clone(), true, true, false);
                add_bound(arm, v, true, false, false);
            }
        },
    }
    Ok(())
}

fn parse_caret_tilde(tok: Tok<'_>, arm: &mut Arm, caret: bool) -> Result<(), RangeError> {
    let operand = Tok {
        text: &tok.text[1..],
        at: tok.at + 1,
    };
    match parse_partial(operand)? {
        Partial::Full(v) => {
            if v.pre.is_empty() {
                let tuple = core(&v);
                let k = 3;
                add_bound(arm, v, true, true, true);
                let field = if caret {
                    if tuple.0 > 0 { 0 } else if tuple.1 > 0 { 1 } else { 2 }
                } else {
                    1
                };
                let upper = upper_bump(tuple, field, operand.at)?;
                add_bound(arm, upper, false, false, false);
            } else {
                let tuple = core(&v);
                add_bound(arm, v.clone(), true, true, true);
                let field = if caret {
                    if tuple.0 > 0 { 0 } else if tuple.1 > 0 { 1 } else { 2 }
                } else {
                    1
                };
                let upper = upper_bump(tuple, field, operand.at)?;
                add_bound(arm, upper, false, false, false);
            }
        }
        Partial::Partial { fields, k } => {
            if k == 0 {
                return Err(RangeError::Syntax { at: tok.at });
            }
            let tuple = padded(&fields);
            add_bound(
                arm,
                zero_version(tuple.0, tuple.1, tuple.2),
                true,
                true,
                false,
            );
            let field = if caret {
                if tuple.0 > 0 || k == 1 {
                    0
                } else if tuple.1 > 0 || k == 2 {
                    1
                } else {
                    2
                }
            } else if k == 1 {
                0
            } else {
                1
            };
            let upper = upper_bump(tuple, field, operand.at)?;
            add_bound(arm, upper, false, false, false);
        }
    }
    Ok(())
}

fn parse_hyphen_arm(ts: &[Tok<'_>], arm: &mut Arm) -> Result<(), RangeError> {
    let minus = ts.iter().position(|t| t.text == "-").unwrap();
    if ts.len() != 3 || minus != 1 {
        return Err(RangeError::Syntax { at: ts[minus].at });
    }
    let lo = parse_partial(ts[0])?;
    let hi = parse_partial(ts[2])?;
    let (lo_v, lo_k) = match lo {
        Partial::Full(v) => (v, 3),
        Partial::Partial { fields, k } => {
            if k == 0 {
                return Err(RangeError::Syntax { at: ts[0].at });
            }
            let t = padded(&fields);
            (zero_version(t.0, t.1, t.2), k)
        }
    };
    let (hi_v, hi_k) = match hi {
        Partial::Full(v) => (v, 3),
        Partial::Partial { fields, k } => {
            if k == 0 {
                return Err(RangeError::Syntax { at: ts[2].at });
            }
            let t = padded(&fields);
            (zero_version(t.0, t.1, t.2), k)
        }
    };
    let _ = lo_k;
    add_bound(arm, lo_v, true, true, true);
    if hi_k == 3 && !hi_v.pre.is_empty() {
        add_bound(arm, hi_v, true, false, true);
    } else if hi_k == 3 {
        add_bound(arm, hi_v, true, false, true);
    } else {
        let t = core(&hi_v);
        let upper = upper_bump(t, if hi_k == 1 { 0 } else { 1 }, ts[2].at)?;
        add_bound(arm, upper, false, false, false);
    }
    Ok(())
}

fn parse_arm(ts: &[Tok<'_>]) -> Result<Arm, RangeError> {
    if let Some(minus) = ts.iter().position(|t| t.text == "-") {
        let mut arm = Arm {
            bounds: Vec::new(),
            anchors: Vec::new(),
        };
        parse_hyphen_arm(ts, &mut arm)?;
        return Ok(arm);
    }

    let mut arm = Arm {
        bounds: Vec::new(),
        anchors: Vec::new(),
    };
    for tok in ts {
        let first = tok.text.as_bytes()[0];
        if matches!(first, b'<' | b'>' | b'=') {
            parse_comparator(*tok, &mut arm)?;
        } else if first == b'^' || first == b'~' {
            parse_caret_tilde(*tok, &mut arm, first == b'^')?;
        } else if first.is_ascii_digit() || matches!(first, b'*' | b'x' | b'X') {
            parse_bare(*tok, &mut arm)?;
        } else {
            return Err(RangeError::Syntax { at: tok.at });
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let ts = tokens(s, 0);
    if ts.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    let mut arms = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < ts.len() {
        if ts[i].text == "||" {
            if start == i {
                return Err(RangeError::Empty { at: ts[i].at });
            }
            arms.push(parse_arm(&ts[start..i])?);
            start = i + 1;
        }
        i += 1;
    }
    if start == ts.len() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(&ts[start..])?);
    Ok(Range { arms })
}

fn cmp_bound_lower(a: &Bound, b: &Bound) -> Ordering {
    compare_versions(&a.version, &b.version)
        .then_with(|| a.inclusive.cmp(&b.inclusive).reverse())
}

fn cmp_bound_upper(a: &Bound, b: &Bound) -> Ordering {
    compare_versions(&a.version, &b.version)
        .then_with(|| a.inclusive.cmp(&b.inclusive))
}

fn arm_matches(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    for b in &arm.bounds {
        let ord = compare_versions(v, &b.version);
        if b.lower {
            if ord == Ordering::Less || (ord == Ordering::Equal && !b.inclusive) {
                return false;
            }
        } else if ord == Ordering::Greater || (ord == Ordering::Equal && !b.inclusive) {
            return false;
        }
    }
    if !v.pre.is_empty() && !include_prerelease && !arm.anchors.contains(&core(v)) {
        return false;
    }
    true
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|a| arm_matches(v, a, include_prerelease))
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

fn extrema(candidates: &[String], r: &Range, max: bool) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::new();
    for (i, s) in candidates.iter().enumerate() {
        parsed.push(parse_version(s).map_err(|error| CandidateError { index: i, error })?);
    }
    let mut best: Option<usize> = None;
    for (i, v) in parsed.iter().enumerate() {
        if !matches(v, r) {
            continue;
        }
        best = match best {
            None => Some(i),
            Some(j) => {
                let ord = compare_versions(v, &parsed[j]);
                if (max && ord == Ordering::Greater) || (!max && ord == Ordering::Less) {
                    Some(i)
                } else {
                    Some(j)
                }
            }
        };
    }
    Ok(best.map(|i| candidates[i].clone()))
}

fn arm_has_stable_witness(arm: &Arm) -> bool {
    let mut lower: Option<&Bound> = None;
    let mut upper: Option<&Bound> = None;
    for b in &arm.bounds {
        if b.lower {
            if lower.map_or(true, |x| cmp_bound_lower(b, x) == Ordering::Greater) {
                lower = Some(b);
            }
        } else if upper.map_or(true, |x| cmp_bound_upper(b, x) == Ordering::Less) {
            upper = Some(b);
        }
    }
    match (lower, upper) {
        (Some(l), Some(u)) => {
            let c = compare_versions(&l.version, &u.version);
            c == Ordering::Less || (c == Ordering::Equal && l.inclusive && u.inclusive && l.version.pre.is_empty())
        }
        _ => true,
    }
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for aa in &a.arms {
        for bb in &b.arms {
            let mut merged = aa.clone();
            merged.bounds.extend(bb.bounds.clone());
            if include_prerelease {
                if arm_has_stable_witness(&merged) || bounds_allow_prerelease_pair(&merged, &[], true) {
                    return true;
                }
            } else {
                let anchors: Vec<_> = aa
                    .anchors
                    .iter()
                    .copied()
                    .filter(|x| bb.anchors.contains(x))
                    .collect();
                if arm_has_stable_witness(&merged) {
                    return true;
                }
                for c in anchors {
                    if anchor_witness(c, &merged) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn bounds_allow_prerelease_pair(_arm: &Arm, _anchors: &[(u64, u64, u64)], _include: bool) -> bool {
    false
}

fn anchor_witness(c: (u64, u64, u64), arm: &Arm) -> bool {
    let mut v = zero_version(c.0, c.1, c.2);
    v.pre.push("0".into());
    arm.bounds.iter().all(|b| {
        let o = compare_versions(&v, &b.version);
        if b.lower {
            o == Ordering::Greater || (o == Ordering::Equal && b.inclusive)
        } else {
            o == Ordering::Less || (o == Ordering::Equal && b.inclusive)
        }
    })
}

fn version_no_build(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

pub fn canonical(r: &Range) -> String {
    let mut arms = Vec::new();
    for arm in &r.arms {
        let mut lower: Option<&Bound> = None;
        let mut upper: Option<&Bound> = None;
        for b in &arm.bounds {
            if b.lower {
                if lower.map_or(true, |x| cmp_bound_lower(b, x) == Ordering::Greater) {
                    lower = Some(b);
                }
            } else if upper.map_or(true, |x| cmp_bound_upper(b, x) == Ordering::Less) {
                upper = Some(b);
            }
        }
        let anchors = {
            let mut a = arm.anchors.clone();
            a.sort();
            a.dedup();
            a
        };
        let anchor_text = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors
                .iter()
                .map(|(a, b, c)| format!("{a}.{b}.{c}"))
                .collect::<Vec<_>>()
                .join(",")
        };
        let lower_text = lower.map_or("*".to_string(), |b| {
            format!(
                "{}{}",
                if b.inclusive { ">=" } else { ">" },
                version_no_build(&b.version)
            )
        });
        let upper_text = upper.map_or("*".to_string(), |b| {
            format!(
                "{}{}",
                if b.inclusive { "<=" } else { "<" },
                version_no_build(&b.version)
            )
        });
        arms.push(format!("[{lower_text};{upper_text};pre={anchor_text}]"));
    }
    arms.sort();
    arms.dedup();
    if arms.is_empty() {
        "EMPTY".into()
    } else {
        arms.join(" || ")
    }
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
    leaves: &mut Vec<Range>,
) -> Result<(), SelectError> {
    match expr {
        SetExpr::Range(s) => {
            let r = parse_range(s).map_err(|error| SelectError::InvalidRange {
                package: package.to_string(),
                label: label.to_string(),
                path: path.clone(),
                error,
            })?;
            leaves.push(r);
        }
        SetExpr::Union(xs) | SetExpr::Intersection(xs) => {
            for (i, x) in xs.iter().enumerate() {
                path.push(i);
                validate_expr(x, package, label, path, leaves)?;
                path.pop();
            }
        }
    }
    Ok(())
}

fn eval_expr(expr: &SetExpr, v: &Version, include: bool, cache: &mut Vec<Range>) -> bool {
    match expr {
        SetExpr::Range(_) => {
            let r = cache.remove(0);
            matches_with(v, &r, include)
        }
        SetExpr::Union(xs) => xs.iter().any(|x| eval_expr(x, v, include, cache)),
        SetExpr::Intersection(xs) => xs.iter().all(|x| eval_expr(x, v, include, cache)),
    }
}

fn eval_expr_ranges(
    expr: &SetExpr,
    v: &Version,
    include: bool,
    ranges: &[Range],
    index: &mut usize,
) -> bool {
    match expr {
        SetExpr::Range(_) => {
            let ok = matches_with(v, &ranges[*index], include);
            *index += 1;
            ok
        }
        SetExpr::Union(xs) => {
            let mut result = false;
            for x in xs {
                result |= eval_expr_ranges(x, v, include, ranges, index);
            }
            result
        }
        SetExpr::Intersection(xs) => {
            let mut result = true;
            for x in xs {
                result &= eval_expr_ranges(x, v, include, ranges, index);
            }
            result
        }
    }
}

fn conflict_labels(
    constraints: &[Constraint],
    parsed: &[Vec<Range>],
    versions: &[Version],
    include: bool,
) -> Vec<String> {
    let n = constraints.len();
    if versions.is_empty() {
        return Vec::new();
    }
    for size in 1..=n {
        let mut found: Option<Vec<String>> = None;
        let mut selected = Vec::new();
        fn search(
            start: usize,
            need: usize,
            selected: &mut Vec<usize>,
            found: &mut Option<Vec<String>>,
            constraints: &[Constraint],
            parsed: &[Vec<Range>],
            versions: &[Version],
            include: bool,
        ) {
            if need == 0 {
                let satisfiable = versions.iter().any(|v| {
                    selected.iter().all(|&ci| {
                        let mut ri = 0;
                        eval_expr_ranges(
                            &constraints[ci].expr,
                            v,
                            include,
                            &parsed[ci],
                            &mut ri,
                        )
                    })
                });
                if !satisfiable {
                    let mut labels: Vec<String> = selected
                        .iter()
                        .map(|&i| constraints[i].label.clone())
                        .collect();
                    labels.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
                    if found.as_ref().map_or(true, |old| {
                        labels
                            .iter()
                            .map(String::as_bytes)
                            .cmp(old.iter().map(String::as_bytes))
                            == Ordering::Less
                    }) {
                        *found = Some(labels);
                    }
                }
                return;
            }
            if constraints.len().saturating_sub(start) < need {
                return;
            }
            for i in start..constraints.len() {
                selected.push(i);
                search(
                    i + 1,
                    need - 1,
                    selected,
                    found,
                    constraints,
                    parsed,
                    versions,
                    include,
                );
                selected.pop();
            }
        }
        search(
            0,
            size,
            &mut selected,
            &mut found,
            constraints,
            parsed,
            versions,
            include,
        );
        if let Some(v) = found {
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
            return Err(SelectError::DuplicatePackage {
                name: p.name.clone(),
            });
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

    let mut versions_by_package = Vec::new();
    for p in packages {
        let mut versions = Vec::new();
        for (i, s) in p.candidates.iter().enumerate() {
            versions.push(parse_version(s).map_err(|error| SelectError::InvalidCandidate {
                package: p.name.clone(),
                index: i,
                error,
            })?);
        }
        versions_by_package.push(versions);
    }

    let mut ranges_by_package = Vec::new();
    for p in packages {
        let mut package_ranges = Vec::new();
        for c in &p.constraints {
            let mut leaves = Vec::new();
            validate_expr(
                &c.expr,
                &p.name,
                &c.label,
                &mut Vec::new(),
                &mut leaves,
            )?;
            package_ranges.push(leaves);
        }
        ranges_by_package.push(package_ranges);
    }

    let mut result = Vec::new();
    for (pi, p) in packages.iter().enumerate() {
        let versions = &versions_by_package[pi];
        let mut qualifying = Vec::new();
        for (vi, v) in versions.iter().enumerate() {
            let accepted = p.constraints.iter().enumerate().all(|(ci, c)| {
                let mut ri = 0;
                eval_expr_ranges(
                    &c.expr,
                    v,
                    include_prerelease,
                    &ranges_by_package[pi][ci],
                    &mut ri,
                )
            });
            if accepted {
                qualifying.push(vi);
            }
        }
        if qualifying.is_empty() {
            let labels = conflict_labels(
                &p.constraints,
                &ranges_by_package[pi],
                versions,
                include_prerelease,
            );
            return Err(SelectError::NoSolution {
                package: p.name.clone(),
                labels,
            });
        }
        let stable_qualifying: Vec<usize> = qualifying
            .iter()
            .copied()
            .filter(|&i| versions[i].pre.is_empty())
            .collect();
        let pool = match preference {
            Preference::StableFirst if !stable_qualifying.is_empty() => stable_qualifying,
            _ => qualifying,
        };
        let mut best = pool[0];
        for &i in &pool[1..] {
            let ord = compare_versions(&versions[i], &versions[best]);
            let replace = match preference {
                Preference::Lowest => ord == Ordering::Less,
                Preference::Highest | Preference::StableFirst => ord == Ordering::Greater,
            };
            if replace {
                best = i;
            }
        }
        result.push(Selection {
            package: p.name.clone(),
            version: p.candidates[best].clone(),
        });
    }
    Ok(result)
}