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

fn parse_core_field(bytes: &[u8], pos: &mut usize) -> Result<u64, VersionError> {
    let start = *pos;
    while *pos < bytes.len() && is_digit(bytes[*pos]) {
        *pos += 1;
    }
    if *pos == start {
        if start == bytes.len() || matches!(bytes[start], b'.' | b'-' | b'+') {
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
    offset: usize,
    pre: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut start = 0;
    loop {
        let mut end = start;
        while end < bytes.len() && bytes[end] != b'.' {
            if pre && bytes[end] == b'+' {
                break;
            }
            if !is_ident_char(bytes[end]) {
                return Err(VersionError::InvalidChar { at: offset + end });
            }
            end += 1;
        }
        if end == start {
            return Err(VersionError::EmptyIdentifier { at: offset + start });
        }
        let part = &s[start..end];
        if pre
            && part.len() > 1
            && part.as_bytes().iter().all(|b| is_digit(*b))
            && part.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: offset + start });
        }
        result.push(part.to_string());
        if end == bytes.len() || (pre && bytes[end] == b'+') {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(VersionError::EmptyIdentifier { at: offset + start });
        }
    }
    Ok(result)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let bytes = s.as_bytes();
    let mut pos = 0;
    let major = parse_core_field(bytes, &mut pos)?;
    for expected in [b'.', b'.'] {
        if pos >= bytes.len() || bytes[pos] != expected {
            if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            return Err(VersionError::InvalidChar { at: pos });
        }
        pos += 1;
        if expected == b'.' {
            break;
        }
    }
    let minor = parse_core_field(bytes, &mut pos)?;
    if pos >= bytes.len() || bytes[pos] != b'.' {
        if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;
    let patch = parse_core_field(bytes, &mut pos)?;
    if pos < bytes.len() && !matches!(bytes[pos], b'-' | b'+') {
        return Err(VersionError::InvalidChar { at: pos });
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();
    if pos < bytes.len() && bytes[pos] == b'-' {
        let start = pos + 1;
        let plus = s[start..].find('+').map(|i| start + i);
        let pre_end = plus.unwrap_or(bytes.len());
        pre = parse_identifiers(&s[start..pre_end], start, false)?;
        if let Some(p) = plus {
            if p + 1 == bytes.len() {
                return Err(VersionError::EmptyIdentifier { at: p + 1 });
            }
            build = parse_identifiers(&s[p + 1..], p + 1, false)?;
        }
    } else if pos < bytes.len() && bytes[pos] == b'+' {
        if pos + 1 == bytes.len() {
            return Err(VersionError::EmptyIdentifier { at: pos + 1 });
        }
        build = parse_identifiers(&s[pos + 1..], pos + 1, false)?;
    }

    // Validate pre numeric leading zeros after character validation.
    if !pre.is_empty() {
        let dash = s.find('-').unwrap();
        let mut at = dash + 1;
        for part in &pre {
            if part.len() > 1
                && part.as_bytes().iter().all(|b| is_digit(*b))
                && part.as_bytes()[0] == b'0'
            {
                return Err(VersionError::LeadingZero { at });
            }
            at += part.len() + 1;
        }
    }

    Ok(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    for (x, y) in a.iter().zip(b.iter()) {
        let xn = numeric_identifier(x);
        let yn = numeric_identifier(y);
        let ord = match (xn, yn) {
            (true, true) => x.len().cmp(&y.len()).then_with(|| x.as_bytes().cmp(y.as_bytes())),
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
        .then_with(|| compare_pre(&a.pre, &b.pre))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

fn core_version(major: u64, minor: u64, patch: u64, pre: Vec<String>) -> Version {
    Version { major, minor, patch, pre, build: Vec::new() }
}

fn with_pre_zero(major: u64, minor: u64, patch: u64) -> Version {
    core_version(major, minor, patch, vec!["0".to_string()])
}

fn whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokens_with_offsets(s: &str, start: usize, end: usize) -> Vec<(usize, &str)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut p = start;
    while p < end {
        while p < end && whitespace(b[p]) { p += 1; }
        if p == end { break; }
        let a = p;
        while p < end && !whitespace(b[p]) { p += 1; }
        out.push((a, &s[a..p]));
    }
    out
}

#[derive(Clone, Debug)]
struct Partial {
    nums: Vec<u64>,
    full: Option<Version>,
    has_pre: bool,
}

fn parse_partial_operand(s: &str, at: usize) -> Result<Partial, RangeError> {
    let has_suffix = s.contains('-') || s.contains('+');
    let fields: Vec<&str> = s.split('.').collect();
    let has_wild = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));
    if has_suffix || (fields.len() >= 3 && !has_wild) {
        let v = parse_version(s).map_err(|error| RangeError::Version { at, error })?;
        return Ok(Partial { nums: vec![v.major, v.minor, v.patch], has_pre: !v.pre.is_empty(), full: Some(v) });
    }
    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at });
    }
    let mut nums = Vec::new();
    let mut wildcard_seen = false;
    for f in fields {
        if matches!(f, "*" | "x" | "X") {
            wildcard_seen = true;
        } else {
            if wildcard_seen || f.is_empty() || !f.bytes().all(is_digit) || (f.len() > 1 && f.starts_with('0')) {
                return Err(RangeError::Syntax { at });
            }
            let mut n = 0u64;
            for b in f.bytes() {
                n = n.checked_mul(10).and_then(|x| x.checked_add((b-b'0') as u64))
                    .ok_or(RangeError::Syntax { at })?;
            }
            nums.push(n);
        }
    }
    Ok(Partial { nums, full: None, has_pre: false })
}

