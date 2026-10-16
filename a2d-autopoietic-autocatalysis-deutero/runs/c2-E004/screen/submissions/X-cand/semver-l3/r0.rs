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

fn valid_identifier_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

fn parse_core_number(s: &str, start: usize) -> Result<(u64, usize), VersionError> {
    let bytes = s.as_bytes();
    let mut end = start;
    while end < bytes.len() && is_digit(bytes[end]) {
        end += 1;
    }

    if end == start {
        if start == bytes.len()
            || matches!(bytes[start], b'.' | b'-' | b'+')
        {
            return Err(VersionError::MissingCore { at: start });
        }
        return Err(VersionError::InvalidChar { at: start });
    }

    if end - start > 1 && bytes[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }

    let mut value = 0u64;
    for &b in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u64))
            .ok_or(VersionError::Overflow { at: start })?;
    }

    Ok((value, end))
}

fn parse_identifiers(
    s: &str,
    base: usize,
    numeric_pre_check: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut start = 0;

    loop {
        let mut end = start;
        while end < bytes.len() && bytes[end] != b'.' {
            if !valid_identifier_byte(bytes[end]) {
                return Err(VersionError::InvalidChar { at: base + end });
            }
            end += 1;
        }

        if end == start {
            return Err(VersionError::EmptyIdentifier { at: base + start });
        }

        if numeric_pre_check
            && bytes[start..end].iter().all(|b| is_digit(*b))
            && end - start > 1
            && bytes[start] == b'0'
        {
            return Err(VersionError::LeadingZero { at: base + start });
        }

        result.push(s[start..end].to_string());

        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(VersionError::EmptyIdentifier { at: base + start });
        }
    }

    Ok(result)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let (major, p1) = parse_core_number(s, 0)?;
    let bytes = s.as_bytes();
    if p1 >= bytes.len() || bytes[p1] != b'.' {
        if p1 >= bytes.len() || matches!(bytes[p1], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: p1 });
        }
        return Err(VersionError::InvalidChar { at: p1 });
    }

    let (minor, p2) = parse_core_number(s, p1 + 1)?;
    let bytes = s.as_bytes();
    if p2 >= bytes.len() || bytes[p2] != b'.' {
        if p2 >= bytes.len() || matches!(bytes[p2], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: p2 });
        }
        return Err(VersionError::InvalidChar { at: p2 });
    }

    let (patch, p3) = parse_core_number(s, p2 + 1)?;
    if p3 < bytes.len() && !matches!(bytes[p3], b'-' | b'+') {
        return Err(VersionError::InvalidChar { at: p3 });
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();
    let mut pos = p3;

    if pos < bytes.len() && bytes[pos] == b'-' {
        let pre_start = pos + 1;
        let plus = bytes[pre_start..]
            .iter()
            .position(|b| *b == b'+')
            .map(|n| pre_start + n)
            .unwrap_or(bytes.len());
        pre = parse_identifiers(&s[pre_start..plus], pre_start, true)?;
        pos = plus;
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        let build_start = pos + 1;
        build = parse_identifiers(&s[build_start..], build_start, false)?;
        pos = bytes.len();
    }

    if pos != bytes.len() {
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

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
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
    core(a)
        .cmp(&core(b))
        .then_with(|| compare_pre(&a.pre, &b.pre))
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

#[derive(Clone, Debug)]
struct Bound {
    version: Version,
    inclusive: bool,
}

#[derive(Clone, Debug)]
struct Comparator {
    op: Op,
    version: Version,
    grants_anchor: bool,
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug)]
struct Arm {
    comparators: Vec<Comparator>,
    anchors: Vec<(u64, u64, u64)>,
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

fn split_arms(s: &str) -> Result<Vec<(String, usize)>, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = Vec::new();
    let mut arm_start = 0usize;
    let mut i = 0usize;
    let mut saw_separator = false;

    while i < bytes.len() {
        if bytes[i..].starts_with(b"||") {
            let left = &s[arm_start..i];
            if split_tokens(left, arm_start).is_empty() {
                return Err(RangeError::Empty { at: i });
            }
            arms.push((left.to_string(), arm_start));
            i += 2;
            while i < bytes.len() && whitespace(bytes[i]) {
                i += 1;
            }
            arm_start = i;
            saw_separator = true;
        } else {
            i += 1;
        }
    }

    let right = &s[arm_start..];
    if split_tokens(right, arm_start).is_empty() {
        if saw_separator {
            return Err(RangeError::Empty { at: s.len() });
        }
        return Err(RangeError::Empty { at: 0 });
    }
    arms.push((right.to_string(), arm_start));
    Ok(arms)
}

fn parse_full_operand(token: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(token).map_err(|error| RangeError::Version { at, error })
}

#[derive(Clone, Debug)]
enum PartialOperand {
    Full(Version),
    Partial {
        fields: Vec<Option<u64>>,
        k: usize,
    },
}

fn parse_partial_or_full(token: &str, at: usize) -> Result<PartialOperand, RangeError> {
    let has_suffix = token.contains('-') || token.contains('+');
    let fields: Vec<&str> = token.split('.').collect();
    let has_wild = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));

    if has_suffix || (fields.len() >= 3 && !has_wild) {
        return parse_full_operand(token, at).map(PartialOperand::Full);
    }

    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at });
    }

    let mut parsed = Vec::new();
    let mut wildcard_seen = false;
    let mut k = 0usize;

    for field in fields {
        if field.is_empty() {
            return Err(RangeError::Syntax { at });
        }
        if matches!(field, "*" | "x" | "X") {
            wildcard_seen = true;
            parsed.push(None);
            continue;
        }
        if wildcard_seen
            || !field.bytes().all(is_digit)
            || (field.len() > 1 && field.starts_with('0'))
        {
            return Err(RangeError::Syntax { at });
        }

        let mut n = 0u64;
        for b in field.bytes() {
            n = n
                .checked_mul(10)
                .and_then(|x| x.checked_add((b - b'0') as u64))
                .ok_or(RangeError::Syntax { at })?;
        }
        parsed.push(Some(n));
        k += 1;
    }

    if parsed.len() < 3 {
        parsed.resize(3, None);
    }

    Ok(PartialOperand::Partial { fields: parsed, k })
}

