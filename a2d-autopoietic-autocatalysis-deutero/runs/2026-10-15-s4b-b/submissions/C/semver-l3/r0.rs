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

fn parse_core_num(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if *pos == start {
        if *pos == bytes.len() || matches!(bytes[*pos], b'.' | b'-' | b'+') {
            return Err(VersionError::MissingCore { at: start });
        }
        return Err(VersionError::InvalidChar { at: start });
    }
    if *pos - start > 1 && bytes[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    let mut value = 0u64;
    for &b in &bytes[start..*pos] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u64))
            .ok_or(VersionError::Overflow { at: start })?;
    }
    Ok(value)
}

fn parse_ids(
    s: &str,
    base: usize,
    numeric_pre: bool,
    stop_plus: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    loop {
        while i < bytes.len() && bytes[i] != b'.' && !(stop_plus && bytes[i] == b'+') {
            if !bytes[i].is_ascii_alphanumeric() && bytes[i] != b'-' {
                return Err(VersionError::InvalidChar { at: base + i });
            }
            i += 1;
        }
        if i == start {
            return Err(VersionError::EmptyIdentifier { at: base + start });
        }
        let ident = &s[start..i];
        if numeric_pre
            && ident.len() > 1
            && ident.bytes().all(|b| b.is_ascii_digit())
            && ident.starts_with('0')
        {
            return Err(VersionError::LeadingZero { at: base + start });
        }
        out.push(ident.to_string());
        if i == bytes.len() || (stop_plus && bytes[i] == b'+') {
            break;
        }
        i += 1;
        start = i;
        if i == bytes.len() {
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
    let major = parse_core_num(s, &mut pos)?;
    if pos >= bytes.len() || bytes[pos] != b'.' {
        if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;
    let minor = parse_core_num(s, &mut pos)?;
    if pos >= bytes.len() || bytes[pos] != b'.' {
        if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;
    let patch = parse_core_num(s, &mut pos)?;
    if pos < bytes.len() && !matches!(bytes[pos], b'-' | b'+') {
        return Err(VersionError::InvalidChar { at: pos });
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();
    if pos < bytes.len() && bytes[pos] == b'-' {
        let start = pos + 1;
        let tail = &s[start..];
        pre = parse_ids(tail, start, true, true)?;
        pos = start + tail.find('+').unwrap_or(tail.len());
    }
    if pos < bytes.len() && bytes[pos] == b'+' {
        let start = pos + 1;
        build = parse_ids(&s[start..], start, false, false)?;
    }
    Ok(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn core_cmp(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch))
}

fn numeric(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn cmp_id(a: &str, b: &str) -> Ordering {
    match (numeric(a), numeric(b)) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    let c = core_cmp(a, b);
    if c != Ordering::Equal {
        return c;
    }
    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (x, y) in a.pre.iter().zip(&b.pre) {
                let c = cmp_id(x, y);
                if c != Ordering::Equal {
                    return c;
                }
            }
            a.pre.len().cmp(&b.pre.len())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Range {
    arms: Vec<Arm>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Cmp {
    op: Op,
    v: Version,
    explicit: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Arm {
    cmps: Vec<Cmp>,
    anchors: Vec<(u64, u64, u64)>,
}

fn ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn version_at(at: usize, operand: &str) -> Result<Version, RangeError> {
    parse_version(operand).map_err(|error| RangeError::Version { at, error })
}

fn partial(start: usize, op: &str) -> Result<(Version, usize, bool), RangeError> {
    let fullish = op.contains('-') || op.contains('+');
    let fields: Vec<&str> = op.split('.').collect();
    let has_wild = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));
    if fullish || (fields.len() >= 3 && !has_wild) {
        return Ok((version_at(start, op)?, 3, true));
    }
    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|f| f.is_empty()) {
        return Err(RangeError::Syntax { at: start });
    }
    let mut nums = Vec::new();
    let mut wildcard_seen = false;
    for f in fields {
        if matches!(f, "*" | "x" | "X") {
            wildcard_seen = true;
            continue;
        }
        if wildcard_seen
            || !f.bytes().all(|b| b.is_ascii_digit())
            || (f.len() > 1 && f.starts_with('0'))
        {
            return Err(RangeError::Syntax { at: start });
        }
        let Some(n) = f.parse::<u64>().ok() else {
            return Err(RangeError::Syntax { at: start });
        };
        nums.push(n);
    }
    let k = nums.len();
    let mut core = [0u64; 3];
    for (i, n) in nums.into_iter().enumerate() {
        core[i] = n;
    }
    Ok((
        Version {
            major: core[0],
            minor: core[1],
            patch: core[2],
            pre: vec![],
            build: vec![],
        },
        k,
        false,
    ))
}

fn endpoint_upper(v: &Version, k: usize, mode: u8, at: usize) -> Result<Version, RangeError> {
    let (idx, value) = match mode {
        0 if k == 1 => (0, v.major),
        0 if k == 2 => (1, v.minor),
        1 if v.major > 0 || k == 1 => (0, v.major),
        1 if v.minor > 0 || k == 2 => (1, v.minor),
        1 => (2, v.patch),
        2 if k == 1 => (0, v.major),
        _ => (1, v.minor),
    };
    let next = value.checked_add(1).ok_or(RangeError::Syntax { at })?;
    let mut out = Version {
        major: 0,
        minor: 0,
        patch: 0,
        pre: vec!["0".into()],
        build: vec![],
    };
    match idx {
        0 => out.major = next,
        1 => {
            out.major = v.major;
            out.minor = next;
        }
        _ => {
            out.major = v.major;
            out.minor = v.minor;
            out.patch = next;
        }
    }
    Ok(out)
}

fn add_cmp(arm: &mut Arm, op: Op, v: Version, explicit: bool) {
    if explicit && !v.pre.is_empty() {
        arm.anchors.push((v.major, v.minor, v.patch));
    }
    arm.cmps.push(Cmp { op, v, explicit });
}

fn token_cmp(v: &Version, c: &Cmp) -> bool {
    match c.op {
        Op::Lt => compare_versions(v, &c.v) == Ordering::Less,
        Op::Le => compare_versions(v, &c.v) != Ordering::Greater,
        Op::Eq => compare_versions(v, &c.v) == Ordering::Equal,
        Op::Ge => compare_versions(v, &c.v) != Ordering::Less,
        Op::Gt => compare_versions(v, &c.v) == Ordering::Greater,
    }
}

fn parse_arm(tokens: &[(usize, &str)]) -> Result<Arm, RangeError> {
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    let hyphen = tokens.iter().position(|(_, t)| *t == "-");
    if let Some(h) = hyphen {
        if tokens.len() != 3 || h != 1 {
            return Err(RangeError::Syntax { at: tokens[h].0 });
        }
        let (lv, lk, _) = partial(tokens[0].0, tokens[0].1)?;
        let (uv, uk, uf) = partial(tokens[2].0, tokens[2].1)?;
        if lk == 0 {
            return Err(RangeError::Syntax { at: tokens[0].0 });
        }
        if uk == 0 {
            return Err(RangeError::Syntax { at: tokens[2].0 });
        }
        let mut arm = Arm { cmps: vec![], anchors: vec![] };
        add_cmp(&mut arm, Op::Ge, lv, true);
        if uf {
            add_cmp(&mut arm, Op::Le, uv, true);
        } else {
            add_cmp(&mut arm, Op::Lt, endpoint_upper(&uv, uk, 0, tokens[2].0)?, false);
        }
        Ok(arm)
    } else {
        let mut arm = Arm { cmps: vec![], anchors: vec![] };
        for &(at, tok) in tokens {
            let bytes = tok.as_bytes();
            if tok.starts_with(">=") || tok.starts_with("<=") || tok.starts_with('>') ||
                tok.starts_with('<') || tok.starts_with('=')
            {
                let (op_len, op) = if tok.starts_with(">=") {
                    (2, Op::Ge)
                } else if tok.starts_with("<=") {
                    (2, Op::Le)
                } else if tok.starts_with('>') {
                    (1, Op::Gt)
                } else if tok.starts_with('<') {
                    (1, Op::Lt)
                } else {
                    (1, Op::Eq)
                };
                let operand = &tok[op_len..];
                if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
                    return Err(RangeError::Syntax { at });
                }
                add_cmp(&mut arm, op, version_at(at + op_len, operand)?, true);
            } else if tok.starts_with('^') || tok.starts_with('~') {
                let operand = &tok[1..];
                if operand.is_empty() {
                    return Err(RangeError::Syntax { at });
                }
                let (v, k, _) = partial(at + 1, operand)?;
                if k == 0 {
                    return Err(RangeError::Syntax { at });
                }
                add_cmp(&mut arm, Op::Ge, v.clone(), true);
                let mode = if tok.starts_with('^') { 1 } else { 2 };
                add_cmp(&mut arm, Op::Lt, endpoint_upper(&v, k, mode, at + 1)?, false);
            } else if tok.starts_with(|c: char| c.is_ascii_digit())
                || matches!(bytes.first(), Some(b'*' | b'x' | b'X'))
            {
                let (v, k, full) = partial(at, tok)?;
                if full {
                    add_cmp(&mut arm, Op::Eq, v, true);
                } else if k != 0 && k < 3 {
                    add_cmp(&mut arm, Op::Ge, v.clone(), true);
                    add_cmp(&mut arm, Op::Lt, endpoint_upper(&v, k, 0, at)?, false);
                } else if k == 3 {
                    add_cmp(&mut arm, Op::Eq, v, true);
                }
            } else {
                return Err(RangeError::Syntax { at });
            }
        }
        Ok(arm)
    }
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let b = s.as_bytes();
    let mut arms = Vec::new();
    let mut pos = 0;
    let mut tokens = Vec::<(usize, &str)>::new();

    while pos < b.len() {
        while pos < b.len() && ws(b[pos]) {
            pos += 1;
        }
        if pos == b.len() {
            break;
        }
        if pos + 1 < b.len() && &b[pos..pos + 2] == b"||" {
            if tokens.is_empty() {
                return Err(RangeError::Empty { at: pos });
            }
            arms.push(parse_arm(&tokens)?);
            tokens.clear();
            pos += 2;
            while pos < b.len() && ws(b[pos]) {
                pos += 1;
            }
            if pos == b.len() {
                return Err(RangeError::Empty { at: b.len() });
            }
            continue;
        }
        let start = pos;
        while pos < b.len() && !ws(b[pos]) {
            pos += 1;
        }
        tokens.push((start, &s[start..pos]));
    }

    if tokens.is_empty() {
        if arms.is_empty() {
            return Err(RangeError::Empty { at: 0 });
        }
        return Err(RangeError::Empty { at: b.len() });
    }
    arms.push(parse_arm(&tokens)?);
    Ok(Range { arms })
}

fn arm_admits(v: &Version, a: &Arm) -> bool {
    v.pre.is_empty() || a.anchors.contains(&(v.major, v.minor, v.patch))
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|a| {
        (include_prerelease || arm_admits(v, a)) && a.cmps.iter().all(|c| token_cmp(v, c))
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

pub fn max_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, true)
}

pub fn min_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, false)
}

fn extrema(candidates: &[String], r: &Range, max: bool) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::new();
    for (index, s) in candidates.iter().enumerate() {
        let v = parse_version(s).map_err(|error| CandidateError { index, error })?;
        parsed.push(v);
    }
    let mut best: Option<usize> = None;
    for (i, v) in parsed.iter().enumerate() {
        if !matches(v, r) {
            continue;
        }
        if let Some(j) = best {
            let c = compare_versions(v, &parsed[j]);
            if (max && c == Ordering::Greater) || (!max && c == Ordering::Less) {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

fn cmp_tuple(a: (u64, u64, u64), b: (u64, u64, u64)) -> Ordering {
    a.cmp(&b)
}

fn arm_bounds(
    a: &Arm,
) -> (
    Option<(Version, bool)>,
    Option<(Version, bool)>,
    Vec<(u64, u64, u64)>,
) {
    let mut lo: Option<(Version, bool)> = None;
    let mut hi: Option<(Version, bool)> = None;
    for c in &a.cmps {
        let lower = matches!(c.op, Op::Ge | Op::Gt | Op::Eq);
        let slot = if lower { &mut lo } else { &mut hi };
        let inclusive = matches!(c.op, Op::Ge | Op::Le | Op::Eq);
        let mut v = c.v.clone();
        v.build.clear();
        let replace = match slot.as_ref() {
            None => true,
            Some((old, old_inc)) => {
                let ord = compare_versions(&v, old);
                if lower {
                    ord == Ordering::Greater || (ord == Ordering::Equal && !inclusive && *old_inc)
                } else {
                    ord == Ordering::Less || (ord == Ordering::Equal && !inclusive && *old_inc)
                }
            }
        };
        if replace {
            *slot = Some((v, inclusive));
        }
    }
    if let Some(c) = a.cmps.iter().find(|c| c.op == Op::Eq) {
        let mut v = c.v.clone();
        v.build.clear();
        lo = Some((v.clone(), true));
        hi = Some((v, true));
    }
    (lo, hi, a.anchors.clone())
}

fn lower_ok(v: &Version, bound: &Option<(Version, bool)>) -> bool {
    bound.as_ref().map_or(true, |(x, inc)| {
        let c = compare_versions(v, x);
        c == Ordering::Greater || (*inc && c == Ordering::Equal)
    })
}

fn upper_ok(v: &Version, bound: &Option<(Version, bool)>) -> bool {
    bound.as_ref().map_or(true, |(x, inc)| {
        let c = compare_versions(v, x);
        c == Ordering::Less || (*inc && c == Ordering::Equal)
    })
}

fn arm_has_witness(a: &Arm, include: bool) -> bool {
    let (lo, hi, anchors) = arm_bounds(a);
    if let (Some((l, li)), Some((h, ui))) = (&lo, &hi) {
        let c = compare_versions(l, h);
        if c == Ordering::Greater || (c == Ordering::Equal && (!li || !ui)) {
            return false;
        }
    }
    if include {
        return true;
    }
    if lo.as_ref().is_some_and(|(v, _)| !v.pre.is_empty())
        || hi.as_ref().is_some_and(|(v, _)| !v.pre.is_empty())
    {
        if !anchors.iter().any(|t| {
            let v = Version {
                major: t.0,
                minor: t.1,
                patch: t.2,
                pre: vec![],
                build: vec![],
            };
            lower_ok(&v, &lo) && upper_ok(&v, &hi)
        }) {
            return false;
        }
    }
    true
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for x in &a.arms {
        for y in &b.arms {
            let mut both = x.clone();
            both.cmps.extend(y.cmps.clone());
            both.anchors.retain(|t| y.anchors.contains(t));
            if arm_has_witness(&both, include_prerelease) {
                return true;
            }
        }
    }
    false
}

pub fn canonical(r: &Range) -> String {
    let mut out = Vec::new();
    for a in &r.arms {
        if !arm_has_witness(a, false) {
            continue;
        }
        let (lo, hi, mut anchors) = arm_bounds(a);
        anchors.sort_by(|x, y| cmp_tuple(*x, *y));
        let anchor_text = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors
                .iter()
                .map(|t| format!("{}.{}.{}", t.0, t.1, t.2))
                .collect::<Vec<_>>()
                .join(",")
        };
        let bound = |b: Option<(Version, bool)>, lower: bool| -> String {
            match b {
                None => "*".into(),
                Some((v, inc)) => {
                    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
                    if !v.pre.is_empty() {
                        s.push('-');
                        s.push_str(&v.pre.join("."));
                    }
                    let op = if lower {
                        if inc { ">=" } else { ">" }
                    } else if inc {
                        "<="
                    } else {
                        "<"
                    };
                    format!("{}{}", op, s)
                }
            }
        };
        out.push(format!(
            "[{};{};pre={}]",
            bound(lo, true),
            bound(hi, false),
            anchor_text
        ));
    }
    out.sort();
    out.dedup();
    if out.is_empty() {
        "EMPTY".into()
    } else {
        out.join(" || ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pv(s: &str) -> Version {
        parse_version(s).unwrap()
    }

    fn pr(s: &str) -> Range {
        parse_range(s).unwrap()
    }

    #[test]
    fn version_errors_and_precedence() {
        assert_eq!(parse_version(""), Err(VersionError::Empty));
        assert_eq!(
            parse_version("01.0.0"),
            Err(VersionError::LeadingZero { at: 0 })
        );
        assert_eq!(
            parse_version("1.0"),
            Err(VersionError::MissingCore { at: 3 })
        );
        assert_eq!(
            compare_versions(&pv("1.0.0-alpha.2"), &pv("1.0.0-alpha.10")),
            Ordering::Less
        );
        assert_eq!(
            compare_versions(&pv("1.0.0+one"), &pv("1.0.0+two")),
            Ordering::Equal
        );
    }

    #[test]
    fn range_matching_and_admission() {
        assert!(matches(&pv("2.5.1"), &pr("^2.1")));
        assert!(!matches(&pv("2.1.3-rc"), &pr(">=2.1.0 <3.0.0")));
        assert!(matches(&pv("2.1.3-rc"), &pr(">=2.1.3-beta <2.1.3")));
        assert!(matches(&pv("4.0.0"), &pr("2.x || 4.0.0")));
        assert!(matches_with(&pv("2.5.0-rc"), &pr("2.x"), true));
    }

    #[test]
    fn range_errors_and_expansion_boundaries() {
        assert_eq!(
            parse_range("|| 1.0.0"),
            Err(RangeError::Empty { at: 0 })
        );
        assert_eq!(
            parse_range("^18446744073709551615"),
            Err(RangeError::Syntax { at: 1 })
        );
        assert!(matches(&pv("0.0.4"), &pr("^0.0.3")));
        assert!(!matches(&pv("0.0.4"), &pr("~0.0.3")));
    }

    #[test]
    fn extrema_and_algebra() {
        let candidates = vec![
            "2.2.0".to_string(),
            "2.8.0".to_string(),
            "3.0.0".to_string(),
        ];
        let r = pr("2.x");
        assert_eq!(max_satisfying(&candidates, &r), Ok(Some("2.8.0".into())));
        assert_eq!(min_satisfying(&candidates, &r), Ok(Some("2.2.0".into())));
        assert!(intersects(&pr(">=2.1.0 <2.8.0"), &pr("2.5.x"), false));
        assert_eq!(canonical(&pr(">=2.1.0 <2.8.0")), "[>=2.1.0;<2.8.0;pre=-]");
        assert_eq!(canonical(&pr(">=2.0.0 <2.0.0")), "EMPTY");
    }
}

fn main() {}