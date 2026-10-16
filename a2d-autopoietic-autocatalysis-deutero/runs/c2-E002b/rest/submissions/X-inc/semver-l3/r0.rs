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

fn core_tuple(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let bytes = s.as_bytes();
    let mut pos = 0usize;
    let mut core = [0u64; 3];

    for index in 0..3 {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }

        if start == pos {
            if pos == bytes.len() || matches!(bytes[pos], b'.' | b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            return Err(VersionError::InvalidChar { at: pos });
        }

        if pos - start > 1 && bytes[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }

        core[index] = match s[start..pos].parse::<u64>() {
            Ok(n) => n,
            Err(_) => return Err(VersionError::Overflow { at: start }),
        };

        if index < 2 {
            if pos == bytes.len() || matches!(bytes[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            if bytes[pos] != b'.' {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
        }
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] != b'-' && bytes[pos] != b'+' {
        return Err(VersionError::InvalidChar { at: pos });
    }

    if pos < bytes.len() && bytes[pos] == b'-' {
        let pre_start = pos + 1;
        let plus = bytes[pre_start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|i| pre_start + i);
        let pre_end = plus.unwrap_or(bytes.len());
        parse_identifiers(s, pre_start, pre_end, true, &mut pre)?;
        if let Some(plus_at) = plus {
            parse_identifiers(s, plus_at + 1, bytes.len(), false, &mut build)?;
        }
    } else if pos < bytes.len() && bytes[pos] == b'+' {
        parse_identifiers(s, pos + 1, bytes.len(), false, &mut build)?;
    }

    Ok(Version {
        major: core[0],
        minor: core[1],
        patch: core[2],
        pre,
        build,
    })
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    is_pre: bool,
    out: &mut Vec<String>,
) -> Result<(), VersionError> {
    if start == end {
        return Err(VersionError::EmptyIdentifier { at: start });
    }

    let bytes = s.as_bytes();
    let mut identifier_start = start;

    for pos in start..=end {
        if pos == end || bytes[pos] == b'.' {
            if pos == identifier_start {
                return Err(VersionError::EmptyIdentifier {
                    at: identifier_start,
                });
            }

            for i in identifier_start..pos {
                let b = bytes[i];
                if !(b.is_ascii_alphanumeric() || b == b'-') {
                    return Err(VersionError::InvalidChar { at: i });
                }
            }

            let identifier = &s[identifier_start..pos];
            if is_pre
                && identifier.len() > 1
                && identifier.bytes().all(|b| b.is_ascii_digit())
                && identifier.starts_with('0')
            {
                return Err(VersionError::LeadingZero {
                    at: identifier_start,
                });
            }

            out.push(identifier.to_string());
            identifier_start = pos + 1;
        }
    }

    Ok(())
}

fn is_numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn cmp_identifier(a: &str, b: &str) -> Ordering {
    let an = is_numeric_identifier(a);
    let bn = is_numeric_identifier(b);
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
            for (left, right) in a.pre.iter().zip(&b.pre) {
                let cmp = cmp_identifier(left, right);
                if cmp != Ordering::Equal {
                    return cmp;
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct Bound {
    version: Version,
    inclusive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Comparator {
    op: Op,
    version: Version,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

fn whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn split_tokens(s: &str) -> Vec<(usize, &str)> {
    let bytes = s.as_bytes();
    let mut tokens = Vec::new();
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
        tokens.push((start, &s[start..i]));
    }
    tokens
}

fn version_with(core: (u64, u64, u64), pre: Vec<String>) -> Version {
    Version {
        major: core.0,
        minor: core.1,
        patch: core.2,
        pre,
        build: Vec::new(),
    }
}

fn padded_version(fields: &[u64]) -> Version {
    version_with(
        (
            *fields.first().unwrap_or(&0),
            *fields.get(1).unwrap_or(&0),
            *fields.get(2).unwrap_or(&0),
        ),
        Vec::new(),
    )
}

#[derive(Clone, Debug)]
enum Operand {
    Full(Version),
    Partial { fields: Vec<u64>, count: usize },
}

fn parse_operand(s: &str, at: usize, token_start: usize) -> Result<Operand, RangeError> {
    if s.contains('-') || s.contains('+') {
        return parse_version(s)
            .map(Operand::Full)
            .map_err(|error| RangeError::Version { at, error });
    }

    let fields: Vec<&str> = s.split('.').collect();
    let full_candidate = fields.len() >= 3
        && !fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));
    if full_candidate {
        return parse_version(s)
            .map(Operand::Full)
            .map_err(|error| RangeError::Version { at, error });
    }

    if fields.len() > 3 || fields.iter().any(|f| f.is_empty()) {
        return Err(RangeError::Syntax { at: token_start });
    }

    let mut nums = Vec::new();
    let mut wildcard_seen = false;
    for field in &fields {
        if matches!(*field, "*" | "x" | "X") {
            wildcard_seen = true;
        } else {
            if wildcard_seen
                || !field.bytes().all(|b| b.is_ascii_digit())
                || (field.len() > 1 && field.starts_with('0'))
            {
                return Err(RangeError::Syntax { at: token_start });
            }
            let n = field
                .parse::<u64>()
                .map_err(|_| RangeError::Syntax { at: token_start })?;
            nums.push(n);
        }
    }

    let count = nums.len();
    Ok(Operand::Partial {
        fields: nums,
        count,
    })
}

fn next_core(
    core: (u64, u64, u64),
    field: usize,
    at: usize,
) -> Result<(u64, u64, u64), RangeError> {
    let mut c = core;
    match field {
        0 => {
            c.0 = c.0.checked_add(1).ok_or(RangeError::Syntax { at })?;
            c.1 = 0;
            c.2 = 0;
        }
        1 => {
            c.1 = c.1.checked_add(1).ok_or(RangeError::Syntax { at })?;
            c.2 = 0;
        }
        _ => {
            c.2 = c.2.checked_add(1).ok_or(RangeError::Syntax { at })?;
        }
    }
    Ok(c)
}

fn add_cmp(arm: &mut Arm, op: Op, version: Version) {
    if !version.pre.is_empty() {
        arm.anchors.push(core_tuple(&version));
    }
    arm.comparators.push(Comparator { op, version });
}

fn expand_bare(
    arm: &mut Arm,
    op: &str,
    operand: Operand,
    at: usize,
) -> Result<(), RangeError> {
    match (op, operand) {
        ("", Operand::Full(v)) => add_cmp(arm, Op::Eq, v),
        ("", Operand::Partial { fields, count }) => match count {
            0 => {}
            1 | 2 => {
                let lower = padded_version(&fields);
                let upper_core = next_core(core_tuple(&lower), count - 1, at)?;
                add_cmp(arm, Op::Ge, lower);
                add_cmp(
                    arm,
                    Op::Lt,
                    version_with(upper_core, vec!["0".to_string()]),
                );
            }
            _ => return Err(RangeError::Syntax { at }),
        },
        ("^", Operand::Full(v)) => {
            let core = core_tuple(&v);
            let count = 3;
            let field = if v.major > 0 || count == 1 {
                0
            } else if v.minor > 0 || count == 2 {
                1
            } else {
                2
            };
            let upper = next_core(core, field, at)?;
            add_cmp(arm, Op::Ge, v);
            add_cmp(arm, Op::Lt, version_with(upper, vec!["0".to_string()]));
        }
        ("^", Operand::Partial { fields, count }) => {
            if count == 0 {
                return Err(RangeError::Syntax { at });
            }
            let lower = padded_version(&fields);
            let field = if lower.major > 0 || count == 1 {
                0
            } else if lower.minor > 0 || count == 2 {
                1
            } else {
                2
            };
            let upper = next_core(core_tuple(&lower), field, at)?;
            add_cmp(arm, Op::Ge, lower);
            add_cmp(arm, Op::Lt, version_with(upper, vec!["0".to_string()]));
        }
        ("~", Operand::Full(v)) => {
            let core = core_tuple(&v);
            let upper = next_core(core, 1, at)?;
            add_cmp(arm, Op::Ge, v);
            add_cmp(arm, Op::Lt, version_with(upper, vec!["0".to_string()]));
        }
        ("~", Operand::Partial { fields, count }) => {
            if count == 0 {
                return Err(RangeError::Syntax { at });
            }
            let lower = padded_version(&fields);
            let field = if count == 1 { 0 } else { 1 };
            let upper = next_core(core_tuple(&lower), field, at)?;
            add_cmp(arm, Op::Ge, lower);
            add_cmp(arm, Op::Lt, version_with(upper, vec!["0".to_string()]));
        }
        _ => return Err(RangeError::Syntax { at }),
    }
    Ok(())
}

fn parse_explicit_comparator(
    arm: &mut Arm,
    token: &str,
    token_at: usize,
) -> Result<(), RangeError> {
    let (op, n) = if token.starts_with(">=") {
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
        return Err(RangeError::Syntax { at: token_at });
    };

    let operand = &token[n..];
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: token_at });
    }

    let version = parse_version(operand).map_err(|error| RangeError::Version {
        at: token_at + n,
        error,
    })?;
    add_cmp(arm, op, version);
    Ok(())
}

fn parse_arm(tokens: &[(usize, &str)]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        comparators: Vec::new(),
        anchors: Vec::new(),
    };

    let standalone_minus = tokens.iter().position(|(_, t)| *t == "-");
    if let Some(minus_i) = standalone_minus {
        if tokens.len() != 3 || minus_i != 1 {
            return Err(RangeError::Syntax {
                at: tokens[minus_i].0,
            });
        }

        let (lower_at, lower_s) = tokens[0];
        let (upper_at, upper_s) = tokens[2];
        let lower = parse_operand(lower_s, lower_at, lower_at)?;
        let upper = parse_operand(upper_s, upper_at, upper_at)?;

        let (lower_v, lower_count) = match lower {
            Operand::Full(v) => (v, 3),
            Operand::Partial { fields, count } => (padded_version(&fields), count),
        };
        let (upper_v, upper_count) = match upper {
            Operand::Full(v) => (v, 3),
            Operand::Partial { fields, count } => (padded_version(&fields), count),
        };
        if lower_count == 0 {
            return Err(RangeError::Syntax { at: lower_at });
        }
        if upper_count == 0 {
            return Err(RangeError::Syntax { at: upper_at });
        }

        add_cmp(&mut arm, Op::Ge, lower_v);
        if upper_count == 3 {
            add_cmp(&mut arm, Op::Le, upper_v);
        } else {
            let upper_core = next_core(core_tuple(&upper_v), upper_count - 1, upper_at)?;
            add_cmp(
                &mut arm,
                Op::Lt,
                version_with(upper_core, vec!["0".to_string()]),
            );
        }
        return Ok(arm);
    }

    for (token_at, token) in tokens {
        if token.starts_with('^') || token.starts_with('~') {
            let prefix = &token[..1];
            let operand = &token[1..];
            if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
                return Err(RangeError::Syntax { at: *token_at });
            }
            let parsed = parse_operand(operand, token_at + 1, *token_at)?;
            expand_bare(&mut arm, prefix, parsed, token_at + 1)?;
        } else if token.starts_with('<') || token.starts_with('>') || token.starts_with('=') {
            parse_explicit_comparator(&mut arm, token, *token_at)?;
        } else if token.as_bytes()[0].is_ascii_digit() || matches!(*token, "*" | "x" | "X") {
            let parsed = parse_operand(token, *token_at, *token_at)?;
            expand_bare(&mut arm, "", parsed, *token_at)?;
        } else {
            return Err(RangeError::Syntax { at: *token_at });
        }
    }

    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let tokens = split_tokens(s);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms = Vec::new();
    let mut current = Vec::new();
    for (at, token) in tokens {
        if token == "||" {
            if current.is_empty() {
                return Err(RangeError::Empty { at });
            }
            arms.push(parse_arm(&current)?);
            current.clear();
        } else {
            current.push((at, token));
        }
    }

    if current.is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(&current)?);
    Ok(Range { arms })
}

