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

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let bytes = s.as_bytes();
    let mut pos = 0;
    let mut core = [0u64; 3];

    for (field, slot) in core.iter_mut().enumerate() {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }

        if pos == start {
            if pos == bytes.len() || matches!(bytes[pos], b'.' | b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            return Err(VersionError::InvalidChar { at: pos });
        }

        if pos - start > 1 && bytes[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }

        *slot = s[start..pos]
            .parse::<u64>()
            .map_err(|_| VersionError::Overflow { at: start })?;

        if field < 2 {
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

    if pos < bytes.len() && bytes[pos] == b'-' {
        pos += 1;
        let end = bytes[pos..]
            .iter()
            .position(|&byte| byte == b'+')
            .map(|offset| pos + offset)
            .unwrap_or(bytes.len());
        pre = parse_identifiers(s, pos, end, true)?;
        pos = end;
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        pos += 1;
        build = parse_identifiers(s, pos, bytes.len(), false)?;
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

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    prerelease: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut id_start = start;

    loop {
        let mut pos = id_start;
        while pos < end && bytes[pos] != b'.' {
            if !bytes[pos].is_ascii_alphanumeric() && bytes[pos] != b'-' {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
        }

        if pos == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }

        let id = &s[id_start..pos];
        if prerelease
            && id.len() > 1
            && id.starts_with('0')
            && id.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }

        result.push(id.to_string());
        if pos == end {
            break;
        }
        id_start = pos + 1;
    }

    Ok(result)
}

fn is_numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|byte| byte.is_ascii_digit())
}

fn compare_identifiers(a: &str, b: &str) -> Ordering {
    match (is_numeric_identifier(a), is_numeric_identifier(b)) {
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
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
                for (left, right) in a.pre.iter().zip(&b.pre) {
                    let ordering = compare_identifiers(left, right);
                    if ordering != Ordering::Equal {
                        return ordering;
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
    arms: Vec<Arm>,
}

#[derive(Clone, Debug)]
struct Comparator {
    op: Op,
    version: Version,
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

#[derive(Clone, Debug)]
struct Partial {
    numbers: Vec<u64>,
    full: Option<Version>,
}

fn is_range_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokenize(s: &str, base: usize) -> Vec<(String, usize)> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut pos = 0;

    while pos < bytes.len() {
        while pos < bytes.len() && is_range_whitespace(bytes[pos]) {
            pos += 1;
        }
        if pos == bytes.len() {
            break;
        }

        let start = pos;
        while pos < bytes.len() && !is_range_whitespace(bytes[pos]) {
            pos += 1;
        }
        result.push((s[start..pos].to_string(), base + start));
    }

    result
}

fn parse_partial(raw: &str, at: usize) -> Result<Partial, RangeError> {
    let fields: Vec<&str> = raw.split('.').collect();
    let has_wildcard = fields.iter().any(|field| matches!(*field, "*" | "x" | "X"));

    if raw.contains('-') || raw.contains('+') || (fields.len() >= 3 && !has_wildcard) {
        return parse_version(raw)
            .map(|version| Partial {
                numbers: vec![version.major, version.minor, version.patch],
                full: Some(version),
            })
            .map_err(|error| RangeError::Version { at, error });
    }

    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at });
    }

    let mut numbers = Vec::new();
    let mut wildcard_seen = false;

    for field in fields {
        if field.is_empty() {
            return Err(RangeError::Syntax { at });
        }

        if matches!(field, "*" | "x" | "X") {
            wildcard_seen = true;
            continue;
        }

        if wildcard_seen
            || !field.bytes().all(|byte| byte.is_ascii_digit())
            || (field.len() > 1 && field.starts_with('0'))
        {
            return Err(RangeError::Syntax { at });
        }

        numbers.push(
            field
                .parse::<u64>()
                .map_err(|_| RangeError::Syntax { at })?,
        );
    }

    Ok(Partial {
        numbers,
        full: None,
    })
}

