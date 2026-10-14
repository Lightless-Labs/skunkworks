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
        return match bytes.get(start) {
            None | Some(b'.' | b'-' | b'+') => {
                Err(VersionError::MissingCore { at: start })
            }
            _ => Err(VersionError::InvalidChar { at: start }),
        };
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
    pos: &mut usize,
    stop_at_plus: bool,
    numeric_leading_zero: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();

    loop {
        let start = *pos;
        while *pos < bytes.len()
            && (bytes[*pos].is_ascii_alphanumeric() || bytes[*pos] == b'-')
            && !(stop_at_plus && bytes[*pos] == b'+')
        {
            *pos += 1;
        }

        if *pos == start {
            return Err(VersionError::EmptyIdentifier { at: start });
        }

        let identifier = &s[start..*pos];
        let all_digits = identifier.bytes().all(|b| b.is_ascii_digit());

        if *pos < bytes.len() && bytes[*pos] != b'.' && !(stop_at_plus && bytes[*pos] == b'+') {
            return Err(VersionError::InvalidChar { at: *pos });
        }
        if numeric_leading_zero && all_digits && identifier.len() > 1 && identifier.starts_with('0') {
            return Err(VersionError::LeadingZero { at: start });
        }

        result.push(identifier.to_owned());

        if *pos >= bytes.len() || (stop_at_plus && bytes[*pos] == b'+') {
            break;
        }

        *pos += 1;
        if *pos == bytes.len() || (stop_at_plus && bytes[*pos] == b'+') {
            return Err(VersionError::EmptyIdentifier { at: *pos });
        }
    }

    Ok(result)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let mut pos = 0;
    let major = parse_core(s, &mut pos)?;
    if s.as_bytes().get(pos) != Some(&b'.') {
        return match s.as_bytes().get(pos) {
            None | Some(b'-' | b'+') => Err(VersionError::MissingCore { at: pos }),
            _ => Err(VersionError::InvalidChar { at: pos }),
        };
    }
    pos += 1;

    let minor = parse_core(s, &mut pos)?;
    if s.as_bytes().get(pos) != Some(&b'.') {
        return match s.as_bytes().get(pos) {
            None | Some(b'-' | b'+') => Err(VersionError::MissingCore { at: pos }),
            _ => Err(VersionError::InvalidChar { at: pos }),
        };
    }
    pos += 1;

    let patch = parse_core(s, &mut pos)?;
    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < s.len() && s.as_bytes()[pos] != b'-' && s.as_bytes()[pos] != b'+' {
        return Err(VersionError::InvalidChar { at: pos });
    }
    if s.as_bytes().get(pos) == Some(&b'-') {
        pos += 1;
        pre = parse_identifiers(s, &mut pos, true, true)?;
    }
    if s.as_bytes().get(pos) == Some(&b'+') {
        pos += 1;
        build = parse_identifiers(s, &mut pos, false, false)?;
    }
    if pos != s.len() {
        return Err(VersionError::InvalidChar { at: pos });
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

fn compare_numeric_identifiers(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    for (x, y) in a.iter().zip(b) {
        let xn = is_numeric(x);
        let yn = is_numeric(y);
        let ordering = match (xn, yn) {
            (true, true) => compare_numeric_identifiers(x, y),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => x.as_bytes().cmp(y.as_bytes()),
        };
        if ordering != Ordering::Equal {
            return ordering;
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
    inclusive: bool,
}

#[derive(Clone, Debug)]
struct Arm {
    constraints: Vec<(Op, Version)>,
    anchors: Vec<(u64, u64, u64)>,
    lower: Option<Bound>,
    upper: Option<Bound>,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

#[derive(Clone, Copy)]
struct Token<'a> {
    text: &'a str,
    start: usize,
}

fn is_range_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn split_tokens(s: &str, base: usize) -> Vec<Token<'_>> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && is_range_space(bytes[i]) {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !is_range_space(bytes[i]) {
            i += 1;
        }
        out.push(Token {
            text: &s[start..i],
            start: base + start,
        });
    }
    out
}

fn version_error(at: usize, error: VersionError) -> RangeError {
    RangeError::Version { at, error }
}

fn parse_full_operand(text: &str, start: usize) -> Result<Version, RangeError> {
    if text.is_empty() || !text.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: start });
    }
    parse_version(text).map_err(|e| version_error(start, e))
}

