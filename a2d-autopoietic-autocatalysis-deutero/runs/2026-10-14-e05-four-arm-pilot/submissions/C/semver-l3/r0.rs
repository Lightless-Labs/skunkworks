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

fn byte(s: &str, i: usize) -> Option<u8> {
    s.as_bytes().get(i).copied()
}

fn parse_core(s: &str, i: &mut usize) -> Result<u64, VersionError> {
    let start = *i;
    while byte(s, *i).is_some_and(|b| b.is_ascii_digit()) {
        *i += 1;
    }
    if *i == start {
        return match byte(s, start) {
            None | Some(b'.' | b'-' | b'+') => Err(VersionError::MissingCore { at: start }),
            _ => Err(VersionError::InvalidChar { at: start }),
        };
    }
    let digits = &s[start..*i];
    if digits.len() > 1 && digits.starts_with('0') {
        return Err(VersionError::LeadingZero { at: start });
    }
    digits.parse().map_err(|_| VersionError::Overflow { at: start })
}

fn parse_ids(s: &str, start: usize, end: usize, pre: bool) -> Result<Vec<String>, VersionError> {
    let mut ids = Vec::new();
    let mut pos = start;
    loop {
        let id_start = pos;
        while pos < end {
            let b = s.as_bytes()[pos];
            if b.is_ascii_alphanumeric() || b == b'-' {
                pos += 1;
            } else if b == b'.' {
                break;
            } else {
                return Err(VersionError::InvalidChar { at: pos });
            }
        }
        if pos == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        let id = &s[id_start..pos];
        if pre && id.bytes().all(|b| b.is_ascii_digit()) && id.len() > 1 && id.starts_with('0') {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        ids.push(id.to_string());
        if pos == end {
            break;
        }
        pos += 1;
        if pos == end {
            return Err(VersionError::EmptyIdentifier { at: pos });
        }
    }
    Ok(ids)
}

/// Parse a semantic version string.
pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let mut i = 0;
    let major = parse_core(s, &mut i)?;
    for _ in 0..2 {
        if byte(s, i) != Some(b'.') {
            return match byte(s, i) {
                None | Some(b'-' | b'+') => Err(VersionError::MissingCore { at: i }),
                _ => Err(VersionError::InvalidChar { at: i }),
            };
        }
        i += 1;
        if _ == 0 {
            // The loop variable is only used to keep parsing order explicit.
        }
    }
    // Reparse with the required ordering, preserving numeric errors at each field.
    let mut i = 0;
    let major = parse_core(s, &mut i)?;
    if byte(s, i) != Some(b'.') {
        return match byte(s, i) {
            None | Some(b'-' | b'+') => Err(VersionError::MissingCore { at: i }),
            _ => Err(VersionError::InvalidChar { at: i }),
        };
    }
    i += 1;
    let minor = parse_core(s, &mut i)?;
    if byte(s, i) != Some(b'.') {
        return match byte(s, i) {
            None | Some(b'-' | b'+') => Err(VersionError::MissingCore { at: i }),
            _ => Err(VersionError::InvalidChar { at: i }),
        };
    }
    i += 1;
    let patch = parse_core(s, &mut i)?;
    let mut pre = Vec::new();
    let mut build = Vec::new();
    if byte(s, i) == Some(b'-') {
        i += 1;
        let end = s[i..].find('+').map_or(s.len(), |n| i + n);
        pre = parse_ids(s, i, end, true)?;
        i = end;
    }
    if byte(s, i) == Some(b'+') {
        i += 1;
        build = parse_ids(s, i, s.len(), false)?;
        i = s.len();
    }
    if i != s.len() {
        return Err(VersionError::InvalidChar { at: i });
    }
    Ok(Version { major, minor, patch, pre, build })
}

