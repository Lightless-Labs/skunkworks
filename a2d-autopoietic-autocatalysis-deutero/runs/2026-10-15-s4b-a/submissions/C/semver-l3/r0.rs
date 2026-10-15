use std::cmp::Ordering;

fn main() {}

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
        return if *pos == bytes.len() || matches!(bytes[*pos], b'.' | b'-' | b'+') {
            Err(VersionError::MissingCore { at: *pos })
        } else {
            Err(VersionError::InvalidChar { at: *pos })
        };
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
    let mut out = Vec::new();
    let mut at = start;
    loop {
        let id_start = at;
        while at < end && bytes[at] != b'.' {
            if !(bytes[at].is_ascii_alphanumeric() || bytes[at] == b'-') {
                return Err(VersionError::InvalidChar { at });
            }
            at += 1;
        }
        if at == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        let id = &s[id_start..at];
        if prerelease
            && id.bytes().all(|b| b.is_ascii_digit())
            && id.len() > 1
            && id.starts_with('0')
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        out.push(id.to_string());
        if at == end {
            break;
        }
        at += 1;
        if at == end {
            return Err(VersionError::EmptyIdentifier { at });
        }
    }
    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let mut pos = 0;
    let major = parse_core(s, &mut pos)?;
    let mut core = [major, 0, 0];

    for i in 1..3 {
        if pos >= s.len() {
            return Err(VersionError::MissingCore { at: pos });
        }
        match s.as_bytes()[pos] {
            b'.' => pos += 1,
            b'-' | b'+' => return Err(VersionError::MissingCore { at: pos }),
            _ => return Err(VersionError::InvalidChar { at: pos }),
        }
        core[i] = parse_core(s, &mut pos)?;
    }

    let bytes = s.as_bytes();
    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] == b'-' {
        let pre_start = pos + 1;
        let build_sep = bytes[pre_start..]
            .iter()
            .position(|b| *b == b'+')
            .map(|n| pre_start + n);
        let pre_end = build_sep.unwrap_or(bytes.len());
        pre = parse_identifiers(s, pre_start, pre_end, true)?;
        pos = pre_end;
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        let build_start = pos + 1;
        build = parse_identifiers(s, build_start, bytes.len(), false)?;
        pos = bytes.len();
    }

    if pos < bytes.len() {
        return Err(VersionError::InvalidChar { at: pos });
    }

    Ok(Version {
        major: core[0],
        minor: core[1],
        patch: core[2],
        pre,
        build,
    })
}