#[derive(Clone)]
struct Partial {
    fields: Vec<Option<u64>>,
    full: Option<Version>,
    numeric_count: usize,
}

fn partial_operand(text: &str, start: usize) -> Result<Partial, RangeError> {
    if text.contains('-') || text.contains('+') {
        let v = parse_full_operand(text, start)?;
        return Ok(Partial {
            fields: vec![Some(v.major), Some(v.minor), Some(v.patch)],
            full: Some(v),
            numeric_count: 3,
        });
    }

    let raw: Vec<&str> = text.split('.').collect();
    let has_no_wildcard = !raw.iter().any(|f| matches!(*f, "*" | "x" | "X"));
    if raw.len() >= 3 && has_no_wildcard {
        let v = parse_full_operand(text, start)?;
        return Ok(Partial {
            fields: vec![Some(v.major), Some(v.minor), Some(v.patch)],
            full: Some(v),
            numeric_count: 3,
        });
    }

    if raw.is_empty() || raw.len() > 3 {
        return Err(RangeError::Syntax { at: start });
    }
    let mut fields = Vec::new();
    let mut saw_wildcard = false;
    let mut numeric_count = 0;
    for field in raw {
        if field.is_empty() {
            return Err(RangeError::Syntax { at: start });
        }
        if matches!(field, "*" | "x" | "X") {
            saw_wildcard = true;
            fields.push(None);
        } else {
            if saw_wildcard
                || !field.bytes().all(|b| b.is_ascii_digit())
                || (field.len() > 1 && field.starts_with('0'))
            {
                return Err(RangeError::Syntax { at: start });
            }
            let n = field.parse::<u64>().map_err(|_| RangeError::Syntax { at: start })?;
            fields.push(Some(n));
            numeric_count += 1;
        }
    }
    while fields.len() < 3 {
        fields.push(None);
    }
    Ok(Partial {
        fields,
        full: None,
        numeric_count,
    })
}

fn tuple_version(tuple: (u64, u64, u64), pre: Vec<String>) -> Version {
    Version {
        major: tuple.0,
        minor: tuple.1,
        patch: tuple.2,
        pre,
        build: Vec::new(),
    }
}

fn padded_lower(partial: &Partial) -> Version {
    let t = (
        partial.fields[0].unwrap_or(0),
        partial.fields[1].unwrap_or(0),
        partial.fields[2].unwrap_or(0),
    );
    partial.full.clone().unwrap_or_else(|| tuple_version(t, Vec::new()))
}

fn increment_upper(
    partial: &Partial,
    mode: &str,
    start: usize,
) -> Result<Version, RangeError> {
    let mut t = (
        partial.fields[0].unwrap_or(0),
        partial.fields[1].unwrap_or(0),
        partial.fields[2].unwrap_or(0),
    );
    let k = partial.numeric_count;

    let field = match mode {
        "bare" => match k {
            1 => Some(0),
            2 => Some(1),
            _ => None,
        },
        "caret" => {
            if t.0 > 0 || k == 1 {
                Some(0)
            } else if t.1 > 0 || k == 2 {
                Some(1)
            } else {
                Some(2)
            }
        }
        "tilde" => {
            if k == 1 {
                Some(0)
            } else {
                Some(1)
            }
        }
        "hyphen" => match k {
            1 => Some(0),
            2 => Some(1),
            _ => None,
        },
        _ => None,
    };

    if let Some(field) = field {
        let value = match field {
            0 => &mut t.0,
            1 => &mut t.1,
            _ => &mut t.2,
        };
        *value = value
            .checked_add(1)
            .ok_or(RangeError::Syntax { at: start })?;
        match field {
            0 => {
                t.1 = 0;
                t.2 = 0;
            }
            1 => t.2 = 0,
            _ => {}
        }
    }
    Ok(tuple_version(t, vec!["0".to_owned()]))
}

fn add_constraint(arm: &mut Arm, op: Op, v: Version, anchor: bool) {
    if anchor && !v.pre.is_empty() {
        arm.anchors.push((v.major, v.minor, v.patch));
    }
    arm.constraints.push((op, v));
}