fn numeric(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn compare_id(a: &str, b: &str) -> Ordering {
    match (numeric(a), numeric(b)) {
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

/// Compare semantic precedence, ignoring build metadata.
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
enum Op {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug)]
struct Check {
    op: Op,
    version: Version,
}

#[derive(Clone, Debug)]
struct Arm {
    checks: Vec<Check>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

fn ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokenize(s: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        while i < s.len() && ws(s.as_bytes()[i]) {
            i += 1;
        }
        if i == s.len() {
            break;
        }
        let start = i;
        while i < s.len() && !ws(s.as_bytes()[i]) {
            i += 1;
        }
        out.push((start, &s[start..i]));
    }
    out
}

fn padded(v: &Version, k: usize) -> Version {
    Version {
        major: v.major,
        minor: if k >= 2 { v.minor } else { 0 },
        patch: if k >= 3 { v.patch } else { 0 },
        pre: v.pre.clone(),
        build: v.build.clone(),
    }
}

#[derive(Clone)]
struct Partial {
    version: Version,
    k: usize,
    full: bool,
}

fn parse_partial(raw: &str, at: usize) -> Result<Partial, RangeError> {
    if raw.contains('-') || raw.contains('+') ||
        (raw.split('.').count() >= 3 && !raw.split('.').any(|x| matches!(x, "*" | "x" | "X")))
    {
        return parse_version(raw)
            .map(|version| Partial { version, k: 3, full: true })
            .map_err(|error| RangeError::Version { at, error });
    }
    let fields: Vec<&str> = raw.split('.').collect();
    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|x| x.is_empty()) {
        return Err(RangeError::Syntax { at });
    }
    let mut nums = Vec::new();
    let mut wild = false;
    for field in fields {
        if matches!(field, "*" | "x" | "X") {
            wild = true;
        } else {
            if wild || !field.bytes().all(|b| b.is_ascii_digit()) ||
                (field.len() > 1 && field.starts_with('0'))
            {
                return Err(RangeError::Syntax { at });
            }
            let n = field.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
            nums.push(n);
        }
    }
    let mut values = [0u64; 3];
    for (idx, n) in nums.iter().enumerate() {
        values[idx] = *n;
    }
    Ok(Partial {
        version: Version { major: values[0], minor: values[1], patch: values[2], pre: vec![], build: vec![] },
        k: nums.len(),
        full: false,
    })
}

fn upper(v: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let mut result = v.clone();
    let target = match field {
        0 => &mut result.major,
        1 => &mut result.minor,
        _ => &mut result.patch,
    };
    *target = target.checked_add(1).ok_or(RangeError::Syntax { at })?;
    if field == 0 { result.minor = 0; result.patch = 0; }
    if field == 1 { result.patch = 0; }
    result.pre = vec!["0".into()];
    result.build.clear();
    Ok(result)
}

fn apply_partial(p: Partial, op_prefix: &str, at: usize, token_at: usize) -> Result<(Vec<Check>, Vec<(u64,u64,u64)>), RangeError> {
    let mut checks = Vec::new();
    let mut anchors = Vec::new();
    let v = p.version.clone();
    if !p.version.pre.is_empty() {
        anchors.push((v.major, v.minor, v.patch));
    }
    match op_prefix {
        "^" | "~" => {
            if p.k == 0 { return Err(RangeError::Syntax { at: token_at }); }
            let low = padded(&v, p.k);
            checks.push(Check { op: Op::Ge, version: low.clone() });
            let field = if op_prefix == "~" {
                if p.k == 1 { 0 } else { 1 }
            } else if v.major > 0 || p.k == 1 {
                0
            } else if v.minor > 0 || p.k == 2 {
                1
            } else { 2 };
            checks.push(Check { op: Op::Lt, version: upper(&low, field, token_at)? });
        }
        "" => {
            if p.full {
                checks.push(Check { op: Op::Eq, version: v });
            } else if p.k == 1 || p.k == 2 {
                let low = padded(&v, p.k);
                checks.push(Check { op: Op::Ge, version: low.clone() });
                checks.push(Check { op: Op::Lt, version: upper(&low, p.k - 1, token_at)? });
            }
        }
        _ => return Err(RangeError::Syntax { at: token_at }),
    }
    Ok((checks, anchors))
}

fn parse_arm(tokens: &[(usize, &str)]) -> Result<Arm, RangeError> {
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    if let Some((pos, _)) = tokens.iter().find(|(_, t)| *t == &"-") {
        if tokens.len() != 3 || tokens[1].1 != "-" {
            return Err(RangeError::Syntax { at: *pos });
        }
        let (lo_at, lo_raw) = tokens[0];
        let (hi_at, hi_raw) = tokens[2];
        let lo = parse_partial(lo_raw, lo_at)?;
        let hi = parse_partial(hi_raw, hi_at)?;
        if lo.k == 0 { return Err(RangeError::Syntax { at: lo_at }); }
        if hi.k == 0 { return Err(RangeError::Syntax { at: hi_at }); }
        let mut checks = vec![Check { op: Op::Ge, version: padded(&lo.version, lo.k) }];
        if hi.full {
            checks.push(Check { op: Op::Le, version: hi.version.clone() });
        } else if hi.k == 1 || hi.k == 2 {
            checks.push(Check { op: Op::Lt, version: upper(&padded(&hi.version, hi.k), hi.k - 1, hi_at)? });
        }
        let mut anchors = Vec::new();
        for p in [lo, hi] {
            if !p.version.pre.is_empty() {
                anchors.push((p.version.major, p.version.minor, p.version.patch));
            }
        }
        return Ok(Arm { checks, anchors });
    }

    let mut arm = Arm { checks: Vec::new(), anchors: Vec::new() };
    for &(at, token) in tokens {
        let first = token.as_bytes()[0];
        if !first.is_ascii_digit() && !matches!(first, b'*' | b'x' | b'X' | b'^' | b'~' | b'<' | b'>' | b'=') {
            return Err(RangeError::Syntax { at });
        }
        let (kind, raw, operand_at) = if token.starts_with(">=") {
            ("ge", &token[2..], at + 2)
        } else if token.starts_with("<=") {
            ("le", &token[2..], at + 2)
        } else if token.starts_with('>') {
            ("gt", &token[1..], at + 1)
        } else if token.starts_with('<') {
            ("lt", &token[1..], at + 1)
        } else if token.starts_with('=') {
            ("eq", &token[1..], at + 1)
        } else if token.starts_with('^') {
            ("caret", &token[1..], at + 1)
        } else if token.starts_with('~') {
            ("tilde", &token[1..], at + 1)
        } else {
            ("bare", token, at)
        };
        if raw.is_empty() || !raw.as_bytes()[0].is_ascii_digit() && !matches!(raw.as_bytes()[0], b'*' | b'x' | b'X') {
            return Err(RangeError::Syntax { at });
        }
        match kind {
            "ge" | "le" | "gt" | "lt" | "eq" => {
                let version = parse_version(raw).map_err(|error| RangeError::Version { at: operand_at, error })?;
                if !version.pre.is_empty() { arm.anchors.push((version.major, version.minor, version.patch)); }
                let op = match kind { "ge" => Op::Ge, "le" => Op::Le, "gt" => Op::Gt, "lt" => Op::Lt, _ => Op::Eq };
                arm.checks.push(Check { op, version });
            }
            "bare" | "caret" | "tilde" => {
                let p = parse_partial(raw, operand_at)?;
                let (checks, anchors) = apply_partial(p, if kind == "caret" { "^" } else if kind == "tilde" { "~" } else { "" }, operand_at, at)?;
                arm.checks.extend(checks);
                arm.anchors.extend(anchors);
            }
            _ => unreachable!(),
        }
    }
    Ok(arm)
}

/// Parse a whitespace-separated range expression.
pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let tokens = tokenize(s);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    let mut arms = Vec::new();
    let mut current = Vec::new();
    let mut expect_arm = false;
    for token in tokens {
        if token.1 == "||" {
            if current.is_empty() {
                return Err(RangeError::Empty { at: token.0 });
            }
            arms.push(parse_arm(&current)?);
            current.clear();
            expect_arm = true;
        } else {
            current.push(token);
            expect_arm = false;
        }
    }
    if current.is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(&current)?);
    let _ = expect_arm;
    Ok(Range { arms })
}

