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

fn parse_core_number(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if *pos == start {
        if *pos == bytes.len() || matches!(bytes[*pos], b'.' | b'-' | b'+') {
            return Err(VersionError::MissingCore { at: *pos });
        }
        return Err(VersionError::InvalidChar { at: *pos });
    }
    let digits = &s[start..*pos];
    if digits.len() > 1 && digits.as_bytes()[0] == b'0' {
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
    if start == end {
        return Err(VersionError::EmptyIdentifier { at: start });
    }
    let mut result = Vec::new();
    let mut id_start = start;
    loop {
        let mut id_end = id_start;
        while id_end < end && bytes[id_end] != b'.' {
            id_end += 1;
        }
        if id_start == id_end {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        for i in id_start..id_end {
            let b = bytes[i];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: i });
            }
        }
        let id = &s[id_start..id_end];
        if prerelease
            && id.len() > 1
            && id.as_bytes()[0] == b'0'
            && id.as_bytes().iter().all(u8::is_ascii_digit)
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        result.push(id.to_string());
        if id_end == end {
            break;
        }
        id_start = id_end + 1;
        if id_start == end {
            return Err(VersionError::EmptyIdentifier { at: id_start });
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
    let major = parse_core_number(s, &mut pos)?;

    for field in 0..2 {
        if pos == bytes.len() {
            return Err(VersionError::MissingCore { at: pos });
        }
        if bytes[pos] != b'.' {
            if matches!(bytes[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            return Err(VersionError::InvalidChar { at: pos });
        }
        pos += 1;

        let number = parse_core_number(s, &mut pos)?;
        if field == 0 {
            let minor = number;

            if pos == bytes.len() {
                return Err(VersionError::MissingCore { at: pos });
            }
            if bytes[pos] != b'.' {
                if matches!(bytes[pos], b'-' | b'+') {
                    return Err(VersionError::MissingCore { at: pos });
                }
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;

            let patch = parse_core_number(s, &mut pos)?;
            let mut pre = Vec::new();
            let mut build = Vec::new();

            if pos < bytes.len() && bytes[pos] == b'-' {
                let pre_start = pos + 1;
                let plus = bytes[pre_start..]
                    .iter()
                    .position(|&b| b == b'+')
                    .map(|i| pre_start + i);
                let pre_end = plus.unwrap_or(bytes.len());
                pre = parse_identifiers(s, pre_start, pre_end, true)?;
                pos = pre_end;
            }

            if pos < bytes.len() && bytes[pos] == b'+' {
                build = parse_identifiers(s, pos + 1, bytes.len(), false)?;
                pos = bytes.len();
            }

            if pos < bytes.len() {
                return Err(VersionError::InvalidChar { at: pos });
            }

            return Ok(Version {
                major,
                minor,
                patch,
                pre,
                build,
            });
        }
    }
    unreachable!()
}

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn numeric_id(s: &str) -> bool {
    !s.is_empty() && s.as_bytes().iter().all(u8::is_ascii_digit)
}

fn cmp_pre_id(a: &str, b: &str) -> Ordering {
    match (numeric_id(a), numeric_id(b)) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a
            .len()
            .cmp(&b.len())
            .then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (x, y) in a.iter().zip(b) {
                let ord = cmp_pre_id(x, y);
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            a.len().cmp(&b.len())
        }
    }
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    core(a)
        .cmp(&core(b))
        .then_with(|| cmp_pre(&a.pre, &b.pre))
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
struct Cmp {
    op: Op,
    version: Version,
    anchor: bool,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Vec<Cmp>>,
}

#[derive(Clone, Copy)]
struct Token<'a> {
    text: &'a str,
    start: usize,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokens_in(s: &str, start: usize, end: usize) -> Vec<Token<'_>> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut i = start;
    while i < end {
        while i < end && is_ws(bytes[i]) {
            i += 1;
        }
        if i == end {
            break;
        }
        let token_start = i;
        while i < end && !is_ws(bytes[i]) {
            i += 1;
        }
        result.push(Token {
            text: &s[token_start..i],
            start: token_start,
        });
    }
    result
}

fn full_version_operand(text: &str, start: usize) -> Result<(Op, Version, bool), RangeError> {
    let (op, n) = if text.starts_with(">=") {
        (Op::Ge, 2)
    } else if text.starts_with("<=") {
        (Op::Le, 2)
    } else if text.starts_with('>') {
        (Op::Gt, 1)
    } else if text.starts_with('<') {
        (Op::Lt, 1)
    } else if text.starts_with('=') {
        (Op::Eq, 1)
    } else {
        (Op::Eq, 0)
    };
    if n == text.len() || !text.as_bytes()[n].is_ascii_digit() {
        return Err(RangeError::Syntax { at: start });
    }
    let version = parse_version(&text[n..]).map_err(|error| RangeError::Version {
        at: start + n,
        error,
    })?;
    let anchor = !version.pre.is_empty();
    Ok((op, version, anchor))
}

#[derive(Clone, Debug)]
struct Partial {
    nums: Vec<u64>,
    full: Option<Version>,
    start: usize,
}

fn parse_partial(token: Token<'_>) -> Result<Partial, RangeError> {
    let s = token.text;
    let fields: Vec<&str> = s.split('.').collect();
    if s.contains('-')
        || s.contains('+')
        || (fields.len() >= 3 && !fields.iter().any(|f| matches!(*f, "*" | "x" | "X")))
    {
        let version = parse_version(s).map_err(|error| RangeError::Version {
            at: token.start,
            error,
        })?;
        return Ok(Partial {
            nums: vec![version.major, version.minor, version.patch],
            full: Some(version),
            start: token.start,
        });
    }
    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at: token.start });
    }
    let mut nums = Vec::new();
    let mut wildcard_seen = false;
    for field in fields {
        if field.is_empty() {
            return Err(RangeError::Syntax { at: token.start });
        }
        if matches!(field, "*" | "x" | "X") {
            wildcard_seen = true;
        } else {
            if wildcard_seen
                || !field.as_bytes().iter().all(u8::is_ascii_digit)
                || (field.len() > 1 && field.as_bytes()[0] == b'0')
            {
                return Err(RangeError::Syntax { at: token.start });
            }
            nums.push(
                field
                    .parse::<u64>()
                    .map_err(|_| RangeError::Syntax { at: token.start })?,
            );
        }
    }
    Ok(Partial {
        nums,
        full: None,
        start: token.start,
    })
}

