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

fn numeric_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn parse_identifier_part(
    s: &str,
    base: usize,
    numeric_leading_zero: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut start = 0;
    loop {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'.')
            .map(|i| start + i)
            .unwrap_or(bytes.len());
        if end == start {
            return Err(VersionError::EmptyIdentifier { at: base + start });
        }
        for i in start..end {
            if !is_ident_char(bytes[i]) {
                return Err(VersionError::InvalidChar { at: base + i });
            }
        }
        let id = &s[start..end];
        if numeric_leading_zero
            && numeric_id(id)
            && id.len() > 1
            && id.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: base + start });
        }
        result.push(id.to_string());
        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(VersionError::EmptyIdentifier { at: base + start });
        }
    }
    Ok(result)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let bytes = s.as_bytes();
    let mut cursor = 0;
    let mut core = [0u64; 3];

    for field in 0..3 {
        let start = cursor;
        while cursor < bytes.len() && is_digit(bytes[cursor]) {
            cursor += 1;
        }
        if cursor == start {
            if cursor == bytes.len()
                || matches!(bytes[cursor], b'.' | b'-' | b'+')
            {
                return Err(VersionError::MissingCore { at: start });
            }
            return Err(VersionError::InvalidChar { at: cursor });
        }

        let digits = &s[start..cursor];
        if digits.len() > 1 && digits.as_bytes()[0] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }
        core[field] = digits
            .parse::<u64>()
            .map_err(|_| VersionError::Overflow { at: start })?;

        if field < 2 {
            if cursor == bytes.len()
                || matches!(bytes[cursor], b'-' | b'+')
            {
                return Err(VersionError::MissingCore { at: cursor });
            }
            if bytes[cursor] != b'.' {
                return Err(VersionError::InvalidChar { at: cursor });
            }
            cursor += 1;
        }
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if cursor < bytes.len() {
        match bytes[cursor] {
            b'-' => {
                let pre_start = cursor + 1;
                let plus = bytes[pre_start..]
                    .iter()
                    .position(|&b| b == b'+')
                    .map(|i| pre_start + i)
                    .unwrap_or(bytes.len());
                pre = parse_identifier_part(
                    &s[pre_start..plus],
                    pre_start,
                    true,
                )?;
                cursor = plus;
            }
            b'+' => {}
            _ => return Err(VersionError::InvalidChar { at: cursor }),
        }
    }

    if cursor < bytes.len() && bytes[cursor] == b'+' {
        let build_start = cursor + 1;
        build = parse_identifier_part(&s[build_start..], build_start, false)?;
        cursor = bytes.len();
    }

    if cursor < bytes.len() {
        return Err(VersionError::InvalidChar { at: cursor });
    }

    Ok(Version {
        major: core[0],
        minor: core[1],
        patch: core[2],
        pre,
        build,
    })
}

fn cmp_numeric_str(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    for (x, y) in a.iter().zip(b.iter()) {
        let xn = numeric_id(x);
        let yn = numeric_id(y);
        let ord = match (xn, yn) {
            (true, true) => cmp_numeric_str(x, y),
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

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone)]
struct Partial {
    nums: Vec<u64>,
    count: usize,
    full: Option<Version>,
}

fn ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn split_tokens(s: &str, base: usize) -> Vec<(String, usize)> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && ws(bytes[i]) {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !ws(bytes[i]) {
            i += 1;
        }
        out.push((s[start..i].to_string(), base + start));
    }
    out
}

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn version(major: u64, minor: u64, patch: u64) -> Version {
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

fn syntax(at: usize) -> RangeError {
    RangeError::Syntax { at }
}

fn parse_partial_operand(
    text: &str,
    at: usize,
) -> Result<Partial, RangeError> {
    let has_sign = text.bytes().any(|b| b == b'-' || b == b'+');
    let fields: Vec<&str> = text.split('.').collect();
    let has_wild = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));
    if has_sign || (fields.len() >= 3 && !has_wild) {
        return parse_version(text)
            .map(|v| Partial {
                nums: vec![v.major, v.minor, v.patch],
                count: 3,
                full: Some(v),
            })
            .map_err(|error| RangeError::Version { at, error });
    }
    if fields.is_empty() || fields.len() > 3 {
        return Err(syntax(at));
    }
    let mut nums = Vec::new();
    let mut wildcard_seen = false;
    for f in fields {
        if f.is_empty() {
            return Err(syntax(at));
        }
        if matches!(f, "*" | "x" | "X") {
            wildcard_seen = true;
        } else {
            if wildcard_seen
                || !f.bytes().all(is_digit)
                || (f.len() > 1 && f.as_bytes()[0] == b'0')
            {
                return Err(syntax(at));
            }
            let n = f.parse::<u64>().map_err(|_| syntax(at))?;
            nums.push(n);
        }
    }
    let count = nums.len();
    Ok(Partial {
        nums,
        count,
        full: None,
    })
}