fn partial_lower(fields: &[Option<u64>]) -> Version {
    Version {
        major: fields.first().and_then(|x| *x).unwrap_or(0),
        minor: fields.get(1).and_then(|x| *x).unwrap_or(0),
        patch: fields.get(2).and_then(|x| *x).unwrap_or(0),
        pre: Vec::new(),
        build: Vec::new(),
    }
}

fn bump_upper(v: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let mut out = Version {
        major: v.major,
        minor: v.minor,
        patch: v.patch,
        pre: vec!["0".to_string()],
        build: Vec::new(),
    };
    match field {
        0 => {
            out.major = out.major.checked_add(1).ok_or(RangeError::Syntax { at })?;
            out.minor = 0;
            out.patch = 0;
        }
        1 => {
            out.minor = out.minor.checked_add(1).ok_or(RangeError::Syntax { at })?;
            out.patch = 0;
        }
        _ => {
            out.patch = out.patch.checked_add(1).ok_or(RangeError::Syntax { at })?;
        }
    }
    Ok(out)
}

fn add_comparator(arm: &mut Arm, op: Op, version: Version, grants_anchor: bool) {
    if grants_anchor && !version.pre.is_empty() {
        arm.anchors.push(core(&version));
    }
    arm.comparators.push(Comparator {
        op,
        version,
        grants_anchor,
    });
}

fn parse_bare_or_expanded(
    token: &str,
    at: usize,
    arm: &mut Arm,
) -> Result<(), RangeError> {
    let parsed = parse_partial_or_full(token, at)?;
    match parsed {
        PartialOperand::Full(v) => add_comparator(arm, Op::Eq, v, true),
        PartialOperand::Partial { fields, k } => {
            if k == 0 {
                return Ok(());
            }
            let lower = partial_lower(&fields);
            add_comparator(arm, Op::Ge, lower.clone(), true);
            if k <= 2 {
                let upper = bump_upper(&lower, k - 1, at)?;
                add_comparator(arm, Op::Lt, upper, false);
            }
        }
    }
    Ok(())
}

fn parse_caret_tilde(
    token: &str,
    at: usize,
    caret: bool,
    arm: &mut Arm,
) -> Result<(), RangeError> {
    let operand_at = at + 1;
    let parsed = parse_partial_or_full(&token[1..], operand_at)?;
    let (lower, fields, k, full) = match parsed {
        PartialOperand::Full(v) => {
            let fields = vec![Some(v.major), Some(v.minor), Some(v.patch)];
            (v, fields, 3, true)
        }
        PartialOperand::Partial { fields, k } => {
            if k == 0 {
                return Err(RangeError::Syntax { at });
            }
            (partial_lower(&fields), fields, k, false)
        }
    };
    add_comparator(arm, Op::Ge, lower.clone(), true);

    let upper_field = if caret {
        if lower.major > 0 || k == 1 {
            0
        } else if lower.minor > 0 || k == 2 {
            1
        } else {
            2
        }
    } else if k == 1 {
        0
    } else {
        1
    };
    let _ = fields;
    let _ = full;
    let upper = bump_upper(&lower, upper_field, operand_at)?;
    add_comparator(arm, Op::Lt, upper, false);
    Ok(())
}

