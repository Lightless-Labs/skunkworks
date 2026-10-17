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

fn parse_core(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let start = *pos;
    let bytes = s.as_bytes();
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if *pos == start {
        return Err(if *pos == bytes.len()
            || matches!(bytes[*pos], b'.' | b'-' | b'+')
        {
            VersionError::MissingCore { at: start }
        } else {
            VersionError::InvalidChar { at: start }
        });
    }

    let digits = &s[start..*pos];
    if digits.len() > 1 && digits.starts_with('0') {
        return Err(VersionError::LeadingZero { at: start });
    }
    digits
        .parse::<u64>()
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
        let ident_start = p;
        while p < end && bytes[p] != b'.' {
            if !bytes[p].is_ascii_alphanumeric() && bytes[p] != b'-' {
                return Err(VersionError::InvalidChar { at: p });
            }
            p += 1;
        }

        if p == ident_start {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }

        let ident = &s[ident_start..p];
        if prerelease
            && ident.bytes().all(|b| b.is_ascii_digit())
            && ident.len() > 1
            && ident.starts_with('0')
        {
            return Err(VersionError::LeadingZero { at: ident_start });
        }
        result.push(ident.to_string());

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

    let mut p = 0;
    let major = parse_core(s, &mut p)?;
    if p >= s.len() || s.as_bytes()[p] != b'.' {
        return Err(if p >= s.len() || matches!(s.as_bytes()[p], b'-' | b'+') {
            VersionError::MissingCore { at: p }
        } else {
            VersionError::InvalidChar { at: p }
        });
    }
    p += 1;

    let minor = parse_core(s, &mut p)?;
    if p >= s.len() || s.as_bytes()[p] != b'.' {
        return Err(if p >= s.len() || matches!(s.as_bytes()[p], b'-' | b'+') {
            VersionError::MissingCore { at: p }
        } else {
            VersionError::InvalidChar { at: p }
        });
    }
    p += 1;

    let patch = parse_core(s, &mut p)?;
    if p < s.len() && !matches!(s.as_bytes()[p], b'-' | b'+') {
        return Err(VersionError::InvalidChar { at: p });
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if p < s.len() && s.as_bytes()[p] == b'-' {
        p += 1;
        let pre_start = p;
        let build_marker = s.as_bytes()[p..]
            .iter()
            .position(|&b| b == b'+')
            .map(|i| p + i);
        let pre_end = build_marker.unwrap_or(s.len());
        pre = parse_identifiers(s, pre_start, pre_end, true)?;
        p = pre_end;
    }

    if p < s.len() && s.as_bytes()[p] == b'+' {
        p += 1;
        build = parse_identifiers(s, p, s.len(), false)?;
    }

    Ok(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn is_numeric(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }

    for (x, y) in a.iter().zip(b.iter()) {
        let xn = is_numeric(x);
        let yn = is_numeric(y);
        let order = match (xn, yn) {
            (true, true) => x.len().cmp(&y.len()).then_with(|| x.as_bytes().cmp(y.as_bytes())),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => x.as_bytes().cmp(y.as_bytes()),
        };
        if order != Ordering::Equal {
            return order;
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
enum Op {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
}

#[derive(Clone, Debug)]
struct Comparator {
    op: Op,
    version: Version,
    explicit_anchor: bool,
}

#[derive(Clone, Debug)]
struct Arm {
    comparators: Vec<Comparator>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug)]
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
struct Partial {
    nums: Vec<u64>,
    full: Option<Version>,
}

fn syntax(at: usize) -> RangeError {
    RangeError::Syntax { at }
}

fn whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

#[derive(Clone)]
struct Token {
    text: String,
    start: usize,
}

fn tokenize(s: &str, start: usize, end: usize) -> Vec<Token> {
    let bytes = s.as_bytes();
    let mut tokens = Vec::new();
    let mut p = start;
    while p < end {
        while p < end && whitespace(bytes[p]) {
            p += 1;
        }
        if p == end {
            break;
        }
        let t = p;
        while p < end && !whitespace(bytes[p]) {
            p += 1;
        }
        tokens.push(Token {
            text: s[t..p].to_string(),
            start: t,
        });
    }
    tokens
}

fn parse_partial_or_full(s: &str, start: usize) -> Result<Partial, RangeError> {
    if s.is_empty() {
        return Err(syntax(start));
    }
    let fields: Vec<&str> = s.split('.').collect();
    let full_rule = s.contains('-')
        || s.contains('+')
        || (fields.len() >= 3
            && !fields.iter().any(|f| matches!(*f, "*" | "x" | "X")));

    if full_rule {
        let version = parse_version(s).map_err(|error| RangeError::Version { at: start, error })?;
        return Ok(Partial {
            nums: vec![version.major, version.minor, version.patch],
            full: Some(version),
        });
    }

    if fields.is_empty() || fields.len() > 3 {
        return Err(syntax(start));
    }
    let mut nums = Vec::new();
    let mut wildcard_seen = false;
    for field in fields {
        if field.is_empty() {
            return Err(syntax(start));
        }
        if matches!(field, "*" | "x" | "X") {
            wildcard_seen = true;
        } else {
            if wildcard_seen
                || !field.bytes().all(|b| b.is_ascii_digit())
                || (field.len() > 1 && field.starts_with('0'))
            {
                return Err(syntax(start));
            }
            let n = field.parse::<u64>().map_err(|_| syntax(start))?;
            nums.push(n);
        }
    }

    Ok(Partial { nums, full: None })
}

fn version_core(major: u64, minor: u64, patch: u64) -> Version {
    Version {
        major,
        minor,
        patch,
        pre: Vec::new(),
        build: Vec::new(),
    }
}

fn add_comparator(arm: &mut Arm, op: Op, version: Version, anchor: bool) {
    if anchor && !version.pre.is_empty() {
        arm.anchors
            .push((version.major, version.minor, version.patch));
    }
    arm.comparators.push(Comparator {
        op,
        version,
        explicit_anchor: anchor,
    });
}

fn increment_upper(
    p: &Partial,
    mode: u8,
    start: usize,
) -> Result<Version, RangeError> {
    let k = p.nums.len();
    if k == 0 {
        return Err(syntax(start));
    }

    let mut m = p.nums[0];
    let mut n = *p.nums.get(1).unwrap_or(&0);
    let mut q = *p.nums.get(2).unwrap_or(&0);

    let bump_major = match mode {
        0 => k == 1,
        1 => m > 0 || k == 1,
        _ => false,
    };
    let bump_minor = match mode {
        0 => k != 1,
        1 => !bump_major && (n > 0 || k == 2),
        _ => false,
    };
    let bump_patch = mode == 1 && !bump_major && !bump_minor;

    if bump_major {
        m = m.checked_add(1).ok_or_else(|| syntax(start))?;
        n = 0;
        q = 0;
    } else if bump_minor {
        n = n.checked_add(1).ok_or_else(|| syntax(start))?;
        q = 0;
    } else if bump_patch {
        q = q.checked_add(1).ok_or_else(|| syntax(start))?;
    }
    let mut v = version_core(m, n, q);
    v.pre.push("0".to_string());
    Ok(v)
}

fn lower_partial(p: &Partial) -> Version {
    version_core(
        p.nums[0],
        *p.nums.get(1).unwrap_or(&0),
        *p.nums.get(2).unwrap_or(&0),
    )
}

fn parse_bare(token: &Token, arm: &mut Arm) -> Result<(), RangeError> {
    let p = parse_partial_or_full(&token.text, token.start)?;
    if let Some(v) = p.full {
        add_comparator(arm, Op::Eq, v, true);
    } else if p.nums.is_empty() {
        // A wildcard operand places no bounds.
    } else if p.nums.len() == 3 {
        // A three-field partial with wildcards is an interval.
        let lower = lower_partial(&p);
        add_comparator(arm, Op::Ge, lower, true);
        if token.text.split('.').any(|f| matches!(f, "*" | "x" | "X")) {
            let upper = increment_upper(&p, 0, token.start)?;
            add_comparator(arm, Op::Lt, upper, false);
        } else {
            add_comparator(arm, Op::Eq, lower_partial(&p), true);
        }
    } else {
        let lower = lower_partial(&p);
        add_comparator(arm, Op::Ge, lower, true);
        let upper = increment_upper(&p, 0, token.start)?;
        add_comparator(arm, Op::Lt, upper, false);
    }
    Ok(())
}

fn parse_caret_tilde(token: &Token, arm: &mut Arm, caret: bool) -> Result<(), RangeError> {
    let operand_start = token.start + 1;
    let p = parse_partial_or_full(&token.text[1..], operand_start)?;
    if p.nums.is_empty() {
        return Err(syntax(token.start));
    }
    let lower = p.full.clone().unwrap_or_else(|| lower_partial(&p));
    add_comparator(arm, Op::Ge, lower, true);
    let upper = increment_upper(&p, if caret { 1 } else { 0 }, operand_start)?;
    add_comparator(arm, Op::Lt, upper, false);
    Ok(())
}

fn parse_explicit_comparator(token: &Token, arm: &mut Arm) -> Result<(), RangeError> {
    let bytes = token.text.as_bytes();
    let (op, n) = match bytes {
        [b'<', b'=', ..] => (Op::Le, 2),
        [b'>', b'=', ..] => (Op::Ge, 2),
        [b'=', ..] => (Op::Eq, 1),
        [b'<', ..] => (Op::Lt, 1),
        [b'>', ..] => (Op::Gt, 1),
        _ => return Err(syntax(token.start)),
    };
    if n >= token.text.len() || !token.text.as_bytes()[n].is_ascii_digit() {
        return Err(syntax(token.start));
    }
    let operand_start = token.start + n;
    let version = parse_version(&token.text[n..]).map_err(|error| RangeError::Version {
        at: operand_start,
        error,
    })?;
    add_comparator(arm, op, version, true);
    Ok(())
}

fn parse_arm(s: &str, tokens: &[Token]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        comparators: Vec::new(),
        anchors: Vec::new(),
    };
    let hyphens: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| t.text == "-")
        .map(|(i, _)| i)
        .collect();

    if let Some(&minus) = hyphens.first() {
        if hyphens.len() != 1 || tokens.len() != 3 || minus != 1 {
            return Err(syntax(tokens[minus].start));
        }
        let lo = parse_partial_or_full(&tokens[0].text, tokens[0].start)?;
        let hi = parse_partial_or_full(&tokens[2].text, tokens[2].start)?;
        if lo.nums.is_empty() {
            return Err(syntax(tokens[0].start));
        }
        if hi.nums.is_empty() {
            return Err(syntax(tokens[2].start));
        }
        let lower = lo.full.clone().unwrap_or_else(|| lower_partial(&lo));
        add_comparator(&mut arm, Op::Ge, lower, true);

        if let Some(v) = hi.full {
            add_comparator(&mut arm, Op::Le, v, true);
        } else if hi.nums.len() == 3
            && !tokens[2].text.split('.').any(|f| matches!(f, "*" | "x" | "X"))
        {
            add_comparator(&mut arm, Op::Le, lower_partial(&hi), true);
        } else {
            let upper = increment_upper(&hi, 0, tokens[2].start)?;
            add_comparator(&mut arm, Op::Lt, upper, false);
        }
        return Ok(arm);
    }

    for token in tokens {
        let first = token.text.as_bytes().first().copied();
        match first {
            Some(b'<') | Some(b'>') | Some(b'=') => {
                parse_explicit_comparator(token, &mut arm)?
            }
            Some(b'^') => parse_caret_tilde(token, &mut arm, true)?,
            Some(b'~') => parse_caret_tilde(token, &mut arm, false)?,
            Some(b'-') => return Err(syntax(token.start)),
            Some(b) if b.is_ascii_digit() || matches!(b, b'*' | b'x' | b'X') => {
                parse_bare(token, &mut arm)?
            }
            _ => return Err(syntax(token.start)),
        }
    }
    let _ = s;
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut nonwhite = false;
    for &b in bytes {
        if !whitespace(b) {
            nonwhite = true;
            break;
        }
    }
    if !nonwhite {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms = Vec::new();
    let mut p = 0;
    let mut arm_start = 0;
    let mut token_start = None;
    let mut i = 0;
    while i < bytes.len() {
        if whitespace(bytes[i]) {
            token_start = None;
            i += 1;
            continue;
        }
        if token_start.is_none() {
            token_start = Some(i);
        }

        if i + 1 < bytes.len() && bytes[i] == b'|' && bytes[i + 1] == b'|' {
            let standalone_left = token_start == Some(i);
            let standalone_right = i + 2 == bytes.len() || whitespace(bytes[i + 2]);
            if standalone_left && standalone_right {
                let left_end = i;
                let tokens = tokenize(s, arm_start, left_end);
                if tokens.is_empty() {
                    return Err(RangeError::Empty { at: i });
                }
                arms.push(parse_arm(s, &tokens)?);
                i += 2;
                while i < bytes.len() && whitespace(bytes[i]) {
                    i += 1;
                }
                if i == bytes.len() {
                    return Err(RangeError::Empty { at: bytes.len() });
                }
                arm_start = i;
                token_start = None;
                continue;
            }
        }
        i += 1;
    }

    let tokens = tokenize(s, arm_start, bytes.len());
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: bytes.len() });
    }
    arms.push(parse_arm(s, &tokens)?);
    let _ = p;
    Ok(Range { arms })
}

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn comparator_matches(v: &Version, c: &Comparator) -> bool {
    let ord = compare_versions(v, &c.version);
    match c.op {
        Op::Lt => ord == Ordering::Less,
        Op::Le => ord != Ordering::Greater,
        Op::Eq => ord == Ordering::Equal,
        Op::Ge => ord != Ordering::Less,
        Op::Gt => ord == Ordering::Greater,
    }
}