fn version_tuple(major: u64, minor: u64, patch: u64, pre: Vec<String>) -> Version {
    Version {
        major,
        minor,
        patch,
        pre,
        build: Vec::new(),
    }
}

fn zero_pre() -> Vec<String> {
    vec!["0".to_string()]
}

fn increment(value: u64, at: usize) -> Result<u64, RangeError> {
    value.checked_add(1).ok_or(RangeError::Syntax { at })
}

fn padded(p: &Partial) -> (u64, u64, u64) {
    (
        *p.nums.first().unwrap_or(&0),
        *p.nums.get(1).unwrap_or(&0),
        *p.nums.get(2).unwrap_or(&0),
    )
}

fn add_cmp(out: &mut Vec<Cmp>, op: Op, version: Version, anchor: bool) {
    out.push(Cmp {
        op,
        version,
        anchor,
    });
}

fn expand_bare(token: Token<'_>, out: &mut Vec<Cmp>) -> Result<(), RangeError> {
    let p = parse_partial(token)?;
    if let Some(v) = p.full {
        add_cmp(out, Op::Eq, v.clone(), !v.pre.is_empty());
        return Ok(());
    }
    let k = p.nums.len();
    if k == 0 {
        return Ok(());
    }
    let (major, minor, patch) = padded(&p);
    add_cmp(
        out,
        Op::Ge,
        version_tuple(major, minor, patch, Vec::new()),
        false,
    );
    if k == 1 {
        add_cmp(
            out,
            Op::Lt,
            version_tuple(increment(major, p.start)?, 0, 0, zero_pre()),
            false,
        );
    } else if k == 2 {
        add_cmp(
            out,
            Op::Lt,
            version_tuple(major, increment(minor, p.start)?, 0, zero_pre()),
            false,
        );
    }
    Ok(())
}

fn expand_caret_tilde(
    token: Token<'_>,
    out: &mut Vec<Cmp>,
    caret: bool,
) -> Result<(), RangeError> {
    let operand = Token {
        text: &token.text[1..],
        start: token.start + 1,
    };
    let p = parse_partial(operand)?;
    let k = p.nums.len();
    if k == 0 {
        return Err(RangeError::Syntax { at: token.start });
    }
    let (major, minor, patch) = padded(&p);
    let lower = p
        .full
        .clone()
        .unwrap_or_else(|| version_tuple(major, minor, patch, Vec::new()));
    add_cmp(out, Op::Ge, lower.clone(), !lower.pre.is_empty());

    let upper = if caret {
        if major > 0 || k == 1 {
            version_tuple(increment(major, operand.start)?, 0, 0, zero_pre())
        } else if minor > 0 || k == 2 {
            version_tuple(major, increment(minor, operand.start)?, 0, zero_pre())
        } else {
            version_tuple(
                major,
                minor,
                increment(patch, operand.start)?,
                zero_pre(),
            )
        }
    } else if k == 1 {
        version_tuple(increment(major, operand.start)?, 0, 0, zero_pre())
    } else {
        version_tuple(major, increment(minor, operand.start)?, 0, zero_pre())
    };
    add_cmp(out, Op::Lt, upper, false);
    Ok(())
}