fn add_lower(arm: &mut Arm, v: Version, inclusive: bool, anchor: bool) {
    add_constraint(
        arm,
        if inclusive { Op::Ge } else { Op::Gt },
        v.clone(),
        anchor,
    );
    let candidate = Bound { version: v, inclusive };
    if arm.lower.as_ref().map_or(true, |old| {
        compare_versions(&candidate.version, &old.version) == Ordering::Greater
            || (compare_versions(&candidate.version, &old.version) == Ordering::Equal
                && !candidate.inclusive
                && old.inclusive)
    }) {
        arm.lower = Some(candidate);
    }
}

fn add_upper(arm: &mut Arm, v: Version, inclusive: bool, anchor: bool) {
    add_constraint(
        arm,
        if inclusive { Op::Le } else { Op::Lt },
        v.clone(),
        anchor,
    );
    let candidate = Bound { version: v, inclusive };
    if arm.upper.as_ref().map_or(true, |old| {
        compare_versions(&candidate.version, &old.version) == Ordering::Less
            || (compare_versions(&candidate.version, &old.version) == Ordering::Equal
                && !candidate.inclusive
                && old.inclusive)
    }) {
        arm.upper = Some(candidate);
    }
}

fn add_equality(arm: &mut Arm, v: Version) {
    if !v.pre.is_empty() {
        arm.anchors.push((v.major, v.minor, v.patch));
    }
    arm.constraints.push((Op::Eq, v.clone()));
    let lower = Bound {
        version: v.clone(),
        inclusive: true,
    };
    let upper = Bound {
        version: v,
        inclusive: true,
    };
    if arm.lower.as_ref().map_or(true, |b| {
        compare_versions(&lower.version, &b.version) == Ordering::Greater
    }) {
        arm.lower = Some(lower);
    }
    if arm.upper.as_ref().map_or(true, |b| {
        compare_versions(&upper.version, &b.version) == Ordering::Less
    }) {
        arm.upper = Some(upper);
    }
}

fn parse_bare_or_expansion(
    arm: &mut Arm,
    text: &str,
    start: usize,
    kind: &str,
) -> Result<(), RangeError> {
    let p = partial_operand(text, start)?;
    if kind != "bare" && p.numeric_count == 0 {
        return Err(RangeError::Syntax { at: start });
    }
    if let Some(v) = p.full.clone() {
        if kind == "bare" {
            add_equality(arm, v);
        } else {
            add_lower(arm, padded_lower(&p), true, true);
            let upper = increment_upper(&p, kind, start)?;
            add_upper(arm, upper, false, false);
        }
        return Ok(());
    }
    if kind == "bare" {
        if p.numeric_count == 0 {
            return Ok(());
        }
        let lower = padded_lower(&p);
        add_lower(arm, lower, true, false);
        if p.numeric_count < 3 {
            let upper = increment_upper(&p, "bare", start)?;
            add_upper(arm, upper, false, false);
        }
    } else {
        add_lower(arm, padded_lower(&p), true, false);
        let upper = increment_upper(&p, kind, start)?;
        add_upper(arm, upper, false, false);
    }
    Ok(())
}

fn parse_comparator_token(arm: &mut Arm, token: Token<'_>) -> Result<(), RangeError> {
    let (op, n) = if token.text.starts_with(">=") {
        (Op::Ge, 2)
    } else if token.text.starts_with("<=") {
        (Op::Le, 2)
    } else if token.text.starts_with('>') {
        (Op::Gt, 1)
    } else if token.text.starts_with('<') {
        (Op::Lt, 1)
    } else {
        (Op::Eq, 1)
    };
    let operand = &token.text[n..];
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: token.start });
    }
    let v = parse_full_operand(operand, token.start + n)?;
    match op {
        Op::Eq => add_equality(arm, v),
        Op::Ge => add_lower(arm, v, true, true),
        Op::Gt => add_lower(arm, v, false, true),
        Op::Le => add_upper(arm, v, true, true),
        Op::Lt => add_upper(arm, v, false, true),
    }
    Ok(())
}