fn arm_matches(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !v.pre.is_empty()
        && !include_prerelease
        && !arm.anchors.contains(&core(v))
    {
        return false;
    }
    arm.comparators
        .iter()
        .all(|c| comparator_matches(v, c))
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

fn extrema(
    candidates: &[String],
    r: &Range,
    maximum: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let v = parse_version(candidate).map_err(|error| CandidateError { index, error })?;
        parsed.push(v);
    }

    let mut best: Option<usize> = None;
    for (i, v) in parsed.iter().enumerate() {
        if !matches(v, r) {
            continue;
        }
        if let Some(b) = best {
            let ord = compare_versions(v, &parsed[b]);
            if (maximum && ord == Ordering::Greater) || (!maximum && ord == Ordering::Less) {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

#[derive(Clone)]
struct Bound {
    version: Version,
    inclusive: bool,
}

fn strongest_bounds(arm: &Arm) -> (Option<Bound>, Option<Bound>, Option<Version>) {
    let mut lower: Option<Bound> = None;
    let mut upper: Option<Bound> = None;
    let mut equality: Option<Version> = None;

    for c in &arm.comparators {
        match c.op {
            Op::Eq => equality = Some(c.version.clone()),
            Op::Gt | Op::Ge => {
                let candidate = Bound {
                    version: c.version.clone(),
                    inclusive: c.op == Op::Ge,
                };
                let replace = match &lower {
                    None => true,
                    Some(old) => {
                        let ord = compare_versions(&candidate.version, &old.version);
                        ord == Ordering::Greater
                            || (ord == Ordering::Equal && !candidate.inclusive && old.inclusive)
                    }
                };
                if replace {
                    lower = Some(candidate);
                }
            }
            Op::Lt | Op::Le => {
                let candidate = Bound {
                    version: c.version.clone(),
                    inclusive: c.op == Op::Le,
                };
                let replace = match &upper {
                    None => true,
                    Some(old) => {
                        let ord = compare_versions(&candidate.version, &old.version);
                        ord == Ordering::Less
                            || (ord == Ordering::Equal && !candidate.inclusive && old.inclusive)
                    }
                };
                if replace {
                    upper = Some(candidate);
                }
            }
        }
    }

    if let Some(v) = equality {
        lower = Some(Bound {
            version: v.clone(),
            inclusive: true,
        });
        upper = Some(Bound {
            version: v.clone(),
            inclusive: true,
        });
    }
    (lower, upper, None)
}

fn meets_lower(v: &Version, b: &Option<Bound>) -> bool {
    b.as_ref().map_or(true, |bound| {
        let o = compare_versions(v, &bound.version);
        o == Ordering::Greater || (o == Ordering::Equal && bound.inclusive)
    })
}

fn meets_upper(v: &Version, b: &Option<Bound>) -> bool {
    b.as_ref().map_or(true, |bound| {
        let o = compare_versions(v, &bound.version);
        o == Ordering::Less || (o == Ordering::Equal && bound.inclusive)
    })
}

fn next_core(c: (u64, u64, u64)) -> Option<(u64, u64, u64)> {
    let (m, n, p) = c;
    if p < u64::MAX {
        Some((m, n, p + 1))
    } else if n < u64::MAX {
        Some((m, n + 1, 0))
    } else if m < u64::MAX {
        Some((m + 1, 0, 0))
    } else {
        None
    }
}

fn least_release_after(v: &Version) -> Option<Version> {
    if !v.pre.is_empty() {
        return None;
    }
    next_core(core(v)).map(|(m, n, p)| {
        let mut w = version_core(m, n, p);
        w.pre.push("0".to_string());
        w
    })
}

fn interval_has_stable(lower: &Option<Bound>, upper: &Option<Bound>) -> bool {
    let first = match lower {
        None => version_core(0, 0, 0),
        Some(b) if b.version.pre.is_empty() => {
            if b.inclusive {
                b.version.clone()
            } else {
                match least_release_after(&b.version) {
                    Some(v) => v,
                    None => return false,
                }
            }
        }
        Some(b) => version_core(b.version.major, b.version.minor, b.version.patch),
    };

    let first_stable = if !first.pre.is_empty() {
        version_core(first.major, first.minor, first.patch)
    } else {
        first
    };
    meets_lower(&first_stable, lower) && meets_upper(&first_stable, upper)
}

fn arm_has_witness(arm: &Arm) -> bool {
    let (lower, upper, _) = strongest_bounds(arm);
    if interval_has_stable(&lower, &upper) {
        return true;
    }
    for &c in &arm.anchors {
        let v = version_core(c.0, c.1, c.2);
        if meets_lower(&v, &lower) && meets_upper(&v, &upper) {
            return true;
        }
    }
    false
}

fn strip_build(mut v: Version) -> Version {
    v.build.clear();
    v
}

fn fmt_version(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

fn fmt_bound(b: &Option<Bound>, lower: bool) -> String {
    match b {
        None => "*".to_string(),
        Some(bound) => {
            let op = match (lower, bound.inclusive) {
                (true, true) => ">=",
                (true, false) => ">",
                (false, true) => "<=",
                (false, false) => "<",
            };
            format!("{}{}", op, fmt_version(&strip_build(bound.version.clone())))
        }
    }
}

pub fn canonical(r: &Range) -> String {
    let mut strings = Vec::new();
    for arm in &r.arms {
        if !arm_has_witness(arm) {
            continue;
        }
        let (lower, upper, _) = strongest_bounds(arm);
        let mut anchors = arm.anchors.clone();
        anchors.sort();
        anchors.dedup();
        let anchor_text = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors
                .iter()
                .map(|(m, n, p)| format!("{m}.{n}.{p}"))
                .collect::<Vec<_>>()
                .join(",")
        };
        strings.push(format!(
            "[{};{};pre={}]",
            fmt_bound(&lower, true),
            fmt_bound(&upper, false),
            anchor_text
        ));
    }
    strings.sort();
    strings.dedup();
    if strings.is_empty() {
        "EMPTY".to_string()
    } else {
        strings.join(" || ")
    }
}

fn arm_pair_has_witness(a: &Arm, b: &Arm, include_prerelease: bool) -> bool {
    let (al, au, _) = strongest_bounds(a);
    let (bl, bu, _) = strongest_bounds(b);

    let mut lower = al.clone();
    if let Some(candidate) = bl {
        let replace = match &lower {
            None => true,
            Some(old) => {
                let ord = compare_versions(&candidate.version, &old.version);
                ord == Ordering::Greater
                    || (ord == Ordering::Equal && !candidate.inclusive && old.inclusive)
            }
        };
        if replace {
            lower = Some(candidate);
        }
    }

    let mut upper = au.clone();
    if let Some(candidate) = bu {
        let replace = match &upper {
            None => true,
            Some(old) => {
                let ord = compare_versions(&candidate.version, &old.version);
                ord == Ordering::Less
                    || (ord == Ordering::Equal && !candidate.inclusive && old.inclusive)
            }
        };
        if replace {
            upper = Some(candidate);
        }
    }

    if interval_has_stable(&lower, &upper) {
        return true;
    }
    if include_prerelease {
        return prerelease_interval_witness(&lower, &upper);
    }

    let mut common = a.anchors.clone();
    common.retain(|c| b.anchors.contains(c));
    common.sort();
    common.dedup();
    common.into_iter().any(|c| {
        let v = version_core(c.0, c.1, c.2);
        meets_lower(&v, &lower) && meets_upper(&v, &upper)
    })
}

fn prerelease_interval_witness(lower: &Option<Bound>, upper: &Option<Bound>) -> bool {
    if interval_has_stable(lower, upper) {
        return true;
    }
    let candidate = match lower {
        None => version_core(0, 0, 0),
        Some(b) => {
            if b.version.pre.is_empty() {
                match least_release_after(&b.version) {
                    Some(v) => v,
                    None => return false,
                }
            } else if b.inclusive {
                b.version.clone()
            } else {
                let mut v = b.version.clone();
                v.pre.push("0".to_string());
                v
            }
        }
    };
    !candidate.pre.is_empty()
        && meets_lower(&candidate, lower)
        && meets_upper(&candidate, upper)
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    a.arms.iter().any(|aa| {
        b.arms
            .iter()
            .any(|bb| arm_pair_has_witness(aa, bb, include_prerelease))
    })
}