fn check_one(v: &Version, c: &Check) -> bool {
    let ord = compare_versions(v, &c.version);
    match c.op {
        Op::Eq => ord == Ordering::Equal,
        Op::Lt => ord == Ordering::Less,
        Op::Le => ord != Ordering::Greater,
        Op::Gt => ord == Ordering::Greater,
        Op::Ge => ord != Ordering::Less,
    }
}

fn arm_matches(v: &Version, arm: &Arm, include: bool) -> bool {
    arm.checks.iter().all(|c| check_one(v, c)) &&
        (include || v.pre.is_empty() || arm.anchors.contains(&(v.major, v.minor, v.patch)))
}

/// Test whether a version matches a range using default prerelease admission.
pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

/// Test whether a version matches a range, optionally admitting all prereleases.
pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|arm| arm_matches(v, arm, include_prerelease))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn satisfying(candidates: &[String], r: &Range, greatest: bool) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, s) in candidates.iter().enumerate() {
        parsed.push(parse_version(s).map_err(|error| CandidateError { index, error })?);
    }
    let mut best: Option<usize> = None;
    for (i, v) in parsed.iter().enumerate() {
        if !matches(v, r) { continue; }
        if let Some(j) = best {
            let c = compare_versions(v, &parsed[j]);
            if (greatest && c == Ordering::Greater) || (!greatest && c == Ordering::Less) {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

/// Return the greatest matching candidate, after validating every candidate.
pub fn max_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    satisfying(candidates, r, true)
}

/// Return the least matching candidate, after validating every candidate.
pub fn min_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    satisfying(candidates, r, false)
}

fn tuple_version(t: (u64, u64, u64)) -> Version {
    Version { major: t.0, minor: t.1, patch: t.2, pre: vec![], build: vec![] }
}

/// Determine whether two ranges have at least one common stable or admitted prerelease version.
pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    let mut candidates = Vec::new();
    for x in 0..=1u64 {
        for y in 0..=1u64 {
            for z in 0..=1u64 {
                candidates.push(tuple_version((x, y, z)));
            }
        }
    }
    for aa in &a.arms {
        for bb in &b.arms {
            let mut anchors = aa.anchors.clone();
            anchors.extend(bb.anchors.iter().copied());
            for t in anchors {
                candidates.push(Version { major: t.0, minor: t.1, patch: t.2, pre: vec!["0".into()], build: vec![] });
            }
        }
    }
    candidates.iter().any(|v| {
        a.arms.iter().any(|arm| arm_matches(v, arm, include_prerelease)) &&
        b.arms.iter().any(|arm| arm_matches(v, arm, include_prerelease))
    })
}