fn version_from_numbers(numbers: &[u64], pre: Vec<String>) -> Version {
    Version {
        major: *numbers.first().unwrap_or(&0),
        minor: *numbers.get(1).unwrap_or(&0),
        patch: *numbers.get(2).unwrap_or(&0),
        pre,
        build: Vec::new(),
    }
}

fn core_tuple(version: &Version) -> (u64, u64, u64) {
    (version.major, version.minor, version.patch)
}

fn incremented_upper(numbers: &[u64], field: usize, at: usize) -> Result<Version, RangeError> {
    let mut core = [0u64; 3];
    for (index, number) in numbers.iter().enumerate() {
        core[index] = *number;
    }

    if core[field] == u64::MAX {
        return Err(RangeError::Syntax { at });
    }

    core[field] += 1;
    for value in core.iter_mut().skip(field + 1) {
        *value = 0;
    }

    Ok(version_from_numbers(&core, vec!["0".to_string()]))
}

fn add_comparator(arm: &mut Arm, op: Op, version: Version, is_explicit: bool) {
    if is_explicit && !version.pre.is_empty() {
        arm.anchors.push(core_tuple(&version));
    }
    arm.comparators.push(Comparator { op, version });
}

fn expand_partial(
    raw: &str,
    at: usize,
    kind: u8,
    arm: &mut Arm,
) -> Result<(), RangeError> {
    let partial = parse_partial(raw, at)?;
    if kind != 0 && partial.numbers.is_empty() {
        return Err(RangeError::Syntax { at });
    }

    let lower = version_from_numbers(
        &partial.numbers,
        partial
            .full
            .as_ref()
            .map(|version| version.pre.clone())
            .unwrap_or_default(),
    );

    match kind {
        0 => {
            if let Some(version) = partial.full {
                add_comparator(arm, Op::Eq, version, true);
            } else if partial.numbers.is_empty() {
                // A bare all-wildcard operand matches every release.
            } else if partial.numbers.len() == 3 {
                add_comparator(arm, Op::Eq, lower, false);
            } else {
                add_comparator(arm, Op::Ge, lower, false);
                let upper = incremented_upper(&partial.numbers, partial.numbers.len() - 1, at)?;
                add_comparator(arm, Op::Lt, upper, false);
            }
        }
        1 => {
            add_comparator(arm, Op::Ge, lower.clone(), true);
            let field = if lower.major > 0 || partial.numbers.len() == 1 {
                0
            } else if lower.minor > 0 || partial.numbers.len() == 2 {
                1
            } else {
                2
            };
            let upper = incremented_upper(&partial.numbers, field, at)?;
            add_comparator(arm, Op::Lt, upper, false);
        }
        2 => {
            add_comparator(arm, Op::Ge, lower, true);
            let field = if partial.numbers.len() == 1 { 0 } else { 1 };
            let upper = incremented_upper(&partial.numbers, field, at)?;
            add_comparator(arm, Op::Lt, upper, false);
        }
        _ => unreachable!(),
    }

    Ok(())
}

