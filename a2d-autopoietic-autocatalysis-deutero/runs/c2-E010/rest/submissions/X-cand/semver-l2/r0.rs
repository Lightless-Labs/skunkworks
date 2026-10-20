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

fn missing_start(b: Option<u8>) -> bool {
    matches!(b, None | Some(b'.' | b'-' | b'+'))
}

fn parse_digits(s: &str, start: usize) -> Result<(u64, usize), VersionError> {
    let bytes = s.as_bytes();
    let mut i = start;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == start {
        return Err(if missing_start(byte(s, start)) {
            VersionError::MissingCore { at: start }
        } else {
            VersionError::InvalidChar { at: start }
        });
    }
    if i - start > 1 && bytes[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    let n = s[start..i]
        .parse::<u64>()
        .map_err(|_| VersionError::Overflow { at: start })?;
    Ok((n, i))
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    is_pre: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut ident_start = start;
    loop {
        let mut i = ident_start;
        while i < end && bytes[i] != b'.' {
            let b = bytes[i];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }
        if i == ident_start {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }
        if is_pre
            && i - ident_start > 1
            && bytes[ident_start..i].iter().all(u8::is_ascii_digit)
            && bytes[ident_start] == b'0'
        {
            return Err(VersionError::LeadingZero { at: ident_start });
        }
        out.push(s[ident_start..i].to_owned());
        if i == end {
            break;
        }
        ident_start = i + 1;
        if ident_start == end {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }
    }
    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let (major, mut i) = parse_digits(s, 0)?;
    if byte(s, i) != Some(b'.') {
        return Err(if missing_start(byte(s, i)) {
            VersionError::MissingCore { at: i }
        } else {
            VersionError::InvalidChar { at: i }
        });
    }
    i += 1;
    let (minor, j) = parse_digits(s, i)?;
    i = j;
    if byte(s, i) != Some(b'.') {
        return Err(if missing_start(byte(s, i)) {
            VersionError::MissingCore { at: i }
        } else {
            VersionError::InvalidChar { at: i }
        });
    }
    i += 1;
    let (patch, j) = parse_digits(s, i)?;
    i = j;

    let mut pre = Vec::new();
    let mut build = Vec::new();
    match byte(s, i) {
        None => {}
        Some(b'-') => {
            let pre_start = i + 1;
            let plus = s.as_bytes()[pre_start..]
                .iter()
                .position(|&b| b == b'+')
                .map(|p| pre_start + p);
            let pre_end = plus.unwrap_or(s.len());
            pre = parse_identifiers(s, pre_start, pre_end, true)?;
            if let Some(p) = plus {
                build = parse_identifiers(s, p + 1, s.len(), false)?;
            }
        }
        Some(b'+') => {
            build = parse_identifiers(s, i + 1, s.len(), false)?;
        }
        Some(_) => return Err(VersionError::InvalidChar { at: i }),
    }
    Ok(Version { major, minor, patch, pre, build })
}

fn is_numeric(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn cmp_ident(a: &str, b: &str) -> Ordering {
    let an = is_numeric(a);
    let bn = is_numeric(b);
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
    op: Op,
    version: Version,
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

fn cmp_ok(v: &Version, b: &Bound) -> bool {
    match b.op {
        Op::Eq => compare_versions(v, &b.version) == Ordering::Equal,
        Op::Lt => compare_versions(v, &b.version) == Ordering::Less,
        Op::Le => compare_versions(v, &b.version) != Ordering::Greater,
        Op::Gt => compare_versions(v, &b.version) == Ordering::Greater,
        Op::Ge => compare_versions(v, &b.version) != Ordering::Less,
    }
}

fn anchor(v: &Version) -> Option<(u64, u64, u64)> {
    if v.pre.is_empty() {
        None
    } else {
        Some((v.major, v.minor, v.patch))
    }
}

fn full(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

fn next_version(
    mut v: Version,
    field: usize,
    at: usize,
) -> Result<Version, RangeError> {
    let n = match field {
        0 => &mut v.major,
        1 => &mut v.minor,
        _ => &mut v.patch,
    };
    *n = n.checked_add(1).ok_or(RangeError::Syntax { at })?;
    if field == 0 {
        v.minor = 0;
        v.patch = 0;
    } else if field == 1 {
        v.patch = 0;
    }
    v.pre = vec!["0".to_owned()];
    v.build.clear();
    Ok(v)
}

#[derive(Clone, Debug)]
struct Partial {
    version: Version,
    k: usize,
    full: bool,
    any: bool,
}

fn partial(s: &str, at: usize) -> Result<Partial, RangeError> {
    if s.contains('-') || s.contains('+') {
        let v = full(s, at)?;
        return Ok(Partial { version: v, k: 3, full: true, any: false });
    }
    let fields: Vec<&str> = s.split('.').collect();
    if fields.len() >= 3 && fields.iter().all(|f| !matches!(*f, "*" | "x" | "X")) {
        let v = full(s, at)?;
        return Ok(Partial { version: v, k: 3, full: true, any: false });
    }
    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|f| f.is_empty()) {
        return Err(RangeError::Syntax { at });
    }
    let mut nums = Vec::new();
    let mut wildcard_seen = false;
    for f in fields {
        if matches!(f, "*" | "x" | "X") {
            wildcard_seen = true;
        } else {
            if wildcard_seen
                || !f.bytes().all(|b| b.is_ascii_digit())
                || (f.len() > 1 && f.starts_with('0'))
                || f.parse::<u64>().is_err()
            {
                return Err(RangeError::Syntax { at });
            }
            nums.push(f.parse::<u64>().unwrap());
        }
    }
    let k = nums.len();
    let mut v = Version { major: 0, minor: 0, patch: 0, pre: Vec::new(), build: Vec::new() };
    if k > 0 { v.major = nums[0]; }
    if k > 1 { v.minor = nums[1]; }
    if k > 2 { v.patch = nums[2]; }
    Ok(Partial { version: v, k, full: false, any: k == 0 })
}

fn add_bound(arm: &mut Arm, op: Op, v: Version, explicit_anchor: bool) {
    if explicit_anchor {
        if let Some(a) = anchor(&v) {
            arm.anchors.push(a);
        }
    }
    arm.bounds.push(Bound { op, version: v });
}

fn expand_partial(arm: &mut Arm, p: Partial, at: usize) -> Result<(), RangeError> {
    if p.any { return Ok(()); }
    if p.full {
        add_bound(arm, Op::Eq, p.version.clone(), true);
        return Ok(());
    }
    if p.k == 3 {
        add_bound(arm, Op::Eq, p.version, false);
        return Ok(());
    }
    add_bound(arm, Op::Ge, p.version.clone(), false);
    let upper = next_version(p.version, if p.k == 1 { 0 } else { 1 }, at)?;
    add_bound(arm, Op::Lt, upper, false);
    Ok(())
}

fn parse_comparator(tok: &str, start: usize, arm: &mut Arm) -> Result<(), RangeError> {
    let (op, n) = if tok.starts_with(">=") { (Op::Ge, 2) }
        else if tok.starts_with("<=") { (Op::Le, 2) }
        else if tok.starts_with('>') { (Op::Gt, 1) }
        else if tok.starts_with('<') { (Op::Lt, 1) }
        else if tok.starts_with('=') { (Op::Eq, 1) }
        else { return Err(RangeError::Syntax { at: start }); };
    let operand = &tok[n..];
    let at = start + n;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: start });
    }
    let v = full(operand, at)?;
    add_bound(arm, op, v.clone(), true);
    Ok(())
}