fn standalone_minus(tokens: &[Token<'_>]) -> Option<usize> {
    tokens.iter().position(|t| t.text == "-")
}

fn parse_hyphen_arm(tokens: &[Token<'_>], arm: &mut Arm) -> Result<(), RangeError> {
    let minus = standalone_minus(tokens).unwrap();
    if tokens.len() != 3 || minus != 1 {
        return Err(RangeError::Syntax {
            at: tokens[minus].start,
        });
    }

    let lower = partial_operand(tokens[0].text, tokens[0].start)?;
    let upper = partial_operand(tokens[2].text, tokens[2].start)?;
    if lower.numeric_count == 0 {
        return Err(RangeError::Syntax {
            at: tokens[0].start,
        });
    }
    if upper.numeric_count == 0 {
        return Err(RangeError::Syntax {
            at: tokens[2].start,
        });
    }

    let low_v = padded_lower(&lower);
    let low_anchor = lower.full.as_ref().is_some_and(|v| !v.pre.is_empty());
    add_lower(arm, low_v, true, low_anchor);

    if let Some(v) = upper.full {
        add_upper(arm, v.clone(), true, !v.pre.is_empty());
    } else {
        let upper_bound = increment_upper(&upper, "hyphen", tokens[2].start)?;
        add_upper(arm, upper_bound, false, false);
    }
    Ok(())
}

fn parse_arm(s: &str, tokens: &[Token<'_>]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        constraints: Vec::new(),
        anchors: Vec::new(),
        lower: None,
        upper: None,
    };
    if standalone_minus(tokens).is_some() {
        parse_hyphen_arm(tokens, &mut arm)?;
        return Ok(arm);
    }

    for token in tokens {
        let first = token.text.as_bytes()[0];
        if token.text.starts_with('^') {
            parse_bare_or_expansion(&mut arm, &token.text[1..], token.start + 1, "caret")?;
        } else if token.text.starts_with('~') {
            parse_bare_or_expansion(&mut arm, &token.text[1..], token.start + 1, "tilde")?;
        } else if matches!(first, b'<' | b'>' | b'=') {
            parse_comparator_token(&mut arm, *token)?;
        } else if first.is_ascii_digit() || matches!(first, b'*' | b'x' | b'X') {
            parse_bare_or_expansion(&mut arm, token.text, token.start, "bare")?;
        } else {
            return Err(RangeError::Syntax { at: token.start });
        }
    }
    let _ = s;
    Ok(arm)
}

fn parse_range_internal(s: &str) -> Result<Range, RangeError> {
    let tokens = split_tokens(s, 0);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms = Vec::new();
    let mut current = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].text == "||" {
            if current.is_empty() {
                return Err(RangeError::Empty { at: tokens[i].start });
            }
            arms.push(parse_arm(s, &current)?);
            current.clear();
            i += 1;
            if i == tokens.len() {
                return Err(RangeError::Empty { at: s.len() });
            }
        } else {
            current.push(tokens[i]);
            i += 1;
        }
    }
    if current.is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(s, &current)?);
    Ok(Range { arms })
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    parse_range_internal(s)
}

fn cmp_bound_lower(v: &Version, b: &Bound) -> bool {
    match compare_versions(v, &b.version) {
        Ordering::Greater => true,
        Ordering::Equal => b.inclusive,
        Ordering::Less => false,
    }
}

fn cmp_bound_upper(v: &Version, b: &Bound) -> bool {
    match compare_versions(v, &b.version) {
        Ordering::Less => true,
        Ordering::Equal => b.inclusive,
        Ordering::Greater => false,
    }
}

fn arm_admits(arm: &Arm, v: &Version) -> bool {
    arm.anchors
        .iter()
        .any(|t| *t == (v.major, v.minor, v.patch))
}