fn parse_explicit_comparator(
    token: &str,
    at: usize,
    arm: &mut Arm,
) -> Result<(), RangeError> {
    let bytes = token.as_bytes();
    let (op, n) = match bytes.first().copied() {
        Some(b'=') => (Op::Eq, 1),
        Some(b'<') if bytes.get(1) == Some(&b'=') => (Op::Le, 2),
        Some(b'<') => (Op::Lt, 1),
        Some(b'>') if bytes.get(1) == Some(&b'=') => (Op::Ge, 2),
        Some(b'>') => (Op::Gt, 1),
        _ => unreachable!(),
    };
    if n >= token.len() || !token.as_bytes()[n].is_ascii_digit() {
        return Err(RangeError::Syntax { at });
    }
    let v = parse_full_operand(&token[n..], at + n)?;
    add_comparator(arm, op, v, true);
    Ok(())
}

fn parse_hyphen_arm(tokens: &[(String, usize)], arm: &mut Arm) -> Result<(), RangeError> {
    let minus_pos = tokens
        .iter()
        .find(|(t, _)| t == "-")
        .map(|(_, at)| *at)
        .unwrap();
    if tokens.len() != 3 || tokens[1].0 != "-" {
        return Err(RangeError::Syntax { at: minus_pos });
    }

    let lower = parse_partial_or_full(&tokens[0].0, tokens[0].1)?;
    let upper = parse_partial_or_full(&tokens[2].0, tokens[2].1)?;

    let (lower_v, lower_k) = match lower {
        PartialOperand::Full(v) => (v, 3),
        PartialOperand::Partial { fields, k } => (partial_lower(&fields), k),
    };
    let (upper_v, upper_k, upper_full) = match upper {
        PartialOperand::Full(v) => (v, 3, true),
        PartialOperand::Partial { fields, k } => (partial_lower(&fields), k, false),
    };

    if lower_k == 0 {
        return Err(RangeError::Syntax { at: tokens[0].1 });
    }
    if upper_k == 0 {
        return Err(RangeError::Syntax { at: tokens[2].1 });
    }

    add_comparator(arm, Op::Ge, lower_v, true);
    if upper_full {
        add_comparator(arm, Op::Le, upper_v, true);
    } else if upper_k == 1 || upper_k == 2 {
        let end = bump_upper(&upper_v, upper_k - 1, tokens[2].1)?;
        add_comparator(arm, Op::Lt, end, false);
    } else {
        add_comparator(arm, Op::Le, upper_v, true);
    }
    Ok(())
}

fn parse_arm(text: &str, base: usize) -> Result<Arm, RangeError> {
    let tokens = split_tokens(text, base);
    let mut arm = Arm {
        comparators: Vec::new(),
        anchors: Vec::new(),
    };

    if let Some((_, at)) = tokens.iter().find(|(t, _)| t == "-") {
        let _ = at;
        return parse_hyphen_arm(&tokens, &mut arm).map(|_| arm);
    }

    for (token, at) in tokens {
        let first = token.as_bytes().first().copied();
        match first {
            Some(b'^') => parse_caret_tilde(&token, at, true, &mut arm)?,
            Some(b'~') => parse_caret_tilde(&token, at, false, &mut arm)?,
            Some(b'<') | Some(b'>') | Some(b'=') => {
                parse_explicit_comparator(&token, at, &mut arm)?
            }
            Some(b) if b.is_ascii_digit() || matches!(b, b'*' | b'x' | b'X') => {
                parse_bare_or_expanded(&token, at, &mut arm)?
            }
            _ => return Err(RangeError::Syntax { at }),
        }
    }

    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let parts = split_arms(s)?;
    let mut arms = Vec::new();
    for (text, base) in parts {
        arms.push(parse_arm(&text, base)?);
    }
    Ok(Range { arms })
}

fn comparator_matches(v: &Version, c: &Comparator) -> bool {
    let order = compare_versions(v, &c.version);
    match c.op {
        Op::Eq => order == Ordering::Equal,
        Op::Lt => order == Ordering::Less,
        Op::Le => order != Ordering::Greater,
        Op::Gt => order == Ordering::Greater,
        Op::Ge => order != Ordering::Less,
    }
}