fn parse_arm(tokens: &[(String, usize)]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        comparators: Vec::new(),
        anchors: Vec::new(),
    };

    let hyphens: Vec<_> = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| token.0 == "-")
        .collect();

    if !hyphens.is_empty() {
        let (index, (_, at)) = hyphens[0];
        if tokens.len() != 3 || index != 1 {
            return Err(RangeError::Syntax { at: *at });
        }

        let lower = parse_partial(&tokens[0].0, tokens[0].1)?;
        let upper = parse_partial(&tokens[2].0, tokens[2].1)?;

        if lower.numbers.is_empty() {
            return Err(RangeError::Syntax { at: tokens[0].1 });
        }
        if upper.numbers.is_empty() {
            return Err(RangeError::Syntax { at: tokens[2].1 });
        }

        let lower_version = version_from_numbers(
            &lower.numbers,
            lower.full.as_ref().map(|version| version.pre.clone()).unwrap_or_default(),
        );
        add_comparator(&mut arm, Op::Ge, lower_version, true);

        if let Some(version) = upper.full {
            add_comparator(&mut arm, Op::Le, version, true);
        } else {
            let field = if upper.numbers.len() == 1 { 0 } else { 1 };
            let upper_version = incremented_upper(&upper.numbers, field, tokens[2].1)?;
            add_comparator(&mut arm, Op::Lt, upper_version, false);
        }

        return Ok(arm);
    }

    for (token, at) in tokens {
        let bytes = token.as_bytes();

        if token.starts_with('^') || token.starts_with('~') {
            let kind = if bytes[0] == b'^' { 1 } else { 2 };
            expand_partial(&token[1..], at + 1, kind, &mut arm)?;
            continue;
        }

        if bytes.first().is_some_and(|byte| matches!(byte, b'<' | b'>' | b'=')) {
            let (op, length) = if token.starts_with("<=") {
                (Op::Le, 2)
            } else if token.starts_with(">=") {
                (Op::Ge, 2)
            } else if token.starts_with('<') {
                (Op::Lt, 1)
            } else if token.starts_with('>') {
                (Op::Gt, 1)
            } else {
                (Op::Eq, 1)
            };

            let operand = &token[length..];
            if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
                return Err(RangeError::Syntax { at: *at });
            }

            let version = parse_version(operand).map_err(|error| RangeError::Version {
                at: at + length,
                error,
            })?;
            add_comparator(&mut arm, op, version, true);
            continue;
        }

        if !bytes
            .first()
            .is_some_and(|byte| byte.is_ascii_digit() || matches!(byte, b'*' | b'x' | b'X'))
        {
            return Err(RangeError::Syntax { at: *at });
        }

        expand_partial(token, *at, 0, &mut arm)?;
    }

    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let all_tokens = tokenize(s, 0);
    if all_tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut groups: Vec<Vec<(String, usize)>> = vec![Vec::new()];
    let mut index = 0;

    while index < all_tokens.len() {
        if all_tokens[index].0 == "||" {
            if groups.last().unwrap().is_empty() {
                return Err(RangeError::Empty {
                    at: all_tokens[index].1,
                });
            }
            groups.push(Vec::new());
            index += 1;
            if index == all_tokens.len() {
                return Err(RangeError::Empty { at: s.len() });
            }
        } else {
            groups.last_mut().unwrap().push(all_tokens[index].clone());
            index += 1;
        }
    }

    let arms = groups
        .iter()
        .map(|group| parse_arm(group))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Range { arms })
}

fn comparator_matches(version: &Version, comparator: &Comparator) -> bool {
    let ordering = compare_versions(version, &comparator.version);
    match comparator.op {
        Op::Eq => ordering == Ordering::Equal,
        Op::Lt => ordering == Ordering::Less,
        Op::Le => ordering != Ordering::Greater,
        Op::Gt => ordering == Ordering::Greater,
        Op::Ge => ordering != Ordering::Less,
    }
}

fn arm_matches(version: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !include_prerelease
        && !version.pre.is_empty()
        && !arm.anchors.contains(&core_tuple(version))
    {
        return false;
    }
    arm.comparators
        .iter()
        .all(|comparator| comparator_matches(version, comparator))
}

pub fn matches_with(version: &Version, range: &Range, include_prerelease: bool) -> bool {
    range
        .arms
        .iter()
        .any(|arm| arm_matches(version, arm, include_prerelease))
}

pub fn matches(version: &Version, range: &Range) -> bool {
    matches_with(version, range, false)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn extrema(
    candidates: &[String],
    range: &Range,
    want_maximum: bool,
) -> Result<Option<String>, CandidateError> {
    let mut versions = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        versions.push(parse_version(candidate).map_err(|error| CandidateError { index, error })?);
    }

    let mut best: Option<usize> = None;
    for index in 0..candidates.len() {
        if !matches(&versions[index], range) {
            continue;
        }

        if let Some(best_index) = best {
            let ordering = compare_versions(&versions[index], &versions[best_index]);
            if (want_maximum && ordering == Ordering::Greater)
                || (!want_maximum && ordering == Ordering::Less)
            {
                best = Some(index);
            }
        } else {
            best = Some(index);
        }
    }

    Ok(best.map(|index| candidates[index].clone()))
}