fn parse_arm(s: &str, base: usize, arm: &mut Arm) -> Result<(), RangeError> {
    let toks: Vec<(&str, usize)> = token_positions(s, base);
    let hyphens: Vec<usize> = toks.iter().enumerate().filter(|(_, (t, _))| *t == "-").map(|(i, _)| i).collect();
    if !hyphens.is_empty() {
        let h = hyphens[0];
        if h != 1 || toks.len() != 3 {
            return Err(RangeError::Syntax { at: toks[h].1 });
        }
        let (ls, la) = toks[0];
        let (us, ua) = toks[2];
        let l = partial(ls, la)?;
        let u = partial(us, ua)?;
        if l.any { return Err(RangeError::Syntax { at: la }); }
        if u.any { return Err(RangeError::Syntax { at: ua }); }
        if let Some(a) = anchor(&l.version) { arm.anchors.push(a); }
        add_bound(arm, Op::Ge, l.version.clone(), false);
        if u.full {
            add_bound(arm, Op::Le, u.version.clone(), true);
        } else {
            let upper = if u.k == 1 { next_version(u.version, 0, ua)? }
                else if u.k == 2 { next_version(u.version, 1, ua)? }
                else { u.version.clone() };
            add_bound(arm, if u.k == 3 { Op::Le } else { Op::Lt }, upper, false);
        }
        return Ok(());
    }

    for (tok, start) in toks {
        if tok.starts_with('^') || tok.starts_with('~') {
            let caret = tok.starts_with('^');
            let operand = &tok[1..];
            let at = start + 1;
            let p = partial(operand, at)?;
            if p.any { return Err(RangeError::Syntax { at: start }); }
            if let Some(a) = anchor(&p.version) { arm.anchors.push(a); }
            add_bound(arm, Op::Ge, p.version.clone(), false);
            let field = if caret {
                if p.version.major > 0 || p.k == 1 { 0 }
                else if p.version.minor > 0 || p.k == 2 { 1 }
                else { 2 }
            } else if p.k == 1 { 0 } else { 1 };
            let upper = next_version(p.version, field, at)?;
            add_bound(arm, Op::Lt, upper, false);
        } else if tok.starts_with('<') || tok.starts_with('>') || tok.starts_with('=') {
            parse_comparator(tok, start, arm)?;
        } else if tok.starts_with(',') || tok.starts_with('(') || tok.starts_with(')') || tok == "&" {
            return Err(RangeError::Syntax { at: start });
        } else if tok.starts_with('*') || tok.starts_with('x') || tok.starts_with('X') || tok.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            let p = partial(tok, start)?;
            expand_partial(arm, p, start)?;
        } else {
            return Err(RangeError::Syntax { at: start });
        }
    }
    Ok(())
}