fn expand_hyphen(tokens: &[Token<'_>], out: &mut Vec<Cmp>) -> Result<(), RangeError> {
    let lower = parse_partial(tokens[0])?;
    let upper = parse_partial(tokens[2])?;
    if lower.nums.is_empty() {
        return Err(RangeError::Syntax {
            at: tokens[0].start,
        });
    }
    if upper.nums.is_empty() {
        return Err(RangeError::Syntax {
            at: tokens[2].start,
        });
    }

    let (lm, ln, lp) = padded(&lower);
    let lower_v = lower
        .full
        .clone()
        .unwrap_or_else(|| version_tuple(lm, ln, lp, Vec::new()));
    add_cmp(out, Op::Ge, lower_v.clone(), !lower_v.pre.is_empty());

    if let Some(upper_v) = upper.full.clone() {
        add_cmp(out, Op::Le, upper_v.clone(), !upper_v.pre.is_empty());
    } else {
        let (um, un, _) = padded(&upper);
        let k = upper.nums.len();
        let end = if k == 1 {
            version_tuple(increment(um, upper.start)?, 0, 0, zero_pre())
        } else {
            version_tuple(um, increment(un, upper.start)?, 0, zero_pre())
        };
        add_cmp(out, Op::Lt, end, false);
    }
    Ok(())
}

fn parse_arm(tokens: &[Token<'_>]) -> Result<Vec<Cmp>, RangeError> {
    if tokens.is_empty() {
        return Ok(Vec::new());
    }
    let minus: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| (t.text == "-").then_some(i))
        .collect();

    if !minus.is_empty() {
        if tokens.len() != 3 || minus != [1] {
            return Err(RangeError::Syntax {
                at: tokens[minus[0]].start,
            });
        }
        let mut out = Vec::new();
        expand_hyphen(tokens, &mut out)?;
        return Ok(out);
    }

    let mut out = Vec::new();
    for token in tokens {
        let first = token.text.as_bytes()[0];
        match first {
            b'<' | b'>' | b'=' => {
                let (op, version, anchor) = full_version_operand(token.text, token.start)?;
                add_cmp(&mut out, op, version, anchor);
            }
            b'^' | b'~' => {
                if token.text.len() == 1 {
                    return Err(RangeError::Syntax { at: token.start });
                }
                expand_caret_tilde(*token, &mut out, first == b'^')?;
            }
            b'0'..=b'9' | b'*' | b'x' | b'X' => expand_bare(*token, &mut out)?,
            _ => return Err(RangeError::Syntax { at: token.start }),
        }
    }
    Ok(out)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    if bytes.iter().all(|&b| is_ws(b)) {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut segments = Vec::new();
    let mut arm_start = 0;
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i..].starts_with(b"||") {
            let before_ws = i > 0 && is_ws(bytes[i - 1]);
            let after = i + 2;
            let after_ws = after < bytes.len() && is_ws(bytes[after]);
            if before_ws && after_ws {
                segments.push((arm_start, i, Some(i)));
                arm_start = after;
                i = after;
                continue;
            }
        }
        i += 1;
    }
    segments.push((arm_start, bytes.len(), None));

    let mut arms = Vec::new();
    for (start, end, separator) in segments {
        let tokens = tokens_in(s, start, end);
        if tokens.is_empty() {
            return Err(RangeError::Empty {
                at: separator.unwrap_or(bytes.len()),
            });
        }
        arms.push(parse_arm(&tokens)?);
    }
    Ok(Range { arms })
}

fn compare_one(v: &Version, c: &Cmp) -> bool {
    match compare_versions(v, &c.version) {
        Ordering::Equal => matches!(c.op, Op::Eq | Op::Le | Op::Ge),
        Ordering::Less => matches!(c.op, Op::Lt | Op::Le),
        Ordering::Greater => matches!(c.op, Op::Gt | Op::Ge),
    }
}