fn padded(p: &Partial) -> (u64,u64,u64) {
    (p.nums.get(0).copied().unwrap_or(0), p.nums.get(1).copied().unwrap_or(0), p.nums.get(2).copied().unwrap_or(0))
}

fn next_upper(core: (u64,u64,u64), index: usize, at: usize, mode: &str) -> Result<Version, RangeError> {
    let (mut a, mut b, mut c) = core;
    let field = if mode == "caret" {
        if a > 0 || index == 1 { 0 } else if b > 0 || index == 2 { 1 } else { 2 }
    } else if mode == "tilde" {
        if index == 1 { 0 } else { 1 }
    } else if index == 1 { 0 } else { 1 };
    match field {
        0 => { a = a.checked_add(1).ok_or(RangeError::Syntax { at })?; b=0; c=0; }
        1 => { b = b.checked_add(1).ok_or(RangeError::Syntax { at })?; c=0; }
        _ => { c = c.checked_add(1).ok_or(RangeError::Syntax { at })?; }
    }
    Ok(with_pre_zero(a,b,c))
}

fn add_bound(arm: &mut Arm, op: Op, version: Version, explicit: bool) {
    if explicit && !version.pre.is_empty() {
        arm.anchors.push((version.major, version.minor, version.patch));
    }
    arm.bounds.push(Bound { version, op });
}

fn parse_operand_expansion(token: &str, at: usize, arm: &mut Arm) -> Result<(), RangeError> {
    if token.starts_with('^') || token.starts_with('~') {
        let mode = if token.starts_with('^') { "caret" } else { "tilde" };
        let operand_at = at + 1;
        let p = parse_partial_operand(&token[1..], operand_at)?;
        if p.nums.is_empty() { return Err(RangeError::Syntax { at }); }
        let lower = p.full.clone().unwrap_or_else(|| {
            let (a,b,c)=padded(&p); core_version(a,b,c,Vec::new())
        });
        let upper = next_upper(padded(&p), p.nums.len(), operand_at, mode)?;
        add_bound(arm, Op::Ge, lower, p.has_pre);
        add_bound(arm, Op::Lt, upper, false);
        return Ok(());
    }
    if token == "-" { return Err(RangeError::Syntax { at }); }
    let p = parse_partial_operand(token, at)?;
    if let Some(v) = p.full {
        add_bound(arm, Op::Eq, v, p.has_pre);
    } else if p.nums.is_empty() {
        return Ok(());
    } else {
        let lower = padded(&p);
        add_bound(arm, Op::Ge, core_version(lower.0,lower.1,lower.2,Vec::new()), false);
        if p.nums.len() < 3 {
            let upper = next_upper(lower, p.nums.len(), at, "partial")?;
            add_bound(arm, Op::Lt, upper, false);
        }
    }
    Ok(())
}