fn padded(p: &Partial) -> Version {
    version(
        p.nums.first().copied().unwrap_or(0),
        p.nums.get(1).copied().unwrap_or(0),
        p.nums.get(2).copied().unwrap_or(0),
    )
}

fn bump_upper(
    p: &Partial,
    kind: char,
    at: usize,
) -> Result<Version, RangeError> {
    let mut v = padded(p);
    let k = p.count;
    match kind {
        '^' => {
            if (v.major > 0 || k == 1) {
                v.major = v.major.checked_add(1).ok_or_else(|| syntax(at))?;
                v.minor = 0;
                v.patch = 0;
            } else if v.minor > 0 || k == 2 {
                v.minor = v.minor.checked_add(1).ok_or_else(|| syntax(at))?;
                v.patch = 0;
            } else {
                v.patch = v.patch.checked_add(1).ok_or_else(|| syntax(at))?;
            }
        }
        '~' => {
            if k == 1 {
                v.major = v.major.checked_add(1).ok_or_else(|| syntax(at))?;
                v.minor = 0;
                v.patch = 0;
            } else {
                v.minor = v.minor.checked_add(1).ok_or_else(|| syntax(at))?;
                v.patch = 0;
            }
        }
        _ => {}
    }
    Ok(zero_pre(v))
}

fn add_lower(arm: &mut Arm, v: Version, inclusive: bool) {
    let b = Bound { version: v, inclusive };
    let replace = match &arm.lower {
        None => true,
        Some(old) => {
            let c = compare_versions(&b.version, &old.version);
            c == Ordering::Greater
                || (c == Ordering::Equal && !inclusive && old.inclusive)
        }
    };
    if replace {
        arm.lower = Some(b);
    }
}

fn add_upper(arm: &mut Arm, v: Version, inclusive: bool) {
    let b = Bound { version: v, inclusive };
    let replace = match &arm.upper {
        None => true,
        Some(old) => {
            let c = compare_versions(&b.version, &old.version);
            c == Ordering::Less
                || (c == Ordering::Equal && !inclusive && old.inclusive)
        }
    };
    if replace {
        arm.upper = Some(b);
    }
}

fn add_anchor(arm: &mut Arm, v: &Version) {
    if !v.pre.is_empty() {
        arm.anchors.push(core(v));
    }
}

fn add_partial_bare(arm: &mut Arm, p: Partial, at: usize) -> Result<(), RangeError> {
    if let Some(v) = p.full {
        add_anchor(arm, &v);
        add_lower(arm, v.clone(), true);
        add_upper(arm, v, true);
    } else if p.count > 0 {
        add_lower(arm, padded(&p), true);
        if p.count < 3 {
            let upper = if p.count == 1 {
                let mut v = padded(&p);
                v.major = v.major.checked_add(1).ok_or_else(|| syntax(at))?;
                v.minor = 0;
                v.patch = 0;
                zero_pre(v)
            } else {
                let mut v = padded(&p);
                v.minor = v.minor.checked_add(1).ok_or_else(|| syntax(at))?;
                v.patch = 0;
                zero_pre(v)
            };
            add_upper(arm, upper, false);
        }
    }
    Ok(())
}