fn arm_matches(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !arm.comparators.iter().all(|c| comparator_matches(v, c)) {
        return false;
    }
    if include_prerelease || v.pre.is_empty() {
        return true;
    }
    arm.anchors.iter().any(|a| *a == core(v))
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
    for (index, s) in candidates.iter().enumerate() {
        let v = parse_version(s).map_err(|error| CandidateError { index, error })?;
        parsed.push(v);
    }

    let mut best: Option<(usize, usize)> = None;
    for (index, v) in parsed.iter().enumerate() {
        if !matches(v, r) {
            continue;
        }
        match best {
            None => best = Some((index, index)),
            Some((best_index, _)) => {
                let order = compare_versions(v, &parsed[best_index]);
                if (maximum && order == Ordering::Greater)
                    || (!maximum && order == Ordering::Less)
                {
                    best = Some((index, index));
                }
            }
        }
    }

    Ok(best.map(|(index, _)| candidates[index].clone()))
}

#[derive(Clone, Debug)]
struct Interval {
    lower: Option<Bound>,
    upper: Option<Bound>,
}

fn arm_interval(arm: &Arm) -> Interval {
    let mut lower: Option<Bound> = None;
    let mut upper: Option<Bound> = None;

    for c in &arm.comparators {
        match c.op {
            Op::Eq => {
                let b = Bound {
                    version: c.version.clone(),
                    inclusive: true,
                };
                lower = Some(stronger_lower(lower, b.clone()));
                upper = Some(stronger_upper(upper, b));
            }
            Op::Gt | Op::Ge => {
                let b = Bound {
                    version: c.version.clone(),
                    inclusive: matches!(c.op, Op::Ge),
                };
                lower = Some(stronger_lower(lower, b));
            }
            Op::Lt | Op::Le => {
                let b = Bound {
                    version: c.version.clone(),
                    inclusive: matches!(c.op, Op::Le),
                };
                upper = Some(stronger_upper(upper, b));
            }
        }
    }

    Interval { lower, upper }
}

fn stronger_lower(old: Option<Bound>, new: Bound) -> Bound {
    match old {
        None => new,
        Some(o) => match compare_versions(&new.version, &o.version) {
            Ordering::Greater => new,
            Ordering::Less => o,
            Ordering::Equal if !new.inclusive => new,
            Ordering::Equal => o,
        },
    }
}

fn stronger_upper(old: Option<Bound>, new: Bound) -> Bound {
    match old {
        None => new,
        Some(o) => match compare_versions(&new.version, &o.version) {
            Ordering::Less => new,
            Ordering::Greater => o,
            Ordering::Equal if !new.inclusive => new,
            Ordering::Equal => o,
        },
    }
}

fn same_tuple(a: &Version, b: &Version) -> bool {
    core(a) == core(b)
}

fn version_at_tuple(tuple: (u64, u64, u64), pre: Vec<String>) -> Version {
    Version {
        major: tuple.0,
        minor: tuple.1,
        patch: tuple.2,
        pre,
        build: Vec::new(),
    }
}