fn comparator_matches(v: &Version, c: &Comparator) -> bool {
    let ord = compare_versions(v, &c.version);
    match c.op {
        Op::Eq => ord == Ordering::Equal,
        Op::Lt => ord == Ordering::Less,
        Op::Le => ord != Ordering::Greater,
        Op::Gt => ord == Ordering::Greater,
        Op::Ge => ord != Ordering::Less,
    }
}

fn arm_admits(arm: &Arm, v: &Version) -> bool {
    !v.pre.is_empty() && arm.anchors.iter().any(|t| *t == core_tuple(v))
}

fn arm_matches(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !v.pre.is_empty() && !include_prerelease && !arm_admits(arm, v) {
        return false;
    }
    arm.comparators.iter().all(|c| comparator_matches(v, c))
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
    r: &Range,
    max: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let version = parse_version(candidate).map_err(|error| CandidateError { index, error })?;
        parsed.push(version);
    }

    let mut best: Option<usize> = None;
    for i in 0..parsed.len() {
        if !matches(&parsed[i], r) {
            continue;
        }
        match best {
            None => best = Some(i),
            Some(j) => {
                let ord = compare_versions(&parsed[i], &parsed[j]);
                if (max && ord == Ordering::Greater) || (!max && ord == Ordering::Less) {
                    best = Some(i);
                }
            }
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

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    a.arms.iter().any(|aa| {
        b.arms
            .iter()
            .any(|bb| arms_intersect(aa, bb, include_prerelease))
    })
}

fn arms_intersect(a: &Arm, b: &Arm, include_prerelease: bool) -> bool {
    let mut constraints = a.comparators.clone();
    constraints.extend(b.comparators.clone());

    let mut candidates = vec![Version {
        major: 0,
        minor: 0,
        patch: 0,
        pre: Vec::new(),
        build: Vec::new(),
    }];
    for c in &constraints {
        candidates.push(c.version.clone());
        candidates.push(version_with(core_tuple(&c.version), vec!["0".to_string()]));
        let core = core_tuple(&c.version);
        if core.2 < u64::MAX {
            candidates.push(version_with((core.0, core.1, core.2 + 1), Vec::new()));
        }
    }

    for candidate in candidates {
        if (!candidate.pre.is_empty()
            && !include_prerelease
            && !(arm_admits(a, &candidate) && arm_admits(b, &candidate)))
            || !constraints
                .iter()
                .all(|c| comparator_matches(&candidate, c))
        {
            continue;
        }
        return true;
    }
    false
}

fn version_display(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

fn choose_lower(slot: &mut Option<Bound>, candidate: Bound) {
    let replace = match slot {
        None => true,
        Some(current) => {
            let ord = compare_versions(&candidate.version, &current.version);
            ord == Ordering::Greater
                || (ord == Ordering::Equal && !candidate.inclusive && current.inclusive)
        }
    };
    if replace {
        *slot = Some(candidate);
    }
}

fn choose_upper(slot: &mut Option<Bound>, candidate: Bound) {
    let replace = match slot {
        None => true,
        Some(current) => {
            let ord = compare_versions(&candidate.version, &current.version);
            ord == Ordering::Less
                || (ord == Ordering::Equal && !candidate.inclusive && current.inclusive)
        }
    };
    if replace {
        *slot = Some(candidate);
    }
}

fn canonical_arm(arm: &Arm) -> Option<String> {
    let mut lower: Option<Bound> = None;
    let mut upper: Option<Bound> = None;

    for c in &arm.comparators {
        match c.op {
            Op::Eq => {
                choose_lower(
                    &mut lower,
                    Bound {
                        version: c.version.clone(),
                        inclusive: true,
                    },
                );
                choose_upper(
                    &mut upper,
                    Bound {
                        version: c.version.clone(),
                        inclusive: true,
                    },
                );
            }
            Op::Ge | Op::Gt => choose_lower(
                &mut lower,
                Bound {
                    version: c.version.clone(),
                    inclusive: c.op == Op::Ge,
                },
            ),
            Op::Le | Op::Lt => choose_upper(
                &mut upper,
                Bound {
                    version: c.version.clone(),
                    inclusive: c.op == Op::Le,
                },
            ),
        }
    }

    if let (Some(l), Some(u)) = (&lower, &upper) {
        let ord = compare_versions(&l.version, &u.version);
        if ord == Ordering::Greater
            || (ord == Ordering::Equal && !(l.inclusive && u.inclusive))
        {
            return None;
        }
    }

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

    let lower_text = lower.map_or("*".to_string(), |b| {
        format!(
            "{}{}",
            if b.inclusive { ">=" } else { ">" },
            version_display(&b.version)
        )
    });
    let upper_text = upper.map_or("*".to_string(), |b| {
        format!(
            "{}{}",
            if b.inclusive { "<=" } else { "<" },
            version_display(&b.version)
        )
    });

    Some(format!("[{lower_text};{upper_text};pre={anchor_text}]"))
}

pub fn canonical(r: &Range) -> String {
    let mut arms: Vec<String> = r.arms.iter().filter_map(canonical_arm).collect();
    arms.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    arms.dedup();
    if arms.is_empty() {
        "EMPTY".to_string()
    } else {
        arms.join(" || ")
    }
}