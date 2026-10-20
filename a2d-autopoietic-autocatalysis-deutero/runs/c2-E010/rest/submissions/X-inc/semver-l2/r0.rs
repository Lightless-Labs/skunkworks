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

fn missing_or_invalid(s: &str, at: usize) -> VersionError {
    match byte(s, at) {
        None | Some(b'.' | b'-' | b'+') => VersionError::MissingCore { at },
        _ => VersionError::InvalidChar { at },
    }
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let mut pos = 0usize;
    let mut nums = [0u64; 3];

    for field in 0..3 {
        let start = pos;
        while matches!(byte(s, pos), Some(b'0'..=b'9')) {
            pos += 1;
        }
        if pos == start {
            return Err(missing_or_invalid(s, start));
        }

        let digits = &s[start..pos];
        if digits.len() > 1 && digits.as_bytes()[0] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }
        nums[field] = digits
            .parse::<u64>()
            .map_err(|_| VersionError::Overflow { at: start })?;

        if field < 2 {
            match byte(s, pos) {
                Some(b'.') => pos += 1,
                _ => return Err(missing_or_invalid(s, pos)),
            }
        }
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if let Some(b'-') = byte(s, pos) {
        pos += 1;
        let pre_start = pos;
        let plus = s[pos..].find('+').map(|n| pos + n).unwrap_or(s.len());
        parse_identifiers(s, pre_start, plus, true, &mut pre)?;
        pos = plus;
    }

    if let Some(b'+') = byte(s, pos) {
        pos += 1;
        parse_identifiers(s, pos, s.len(), false, &mut build)?;
        pos = s.len();
    }

    if pos != s.len() {
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
    prerelease: bool,
    out: &mut Vec<String>,
) -> Result<(), VersionError> {
    if start == end {
        return Err(VersionError::EmptyIdentifier { at: start });
    }
    let mut id_start = start;
    loop {
        let id_end = s[id_start..end]
            .find('.')
            .map(|n| id_start + n)
            .unwrap_or(end);
        if id_start == id_end {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        let id = &s[id_start..id_end];
        for (offset, b) in id.bytes().enumerate() {
            if !matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-') {
                return Err(VersionError::InvalidChar {
                    at: id_start + offset,
                });
            }
        }
        if prerelease
            && id.bytes().all(|b| b.is_ascii_digit())
            && id.len() > 1
            && id.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        out.push(id.to_string());
        if id_end == end {
            break;
        }
        id_start = id_end + 1;
        if id_start == end {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
    }
    Ok(())
}

fn numeric_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn compare_id(a: &str, b: &str) -> Ordering {
    let an = numeric_id(a);
    let bn = numeric_id(b);
    match (an, bn) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())),
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
                    let ord = compare_id(x, y);
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
    arms: Vec<Vec<Predicate>>,
}

#[derive(Clone, Debug)]
struct Predicate {
    op: Op,
    version: Version,
    anchor: bool,
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

fn whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokens(s: &str, base: usize) -> Vec<(String, usize)> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && whitespace(bytes[i]) {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !whitespace(bytes[i]) {
            i += 1;
        }
        out.push((s[start..i].to_string(), base + start));
    }
    out
}

fn cmp_pred(op: Op, candidate: &Version, bound: &Version) -> bool {
    match op {
        Op::Eq => compare_versions(candidate, bound) == Ordering::Equal,
        Op::Lt => compare_versions(candidate, bound) == Ordering::Less,
        Op::Le => compare_versions(candidate, bound) != Ordering::Greater,
        Op::Gt => compare_versions(candidate, bound) == Ordering::Greater,
        Op::Ge => compare_versions(candidate, bound) != Ordering::Less,
    }
}

fn add_pred(out: &mut Vec<Predicate>, op: Op, version: Version, anchor: bool) {
    out.push(Predicate { op, version, anchor });
}

fn full_at(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

#[derive(Clone)]
struct Partial {
    nums: Vec<u64>,
    full: Option<Version>,
    k: usize,
}

fn parse_partial(s: &str, at: usize) -> Result<Partial, RangeError> {
    let is_full = s.contains(['-', '+'])
        || (s.split('.').count() >= 3
            && !s.split('.').any(|f| matches!(f, "*" | "x" | "X")));
    if is_full {
        return Ok(Partial {
            nums: Vec::new(),
            full: Some(full_at(s, at)?),
            k: 3,
        });
    }
    let fields: Vec<&str> = s.split('.').collect();
    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|f| f.is_empty()) {
        return Err(RangeError::Syntax { at });
    }
    let mut nums = Vec::new();
    let mut wildcard = false;
    for field in fields {
        if matches!(field, "*" | "x" | "X") {
            wildcard = true;
        } else {
            if wildcard
                || !field.bytes().all(|b| b.is_ascii_digit())
                || (field.len() > 1 && field.as_bytes()[0] == b'0')
            {
                return Err(RangeError::Syntax { at });
            }
            let n = field.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
            nums.push(n);
        }
    }
    Ok(Partial {
        k: nums.len(),
        nums,
        full: None,
    })
}

fn padded(p: &Partial) -> Version {
    if let Some(v) = &p.full {
        return v.clone();
    }
    let mut n = [0u64; 3];
    for (i, x) in p.nums.iter().enumerate() {
        n[i] = *x;
    }
    Version {
        major: n[0],
        minor: n[1],
        patch: n[2],
        pre: Vec::new(),
        build: Vec::new(),
    }
}