fn next_tuple(t: (u64, u64, u64)) -> Option<(u64, u64, u64)> {
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

fn least_pre(tuple: (u64, u64, u64)) -> Version {
    version_at_tuple(tuple, vec!["0".to_string()])
}

fn has_stable_witness(interval: &Interval) -> bool {
    let stable = |tuple| version_at_tuple(tuple, Vec::new());
    if let Some(lower) = &interval.lower {
        if let Some(upper) = &interval.upper {
            let ord = compare_versions(&lower.version, &upper.version);
            if ord == Ordering::Greater || (ord == Ordering::Equal && !(lower.inclusive && upper.inclusive)) {
                return false;
            }
            if same_tuple(&lower.version, &upper.version) {
                let candidate = stable(core(&lower.version));
                return satisfies_bound_lower(&candidate, lower)
                    && satisfies_bound_upper(&candidate, upper);
            }

            let mut tuple = core(&lower.version);
            let mut candidate = stable(tuple);
            if !lower.inclusive && compare_versions(&candidate, &lower.version) != Ordering::Greater {
                if !lower.version.pre.is_empty() {
                    candidate = stable(tuple);
                } else if let Some(n) = next_tuple(tuple) {
                    tuple = n;
                    candidate = stable(tuple);
                } else {
                    return false;
                }
            } else if !lower.version.pre.is_empty() {
                candidate = stable(tuple);
            }
            satisfies_bound_lower(&candidate, lower)
                && satisfies_bound_upper(&candidate, upper)
        } else {
            true
        }
    } else {
        true
    }
}

fn satisfies_bound_lower(v: &Version, b: &Bound) -> bool {
    let c = compare_versions(v, &b.version);
    c == Ordering::Greater || (b.inclusive && c == Ordering::Equal)
}

fn satisfies_bound_upper(v: &Version, b: &Bound) -> bool {
    let c = compare_versions(v, &b.version);
    c == Ordering::Less || (b.inclusive && c == Ordering::Equal)
}

fn prerelease_witness(interval: &Interval, tuple: (u64, u64, u64)) -> bool {
    let candidate = least_pre(tuple);
    if let Some(lower) = &interval.lower {
        if !satisfies_bound_lower(&candidate, lower) {
            return false;
        }
    }
    if let Some(upper) = &interval.upper {
        if !satisfies_bound_upper(&candidate, upper) {
            return false;
        }
    }
    true
}

fn interval_has_witness(interval: &Interval, anchors: &[(u64, u64, u64)]) -> bool {
    if has_stable_witness(interval) {
        return true;
    }

    for &tuple in anchors {
        if prerelease_witness(interval, tuple) {
            return true;
        }
    }
    false
}

fn compare_bounds_lower(a: &Option<Bound>, b: &Option<Bound>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, _) => Ordering::Less,
        (_, None) => Ordering::Greater,
        (Some(x), Some(y)) => compare_versions(&x.version, &y.version)
            .then_with(|| y.inclusive.cmp(&x.inclusive)),
    }
}

fn compare_bounds_upper(a: &Option<Bound>, b: &Option<Bound>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, _) => Ordering::Greater,
        (_, None) => Ordering::Less,
        (Some(x), Some(y)) => compare_versions(&x.version, &y.version)
            .then_with(|| x.inclusive.cmp(&y.inclusive)),
    }
}

fn intersect_interval(a: &Interval, b: &Interval) -> Interval {
    let lower = match (&a.lower, &b.lower) {
        (None, None) => None,
        (Some(x), None) | (None, Some(x)) => Some(x.clone()),
        (Some(x), Some(y)) => Some(stronger_lower(Some(x.clone()), y.clone())),
    };
    let upper = match (&a.upper, &b.upper) {
        (None, None) => None,
        (Some(x), None) | (None, Some(x)) => Some(x.clone()),
        (Some(x), Some(y)) => Some(stronger_upper(Some(x.clone()), y.clone())),
    };
    Interval { lower, upper }
}

fn combined_anchors(a: &Arm, b: &Arm) -> Vec<(u64, u64, u64)> {
    let mut out = Vec::new();
    for x in &a.anchors {
        if b.anchors.contains(x) {
            out.push(*x);
        }
    }
    out
}

fn arm_intersects(a: &Arm, b: &Arm, include_prerelease: bool) -> bool {
    let interval = intersect_interval(&arm_interval(a), &arm_interval(b));
    if !include_prerelease {
        return interval_has_witness(&interval, &combined_anchors(a, b));
    }

    if has_stable_witness(&interval) {
        return true;
    }

    let mut anchors = a.anchors.clone();
    anchors.extend_from_slice(&b.anchors);
    anchors.sort_unstable();
    anchors.dedup();
    anchors.iter().any(|t| prerelease_witness(&interval, *t))
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    a.arms
        .iter()
        .any(|x| b.arms.iter().any(|y| arm_intersects(x, y, include_prerelease)))
}

fn clean_bound(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

fn arm_canonical(arm: &Arm) -> Option<String> {
    let interval = arm_interval(arm);
    if !interval_has_witness(&interval, &arm.anchors) {
        return None;
    }

    let lower = match &interval.lower {
        None => "*".to_string(),
        Some(b) => format!(
            "{}{}",
            if b.inclusive { ">=" } else { ">" },
            clean_bound(&b.version)
        ),
    };
    let upper = match &interval.upper {
        None => "*".to_string(),
        Some(b) => format!(
            "{}{}",
            if b.inclusive { "<=" } else { "<" },
            clean_bound(&b.version)
        ),
    };

    let mut anchors = arm.anchors.clone();
    anchors.sort_unstable();
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

    Some(format!("[{lower};{upper};pre={anchor_text}]"))
}

pub fn canonical(r: &Range) -> String {
    let mut arms: Vec<String> = r.arms.iter().filter_map(arm_canonical).collect();
    arms.sort();
    arms.dedup();
    if arms.is_empty() {
        "EMPTY".to_string()
    } else {
        arms.join(" || ")
    }
}