fn version_text(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() { s.push('-'); s.push_str(&v.pre.join(".")); }
    s
}

/// Serialize a display summary of a range.
pub fn canonical(r: &Range) -> String {
    if r.arms.is_empty() { return "EMPTY".into(); }
    let mut out = Vec::new();
    for arm in &r.arms {
        let mut lower: Option<(Version, bool)> = None;
        let mut upper: Option<(Version, bool)> = None;
        let mut impossible = false;
        for c in &arm.checks {
            let strict = matches!(c.op, Op::Gt | Op::Lt);
            let v = c.version.clone();
            match c.op {
                Op::Eq => {
                    lower = Some((v.clone(), false));
                    upper = Some((v, false));
                }
                Op::Ge | Op::Gt => {
                    if lower.as_ref().is_none_or(|(old, old_strict)| {
                        compare_versions(&v, old) == Ordering::Greater ||
                            (compare_versions(&v, old) == Ordering::Equal && strict && !*old_strict)
                    }) { lower = Some((v, strict)); }
                }
                Op::Le | Op::Lt => {
                    if upper.as_ref().is_none_or(|(old, old_strict)| {
                        compare_versions(&v, old) == Ordering::Less ||
                            (compare_versions(&v, old) == Ordering::Equal && strict && !*old_strict)
                    }) { upper = Some((v, strict)); }
                }
            }
        }
        if let (Some((l, ls)), Some((u, us))) = (&lower, &upper) {
            let ord = compare_versions(l, u);
            if ord == Ordering::Greater || (ord == Ordering::Equal && (*ls || *us)) {
                impossible = true;
            }
        }
        if impossible { continue; }
        let lo = lower.map_or("*".into(), |(v, strict)| format!("{}{}", if strict { ">" } else { ">=" }, version_text(&v)));
        let hi = upper.map_or("*".into(), |(v, strict)| format!("{}{}", if strict { "<" } else { "<=" }, version_text(&v)));
        let mut anchors = arm.anchors.clone();
        anchors.sort_unstable();
        anchors.dedup();
        let anchor_text = if anchors.is_empty() { "-".into() } else {
            anchors.iter().map(|t| format!("{}.{}.{}", t.0, t.1, t.2)).collect::<Vec<_>>().join(",")
        };
        out.push(format!("[{};{};pre={}]", lo, hi, anchor_text));
    }
    out.sort();
    out.dedup();
    if out.is_empty() { "EMPTY".into() } else { out.join(" || ") }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn pv(s: &str) -> Version { parse_version(s).unwrap() }
    fn pr(s: &str) -> Range { parse_range(s).unwrap() }

    #[test]
    fn parses_and_orders_versions() {
        let v = pv("2.1.3-rc.1+build.7");
        assert_eq!((v.major, v.minor, v.patch), (2, 1, 3));
        assert_eq!(compare_versions(&pv("2.1.3-alpha"), &pv("2.1.3-beta")), Ordering::Less);
    }

    #[test]
    fn rejects_invalid_versions() {
        assert_eq!(parse_version("01.0.0"), Err(VersionError::LeadingZero { at: 0 }));
        assert_eq!(parse_version("1.0"), Err(VersionError::MissingCore { at: 3 }));
    }

    #[test]
    fn range_and_extrema_end_to_end() {
        let r = pr("2.x");
        assert!(matches(&pv("2.4.0"), &r));
        let candidates = vec!["2.2.0".into(), "2.8.0".into(), "3.0.0".into()];
        assert_eq!(max_satisfying(&candidates, &r).unwrap(), Some("2.8.0".into()));
    }
}