fn core_tuple(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn compare_identifier(a: &str, b: &str) -> Ordering {
    let an = numeric_identifier(a);
    let bn = numeric_identifier(b);
    match (an, bn) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    let core = core_tuple(a).cmp(&core_tuple(b));
    if core != Ordering::Equal {
        return core;
    }
    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (x, y) in a.pre.iter().zip(&b.pre) {
                let ord = compare_identifier(x, y);
                if ord != Ordering::Equal {
                    return ord;
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
struct Comparison {
    op: Op,
    version: Version,
    anchor: bool,
}

#[derive(Clone, Debug)]
struct Arm {
    comparisons: Vec<Comparison>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

fn ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn cmp_matches(v: &Version, c: &Comparison) -> bool {
    let ord = compare_versions(v, &c.version);
    match c.op {
        Op::Eq => ord == Ordering::Equal,
        Op::Lt => ord == Ordering::Less,
        Op::Le => ord != Ordering::Greater,
        Op::Gt => ord == Ordering::Greater,
        Op::Ge => ord != Ordering::Less,
    }
}

fn arm_matches(v: &Version, arm: &Arm, include_pre: bool) -> bool {
    if !arm.comparisons.iter().all(|c| cmp_matches(v, c)) {
        return false;
    }
    v.pre.is_empty()
        || include_pre
        || arm.anchors.contains(&core_tuple(v))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms
        .iter()
        .any(|arm| arm_matches(v, arm, include_prerelease))
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
        match parse_version(candidate) {
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
            if (maximum && ord == Ordering::Greater) || (!maximum && ord == Ordering::Less) {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

fn make_version(core: (u64, u64, u64), pre: Vec<String>, build: Vec<String>) -> Version {
    Version {
        major: core.0,
        minor: core.1,
        patch: core.2,
        pre,
        build,
    }
}

fn full_error_operand(s: &str, start: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at: start, error })
}

#[derive(Clone, Debug)]
struct Partial {
    core: (u64, u64, u64),
    k: usize,
    full: Option<Version>,
    start: usize,
}

fn parse_partial(token: &str, start: usize) -> Result<Partial, RangeError> {
    if token.contains('-') || token.contains('+') {
        let v = full_error_operand(token, start)?;
        return Ok(Partial {
            core: core_tuple(&v),
            k: 3,
            full: Some(v),
            start,
        });
    }
    let fields: Vec<&str> = token.split('.').collect();
    if fields.len() >= 3 && !fields.iter().any(|f| matches!(*f, "*" | "x" | "X")) {
        let v = full_error_operand(token, start)?;
        return Ok(Partial {
            core: core_tuple(&v),
            k: 3,
            full: Some(v),
            start,
        });
    }
    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at: start });
    }
    let mut core = [0u64; 3];
    let mut wildcard_seen = false;
    let mut k = 0;
    for (i, field) in fields.iter().enumerate() {
        if field.is_empty() {
            return Err(RangeError::Syntax { at: start });
        }
        if matches!(*field, "*" | "x" | "X") {
            wildcard_seen = true;
        } else {
            if wildcard_seen
                || !field.bytes().all(|b| b.is_ascii_digit())
                || (field.len() > 1 && field.starts_with('0'))
            {
                return Err(RangeError::Syntax { at: start });
            }
            let value = field.parse::<u64>().map_err(|_| RangeError::Syntax { at: start })?;
            core[i] = value;
            k += 1;
        }
    }
    Ok(Partial {
        core: (core[0], core[1], core[2]),
        k,
        full: None,
        start,
    })
}

fn upper_increment(
    core: (u64, u64, u64),
    field: usize,
    start: usize,
) -> Result<Version, RangeError> {
    let mut values = [core.0, core.1, core.2];
    values[field] = values[field]
        .checked_add(1)
        .ok_or(RangeError::Syntax { at: start })?;
    for value in values.iter_mut().skip(field + 1) {
        *value = 0;
    }
    Ok(make_version((values[0], values[1], values[2]), vec!["0".to_string()], vec![]))
}

fn add_cmp(arm: &mut Arm, op: Op, version: Version, anchor: bool) {
    if anchor && !version.pre.is_empty() {
        arm.anchors.push(core_tuple(&version));
    }
    arm.comparisons.push(Comparison { op, version, anchor });
}

fn parse_full_comparator(token: &str, start: usize, arm: &mut Arm) -> Result<bool, RangeError> {
    let (op, consumed) = if token.starts_with(">=") {
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
        return Ok(false);
    };
    let operand = &token[consumed..];
    let operand_start = start + consumed;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: start });
    }
    let version = full_error_operand(operand, operand_start)?;
    add_cmp(arm, op, version, true);
    Ok(true)
}

fn parse_bare(token: &str, start: usize, arm: &mut Arm) -> Result<(), RangeError> {
    let p = parse_partial(token, start)?;
    if let Some(v) = p.full {
        add_cmp(arm, Op::Eq, v, true);
        return Ok(());
    }
    if p.k == 0 {
        return Ok(());
    }
    let low = make_version(p.core, vec![], vec![]);
    if p.k == 3 {
        add_cmp(arm, Op::Eq, low, false);
        return Ok(());
    }
    add_cmp(arm, Op::Ge, low, false);
    let field = if p.k == 1 { 0 } else { 1 };
    let upper = upper_increment(p.core, field, p.start)?;
    add_cmp(arm, Op::Lt, upper, false);
    Ok(())
}

fn parse_caret_tilde(token: &str, start: usize, arm: &mut Arm) -> Result<(), RangeError> {
    let caret = token.starts_with('^');
    let operand = &token[1..];
    let operand_start = start + 1;
    let p = parse_partial(operand, operand_start)?;
    if p.k == 0 {
        return Err(RangeError::Syntax { at: start });
    }
    let low = p.full.clone().unwrap_or_else(|| make_version(p.core, vec![], vec![]));
    add_cmp(arm, Op::Ge, low, true);
    let field = if caret {
        if p.core.0 > 0 || p.k == 1 {
            0
        } else if p.core.1 > 0 || p.k == 2 {
            1
        } else {
            2
        }
    } else if p.k == 1 {
        0
    } else {
        1
    };
    let upper = upper_increment(p.core, field, operand_start)?;
    add_cmp(arm, Op::Lt, upper, false);
    Ok(())
}

fn lex_tokens(s: &str, start: usize, end: usize) -> Vec<(&str, usize)> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut at = start;
    while at < end {
        while at < end && ws(bytes[at]) {
            at += 1;
        }
        if at >= end {
            break;
        }
        let token_start = at;
        while at < end && !ws(bytes[at]) {
            at += 1;
        }
        out.push((&s[token_start..at], token_start));
    }
    out
}

