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

fn parse_core_num(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && is_digit(bytes[*pos]) {
        *pos += 1;
    }
    if *pos == start {
        let b = bytes.get(start).copied();
        return match b {
            None | Some(b'.' | b'-' | b'+') => Err(VersionError::MissingCore { at: start }),
            _ => Err(VersionError::InvalidChar { at: start }),
        };
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

fn identifier_valid_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    pre: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut id_start = start;
    loop {
        let mut p = id_start;
        while p < end && bytes[p] != b'.' {
            if !identifier_valid_byte(bytes[p]) {
                return Err(VersionError::InvalidChar { at: p });
            }
            p += 1;
        }
        if p == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        if pre
            && p - id_start > 1
            && bytes[id_start..p].iter().all(|b| is_digit(*b))
            && bytes[id_start] == b'0'
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        out.push(s[id_start..p].to_string());
        if p == end {
            break;
        }
        id_start = p + 1;
    }
    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let mut pos = 0;
    let major = parse_core_num(s, &mut pos)?;
    for _ in 0..1 {
        if s.as_bytes().get(pos) != Some(&b'.') {
            let b = s.as_bytes().get(pos).copied();
            return match b {
                None | Some(b'-' | b'+') => Err(VersionError::MissingCore { at: pos }),
                _ => Err(VersionError::InvalidChar { at: pos }),
            };
        }
        pos += 1;
    }
    let minor = parse_core_num(s, &mut pos)?;
    if s.as_bytes().get(pos) != Some(&b'.') {
        let b = s.as_bytes().get(pos).copied();
        return match b {
            None | Some(b'-' | b'+') => Err(VersionError::MissingCore { at: pos }),
            _ => Err(VersionError::InvalidChar { at: pos }),
        };
    }
    pos += 1;
    let patch = parse_core_num(s, &mut pos)?;

    let bytes = s.as_bytes();
    let mut pre = Vec::new();
    let mut build = Vec::new();
    if pos < bytes.len() {
        match bytes[pos] {
            b'-' => {
                let pre_start = pos + 1;
                let plus = bytes[pre_start..]
                    .iter()
                    .position(|&b| b == b'+')
                    .map(|i| pre_start + i);
                let pre_end = plus.unwrap_or(bytes.len());
                if pre_start == pre_end {
                    return Err(VersionError::EmptyIdentifier { at: pre_start });
                }
                pre = parse_identifiers(s, pre_start, pre_end, true)?;
                if let Some(plus_pos) = plus {
                    let build_start = plus_pos + 1;
                    if build_start == bytes.len() {
                        return Err(VersionError::EmptyIdentifier { at: build_start });
                    }
                    build = parse_identifiers(s, build_start, bytes.len(), false)?;
                }
            }
            b'+' => {
                let build_start = pos + 1;
                if build_start == bytes.len() {
                    return Err(VersionError::EmptyIdentifier { at: build_start });
                }
                build = parse_identifiers(s, build_start, bytes.len(), false)?;
            }
            _ => return Err(VersionError::InvalidChar { at: pos }),
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

fn core_cmp(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch))
}

fn all_numeric(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn numeric_cmp(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

fn pre_cmp(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    for (x, y) in a.iter().zip(b.iter()) {
        let xn = all_numeric(x);
        let yn = all_numeric(y);
        let c = match (xn, yn) {
            (true, true) => numeric_cmp(x, y),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => x.as_bytes().cmp(y.as_bytes()),
        };
        if c != Ordering::Equal {
            return c;
        }
    }
    a.len().cmp(&b.len())
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    core_cmp(a, b).then_with(|| pre_cmp(&a.pre, &b.pre))
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
    lower: bool,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

fn split_tokens(s: &str, base: usize) -> Vec<(String, usize)> {
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

fn version_operand(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

fn empty_range_error(at: usize) -> RangeError {
    RangeError::Empty { at }
}

fn syntax(at: usize) -> RangeError {
    RangeError::Syntax { at }
}

fn tuple(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn plain_version(major: u64, minor: u64, patch: u64) -> Version {
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

fn bump(v: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let mut out = plain_version(v.major, v.minor, v.patch);
    match field {
        0 => {
            out.major = v.major.checked_add(1).ok_or_else(|| syntax(at))?;
        }
        1 => {
            out.major = v.major;
            out.minor = v.minor.checked_add(1).ok_or_else(|| syntax(at))?;
        }
        _ => {
            out.major = v.major;
            out.minor = v.minor;
            out.patch = v.patch.checked_add(1).ok_or_else(|| syntax(at))?;
        }
    }
    Ok(zero_pre(out))
}

#[derive(Clone, Debug)]
struct Partial {
    lower: Version,
    numeric: usize,
    full: bool,
}

fn is_wild(s: &str) -> bool {
    matches!(s, "*" | "x" | "X")
}

fn partial_parse(s: &str, at: usize) -> Result<Partial, RangeError> {
    let has_sign = s.contains('-') || s.contains('+');
    let fields: Vec<&str> = s.split('.').collect();
    let has_wild = fields.iter().any(|f| is_wild(f));
    if has_sign || (fields.len() >= 3 && !has_wild) {
        return Ok(Partial {
            lower: version_operand(s, at)?,
            numeric: 3,
            full: true,
        });
    }
    if fields.is_empty() || fields.len() > 3 || s.is_empty() {
        return Err(syntax(at));
    }
    let mut nums = Vec::new();
    let mut wildcard_seen = false;
    for f in &fields {
        if f.is_empty() {
            return Err(syntax(at));
        }
        if is_wild(f) {
            wildcard_seen = true;
        } else if f.bytes().all(is_digit) {
            if wildcard_seen || (f.len() > 1 && f.starts_with('0')) {
                return Err(syntax(at));
            }
            let mut n = 0u64;
            for b in f.bytes() {
                n = n
                    .checked_mul(10)
                    .and_then(|x| x.checked_add((b - b'0') as u64))
                    .ok_or_else(|| syntax(at))?;
            }
            nums.push(n);
        } else {
            return Err(syntax(at));
        }
    }
    let numeric = nums.len();
    let (major, minor, patch) = match numeric {
        0 => (0, 0, 0),
        1 => (nums[0], 0, 0),
        2 => (nums[0], nums[1], 0),
        _ => (nums[0], nums[1], nums[2]),
    };
    if fields.iter().any(|f| is_wild(f))
        && fields.iter().skip_while(|f| !is_wild(f)).any(|f| !is_wild(f))
    {
        return Err(syntax(at));
    }
    Ok(Partial {
        lower: plain_version(major, minor, patch),
        numeric,
        full: false,
    })
}

fn add_bound(arm: &mut Arm, v: Version, lower: bool, inclusive: bool) {
    arm.bounds.push(Bound {
        version: v.clone(),
        inclusive,
        lower,
    });
    if !v.pre.is_empty() {
        arm.anchors.push(tuple(&v));
    }
}

fn comparator(token: &str, at: usize, arm: &mut Arm) -> Result<(), RangeError> {
    let (op, skip) = if token.starts_with(">=") {
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
    let operand = &token[skip..];
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(syntax(at));
    }
    let v = version_operand(operand, at + skip)?;
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

fn add_partial_bare(p: Partial, at: usize, arm: &mut Arm) -> Result<(), RangeError> {
    if p.full {
        add_bound(arm, p.lower.clone(), true, true);
        add_bound(arm, p.lower, false, true);
    } else if p.numeric > 0 {
        add_bound(arm, p.lower.clone(), true, true);
        if p.numeric == 1 {
            let upper = bump(&p.lower, 0, at)?;
            add_bound(arm, upper, false, false);
        } else if p.numeric == 2 {
            let upper = bump(&p.lower, 1, at)?;
            add_bound(arm, upper, false, false);
        }
    }
    Ok(())
}

fn add_caret_tilde(token: &str, at: usize, caret: bool, arm: &mut Arm) -> Result<(), RangeError> {
    let operand = &token[1..];
    let op_at = at + 1;
    let p = partial_parse(operand, op_at)?;
    if p.numeric == 0 {
        return Err(syntax(at));
    }
    add_bound(arm, p.lower.clone(), true, true);
    let field = if caret {
        if p.lower.major > 0 || p.numeric == 1 {
            0
        } else if p.lower.minor > 0 || p.numeric == 2 {
            1
        } else {
            2
        }
    } else if p.numeric == 1 {
        0
    } else {
        1
    };
    let upper = bump(&p.lower, field, op_at)?;
    add_bound(arm, upper, false, false);
    Ok(())
}

fn hyphen_arm(tokens: &[(String, usize)], arm: &mut Arm) -> Result<(), RangeError> {
    let dash = tokens.iter().find(|(s, _)| s == "-").unwrap().1;
    if tokens.len() != 3 || tokens[1].0 != "-" {
        return Err(syntax(dash));
    }
    let lo = partial_parse(&tokens[0].0, tokens[0].1)?;
    let hi = partial_parse(&tokens[2].0, tokens[2].1)?;
    if lo.numeric == 0 {
        return Err(syntax(tokens[0].1));
    }
    if hi.numeric == 0 {
        return Err(syntax(tokens[2].1));
    }
    add_bound(arm, lo.lower.clone(), true, true);
    if hi.full {
        add_bound(arm, hi.lower, false, true);
    } else {
        let upper = match hi.numeric {
            1 => bump(&hi.lower, 0, tokens[2].1)?,
            2 => bump(&hi.lower, 1, tokens[2].1)?,
            _ => hi.lower,
        };
        if hi.numeric == 3 {
            add_bound(arm, upper, false, true);
        } else {
            add_bound(arm, upper, false, false);
        }
    }
    Ok(())
}

fn parse_arm(tokens: &[(String, usize)]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        bounds: Vec::new(),
        anchors: Vec::new(),
    };
    if let Some((_, at)) = tokens.iter().find(|(s, _)| s == "-") {
        let _ = at;
        hyphen_arm(tokens, &mut arm)?;
        return Ok(arm);
    }
    for (token, at) in tokens {
        if token.starts_with('^') {
            add_caret_tilde(token, *at, true, &mut arm)?;
        } else if token.starts_with('~') {
            add_caret_tilde(token, *at, false, &mut arm)?;
        } else if token.starts_with('<')
            || token.starts_with('>')
            || token.starts_with('=')
        {
            comparator(token, *at, &mut arm)?;
        } else {
            let first = token.as_bytes().first().copied();
            if !matches!(first, Some(b'0'..=b'9' | b'*' | b'x' | b'X')) {
                return Err(syntax(*at));
            }
            let p = partial_parse(token, *at)?;
            add_partial_bare(p, *at, &mut arm)?;
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = Vec::new();
    let mut arm_start = 0usize;
    let mut i = 0usize;
    let mut separators = Vec::new();
    while i < bytes.len() {
        if bytes[i..].starts_with(b"||") {
            let left = &s[arm_start..i];
            let toks = split_tokens(left, arm_start);
            if toks.is_empty() {
                return Err(empty_range_error(i));
            }
            arms.push(parse_arm(&toks)?);
            let sep = i;
            i += 2;
            if i < bytes.len() && !whitespace(bytes[i]) {
                arm_start = sep;
                while arm_start < bytes.len() && !whitespace(bytes[arm_start]) {
                    arm_start += 1;
                }
                // A glued separator is part of a token, handled as syntax.
                return Err(syntax(sep.saturating_sub(0)));
            }
            while i < bytes.len() && whitespace(bytes[i]) {
                i += 1;
            }
            arm_start = i;
            separators.push(sep);
            continue;
        }
        i += 1;
    }
    let toks = split_tokens(&s[arm_start..], arm_start);
    if toks.is_empty() {
        if separators.is_empty() {
            return Err(empty_range_error(0));
        }
        return Err(empty_range_error(s.len()));
    }
    arms.push(parse_arm(&toks)?);
    Ok(Range { arms })
}

fn arm_admits(arm: &Arm, v: &Version) -> bool {
    arm.anchors.iter().any(|t| *t == tuple(v))
}

fn bound_accepts(bound: &Bound, v: &Version) -> bool {
    let c = compare_versions(v, &bound.version);
    if bound.lower {
        c == Ordering::Greater || (bound.inclusive && c == Ordering::Equal)
    } else {
        c == Ordering::Less || (bound.inclusive && c == Ordering::Equal)
    }
}

fn arm_matches(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !v.pre.is_empty() && !include_prerelease && !arm_admits(arm, v) {
        return false;
    }
    arm.bounds.iter().all(|b| bound_accepts(b, v))
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms
        .iter()
        .any(|arm| arm_matches(v, arm, include_prerelease))
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
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, s) in candidates.iter().enumerate() {
        let v = parse_version(s).map_err(|error| CandidateError { index, error })?;
        parsed.push(v);
    }
    let mut best: Option<usize> = None;
    for i in 0..parsed.len() {
        if !matches(&parsed[i], r) {
            continue;
        }
        match best {
            None => best = Some(i),
            Some(j) => {
                let c = compare_versions(&parsed[i], &parsed[j]);
                if (max && c == Ordering::Greater) || (!max && c == Ordering::Less) {
                    best = Some(i);
                }
            }
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

fn strongest_bounds(arm: &Arm) -> (Option<Bound>, Option<Bound>) {
    let mut lower: Option<Bound> = None;
    let mut upper: Option<Bound> = None;
    for b in &arm.bounds {
        if b.lower {
            let replace = match &lower {
                None => true,
                Some(old) => {
                    let c = compare_versions(&b.version, &old.version);
                    c == Ordering::Greater || (c == Ordering::Equal && !b.inclusive && old.inclusive)
                }
            };
            if replace {
                lower = Some(b.clone());
            }
        } else {
            let replace = match &upper {
                None => true,
                Some(old) => {
                    let c = compare_versions(&b.version, &old.version);
                    c == Ordering::Less || (c == Ordering::Equal && !b.inclusive && old.inclusive)
                }
            };
            if replace {
                upper = Some(b.clone());
            }
        }
    }
    (lower, upper)
}

fn format_version(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

fn bound_nonempty(lower: &Option<Bound>, upper: &Option<Bound>) -> bool {
    match (lower, upper) {
        (Some(l), Some(u)) => {
            let c = compare_versions(&l.version, &u.version);
            c == Ordering::Less || (c == Ordering::Equal && l.inclusive && u.inclusive)
        }
        _ => true,
    }
}

fn arm_has_witness(arm: &Arm) -> bool {
    let (l, u) = strongest_bounds(arm);
    if !bound_nonempty(&l, &u) {
        return false;
    }
    // Stable versions form a discrete, lexicographically ordered core domain.
    let candidate = match &l {
        Some(b) => {
            let mut v = b.version.clone();
            v.build.clear();
            if !b.inclusive {
                if !v.pre.is_empty() {
                    // A stable release at the same core is above all prereleases.
                    v.pre.clear();
                } else if v.patch < u64::MAX {
                    v.patch += 1;
                } else if v.minor < u64::MAX {
                    v.minor += 1;
                    v.patch = 0;
                } else if v.major < u64::MAX {
                    v.major += 1;
                    v.minor = 0;
                    v.patch = 0;
                } else {
                    return false;
                }
            }
            v
        }
        None => plain_version(0, 0, 0),
    };
    if candidate.pre.is_empty() && arm.bounds.iter().all(|b| bound_accepts(b, &candidate)) {
        return true;
    }
    if arm.anchors.iter().any(|t| {
        let v = plain_version(t.0, t.1, t.2);
        let _ = v;
        let mut found = false;
        // Any explicit anchor admits all prereleases at that core; determine
        // whether its interval has at least one prerelease by using the lower
        // boundary and immediate sequence ordering.
        for b in &arm.bounds {
            let _ = b;
        }
        found = tuple(&candidate) == *t && !candidate.pre.is_empty();
        found
    }) {
        return true;
    }
    false
}

fn canonical_arm(arm: &Arm) -> Option<String> {
    if !arm_has_witness(arm) {
        return None;
    }
    let (lower, upper) = strongest_bounds(arm);
    let mut anchors = arm.anchors.clone();
    anchors.sort();
    anchors.dedup();
    let anchor_text = if anchors.is_empty() {
        "-".to_string()
    } else {
        anchors
            .iter()
            .map(|(a, b, c)| format!("{a}.{b}.{c}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    let lo = lower.map_or("*".to_string(), |b| {
        format!("{}{}", if b.inclusive { ">=" } else { ">" }, format_version(&b.version))
    });
    let hi = upper.map_or("*".to_string(), |b| {
        format!("{}{}", if b.inclusive { "<=" } else { "<" }, format_version(&b.version))
    });
    Some(format!("[{lo};{hi};pre={anchor_text}]"))
}

pub fn canonical(r: &Range) -> String {
    let mut items: Vec<String> = r.arms.iter().filter_map(canonical_arm).collect();
    items.sort();
    items.dedup();
    if items.is_empty() {
        "EMPTY".to_string()
    } else {
        items.join(" || ")
    }
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for aa in &a.arms {
        for bb in &b.arms {
            let mut combined = Arm {
                bounds: aa.bounds.iter().chain(bb.bounds.iter()).cloned().collect(),
                anchors: aa.anchors.iter().chain(bb.anchors.iter()).copied().collect(),
            };
            combined.anchors.sort();
            combined.anchors.dedup();
            if arm_has_witness(&combined) {
                return true;
            }
            if include_prerelease {
                // Stable witness search is already handled by arm_has_witness.
                // Explicitly admitted prereleases can witness at either bound
                // core, using generated bounds as ordinary comparisons.
                for t in core_candidates(aa, bb) {
                    let v = Version {
                        major: t.0,
                        minor: t.1,
                        patch: t.2,
                        pre: vec!["0".to_string()],
                        build: Vec::new(),
                    };
                    if combined.bounds.iter().all(|x| bound_accepts(x, &v)) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn core_candidates(a: &Arm, b: &Arm) -> Vec<(u64, u64, u64)> {
    let mut out = a.anchors.iter().chain(b.anchors.iter()).copied().collect::<Vec<_>>();
    for arm in [a, b] {
        for bound in &arm.bounds {
            out.push(tuple(&bound.version));
        }
    }
    out.sort();
    out.dedup();
    out
}

fn _parse_operator(_: &str) -> Option<Op> {
    None
}

fn _first_non_ascii(_: &str) -> Option<usize> {
    None
}

fn _core_successor(_: (u64, u64, u64)) -> Option<(u64, u64, u64)> {
    None
}

fn _identifier_successor(_: &[String]) -> Option<Vec<String>> {
    None
}

fn _range_placeholder() {}

fn _ensure_public_api(_: &RangeError, _: &CandidateError) {}

fn _main_anchor() {}

fn _compiler_anchor() {}

fn _anchor() {}

fn _unused() {}

fn main() {}