fn bump(v: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let mut out = v.clone();
    let slot = match field {
        0 => &mut out.major,
        1 => &mut out.minor,
        _ => &mut out.patch,
    };
    *slot = slot.checked_add(1).ok_or(RangeError::Syntax { at })?;
    if field <= 1 {
        out.minor = if field == 0 { 0 } else { out.minor };
    }
    if field <= 2 {
        out.patch = if field <= 1 { 0 } else { out.patch };
    }
    out.pre = vec!["0".to_string()];
    out.build.clear();
    Ok(out)
}

fn add_partial_bare(out: &mut Vec<Predicate>, p: Partial, at: usize) -> Result<(), RangeError> {
    if let Some(v) = p.full {
        add_pred(out, Op::Eq, v.clone(), !v.pre.is_empty());
    } else if p.k == 1 || p.k == 2 {
        let low = padded(&p);
        let high = bump(&low, p.k - 1, at)?;
        add_pred(out, Op::Ge, low, false);
        add_pred(out, Op::Lt, high, false);
    }
    Ok(())
}

fn process_arm(ts: &[(String, usize)]) -> Result<Vec<Predicate>, RangeError> {
    let mut out = Vec::new();
    let hyphens: Vec<usize> = ts.iter().enumerate().filter(|(_, t)| t.0 == "-").map(|(i, _)| i).collect();
    if !hyphens.is_empty() {
        let h = hyphens[0];
        if h != 1 || ts.len() != 3 {
            return Err(RangeError::Syntax { at: ts[h].1 });
        }
        let (ls, la) = (&ts[0].0, ts[0].1);
        let (us, ua) = (&ts[2].0, ts[2].1);
        let lp = parse_partial(ls, la)?;
        let up = parse_partial(us, ua)?;
        if lp.k == 0 {
            return Err(RangeError::Syntax { at: la });
        }
        if up.k == 0 {
            return Err(RangeError::Syntax { at: ua });
        }
        let low = padded(&lp);
        add_pred(&mut out, Op::Ge, low, lp.full.as_ref().is_some_and(|v| !v.pre.is_empty()));
        if let Some(v) = up.full {
            add_pred(&mut out, Op::Le, v.clone(), !v.pre.is_empty());
        } else {
            let upper = if up.k == 1 || up.k == 2 {
                bump(&padded(&up), up.k - 1, ua)?
            } else {
                padded(&up)
            };
            if up.k < 3 {
                add_pred(&mut out, Op::Lt, upper, false);
            } else {
                add_pred(&mut out, Op::Le, upper, false);
            }
        }
        return Ok(out);
    }

    for (token, start) in ts {
        let b = token.as_bytes();
        if b.is_empty() {
            return Err(RangeError::Syntax { at: *start });
        }
        if token == "||" {
            return Err(RangeError::Syntax { at: *start });
        }
        if matches!(b[0], b'<' | b'>' | b'=') {
            let (op, n) = if token.starts_with("<=") {
                (Op::Le, 2)
            } else if token.starts_with(">=") {
                (Op::Ge, 2)
            } else {
                (match b[0] {
                    b'<' => Op::Lt,
                    b'>' => Op::Gt,
                    _ => Op::Eq,
                }, 1)
            };
            let operand = &token[n..];
            if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
                return Err(RangeError::Syntax { at: *start });
            }
            let v = full_at(operand, *start + n)?;
            add_pred(&mut out, op, v.clone(), !v.pre.is_empty());
            continue;
        }
        if matches!(b[0], b'^' | b'~') {
            let caret = b[0] == b'^';
            let operand = &token[1..];
            let at = *start + 1;
            let p = parse_partial(operand, at)?;
            if p.k == 0 {
                return Err(RangeError::Syntax { at: *start });
            }
            let low = padded(&p);
            let upper_field = if caret {
                if low.major > 0 || p.k == 1 { 0 }
                else if low.minor > 0 || p.k == 2 { 1 }
                else { 2 }
            } else if p.k == 1 { 0 } else { 1 };
            let high = bump(&low, upper_field, at)?;
            add_pred(&mut out, Op::Ge, low.clone(), p.full.as_ref().is_some_and(|v| !v.pre.is_empty()));
            add_pred(&mut out, Op::Lt, high, false);
            continue;
        }
        if !b[0].is_ascii_digit() && !matches!(b[0], b'*' | b'x' | b'X') {
            return Err(RangeError::Syntax { at: *start });
        }
        let p = parse_partial(token, *start)?;
        add_partial_bare(&mut out, p, *start)?;
    }
    Ok(out)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let ts = tokens(s, 0);
    if ts.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    let mut arms: Vec<Vec<Predicate>> = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < ts.len() {
        if ts[i].0 == "||" {
            if i == start {
                return Err(RangeError::Empty { at: ts[i].1 });
            }
            arms.push(process_arm(&ts[start..i])?);
            start = i + 1;
        }
        i += 1;
    }
    if start == ts.len() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(process_arm(&ts[start..])?);
    Ok(Range { arms })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        if !arm.iter().all(|p| cmp_pred(p.op, v, &p.version)) {
            return false;
        }
        if v.pre.is_empty() {
            return true;
        }
        arm.iter().any(|p| {
            p.anchor
                && (v.major, v.minor, v.patch)
                    == (p.version.major, p.version.minor, p.version.patch)
        })
    })
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