fn arm_matches(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !v.pre.is_empty() && !include_prerelease && !arm_admits(arm, v) {
        return false;
    }
    arm.constraints.iter().all(|(op, rhs)| {
        let cmp = compare_versions(v, rhs);
        match op {
            Op::Eq => cmp == Ordering::Equal,
            Op::Lt => cmp == Ordering::Less,
            Op::Le => cmp != Ordering::Greater,
            Op::Gt => cmp == Ordering::Greater,
            Op::Ge => cmp != Ordering::Less,
        }
    })
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

fn extrema(candidates: &[String], r: &Range, maximum: bool) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let version = parse_version(candidate).map_err(|error| CandidateError { index, error })?;
        parsed.push(version);
    }

    let mut best: Option<usize> = None;
    for (i, version) in parsed.iter().enumerate() {
        if !matches(version, r) {
            continue;
        }
        if let Some(j) = best {
            let cmp = compare_versions(version, &parsed[j]);
            if (maximum && cmp == Ordering::Greater) || (!maximum && cmp == Ordering::Less) {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

fn next_tuple(tuple: (u64, u64, u64)) -> Option<(u64, u64, u64)> {
    if tuple.2 < u64::MAX {
        Some((tuple.0, tuple.1, tuple.2 + 1))
    } else if tuple.1 < u64::MAX {
        Some((tuple.0, tuple.1 + 1, 0))
    } else if tuple.0 < u64::MAX {
        Some((tuple.0 + 1, 0, 0))
    } else {
        None
    }
}

fn tuple(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn interval_bounds_ok(arm: &Arm) -> bool {
    match (&arm.lower, &arm.upper) {
        (Some(l), Some(u)) => match compare_versions(&l.version, &u.version) {
            Ordering::Less => true,
            Ordering::Equal => l.inclusive && u.inclusive,
            Ordering::Greater => false,
        },
        _ => true,
    }
}

fn lower_allows_tuple(arm: &Arm, t: (u64, u64, u64)) -> bool {
    arm.lower.as_ref().map_or(true, |l| {
        let base = tuple(&l.version);
        t > base || (t == base && (l.inclusive || !l.version.pre.is_empty()))
    })
}

fn upper_allows_release(arm: &Arm, t: (u64, u64, u64)) -> bool {
    arm.upper.as_ref().map_or(true, |u| {
        let base = tuple(&u.version);
        t < base || (t == base && u.inclusive && u.version.pre.is_empty())
    })
}

fn stable_witness(arms: &[&Arm]) -> bool {
    let mut lower_tuple: Option<(u64, u64, u64)> = None;
    let mut upper_tuple: Option<(u64, u64, u64)> = None;
    let mut lower_release_excluded_at = None;

    for arm in arms {
        if !interval_bounds_ok(arm) {
            return false;
        }
        if let Some(l) = &arm.lower {
            let t = tuple(&l.version);
            if lower_tuple.map_or(true, |x| t > x) {
                lower_tuple = Some(t);
                lower_release_excluded_at = if l.version.pre.is_empty() && !l.inclusive {
                    Some(t)
                } else {
                    None
                };
            } else if Some(t) == lower_tuple
                && l.version.pre.is_empty()
                && !l.inclusive
            {
                lower_release_excluded_at = Some(t);
            }
        }
        if let Some(u) = &arm.upper {
            let t = tuple(&u.version);
            if upper_tuple.map_or(true, |x| t < x) {
                upper_tuple = Some(t);
            }
        }
    }

    let candidate = lower_tuple.unwrap_or((0, 0, 0));
    if upper_tuple.is_some_and(|u| candidate > u) {
        return false;
    }
    if upper_tuple == Some(candidate)
        && lower_release_excluded_at == Some(candidate)
    {
        return false;
    }
    arms.iter().all(|arm| upper_allows_release(arm, candidate))
}

fn max_lower_bound<'a>(arms: &'a [&Arm]) -> Option<&'a Bound> {
    arms.iter()
        .filter_map(|arm| arm.lower.as_ref())
        .max_by(|a, b| {
            compare_versions(&a.version, &b.version).then_with(|| {
                match (a.inclusive, b.inclusive) {
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    _ => Ordering::Equal,
                }
            })
        })
}

fn min_upper_bound<'a>(arms: &'a [&Arm]) -> Option<&'a Bound> {
    arms.iter()
        .filter_map(|arm| arm.upper.as_ref())
        .min_by(|a, b| {
            compare_versions(&a.version, &b.version).then_with(|| {
                match (a.inclusive, b.inclusive) {
                    (true, false) => Ordering::Greater,
                    (false, true) => Ordering::Less,
                    _ => Ordering::Equal,
                }
            })
        })
}