fn parse_explicit_comparator(
    arm: &mut Arm,
    token: &str,
    at: usize,
) -> Result<bool, RangeError> {
    let (op, len) = if token.starts_with(">=") {
        (">=", 2)
    } else if token.starts_with("<=") {
        ("<=", 2)
    } else if token.starts_with('=') {
        ("=", 1)
    } else if token.starts_with('>') {
        (">", 1)
    } else if token.starts_with('<') {
        ("<", 1)
    } else {
        return Ok(false);
    };
    let operand = &token[len..];
    let operand_at = at + len;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(syntax(at));
    }
    let v = parse_version(operand)
        .map_err(|error| RangeError::Version { at: operand_at, error })?;
    add_anchor(arm, &v);
    match op {
        "=" => {
            add_lower(arm, v.clone(), true);
            add_upper(arm, v, true);
        }
        ">" => add_lower(arm, v, false),
        ">=" => add_lower(arm, v, true),
        "<" => add_upper(arm, v, false),
        "<=" => add_upper(arm, v, true),
        _ => {}
    }
    Ok(true)
}

fn parse_arm(tokens: &[(String, usize)]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        lower: None,
        upper: None,
        anchors: Vec::new(),
    };

    let hyphens: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, (t, _))| t == "-")
        .map(|(i, _)| i)
        .collect();

    if !hyphens.is_empty() {
        if tokens.len() != 3 || hyphens != [1] {
            return Err(syntax(tokens[hyphens[0]].1));
        }
        let (lower_text, lower_at) = (&tokens[0].0, tokens[0].1);
        let (upper_text, upper_at) = (&tokens[2].0, tokens[2].1);
        let lower = parse_partial_operand(lower_text, lower_at)?;
        let upper = parse_partial_operand(upper_text, upper_at)?;
        if lower.count == 0 {
            return Err(syntax(lower_at));
        }
        if upper.count == 0 {
            return Err(syntax(upper_at));
        }
        if let Some(v) = &lower.full {
            add_anchor(&mut arm, v);
        }
        if let Some(v) = &upper.full {
            add_anchor(&mut arm, v);
        }
        add_lower(&mut arm, padded(&lower), true);
        if let Some(v) = upper.full {
            add_upper(&mut arm, v, true);
        } else if upper.count == 1 {
            let mut v = padded(&upper);
            v.major = v.major.checked_add(1).ok_or_else(|| syntax(upper_at))?;
            v.minor = 0;
            v.patch = 0;
            add_upper(&mut arm, zero_pre(v), false);
        } else if upper.count == 2 {
            let mut v = padded(&upper);
            v.minor = v.minor.checked_add(1).ok_or_else(|| syntax(upper_at))?;
            v.patch = 0;
            add_upper(&mut arm, zero_pre(v), false);
        } else {
            add_upper(&mut arm, padded(&upper), true);
        }
        return Ok(arm);
    }

    for (token, at) in tokens {
        if parse_explicit_comparator(&mut arm, token, *at)? {
            continue;
        }
        let first = token.as_bytes().first().copied();
        match first {
            Some(b'^') | Some(b'~') => {
                let kind = first.unwrap() as char;
                let operand = &token[1..];
                if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit()
                    && !matches!(operand.as_bytes()[0], b'*' | b'x' | b'X')
                {
                    return Err(syntax(*at));
                }
                let p = parse_partial_operand(operand, at + 1)?;
                if p.count == 0 {
                    return Err(syntax(*at));
                }
                if let Some(v) = &p.full {
                    add_anchor(&mut arm, v);
                }
                let lower = p.full.clone().unwrap_or_else(|| padded(&p));
                add_lower(&mut arm, lower, true);
                let upper = bump_upper(&p, kind, *at + 1)?;
                add_upper(&mut arm, upper, false);
            }
            Some(b'*' | b'x' | b'X') | Some(b'0'..=b'9') => {
                let p = parse_partial_operand(token, *at)?;
                add_partial_bare(&mut arm, p, *at)?;
            }
            _ => return Err(syntax(*at)),
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let mut saw_token = false;

    while i < bytes.len() {
        while i < bytes.len() && ws(bytes[i]) {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let token_start = i;
        while i < bytes.len() && !ws(bytes[i]) {
            i += 1;
        }
        let token = &s[token_start..i];
        if token == "||" {
            let arm_tokens = split_tokens(&s[start..token_start], start);
            if arm_tokens.is_empty() {
                return Err(RangeError::Empty { at: token_start });
            }
            arms.push(parse_arm(&arm_tokens)?);
            saw_token = true;
            start = i;
        }
    }

    let tail = split_tokens(&s[start..], start);
    if tail.is_empty() {
        if arms.is_empty() {
            return Err(RangeError::Empty { at: 0 });
        }
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(&tail)?);
    let _ = saw_token;
    Ok(Range { arms })
}

fn admitted(v: &Version, arm: &Arm) -> bool {
    !v.pre.is_empty() && arm.anchors.iter().any(|c| *c == core(v))
}

fn version_matches_arm(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !include_prerelease && !v.pre.is_empty() && !admitted(v, arm) {
        return false;
    }
    if let Some(lower) = &arm.lower {
        let c = compare_versions(v, &lower.version);
        if c == Ordering::Less || (c == Ordering::Equal && !lower.inclusive) {
            return false;
        }
    }
    if let Some(upper) = &arm.upper {
        let c = compare_versions(v, &upper.version);
        if c == Ordering::Greater || (c == Ordering::Equal && !upper.inclusive) {
            return false;
        }
    }
    true
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms
        .iter()
        .any(|arm| version_matches_arm(v, arm, include_prerelease))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn satisfying(
    candidates: &[String],
    r: &Range,
    greatest: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, text) in candidates.iter().enumerate() {
        match parse_version(text) {
            Ok(v) => parsed.push(v),
            Err(error) => return Err(CandidateError { index, error }),
        }
    }

    let mut best: Option<usize> = None;
    for i in 0..parsed.len() {
        if !matches(&parsed[i], r) {
            continue;
        }
        if let Some(old) = best {
            let ord = compare_versions(&parsed[i], &parsed[old]);
            if (greatest && ord == Ordering::Greater)
                || (!greatest && ord == Ordering::Less)
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
    satisfying(candidates, r, true)
}

pub fn min_satisfying(
    candidates: &[String],
    r: &Range,
) -> Result<Option<String>, CandidateError> {
    satisfying(candidates, r, false)
}

fn compare_lower(a: &Option<Bound>, b: &Option<Bound>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(x), Some(y)) => compare_versions(&x.version, &y.version)
            .then_with(|| y.inclusive.cmp(&x.inclusive)),
    }
}

fn compare_upper(a: &Option<Bound>, b: &Option<Bound>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => compare_versions(&x.version, &y.version)
            .then_with(|| x.inclusive.cmp(&y.inclusive)),
    }
}

fn next_core(c: (u64, u64, u64)) -> Option<(u64, u64, u64)> {
    let (a, b, p) = c;
    if p < u64::MAX {
        Some((a, b, p + 1))
    } else if b < u64::MAX {
        Some((a, b + 1, 0))
    } else if a < u64::MAX {
        Some((a + 1, 0, 0))
    } else {
        None
    }
}

fn least_version_above(v: &Version) -> Option<Version> {
    if !v.pre.is_empty() {
        let mut successor = v.clone();
        successor.pre.push("0".to_string());
        successor.build.clear();
        return Some(successor);
    }
    next_core(core(v)).map(|(a, b, p)| zero_pre(version(a, b, p)))
}

fn greatest_version_below(v: &Version) -> Option<Version> {
    if !v.pre.is_empty() {
        return None;
    }
    let (a, b, p) = core(v);
    if p > 0 {
        Some(version(a, b, p - 1))
    } else if b > 0 {
        Some(version(a, b - 1, u64::MAX))
    } else if a > 0 {
        Some(version(a - 1, u64::MAX, u64::MAX))
    } else {
        None
    }
}

fn interval_nonempty(arm: &Arm) -> bool {
    match (&arm.lower, &arm.upper) {
        (Some(l), Some(u)) => {
            let c = compare_versions(&l.version, &u.version);
            if c == Ordering::Greater || (c == Ordering::Equal && !(l.inclusive && u.inclusive)) {
                return false;
            }
            if c == Ordering::Less {
                if l.inclusive && version_matches_arm(&l.version, arm, true) {
                    return true;
                }
                if let Some(w) = least_version_above(&l.version) {
                    return version_matches_arm(&w, arm, true)
                        && compare_versions(&w, &u.version) != Ordering::Greater
                        && (compare_versions(&w, &u.version) != Ordering::Equal || u.inclusive);
                }
                return false;
            }
            l.inclusive && u.inclusive
        }
        _ => true,
    }
}

fn arm_has_default_witness(arm: &Arm) -> bool {
    if !interval_nonempty(arm) {
        return false;
    }
    if interval_contains_stable(arm) {
        return true;
    }
    arm.anchors.iter().any(|&c| {
        let v = Version {
            major: c.0,
            minor: c.1,
            patch: c.2,
            pre: vec!["0".to_string()],
            build: Vec::new(),
        };
        version_matches_arm(&v, arm, true)
    })
}

fn interval_contains_stable(arm: &Arm) -> bool {
    let stable_min = match &arm.lower {
        None => Some(version(0, 0, 0)),
        Some(l) if l.version.pre.is_empty() => {
            if l.inclusive {
                Some(l.version.clone())
            } else {
                next_core(core(&l.version)).map(|(a, b, p)| version(a, b, p))
            }
        }
        Some(l) => {
            let c = core(&l.version);
            if let Some((a, b, p)) = next_core(c) {
                Some(version(a, b, p))
            } else {
                None
            }
        }
    };
    let Some(candidate) = stable_min else {
        return false;
    };
    match &arm.upper {
        None => true,
        Some(u) => {
            let c = compare_versions(&candidate, &u.version);
            c == Ordering::Less || (c == Ordering::Equal && u.inclusive)
        }
    }
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for aa in &a.arms {
        for bb in &b.arms {
            let mut combined = Arm {
                lower: None,
                upper: None,
                anchors: Vec::new(),
            };
            if let Some(l) = &aa.lower {
                add_lower(&mut combined, l.version.clone(), l.inclusive);
            }
            if let Some(l) = &bb.lower {
                add_lower(&mut combined, l.version.clone(), l.inclusive);
            }
            if let Some(u) = &aa.upper {
                add_upper(&mut combined, u.version.clone(), u.inclusive);
            }
            if let Some(u) = &bb.upper {
                add_upper(&mut combined, u.version.clone(), u.inclusive);
            }
            combined.anchors = aa.anchors.clone();
            combined.anchors.extend(bb.anchors.iter().copied());
            if include_prerelease {
                if interval_nonempty(&combined) {
                    return true;
                }
            } else if arm_has_default_witness(&combined) {
                return true;
            }
        }
    }
    false
}

fn display_version(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

fn display_bound(bound: &Option<Bound>, lower: bool) -> String {
    match bound {
        None => "*".to_string(),
        Some(b) => {
            let op = match (lower, b.inclusive) {
                (true, true) => ">=",
                (true, false) => ">",
                (false, true) => "<=",
                (false, false) => "<",
            };
            format!("{}{}", op, display_version(&b.version))
        }
    }
}

fn arm_string(arm: &Arm) -> String {
    let mut anchors = arm.anchors.clone();
    anchors.sort();
    anchors.dedup();
    let anchor_text = if anchors.is_empty() {
        "-".to_string()
    } else {
        anchors
            .iter()
            .map(|(a, b, c)| format!("{}.{}.{}", a, b, c))
            .collect::<Vec<_>>()
            .join(",")
    };
    format!(
        "[{};{};pre={}]",
        display_bound(&arm.lower, true),
        display_bound(&arm.upper, false),
        anchor_text
    )
}

pub fn canonical(r: &Range) -> String {
    let mut strings: Vec<String> = r
        .arms
        .iter()
        .filter(|arm| arm_has_default_witness(arm))
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