fn arm_admits(v: &Version, arm: &[Cmp]) -> bool {
    arm.iter()
        .any(|c| c.anchor && core(&c.version) == core(v))
}

fn arm_matches(v: &Version, arm: &[Cmp], include_prerelease: bool) -> bool {
    (include_prerelease || v.pre.is_empty() || arm_admits(v, arm))
        && arm.iter().all(|c| compare_one(v, c))
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

fn satisfying_extreme(
    candidates: &[String],
    range: &Range,
    maximum: bool,
) -> Result<Option<String>, CandidateError> {
    let parsed: Vec<Version> = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            parse_version(candidate).map_err(|error| CandidateError { index, error })
        })
        .collect::<Result<_, _>>()?;

    let mut best: Option<usize> = None;
    for (i, version) in parsed.iter().enumerate() {
        if !matches(version, range) {
            continue;
        }
        if let Some(old) = best {
            let order = compare_versions(version, &parsed[old]);
            if (maximum && order == Ordering::Greater)
                || (!maximum && order == Ordering::Less)
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
    satisfying_extreme(candidates, r, true)
}

pub fn min_satisfying(
    candidates: &[String],
    r: &Range,
) -> Result<Option<String>, CandidateError> {
    satisfying_extreme(candidates, r, false)
}

fn tuple_version(t: (u64, u64, u64), pre: Vec<String>) -> Version {
    version_tuple(t.0, t.1, t.2, pre)
}

fn tuple_successor(t: (u64, u64, u64)) -> Option<(u64, u64, u64)> {
    if t.2 < u64::MAX {
        Some((t.0, t.1, t.2 + 1))
    } else if t.1 < u64::MAX {
        Some((t.0, t.1 + 1, 0))
    } else if t.0 < u64::MAX {
        Some((t.0 + 1, 0, 0))
    } else {
        None
    }
}

fn tighten_lower(slot: &mut Option<(Version, bool)>, v: Version, inclusive: bool) {
    let replace = match slot {
        None => true,
        Some((old, old_inc)) => match compare_versions(&v, old) {
            Ordering::Greater => true,
            Ordering::Equal => !inclusive && *old_inc,
            Ordering::Less => false,
        },
    };
    if replace {
        *slot = Some((v, inclusive));
    }
}

fn tighten_upper(slot: &mut Option<(Version, bool)>, v: Version, inclusive: bool) {
    let replace = match slot {
        None => true,
        Some((old, old_inc)) => match compare_versions(&v, old) {
            Ordering::Less => true,
            Ordering::Equal => !inclusive && *old_inc,
            Ordering::Greater => false,
        },
    };
    if replace {
        *slot = Some((v, inclusive));
    }
}

fn arm_tuple_bounds(
    arm: &[Cmp],
) -> (
    Option<(Version, bool)>,
    Option<(Version, bool)>,
    Vec<(u64, u64, u64)>,
) {
    let mut lower = None;
    let mut upper = None;
    let mut anchors = Vec::new();

    for c in arm {
        if c.anchor {
            anchors.push(core(&c.version));
        }
        match c.op {
            Op::Eq => {
                tighten_lower(&mut lower, c.version.clone(), true);
                tighten_upper(&mut upper, c.version.clone(), true);
            }
            Op::Gt => tighten_lower(&mut lower, c.version.clone(), false),
            Op::Ge => tighten_lower(&mut lower, c.version.clone(), true),
            Op::Lt => tighten_upper(&mut upper, c.version.clone(), false),
            Op::Le => tighten_upper(&mut upper, c.version.clone(), true),
        }
    }
    (lower, upper, anchors)
}

fn within_bounds(
    v: &Version,
    lower: &Option<(Version, bool)>,
    upper: &Option<(Version, bool)>,
) -> bool {
    if let Some((l, inclusive)) = lower {
        let ord = compare_versions(v, l);
        if ord == Ordering::Less || (ord == Ordering::Equal && !inclusive) {
            return false;
        }
    }
    if let Some((u, inclusive)) = upper {
        let ord = compare_versions(v, u);
        if ord == Ordering::Greater || (ord == Ordering::Equal && !inclusive) {
            return false;
        }
    }
    true
}

fn has_stable_witness(
    lower: &Option<(Version, bool)>,
    upper: &Option<(Version, bool)>,
) -> bool {
    let candidate = if let Some((l, inclusive)) = lower {
        if l.pre.is_empty() {
            if *inclusive {
                l.clone()
            } else if let Some(next) = tuple_successor(core(l)) {
                tuple_version(next, Vec::new())
            } else {
                return false;
            }
        } else {
            tuple_version(core(l), Vec::new())
        }
    } else {
        tuple_version((0, 0, 0), Vec::new())
    };
    within_bounds(&candidate, lower, upper)
}

fn satisfying_witness(
    lower: &Option<(Version, bool)>,
    upper: &Option<(Version, bool)>,
    anchors: &[(u64, u64, u64)],
) -> bool {
    if let (Some((l, li)), Some((u, ui))) = (lower, upper) {
        let ord = compare_versions(l, u);
        if ord == Ordering::Greater || (ord == Ordering::Equal && !(*li && *ui)) {
            return false;
        }
    }
    has_stable_witness(lower, upper)
        || anchors
            .iter()
            .any(|&t| within_bounds(&tuple_version(t, zero_pre()), lower, upper))
}

fn canonical_version(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

fn canonical_arm(arm: &[Cmp]) -> Option<String> {
    let (lower, upper, mut anchors) = arm_tuple_bounds(arm);
    if !satisfying_witness(&lower, &upper, &anchors) {
        return None;
    }
    anchors.sort();
    anchors.dedup();

    let lower_text = lower
        .map(|(v, inclusive)| {
            format!(
                "{}{}",
                if inclusive { ">=" } else { ">" },
                canonical_version(&v)
            )
        })
        .unwrap_or_else(|| "*".to_string());
    let upper_text = upper
        .map(|(v, inclusive)| {
            format!(
                "{}{}",
                if inclusive { "<=" } else { "<" },
                canonical_version(&v)
            )
        })
        .unwrap_or_else(|| "*".to_string());
    let anchor_text = if anchors.is_empty() {
        "-".to_string()
    } else {
        anchors
            .iter()
            .map(|(a, b, c)| format!("{a}.{b}.{c}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    Some(format!("[{lower_text};{upper_text};pre={anchor_text}]"))
}

pub fn canonical(r: &Range) -> String {
    let mut arms: Vec<String> = r.arms.iter().filter_map(|a| canonical_arm(a)).collect();
    arms.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    arms.dedup();
    if arms.is_empty() {
        "EMPTY".to_string()
    } else {
        arms.join(" || ")
    }
}

fn range_arm_bounds(
    arm: &[Cmp],
) -> (Option<(Version, bool)>, Option<(Version, bool)>) {
    let (lower, upper, _) = arm_tuple_bounds(arm);
    (lower, upper)
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for arm_a in &a.arms {
        for arm_b in &b.arms {
            let (al, au) = range_arm_bounds(arm_a);
            let (bl, bu) = range_arm_bounds(arm_b);

            let mut lower = al;
            if let Some((v, inclusive)) = bl {
                tighten_lower(&mut lower, v, inclusive);
            }
            let mut upper = au;
            if let Some((v, inclusive)) = bu {
                tighten_upper(&mut upper, v, inclusive);
            }

            if include_prerelease {
                if satisfying_witness(&lower, &upper, &[]) {
                    return true;
                }
            } else {
                let (_, _, anchors_a) = arm_tuple_bounds(arm_a);
                let (_, _, anchors_b) = arm_tuple_bounds(arm_b);
                let common: Vec<_> = anchors_a
                    .iter()
                    .filter(|t| anchors_b.contains(t))
                    .copied()
                    .collect();

                if satisfying_witness(&lower, &upper, &[])
                    || common
                        .iter()
                        .any(|&t| within_bounds(&tuple_version(t, zero_pre()), &lower, &upper))
                {
                    return true;
                }
            }
        }
    }
    false
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
    DuplicatePackage {
        name: String,
    },
    DuplicateLabel {
        package: String,
        label: String,
    },
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
    NoSolution {
        package: String,
        labels: Vec<String>,
    },
}

fn validate_expr(
    expr: &SetExpr,
    package: &str,
    label: &str,
    path: &mut Vec<usize>,
) -> Result<(), SelectError> {
    match expr {
        SetExpr::Range(s) => parse_range(s)
            .map(|_| ())
            .map_err(|error| SelectError::InvalidRange {
                package: package.to_string(),
                label: label.to_string(),
                path: path.clone(),
                error,
            }),
        SetExpr::Union(children) | SetExpr::Intersection(children) => {
            for (i, child) in children.iter().enumerate() {
                path.push(i);
                validate_expr(child, package, label, path)?;
                path.pop();
            }
            Ok(())
        }
    }
}

fn eval_expr(expr: &SetExpr, v: &Version, include_prerelease: bool) -> bool {
    match expr {
        SetExpr::Range(s) => parse_range(s)
            .map(|r| matches_with(v, &r, include_prerelease))
            .unwrap_or(false),
        SetExpr::Union(children) => children
            .iter()
            .any(|child| eval_expr(child, v, include_prerelease)),
        SetExpr::Intersection(children) => children
            .iter()
            .all(|child| eval_expr(child, v, include_prerelease)),
    }
}

fn diagnose(package: &Package, parsed: &[Version], include_prerelease: bool) -> Vec<String> {
    let n = package.constraints.len();
    if parsed.is_empty() {
        return Vec::new();
    }
    for size in 1..=n {
        let mut chosen = Vec::new();
        if let Some(labels) =
            find_conflict(package, parsed, include_prerelease, size, 0, &mut chosen)
        {
            return labels;
        }
    }
    Vec::new()
}

fn find_conflict(
    package: &Package,
    parsed: &[Version],
    include_prerelease: bool,
    remaining: usize,
    start: usize,
    chosen: &mut Vec<usize>,
) -> Option<Vec<String>> {
    if remaining == 0 {
        let satisfiable = parsed.iter().any(|v| {
            chosen
                .iter()
                .all(|&i| eval_expr(&package.constraints[i].expr, v, include_prerelease))
        });
        if !satisfiable {
            let mut labels: Vec<String> = chosen
                .iter()
                .map(|&i| package.constraints[i].label.clone())
                .collect();
            labels.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            return Some(labels);
        }
        return None;
    }
    if package.constraints.len().saturating_sub(start) < remaining {
        return None;
    }
    for i in start..=package.constraints.len() - remaining {
        chosen.push(i);
        if let Some(result) = find_conflict(
            package,
            parsed,
            include_prerelease,
            remaining - 1,
            i + 1,
            chosen,
        ) {
            return Some(result);
        }
        chosen.pop();
    }
    None
}

pub fn select_packages(
    packages: &[Package],
    preference: Preference,
    include_prerelease: bool,
) -> Result<Vec<Selection>, SelectError> {
    let mut names = std::collections::HashSet::new();
    for package in packages {
        if !names.insert(package.name.clone()) {
            return Err(SelectError::DuplicatePackage {
                name: package.name.clone(),
            });
        }
    }

    for package in packages {
        let mut labels = std::collections::HashSet::new();
        for constraint in &package.constraints {
            if !labels.insert(constraint.label.clone()) {
                return Err(SelectError::DuplicateLabel {
                    package: package.name.clone(),
                    label: constraint.label.clone(),
                });
            }
        }
    }

    let mut parsed_candidates = Vec::new();
    for package in packages {
        let mut parsed = Vec::new();
        for (index, candidate) in package.candidates.iter().enumerate() {
            parsed.push(
                parse_version(candidate).map_err(|error| SelectError::InvalidCandidate {
                    package: package.name.clone(),
                    index,
                    error,
                })?,
            );
        }
        parsed_candidates.push(parsed);
    }

    for package in packages {
        for constraint in &package.constraints {
            validate_expr(
                &constraint.expr,
                &package.name,
                &constraint.label,
                &mut Vec::new(),
            )?;
        }
    }

    let mut selections = Vec::new();
    for (package_index, package) in packages.iter().enumerate() {
        let parsed = &parsed_candidates[package_index];
        let mut best: Option<usize> = None;

        for (i, version) in parsed.iter().enumerate() {
            if !package
                .constraints
                .iter()
                .all(|c| eval_expr(&c.expr, version, include_prerelease))
            {
                continue;
            }

            if let Some(old) = best {
                let order = compare_versions(version, &parsed[old]);
                let choose = match preference {
                    Preference::Highest => order == Ordering::Greater,
                    Preference::Lowest => order == Ordering::Less,
                    Preference::StableFirst => {
                        let stable = version.pre.is_empty();
                        let old_stable = parsed[old].pre.is_empty();
                        if stable != old_stable {
                            stable
                        } else {
                            order == Ordering::Greater
                        }
                    }
                };
                if choose {
                    best = Some(i);
                }
            } else {
                best = Some(i);
            }
        }

        if let Some(i) = best {
            selections.push(Selection {
                package: package.name.clone(),
                version: package.candidates[i].clone(),
            });
        } else {
            return Err(SelectError::NoSolution {
                package: package.name.clone(),
                labels: diagnose(package, parsed, include_prerelease),
            });
        }
    }
    Ok(selections)
}