fn prerelease_witness(arms: &[&Arm]) -> bool {
    let anchor_set: Vec<(u64, u64, u64)> = arms
        .iter()
        .flat_map(|a| a.anchors.iter().copied())
        .collect();
    for t in anchor_set {
        if !arms.iter().all(|a| arm_admits(a, &tuple_version(t, vec!["0".into()]))) {
            continue;
        }
        let least = tuple_version(t, vec!["0".into()]);
        if arms.iter().all(|arm| {
            arm.constraints.iter().all(|(op, rhs)| {
                let cmp = compare_versions(&least, rhs);
                match op {
                    Op::Eq => cmp == Ordering::Equal,
                    Op::Lt => cmp == Ordering::Less,
                    Op::Le => cmp != Ordering::Greater,
                    Op::Gt => cmp == Ordering::Greater,
                    Op::Ge => cmp != Ordering::Less,
                }
            })
        }) {
            return true;
        }

        // If -0 is below a strict lower bound at this tuple, its immediate
        // successor may still be a witness, unless the upper endpoint leaves no room.
        let mut seq = vec!["0".to_owned()];
        for _ in 0..3 {
            seq.push("0".to_owned());
            let candidate = tuple_version(t, seq.clone());
            if arms.iter().all(|arm| arm.constraints.iter().all(|(op, rhs)| {
                let cmp = compare_versions(&candidate, rhs);
                match op {
                    Op::Eq => cmp == Ordering::Equal,
                    Op::Lt => cmp == Ordering::Less,
                    Op::Le => cmp != Ordering::Greater,
                    Op::Gt => cmp == Ordering::Greater,
                    Op::Ge => cmp != Ordering::Less,
                }
            })) {
                return true;
            }
        }
    }
    false
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for aa in &a.arms {
        for bb in &b.arms {
            let pair = [aa, bb];
            if stable_witness(&pair) {
                return true;
            }
            if include_prerelease {
                // With unrestricted admission, any tuple-local prerelease witness
                // is determined by the same interval checks; the anchor list is not needed.
                if prerelease_witness_unrestricted(&pair) {
                    return true;
                }
            } else if prerelease_witness(&pair) {
                return true;
            }
        }
    }
    false
}

fn prerelease_witness_unrestricted(arms: &[&Arm]) -> bool {
    // Generated upper bounds use -0, so test their tuple minima, plus explicit
    // bound tuples. These are the only finite transition points in the interval.
    let mut tuples = Vec::new();
    for arm in arms {
        for bound in arm.lower.iter().chain(arm.upper.iter()) {
            tuples.push(tuple(&bound.version));
        }
    }
    tuples.push((0, 0, 0));
    tuples.sort_unstable();
    tuples.dedup();

    for t in tuples {
        let mut candidates = vec![vec!["0".to_owned()]];
        for arm in arms {
            for bound in arm.lower.iter().chain(arm.upper.iter()) {
                if tuple(&bound.version) == t && !bound.version.pre.is_empty() {
                    candidates.push(bound.version.pre.clone());
                }
            }
        }
        for pre in candidates {
            let v = tuple_version(t, pre);
            if arms.iter().all(|arm| arm.constraints.iter().all(|(op, rhs)| {
                let cmp = compare_versions(&v, rhs);
                match op {
                    Op::Eq => cmp == Ordering::Equal,
                    Op::Lt => cmp == Ordering::Less,
                    Op::Le => cmp != Ordering::Greater,
                    Op::Gt => cmp == Ordering::Greater,
                    Op::Ge => cmp != Ordering::Less,
                }
            })) {
                return true;
            }
        }
    }
    false
}

fn arm_has_any_default_witness(arm: &Arm) -> bool {
    if stable_witness(&[arm]) || prerelease_witness(&[arm]) {
        return true;
    }
    false
}

fn canonical_version(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

fn strip_build(v: &Version) -> Version {
    let mut out = v.clone();
    out.build.clear();
    out
}

fn arm_canonical(arm: &Arm) -> String {
    let lower = arm.lower.as_ref().map_or("*".to_owned(), |b| {
        format!(
            "{}{}",
            if b.inclusive { ">=" } else { ">" },
            canonical_version(&strip_build(&b.version))
        )
    });
    let upper = arm.upper.as_ref().map_or("*".to_owned(), |b| {
        format!(
            "{}{}",
            if b.inclusive { "<=" } else { "<" },
            canonical_version(&strip_build(&b.version))
        )
    });
    let mut anchors = arm.anchors.clone();
    anchors.sort_unstable();
    anchors.dedup();
    let anchors = if anchors.is_empty() {
        "-".to_owned()
    } else {
        anchors
            .iter()
            .map(|(a, b, c)| format!("{a}.{b}.{c}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    format!("[{lower};{upper};pre={anchors}]")
}

pub fn canonical(r: &Range) -> String {
    let mut arms: Vec<String> = r
        .arms
        .iter()
        .filter(|arm| arm_has_any_default_witness(arm))
        .map(arm_canonical)
        .collect();
    arms.sort();
    arms.dedup();
    if arms.is_empty() {
        "EMPTY".to_owned()
    } else {
        arms.join(" || ")
    }
}