fn parse_hyphen_arm(tokens: &[(&str, usize)]) -> Result<Arm, RangeError> {
    let hyphen_index = tokens.iter().position(|(t, _)| *t == "-").unwrap_or(0);
    if tokens.len() != 3 || hyphen_index != 1 {
        return Err(RangeError::Syntax {
            at: tokens[hyphen_index].1,
        });
    }
    let lower = parse_partial(tokens[0].0, tokens[0].1)?;
    let upper = parse_partial(tokens[2].0, tokens[2].1)?;
    if lower.k == 0 {
        return Err(RangeError::Syntax { at: tokens[0].1 });
    }
    if upper.k == 0 {
        return Err(RangeError::Syntax { at: tokens[2].1 });
    }
    let mut arm = Arm {
        comparisons: Vec::new(),
        anchors: Vec::new(),
    };
    let low_version = lower
        .full
        .clone()
        .unwrap_or_else(|| make_version(lower.core, vec![], vec![]));
    add_cmp(&mut arm, Op::Ge, low_version, true);
    if let Some(v) = upper.full {
        add_cmp(&mut arm, Op::Le, v, true);
    } else if upper.k == 3 {
        add_cmp(&mut arm, Op::Le, make_version(upper.core, vec![], vec![]), false);
    } else {
        let field = if upper.k == 1 { 0 } else { 1 };
        add_cmp(&mut arm, Op::Lt, upper_increment(upper.core, field, upper.start)?, false);
    }
    Ok(arm)
}

fn parse_arm(s: &str, start: usize, end: usize) -> Result<Arm, RangeError> {
    let tokens = lex_tokens(s, start, end);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: start });
    }
    if tokens.iter().any(|(token, _)| *token == "-") {
        return parse_hyphen_arm(&tokens);
    }

    let mut arm = Arm {
        comparisons: Vec::new(),
        anchors: Vec::new(),
    };
    for (token, at) in tokens {
        if parse_full_comparator(token, at, &mut arm)? {
            continue;
        }
        if token.starts_with('^') || token.starts_with('~') {
            parse_caret_tilde(token, at, &mut arm)?;
        } else if token.starts_with(|c: char| c.is_ascii_digit())
            || matches!(token, "*" | "x" | "X")
        {
            parse_bare(token, at, &mut arm)?;
        } else {
            return Err(RangeError::Syntax { at });
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
        if bytes[i] == b'|' && i + 1 < bytes.len() && bytes[i + 1] == b'|' {
            let before = i == 0 || ws(bytes[i - 1]);
            let after = i + 2 == bytes.len() || ws(bytes[i + 2]);
            if before && after {
                separators.push(i);
                let arm_end = i;
                let parsed = parse_arm(s, arm_start, arm_end)?;
                arms.push(parsed);
                i += 2;
                arm_start = i;
                while arm_start < bytes.len() && ws(bytes[arm_start]) {
                    arm_start += 1;
                }
                i = arm_start;
                continue;
            }
        }
        i += 1;
    }

    if separators.is_empty() {
        let first = bytes.iter().position(|b| !ws(*b));
        if first.is_none() {
            return Err(RangeError::Empty { at: 0 });
        }
        let last = bytes.iter().rposition(|b| !ws(*b)).unwrap_or(0) + 1;
        arms.push(parse_arm(s, first.unwrap_or(0), last)?);
    } else {
        let first = bytes.iter().position(|b| !ws(*b));
        if first.is_none() {
            return Err(RangeError::Empty { at: 0 });
        }
        if arm_start >= bytes.len() {
            return Err(RangeError::Empty { at: bytes.len() });
        }
        let last = bytes.iter().rposition(|b| !ws(*b)).unwrap_or(0) + 1;
        arms.push(parse_arm(s, arm_start, last)?);
    }
    Ok(Range { arms })
}

fn cmp_strength_lower(a: &Comparison, b: &Comparison) -> Ordering {
    let ord = compare_versions(&a.version, &b.version);
    if ord != Ordering::Equal {
        return ord;
    }
    match (&a.op, &b.op) {
        (Op::Gt, Op::Ge) => Ordering::Greater,
        (Op::Ge, Op::Gt) => Ordering::Less,
        _ => Ordering::Equal,
    }
}

fn cmp_strength_upper(a: &Comparison, b: &Comparison) -> Ordering {
    let ord = compare_versions(&a.version, &b.version);
    if ord != Ordering::Equal {
        return ord;
    }
    match (&a.op, &b.op) {
        (Op::Lt, Op::Le) => Ordering::Less,
        (Op::Le, Op::Lt) => Ordering::Greater,
        _ => Ordering::Equal,
    }
}