fn parse_hyphen_arm(tokens: &[(usize, &str)], arm: &mut Arm) -> Result<(), RangeError> {
    let minus_positions: Vec<usize> = tokens.iter().filter(|(_,t)| *t == "-").map(|(p,_)| *p).collect();
    if minus_positions.is_empty() || tokens.len() != 3 || tokens[1].1 != "-" {
        return Err(RangeError::Syntax { at: minus_positions[0] });
    }
    let (la, lt) = tokens[0];
    let (ua, ut) = tokens[2];
    let lo = parse_partial_operand(lt, la)?;
    let hi = parse_partial_operand(ut, ua)?;
    if lo.nums.is_empty() || hi.nums.is_empty() {
        return Err(RangeError::Syntax { at: if lo.nums.is_empty() { la } else { ua } });
    }
    let (a,b,c) = padded(&lo);
    add_bound(arm, Op::Ge, lo.full.clone().unwrap_or_else(|| core_version(a,b,c,Vec::new())), lo.has_pre);
    if let Some(v) = hi.full {
        add_bound(arm, Op::Le, v, hi.has_pre);
    } else {
        let (x,y,z) = padded(&hi);
        if hi.nums.len() == 3 {
            add_bound(arm, Op::Le, core_version(x,y,z,Vec::new()), false);
        } else {
            let upper = next_upper((x,y,z), hi.nums.len(), ua, "partial")?;
            add_bound(arm, Op::Lt, upper, false);
        }
    }
    Ok(())
}