pub fn max_satisfying(
    candidates: &[String],
    range: &Range,
) -> Result<Option<String>, CandidateError> {
    extrema(candidates, range, true)
}

pub fn min_satisfying(
    candidates: &[String],
    range: &Range,
) -> Result<Option<String>, CandidateError> {
    extrema(candidates, range, false)
}

fn next_stable(version: &Version) -> Option<Version> {
    let (mut major, mut minor, mut patch) = (version.major, version.minor, version.patch);

    if patch < u64::MAX {
        patch += 1;
    } else if minor < u64::MAX {
        minor += 1;
        patch = 0;
    } else if major < u64::MAX {
        major += 1;
        minor = 0;
        patch = 0;
    } else {
        return None;
    }

    Some(Version {
        major,
        minor,
        patch,
        pre: Vec::new(),
        build: Vec::new(),
    })
}

fn next_prerelease(version: &Version) -> Version {
    let mut next = version.clone();
    next.pre.push("0".to_string());
    next.build.clear();
    next
}

fn strongest_bounds(arm: &Arm) -> (Option<(&Version, bool)>, Option<(&Version, bool)>) {
    let mut lower: Option<(&Version, bool)> = None;
    let mut upper: Option<(&Version, bool)> = None;

    for comparator in &arm.comparators {
        match comparator.op {
            Op::Eq => {
                lower = Some((&comparator.version, true));
                upper = Some((&comparator.version, true));
            }
            Op::Ge | Op::Gt => {
                let inclusive = matches!(comparator.op, Op::Ge);
                if lower.is_none_or(|(version, old_inclusive)| {
                    let ordering = compare_versions(&comparator.version, version);
                    ordering == Ordering::Greater
                        || (ordering == Ordering::Equal && !inclusive && old_inclusive)
                }) {
                    lower = Some((&comparator.version, inclusive));
                }
            }
            Op::Le | Op::Lt => {
                let inclusive = matches!(comparator.op, Op::Le);
                if upper.is_none_or(|(version, old_inclusive)| {
                    let ordering = compare_versions(&comparator.version, version);
                    ordering == Ordering::Less
                        || (ordering == Ordering::Equal && !inclusive && old_inclusive)
                }) {
                    upper = Some((&comparator.version, inclusive));
                }
            }
        }
    }

    (lower, upper)
}

fn first_version_in_interval(
    lower: Option<(&Version, bool)>,
    upper: Option<(&Version, bool)>,
) -> Option<Version> {
    let candidate = if let Some((bound, inclusive)) = lower {
        if inclusive {
            bound.clone()
        } else if !bound.pre.is_empty() {
            next_prerelease(bound)
        } else {
            next_stable(bound)?
        }
    } else {
        Version {
            major: 0,
            minor: 0,
            patch: 0,
            pre: Vec::new(),
            build: Vec::new(),
        }
    };

    if let Some((bound, inclusive)) = upper {
        let ordering = compare_versions(&candidate, bound);
        if ordering == Ordering::Greater || (ordering == Ordering::Equal && !inclusive) {
            return None;
        }
    }

    Some(candidate)
}

fn inside_bounds(
    version: &Version,
    lower: Option<(&Version, bool)>,
    upper: Option<(&Version, bool)>,
) -> bool {
    if let Some((bound, inclusive)) = lower {
        let ordering = compare_versions(version, bound);
        if ordering == Ordering::Less || (ordering == Ordering::Equal && !inclusive) {
            return false;
        }
    }
    if let Some((bound, inclusive)) = upper {
        let ordering = compare_versions(version, bound);
        if ordering == Ordering::Greater || (ordering == Ordering::Equal && !inclusive) {
            return false;
        }
    }
    true
}