fn token_positions(s: &str, base: usize) -> Vec<(&str, usize)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') { i += 1; }
        if i == b.len() { break; }
        let start = i;
        while i < b.len() && !matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') { i += 1; }
        out.push((&s[start..i], base + start));
    }
    out
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let all = token_positions(s, 0);
    if all.is_empty() { return Err(RangeError::Empty { at: 0 }); }
    let mut arms = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < all.len() {
        if all[i].0 == "||" {
            if i == start {
                return Err(RangeError::Empty { at: all[i].1 });
            }
            let first = all[start].1;
            let last = if i + 1 < all.len() { all[i + 1].1 } else { s.len() };
            let arm_text = &s[first..all[i].1];
            let mut arm = Arm { bounds: Vec::new(), anchors: Vec::new() };
            parse_arm(arm_text, first, &mut arm)?;
            arms.push(arm);
            start = i + 1;
            if start == all.len() {
                return Err(RangeError::Empty { at: s.len() });
            }
            let _ = last;
        } else if all[i].0.contains("||") {
            // A glued separator remains an operand and is diagnosed by arm parsing.
        }
        i += 1;
    }
    let first = all[start].1;
    let mut arm = Arm { bounds: Vec::new(), anchors: Vec::new() };
    parse_arm(&s[first..], first, &mut arm)?;
    arms.push(arm);
    Ok(Range { arms })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        arm.bounds.iter().all(|b| cmp_ok(v, b))
            && (v.pre.is_empty() || arm.anchors.contains(&(v.major, v.minor, v.patch)))
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn satisfying(candidates: &[String], r: &Range, want_max: bool) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, s) in candidates.iter().enumerate() {
        match parse_version(s) {
            Ok(v) => parsed.push(v),
            Err(error) => return Err(CandidateError { index, error }),
        }
    }
    let mut best: Option<usize> = None;
    for (i, v) in parsed.iter().enumerate() {
        if !matches(v, r) { continue; }
        if let Some(j) = best {
            let c = compare_versions(v, &parsed[j]);
            if (want_max && c == Ordering::Greater) || (!want_max && c == Ordering::Less) {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

pub fn max_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    satisfying(candidates, r, true)
}
pub fn min_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    satisfying(candidates, r, false)
}