fn parse_arm(s: &str, tokens: &[(usize, &str)]) -> Result<Arm, RangeError> {
    let mut arm = Arm { bounds: Vec::new(), anchors: Vec::new() };
    if tokens.iter().any(|(_,t)| *t == "-") {
        parse_hyphen_arm(tokens, &mut arm)?;
        return Ok(arm);
    }
    for &(at, token) in tokens {
        let first = token.as_bytes()[0];
        if matches!(first, b'<' | b'>' | b'=') {
            let (op, n) = if token.starts_with("<=") { (Op::Le,2) }
                else if token.starts_with(">=") { (Op::Ge,2) }
                else if token.starts_with('<') { (Op::Lt,1) }
                else if token.starts_with('>') { (Op::Gt,1) }
                else { (Op::Eq,1) };
            let operand = &token[n..];
            let oa = at+n;
            if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
                return Err(RangeError::Syntax { at });
            }
            let v = parse_version(operand).map_err(|error| RangeError::Version { at: oa, error })?;
            add_bound(&mut arm, op, v, true);
        } else if matches!(first, b'^' | b'~' | b'*' | b'x' | b'X') || first.is_ascii_digit() {
            parse_operand_expansion(token, at, &mut arm)?;
        } else {
            return Err(RangeError::Syntax { at });
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = Vec::new();
    let mut pos = 0;
    let mut found_non_ws = false;
    while pos < bytes.len() {
        while pos < bytes.len() && whitespace(bytes[pos]) { pos += 1; }
        if pos == bytes.len() { break; }
        found_non_ws = true;
        let arm_start = pos;
        let mut end = pos;
        let mut sep = None;
        while end < bytes.len() {
            if whitespace(bytes[end]) {
                let mut q = end;
                while q < bytes.len() && whitespace(bytes[q]) { q += 1; }
                if q + 1 < bytes.len() && &s[q..q+2] == "||" {
                    sep = Some((q, q+2));
                    break;
                }
                end = q;
            } else {
                end += 1;
            }
        }
        let arm_end = sep.map(|x| x.0).unwrap_or(bytes.len());
        let tokens = tokens_with_offsets(s, arm_start, arm_end);
        arms.push(parse_arm(s, &tokens)?);
        if let Some((a,b)) = sep {
            let _ = a;
            pos = b;
            let mut q = pos;
            while q < bytes.len() && whitespace(bytes[q]) { q += 1; }
            if q == bytes.len() { return Err(RangeError::Empty { at: bytes.len() }); }
            if s[q..].starts_with("||") {
                return Err(RangeError::Empty { at: q });
            }
            pos = q;
        } else {
            break;
        }
    }
    if !found_non_ws { return Err(RangeError::Empty { at: 0 }); }
    Ok(Range { arms })
}

fn op_matches(v: &Version, b: &Bound) -> bool {
    match b.op {
        Op::Eq => compare_versions(v, &b.version) == Ordering::Equal,
        Op::Lt => compare_versions(v, &b.version) == Ordering::Less,
        Op::Le => compare_versions(v, &b.version) != Ordering::Greater,
        Op::Gt => compare_versions(v, &b.version) == Ordering::Greater,
        Op::Ge => compare_versions(v, &b.version) != Ordering::Less,
    }
}

fn arm_matches(v: &Version, arm: &Arm, include_pre: bool) -> bool {
    if !arm.bounds.iter().all(|b| op_matches(v,b)) { return false; }
    if v.pre.is_empty() || include_pre { return true; }
    let core = (v.major,v.minor,v.patch);
    arm.anchors.contains(&core)
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v,r,false)
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|a| arm_matches(v,a,include_prerelease))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn extremum(candidates: &[String], r: &Range, want_max: bool) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, s) in candidates.iter().enumerate() {
        let v = parse_version(s).map_err(|error| CandidateError { index, error })?;
        parsed.push(v);
    }
    let mut best: Option<usize> = None;
    for i in 0..parsed.len() {
        if !matches(&parsed[i],r) { continue; }
        if let Some(j) = best {
            let ord = compare_versions(&parsed[i], &parsed[j]);
            if (want_max && ord == Ordering::Greater) || (!want_max && ord == Ordering::Less) {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

pub fn max_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    extremum(candidates,r,true)
}

pub fn min_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    extremum(candidates,r,false)
}

fn core_cmp(a: (u64,u64,u64), b: (u64,u64,u64)) -> Ordering { a.cmp(&b) }

fn next_core(c: (u64,u64,u64)) -> Option<(u64,u64,u64)> {
    if c.2 < u64::MAX { Some((c.0,c.1,c.2+1)) }
    else if c.1 < u64::MAX { Some((c.0,c.1+1,0)) }
    else if c.0 < u64::MAX { Some((c.0+1,0,0)) }
    else { None }
}

fn core_tuple(v: &Version) -> (u64,u64,u64) { (v.major,v.minor,v.patch) }

fn prerelease_at_least_zero(v: &Version) -> bool {
    !v.pre.is_empty() && compare_pre(&v.pre, &["0".to_string()]) != Ordering::Less
}

fn arm_has_stable(arm: &Arm) -> bool {
    let mut lower: Option<(&Version,bool)> = None;
    let mut upper: Option<(&Version,bool)> = None;
    for b in &arm.bounds {
        match b.op {
            Op::Eq => {
                lower = Some((&b.version,true));
                upper = Some((&b.version,true));
            }
            Op::Gt | Op::Ge => {
                let inc = matches!(b.op,Op::Ge);
                if lower.map_or(true, |(v,i)| compare_versions(&b.version,v)==Ordering::Greater || (compare_versions(&b.version,v)==Ordering::Equal && !inc && i)) {
                    lower=Some((&b.version,inc));
                }
            }
            Op::Lt | Op::Le => {
                let inc = matches!(b.op,Op::Le);
                if upper.map_or(true, |(v,i)| compare_versions(&b.version,v)==Ordering::Less || (compare_versions(&b.version,v)==Ordering::Equal && !inc && i)) {
                    upper=Some((&b.version,inc));
                }
            }
        }
    }
    let mut candidate = Version { major:0,minor:0,patch:0,pre:Vec::new(),build:Vec::new() };
    if let Some((v,inc))=lower {
        candidate=v.clone();
        if !inc && !candidate.pre.is_empty() {
            candidate.pre.clear();
        } else if !inc {
            if let Some(c)=next_core(core_tuple(v)) { candidate=core_version(c.0,c.1,c.2,Vec::new()); }
            else { return false; }
        }
    }
    arm.bounds.iter().all(|b| op_matches(&candidate,b))
}

fn version_string(v: &Version) -> String {
    let mut s=format!("{}.{}.{}",v.major,v.minor,v.patch);
    if !v.pre.is_empty() { s.push('-'); s.push_str(&v.pre.join(".")); }
    s
}

pub fn canonical(r: &Range) -> String {
    let mut out = Vec::new();
    for arm in &r.arms {
        if !arm_has_stable(arm) { continue; }
        let mut lower: Option<(Version,bool)> = None;
        let mut upper: Option<(Version,bool)> = None;
        for b in &arm.bounds {
            match b.op {
                Op::Eq => {
                    lower=Some((b.version.clone(),true));
                    upper=Some((b.version.clone(),true));
                }
                Op::Gt | Op::Ge => {
                    let inc=matches!(b.op,Op::Ge);
                    if lower.as_ref().map_or(true,|(v,i)| compare_versions(&b.version,v)==Ordering::Greater || (compare_versions(&b.version,v)==Ordering::Equal && !inc && *i)) {
                        lower=Some((b.version.clone(),inc));
                    }
                }
                Op::Lt | Op::Le => {
                    let inc=matches!(b.op,Op::Le);
                    if upper.as_ref().map_or(true,|(v,i)| compare_versions(&b.version,v)==Ordering::Less || (compare_versions(&b.version,v)==Ordering::Equal && !inc && *i)) {
                        upper=Some((b.version.clone(),inc));
                    }
                }
            }
        }
        let lo=lower.map(|(v,i)| format!("{}{}",if i{">="}else{">"},version_string(&v))).unwrap_or_else(||"*".into());
        let hi=upper.map(|(v,i)| format!("{}{}",if i{"<="}else{"<"},version_string(&v))).unwrap_or_else(||"*".into());
        let mut anchors=arm.anchors.clone();
        anchors.sort();
        anchors.dedup();
        let an=if anchors.is_empty() {"-".to_string()} else {
            anchors.iter().map(|(a,b,c)|format!("{}.{}.{}",a,b,c)).collect::<Vec<_>>().join(",")
        };
        out.push(format!("[{};{};pre={}]",lo,hi,an));
    }
    out.sort();
    out.dedup();
    if out.is_empty() {"EMPTY".to_string()} else {out.join(" || ")}
}

fn arm_anchor_witness(a: &Arm, b: &Arm) -> bool {
    for core in &a.anchors {
        if !b.anchors.contains(core) { continue; }
        let v=with_pre_zero(core.0,core.1,core.2);
        if arm_matches(&v,a,false) && arm_matches(&v,b,false) { return true; }
    }
    false
}

fn bounds_overlap(a: &Arm, b: &Arm) -> bool {
    let mut candidates = vec![
        core_version(0,0,0,Vec::new()),
        core_version(u64::MAX,u64::MAX,u64::MAX,Vec::new()),
    ];
    for x in a.bounds.iter().chain(b.bounds.iter()) {
        candidates.push(x.version.clone());
        let c=core_tuple(&x.version);
        if let Some(n)=next_core(c) { candidates.push(core_version(n.0,n.1,n.2,Vec::new())); }
        if !x.version.pre.is_empty() {
            let mut p=x.version.clone();
            p.pre.push("0".to_string());
            candidates.push(p);
        }
    }
    candidates.into_iter().any(|v| arm_matches(&v,a,true) && arm_matches(&v,b,true))
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for x in &a.arms {
        for y in &b.arms {
            if bounds_overlap(x,y) { return true; }
            if !include_prerelease && arm_anchor_witness(x,y) { return true; }
        }
    }
    false
}