fn version_string(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

pub fn canonical(r: &Range) -> String {
    let mut rendered = Vec::new();
    for arm in &r.arms {
        let mut lower: Option<&Comparison> = None;
        let mut upper: Option<&Comparison> = None;
        let mut impossible = false;
        for c in &arm.comparisons {
            match c.op {
                Op::Eq => {
                    let lo = Comparison { op: Op::Ge, version: c.version.clone(), anchor: false };
                    let hi = Comparison { op: Op::Le, version: c.version.clone(), anchor: false };
                    lower = Some(match lower {
                        Some(old) if cmp_strength_lower(old, &lo) == Ordering::Greater => old,
                        _ => Box::leak(Box::new(lo)),
                    });
                    upper = Some(match upper {
                        Some(old) if cmp_strength_upper(old, &hi) == Ordering::Less => old,
                        _ => Box::leak(Box::new(hi)),
                    });
                }
                Op::Ge | Op::Gt => {
                    if lower.map_or(true, |old| cmp_strength_lower(old, c) == Ordering::Less) {
                        lower = Some(c);
                    }
                }
                Op::Le | Op::Lt => {
                    if upper.map_or(true, |old| cmp_strength_upper(old, c) == Ordering::Greater) {
                        upper = Some(c);
                    }
                }
            }
        }
        if let (Some(lo), Some(hi)) = (lower, upper) {
            let ord = compare_versions(&lo.version, &hi.version);
            if ord == Ordering::Greater
                || (ord == Ordering::Equal
                    && (matches!(lo.op, Op::Gt) || matches!(hi.op, Op::Lt)))
            {
                impossible = true;
            }
        }
        if !impossible {
            let low = lower.map_or("*".to_string(), |c| {
                format!(
                    "{}{}",
                    if matches!(c.op, Op::Gt) { ">" } else { ">=" },
                    version_string(&c.version)
                )
            });
            let high = upper.map_or("*".to_string(), |c| {
                format!(
                    "{}{}",
                    if matches!(c.op, Op::Lt) { "<" } else { "<=" },
                    version_string(&c.version)
                )
            });
            let mut anchors = arm.anchors.clone();
            anchors.sort();
            anchors.dedup();
            let anchors_s = if anchors.is_empty() {
                "-".to_string()
            } else {
                anchors
                    .iter()
                    .map(|(a, b, c)| format!("{a}.{b}.{c}"))
                    .collect::<Vec<_>>()
                    .join(",")
            };
            rendered.push(format!("[{low};{high};pre={anchors_s}]"));
        }
    }
    rendered.sort();
    rendered.dedup();
    if rendered.is_empty() {
        "EMPTY".to_string()
    } else {
        rendered.join(" || ")
    }
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for aa in &a.arms {
        for bb in &b.arms {
            if arms_intersect(aa, bb, include_prerelease) {
                return true;
            }
        }
    }
    false
}

fn arms_intersect(a: &Arm, b: &Arm, include_pre: bool) -> bool {
    let mut all = a.comparisons.clone();
    all.extend(b.comparisons.clone());
    let mut lower: Option<&Version> = None;
    let mut upper: Option<&Version> = None;
    let mut lower_strict = false;
    let mut upper_strict = false;
    let mut equal: Option<&Version> = None;

    for c in &all {
        match c.op {
            Op::Eq => equal = Some(&c.version),
            Op::Ge | Op::Gt => {
                let replace = lower.map_or(true, |v| compare_versions(&c.version, v) == Ordering::Greater);
                if replace {
                    lower = Some(&c.version);
                    lower_strict = matches!(c.op, Op::Gt);
                } else if lower.map_or(false, |v| compare_versions(&c.version, v) == Ordering::Equal) {
                    lower_strict |= matches!(c.op, Op::Gt);
                }
            }
            Op::Le | Op::Lt => {
                let replace = upper.map_or(true, |v| compare_versions(&c.version, v) == Ordering::Less);
                if replace {
                    upper = Some(&c.version);
                    upper_strict = matches!(c.op, Op::Lt);
                } else if upper.map_or(false, |v| compare_versions(&c.version, v) == Ordering::Equal) {
                    upper_strict |= matches!(c.op, Op::Lt);
                }
            }
        }
    }
    if let Some(eq) = equal {
        if all.iter().any(|c| !cmp_matches(eq, c)) {
            return false;
        }
        return prerelease_admitted(eq, a, b, include_pre);
    }

    if let (Some(lo), Some(hi)) = (lower, upper) {
        let ord = compare_versions(lo, hi);
        if ord == Ordering::Greater || (ord == Ordering::Equal && (lower_strict || upper_strict)) {
            return false;
        }
    }

    if include_pre {
        return true;
    }
    let mut anchors = a.anchors.clone();
    anchors.retain(|x| b.anchors.contains(x));
    for tuple in anchors {
        let witness = Version {
            major: tuple.0,
            minor: tuple.1,
            patch: tuple.2,
            pre: vec!["0".to_string()],
            build: vec![],
        };
        if all.iter().all(|c| cmp_matches(&witness, c)) {
            return true;
        }
    }

    if interval_has_stable_witness(lower, lower_strict, upper, upper_strict) {
        return true;
    }
    false
}

fn prerelease_admitted(v: &Version, a: &Arm, b: &Arm, include_pre: bool) -> bool {
    if v.pre.is_empty() || include_pre {
        return true;
    }
    a.anchors.contains(&core_tuple(v)) && b.anchors.contains(&core_tuple(v))
}

fn interval_has_stable_witness(
    lower: Option<&Version>,
    lower_strict: bool,
    upper: Option<&Version>,
    upper_strict: bool,
) -> bool {
    let mut candidate = if let Some(lo) = lower {
        if lo.pre.is_empty() {
            if lower_strict {
                next_release(lo)
            } else {
                Some(lo.clone())
            }
        } else {
            Some(make_version(core_tuple(lo), vec![], vec![]))
        }
    } else {
        Some(make_version((0, 0, 0), vec![], vec![]))
    };
    let v = match candidate.take() {
        Some(v) => v,
        None => return false,
    };
    if let Some(hi) = upper {
        let ord = compare_versions(&v, hi);
        if ord == Ordering::Greater || (ord == Ordering::Equal && upper_strict) {
            return false;
        }
    }
    true
}

fn next_release(v: &Version) -> Option<Version> {
    let mut values = [v.major, v.minor, v.patch];
    if values[2] < u64::MAX {
        values[2] += 1;
    } else if values[1] < u64::MAX {
        values[1] += 1;
        values[2] = 0;
    } else if values[0] < u64::MAX {
        values[0] += 1;
        values[1] = 0;
        values[2] = 0;
    } else {
        return None;
    }
    Some(make_version((values[0], values[1], values[2]), vec![], vec![]))
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
    fn version_parsing_and_precedence() {
        let v = pv("2.1.3-rc.1+build.7");
        assert_eq!((v.major, v.minor, v.patch), (2, 1, 3));
        assert_eq!(v.pre, vec!["rc", "1"]);
        assert_eq!(v.build, vec!["build", "7"]);
        assert_eq!(
            compare_versions(&pv("2.1.3-alpha"), &pv("2.1.3-beta")),
            Ordering::Less
        );
        assert_eq!(
            parse_version("01.0.0"),
            Err(VersionError::LeadingZero { at: 0 })
        );
        assert_eq!(
            parse_version("1.2"),
            Err(VersionError::MissingCore { at: 3 })
        );
    }

    #[test]
    fn range_expansions_and_extrema() {
        for s in ["^2.1.0", "~2.5.0", "2.5.x", "2.1 - 2.8"] {
            assert!(matches(&pv("2.5.1"), &pr(s)));
        }
        let candidates = vec![
            "2.2.0".to_string(),
            "2.8.0".to_string(),
            "3.0.0".to_string(),
        ];
        let range = pr("2.x");
        assert_eq!(max_satisfying(&candidates, &range), Ok(Some("2.8.0".into())));
        assert_eq!(min_satisfying(&candidates, &range), Ok(Some("2.2.0".into())));
    }

    #[test]
    fn disjunction_and_prerelease_admission() {
        assert!(matches(&pv("4.0.0"), &pr("2.x || 4.0.0")));
        let range = pr(">=2.1.0 <2.8.0");
        assert!(matches_with(&pv("2.5.0-rc"), &range, true));
        assert!(!matches(&pv("2.5.0-rc"), &range));
        assert!(matches(
            &pv("2.1.3-rc"),
            &pr(">=2.1.3-beta <2.1.3")
        ));
    }

    #[test]
    fn intersection_and_canonicalization() {
        let range = pr(">=2.1.0 <2.8.0");
        assert!(intersects(&range, &pr("2.5.x"), false));
        assert_eq!(canonical(&range), "[>=2.1.0;<2.8.0;pre=-]");
        assert_eq!(canonical(&pr(">=2.0.0 <2.0.0")), "EMPTY");
        assert!(!intersects(&pr("2.x"), &pr("3.x"), true));
    }
}