fn arm_has_witness(a: &Arm, b: Option<&Arm>, include_prerelease: bool) -> bool {
    let (lower_a, upper_a) = strongest_bounds(a);
    let (lower_b, upper_b) = b.map(strongest_bounds).unwrap_or((None, None));

    let lower = match (lower_a, lower_b) {
        (Some(left), Some(right)) => {
            let ordering = compare_versions(left.0, right.0);
            Some(if ordering == Ordering::Greater
                || (ordering == Ordering::Equal && !left.1)
            {
                left
            } else {
                right
            })
        }
        (Some(bound), None) | (None, Some(bound)) => Some(bound),
        (None, None) => None,
    };

    let upper = match (upper_a, upper_b) {
        (Some(left), Some(right)) => {
            let ordering = compare_versions(left.0, right.0);
            Some(if ordering == Ordering::Less || (ordering == Ordering::Equal && !left.1) {
                left
            } else {
                right
            })
        }
        (Some(bound), None) | (None, Some(bound)) => Some(bound),
        (None, None) => None,
    };

    if let Some(candidate) = first_version_in_interval(lower, upper) {
        if candidate.pre.is_empty() || include_prerelease {
            return true;
        }
        if a.anchors.contains(&core_tuple(&candidate))
            && b.is_none_or(|arm| arm.anchors.contains(&core_tuple(&candidate)))
        {
            return true;
        }
    }

    let mut anchors = a.anchors.clone();
    if let Some(other) = b {
        anchors.extend(other.anchors.iter().copied());
    }
    anchors.sort();
    anchors.dedup();

    for (major, minor, patch) in anchors {
        let mut candidate = Version {
            major,
            minor,
            patch,
            pre: vec!["0".to_string()],
            build: Vec::new(),
        };

        if let Some((bound, inclusive)) = lower {
            let ordering = compare_versions(&candidate, bound);
            if ordering == Ordering::Less || (ordering == Ordering::Equal && !inclusive) {
                candidate = bound.clone();
                if candidate.pre.is_empty() {
                    continue;
                }
            }
        }

        if !inside_bounds(&candidate, lower, upper) {
            continue;
        }

        if include_prerelease
            || (a.anchors.contains(&(major, minor, patch))
                && b.is_none_or(|arm| arm.anchors.contains(&(major, minor, patch))))
        {
            return true;
        }
    }

    false
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    a.arms.iter().any(|left| {
        b.arms
            .iter()
            .any(|right| arm_has_witness(left, Some(right), include_prerelease))
    })
}

fn format_core(tuple: (u64, u64, u64)) -> String {
    format!("{}.{}.{}", tuple.0, tuple.1, tuple.2)
}

fn format_version(version: &Version) -> String {
    let mut result = format!("{}.{}.{}", version.major, version.minor, version.patch);
    if !version.pre.is_empty() {
        result.push('-');
        result.push_str(&version.pre.join("."));
    }
    result
}

pub fn canonical(range: &Range) -> String {
    let mut arms = Vec::new();

    for arm in &range.arms {
        if !arm_has_witness(arm, None, false) {
            continue;
        }

        let (lower, upper) = strongest_bounds(arm);
        let lower = lower.map_or_else(
            || "*".to_string(),
            |(version, inclusive)| {
                format!(
                    "{}{}",
                    if inclusive { ">=" } else { ">" },
                    format_version(version)
                )
            },
        );
        let upper = upper.map_or_else(
            || "*".to_string(),
            |(version, inclusive)| {
                format!(
                    "{}{}",
                    if inclusive { "<=" } else { "<" },
                    format_version(version)
                )
            },
        );

        let mut anchors = arm.anchors.clone();
        anchors.sort();
        anchors.dedup();
        let anchors = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors.into_iter().map(format_core).collect::<Vec<_>>().join(",")
        };

        arms.push(format!("[{lower};{upper};pre={anchors}]"));
    }

    arms.sort();
    arms.dedup();

    if arms.is_empty() {
        "EMPTY".to_string()
    } else {
        arms.join(" || ")
    }
}