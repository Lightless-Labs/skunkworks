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
    let mut pos = 0usize;
    let mut nums = [0u64; 3];

    for field in 0..3 {
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
        nums[field] = s[start..pos]
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

    if pos < bytes.len() {
        match bytes[pos] {
            b'-' => {
                pos += 1;
                let plus = bytes[pos..]
                    .iter()
                    .position(|&b| b == b'+')
                    .map(|i| pos + i)
                    .unwrap_or(bytes.len());

                parse_identifiers(s, pos, plus, true, &mut pre)?;
                pos = plus;

                if pos < bytes.len() {
                    pos += 1;
                    parse_identifiers(s, pos, bytes.len(), false, &mut build)?;
                    pos = bytes.len();
                }
            }
            b'+' => {
                pos += 1;
                parse_identifiers(s, pos, bytes.len(), false, &mut build)?;
                pos = bytes.len();
            }
            _ => return Err(VersionError::InvalidChar { at: pos }),
        }
    }

    if pos != bytes.len() {
        return Err(VersionError::InvalidChar { at: pos });
    }

    Ok(Version {
        major: nums[0],
        minor: nums[1],
        patch: nums[2],
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
    let bytes = s.as_bytes();
    let mut pos = start;

    loop {
        let id_start = pos;
        while pos < end && bytes[pos] != b'.' {
            let byte = bytes[pos];
            if !(byte.is_ascii_alphanumeric() || byte == b'-') {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
        }

        if id_start == pos {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }

        let id = &s[id_start..pos];
        if is_pre
            && id.bytes().all(|b| b.is_ascii_digit())
            && id.len() > 1
            && id.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        out.push(id.to_string());

        if pos == end {
            break;
        }

        pos += 1;
        if pos == end {
            return Err(VersionError::EmptyIdentifier { at: pos });
        }
    }

    Ok(())
}

fn numeric_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    let core_order = (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch));
    if core_order != Ordering::Equal {
        return core_order;
    }

    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (x, y) in a.pre.iter().zip(&b.pre) {
                let x_numeric = numeric_id(x);
                let y_numeric = numeric_id(y);

                let order = match (x_numeric, y_numeric) {
                    (true, true) => x
                        .len()
                        .cmp(&y.len())
                        .then_with(|| x.as_bytes().cmp(y.as_bytes())),
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    (false, false) => x.as_bytes().cmp(y.as_bytes()),
                };

                if order != Ordering::Equal {
                    return order;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug)]
struct Comparator {
    op: Op,
    version: Version,
    anchor: bool,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Vec<Comparator>>,
}

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn range_tokens(s: &str) -> Vec<(&str, usize)> {
    let bytes = s.as_bytes();
    let mut tokens = Vec::new();
    let mut pos = 0;

    while pos < bytes.len() {
        while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t' | b'\r' | b'\n') {
            pos += 1;
        }
        if pos == bytes.len() {
            break;
        }

        let start = pos;
        while pos < bytes.len() && !matches!(bytes[pos], b' ' | b'\t' | b'\r' | b'\n') {
            pos += 1;
        }
        tokens.push((&s[start..pos], start));
    }

    tokens
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let tokens = range_tokens(s);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms = Vec::new();
    let mut start = 0;

    for i in 0..=tokens.len() {
        if i == tokens.len() || tokens[i].0 == "||" {
            if start == i {
                let at = if i == tokens.len() {
                    s.len()
                } else {
                    tokens[i].1
                };
                return Err(RangeError::Empty { at });
            }

            arms.push(parse_arm(&tokens[start..i])?);
            start = i + 1;
        }
    }

    Ok(Range { arms })
}

fn parse_arm(ts: &[(&str, usize)]) -> Result<Vec<Comparator>, RangeError> {
    if let Some(index) = ts.iter().position(|t| t.0 == "-") {
        if ts.len() != 3 || index != 1 {
            return Err(RangeError::Syntax { at: ts[index].1 });
        }

        let lower = parse_partial_operand(ts[0].0, ts[0].1)?;
        let upper = parse_partial_operand(ts[2].0, ts[2].1)?;
        if lower.1 == 0 {
            return Err(RangeError::Syntax { at: ts[0].1 });
        }
        if upper.1 == 0 {
            return Err(RangeError::Syntax { at: ts[2].1 });
        }

        let mut out = vec![Comparator {
            op: Op::Ge,
            anchor: !lower.0.pre.is_empty(),
            version: lower.0.clone(),
        }];

        if upper.2 {
            out.push(Comparator {
                op: Op::Le,
                anchor: !upper.0.pre.is_empty(),
                version: upper.0,
            });
        } else {
            out.push(Comparator {
                op: Op::Lt,
                version: upper_for_partial(&upper, ts[2].1)?,
                anchor: false,
            });
        }
        return Ok(out);
    }

    let mut out = Vec::new();

    for &(token, at) in ts {
        if token.starts_with('^') || token.starts_with('~') {
            let operator = token.as_bytes()[0];
            let operand = &token[1..];
            let operand_at = at + 1;
            let parsed = parse_partial_operand(operand, operand_at)?;
            if parsed.1 == 0 {
                return Err(RangeError::Syntax { at });
            }

            out.push(Comparator {
                op: Op::Ge,
                anchor: !parsed.0.pre.is_empty(),
                version: parsed.0.clone(),
            });

            let upper = if operator == b'^' {
                caret_upper(&parsed, at)?
            } else {
                tilde_upper(&parsed, at)?
            };
            out.push(Comparator {
                op: Op::Lt,
                version: upper,
                anchor: false,
            });
        } else if token.starts_with('<')
            || token.starts_with('>')
            || token.starts_with('=')
        {
            let (op, length) = if token.starts_with("<=") {
                (Op::Le, 2)
            } else if token.starts_with(">=") {
                (Op::Ge, 2)
            } else {
                (
                    match token.as_bytes()[0] {
                        b'<' => Op::Lt,
                        b'>' => Op::Gt,
                        _ => Op::Eq,
                    },
                    1,
                )
            };

            let operand = &token[length..];
            if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
                return Err(RangeError::Syntax { at });
            }

            let version = parse_version(operand).map_err(|error| RangeError::Version {
                at: at + length,
                error,
            })?;
            out.push(Comparator {
                op,
                anchor: !version.pre.is_empty(),
                version,
            });
        } else if token.as_bytes()[0].is_ascii_digit()
            || matches!(token, "*" | "x" | "X")
        {
            let parsed = parse_partial_operand(token, at)?;
            if parsed.2 {
                out.push(Comparator {
                    op: Op::Eq,
                    anchor: !parsed.0.pre.is_empty(),
                    version: parsed.0,
                });
            } else if parsed.1 == 0 {
                // An all-wildcard bare operand matches all releases.
            } else if parsed.1 == 1 || parsed.1 == 2 {
                let upper = upper_for_partial(&parsed, at)?;
                out.push(Comparator {
                    op: Op::Ge,
                    version: parsed.0,
                    anchor: false,
                });
                out.push(Comparator {
                    op: Op::Lt,
                    version: upper,
                    anchor: false,
                });
            } else {
                out.push(Comparator {
                    op: Op::Eq,
                    anchor: false,
                    version: parsed.0,
                });
            }
        } else {
            return Err(RangeError::Syntax { at });
        }
    }

    Ok(out)
}

// Returns the padded version, numeric field count, and whether it was full.
fn parse_partial_operand(
    token: &str,
    at: usize,
) -> Result<(Version, usize, bool), RangeError> {
    let fields: Vec<&str> = token.split('.').collect();
    let is_full = token.contains('-')
        || token.contains('+')
        || (fields.len() >= 3
            && !fields.iter().any(|field| matches!(*field, "*" | "x" | "X")));

    if is_full {
        let version = parse_version(token).map_err(|error| RangeError::Version { at, error })?;
        return Ok((version, 3, true));
    }

    if fields.len() > 3 || fields.iter().any(|field| field.is_empty()) {
        return Err(RangeError::Syntax { at });
    }

    let mut numbers = [0u64; 3];
    let mut count = 0;
    let mut wildcard_seen = false;

    for (index, field) in fields.iter().enumerate() {
        if matches!(*field, "*" | "x" | "X") {
            wildcard_seen = true;
            continue;
        }

        if wildcard_seen
            || !field.bytes().all(|byte| byte.is_ascii_digit())
            || (field.len() > 1 && field.as_bytes()[0] == b'0')
        {
            return Err(RangeError::Syntax { at });
        }

        numbers[index] = field.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
        count += 1;
    }

    Ok((
        Version {
            major: numbers[0],
            minor: numbers[1],
            patch: numbers[2],
            pre: Vec::new(),
            build: Vec::new(),
        },
        count,
        false,
    ))
}

fn upper_for_partial(
    parsed: &(Version, usize, bool),
    at: usize,
) -> Result<Version, RangeError> {
    let mut version = parsed.0.clone();
    match parsed.1 {
        1 => {
            version.major = version
                .major
                .checked_add(1)
                .ok_or(RangeError::Syntax { at })?;
            version.minor = 0;
            version.patch = 0;
        }
        2 => {
            version.minor = version
                .minor
                .checked_add(1)
                .ok_or(RangeError::Syntax { at })?;
            version.patch = 0;
        }
        _ => {
            version.patch = version
                .patch
                .checked_add(1)
                .ok_or(RangeError::Syntax { at })?;
        }
    }
    version.pre = vec!["0".into()];
    version.build.clear();
    Ok(version)
}

fn caret_upper(parsed: &(Version, usize, bool), at: usize) -> Result<Version, RangeError> {
    let mut version = parsed.0.clone();

    if version.major > 0 || parsed.1 == 1 {
        version.major = version
            .major
            .checked_add(1)
            .ok_or(RangeError::Syntax { at })?;
        version.minor = 0;
        version.patch = 0;
    } else if version.minor > 0 || parsed.1 == 2 {
        version.minor = version
            .minor
            .checked_add(1)
            .ok_or(RangeError::Syntax { at })?;
        version.patch = 0;
    } else {
        version.patch = version
            .patch
            .checked_add(1)
            .ok_or(RangeError::Syntax { at })?;
    }

    version.pre = vec!["0".into()];
    version.build.clear();
    Ok(version)
}

fn tilde_upper(parsed: &(Version, usize, bool), at: usize) -> Result<Version, RangeError> {
    let mut version = parsed.0.clone();

    if parsed.1 == 1 {
        version.major = version
            .major
            .checked_add(1)
            .ok_or(RangeError::Syntax { at })?;
        version.minor = 0;
        version.patch = 0;
    } else {
        version.minor = version
            .minor
            .checked_add(1)
            .ok_or(RangeError::Syntax { at })?;
        version.patch = 0;
    }

    version.pre = vec!["0".into()];
    version.build.clear();
    Ok(version)
}

fn cmp_one(version: &Version, comparator: &Comparator) -> bool {
    let order = compare_versions(version, &comparator.version);
    match comparator.op {
        Op::Eq => order == Ordering::Equal,
        Op::Lt => order == Ordering::Less,
        Op::Le => order != Ordering::Greater,
        Op::Gt => order == Ordering::Greater,
        Op::Ge => order != Ordering::Less,
    }
}

fn arm_admits(version: &Version, arm: &[Comparator]) -> bool {
    version.pre.is_empty()
        || arm.iter().any(|comparator| {
            comparator.anchor
                && !comparator.version.pre.is_empty()
                && core(&comparator.version) == core(version)
        })
}

pub fn matches(version: &Version, range: &Range) -> bool {
    matches_with(version, range, false)
}

pub fn matches_with(version: &Version, range: &Range, include_prerelease: bool) -> bool {
    range.arms.iter().any(|arm| {
        arm.iter().all(|comparator| cmp_one(version, comparator))
            && (include_prerelease || arm_admits(version, arm))
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
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

fn extrema(
    candidates: &[String],
    range: &Range,
    maximum: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let version = parse_version(candidate)
            .map_err(|error| CandidateError { index, error })?;
        parsed.push(version);
    }

    let mut best: Option<usize> = None;
    for (index, version) in parsed.iter().enumerate() {
        if !matches(version, range) {
            continue;
        }

        if let Some(best_index) = best {
            let order = compare_versions(version, &parsed[best_index]);
            if (maximum && order == Ordering::Greater)
                || (!maximum && order == Ordering::Less)
            {
                best = Some(index);
            }
        } else {
            best = Some(index);
        }
    }

    Ok(best.map(|index| candidates[index].clone()))
}

#[derive(Clone)]
struct Bound {
    version: Version,
    inclusive: bool,
}

fn choose_lower(destination: &mut Option<Bound>, bound: Bound) {
    let replace = destination.as_ref().map_or(true, |old| {
        let order = compare_versions(&bound.version, &old.version);
        order == Ordering::Greater
            || (order == Ordering::Equal && !bound.inclusive && old.inclusive)
    });
    if replace {
        *destination = Some(bound);
    }
}

fn choose_upper(destination: &mut Option<Bound>, bound: Bound) {
    let replace = destination.as_ref().map_or(true, |old| {
        let order = compare_versions(&bound.version, &old.version);
        order == Ordering::Less
            || (order == Ordering::Equal && !bound.inclusive && old.inclusive)
    });
    if replace {
        *destination = Some(bound);
    }
}

fn format_version(version: &Version) -> String {
    let mut result = format!("{}.{}.{}", version.major, version.minor, version.patch);
    if !version.pre.is_empty() {
        result.push('-');
        result.push_str(&version.pre.join("."));
    }
    result
}

fn format_bound(bound: Option<&Bound>, lower: bool) -> String {
    match bound {
        None => "*".into(),
        Some(bound) => {
            let operator = match (lower, bound.inclusive) {
                (true, true) => ">=",
                (true, false) => ">",
                (false, true) => "<=",
                (false, false) => "<",
            };
            format!("{operator}{}", format_version(&bound.version))
        }
    }
}

fn bounds_maybe_nonempty(lower: &Option<Bound>, upper: &Option<Bound>) -> bool {
    match (lower, upper) {
        (Some(lower), Some(upper)) => {
            let order = compare_versions(&lower.version, &upper.version);
            order == Ordering::Less
                || (order == Ordering::Equal && lower.inclusive && upper.inclusive)
        }
        _ => true,
    }
}

pub fn canonical(range: &Range) -> String {
    let mut strings = Vec::new();

    for arm in &range.arms {
        let mut lower = None;
        let mut upper = None;
        let mut anchors: Vec<(u64, u64, u64)> = arm
            .iter()
            .filter(|comparator| comparator.anchor && !comparator.version.pre.is_empty())
            .map(|comparator| core(&comparator.version))
            .collect();
        anchors.sort();
        anchors.dedup();

        for comparator in arm {
            match comparator.op {
                Op::Eq => {
                    choose_lower(
                        &mut lower,
                        Bound {
                            version: comparator.version.clone(),
                            inclusive: true,
                        },
                    );
                    choose_upper(
                        &mut upper,
                        Bound {
                            version: comparator.version.clone(),
                            inclusive: true,
                        },
                    );
                }
                Op::Gt | Op::Ge => choose_lower(
                    &mut lower,
                    Bound {
                        version: comparator.version.clone(),
                        inclusive: comparator.op == Op::Ge,
                    },
                ),
                Op::Lt | Op::Le => choose_upper(
                    &mut upper,
                    Bound {
                        version: comparator.version.clone(),
                        inclusive: comparator.op == Op::Le,
                    },
                ),
            }
        }

        if !bounds_maybe_nonempty(&lower, &upper) {
            continue;
        }

        let lower_text = format_bound(lower.as_ref(), true);
        let upper_text = format_bound(upper.as_ref(), false);
        let anchor_text = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors
                .iter()
                .map(|(major, minor, patch)| format!("{major}.{minor}.{patch}"))
                .collect::<Vec<_>>()
                .join(",")
        };

        strings.push(format!(
            "[{lower_text};{upper_text};pre={anchor_text}]"
        ));
    }

    strings.sort();
    strings.dedup();

    if strings.is_empty() {
        "EMPTY".into()
    } else {
        strings.join(" || ")
    }
}

fn next_core_version(version: &Version) -> Option<Version> {
    let (major, minor, patch) = if let Some(patch) = version.patch.checked_add(1) {
        (version.major, version.minor, patch)
    } else if let Some(minor) = version.minor.checked_add(1) {
        (version.major, minor, 0)
    } else if let Some(major) = version.major.checked_add(1) {
        (major, 0, 0)
    } else {
        return None;
    };

    Some(Version {
        major,
        minor,
        patch,
        pre: Vec::new(),
        build: Vec::new(),
    })
}

fn stable_witness(lower: &Option<Bound>, upper: &Option<Bound>) -> Option<Version> {
    let candidate = match lower {
        None => Version {
            major: 0,
            minor: 0,
            patch: 0,
            pre: Vec::new(),
            build: Vec::new(),
        },
        Some(bound) if bound.version.pre.is_empty() && bound.inclusive => Version {
            build: Vec::new(),
            ..bound.version.clone()
        },
        Some(bound) if !bound.version.pre.is_empty() => Version {
            major: bound.version.major,
            minor: bound.version.minor,
            patch: bound.version.patch,
            pre: Vec::new(),
            build: Vec::new(),
        },
        Some(bound) => next_core_version(&bound.version)?,
    };

    if let Some(upper) = upper {
        let order = compare_versions(&candidate, &upper.version);
        if order == Ordering::Greater || (order == Ordering::Equal && !upper.inclusive) {
            return None;
        }
    }

    Some(candidate)
}

fn arms_intersect(a: &[Comparator], b: &[Comparator], include_prerelease: bool) -> bool {
    let mut all = a.to_vec();
    all.extend_from_slice(b);

    let mut lower = None;
    let mut upper = None;
    for comparator in &all {
        match comparator.op {
            Op::Eq => {
                choose_lower(
                    &mut lower,
                    Bound {
                        version: comparator.version.clone(),
                        inclusive: true,
                    },
                );
                choose_upper(
                    &mut upper,
                    Bound {
                        version: comparator.version.clone(),
                        inclusive: true,
                    },
                );
            }
            Op::Gt | Op::Ge => choose_lower(
                &mut lower,
                Bound {
                    version: comparator.version.clone(),
                    inclusive: comparator.op == Op::Ge,
                },
            ),
            Op::Lt | Op::Le => choose_upper(
                &mut upper,
                Bound {
                    version: comparator.version.clone(),
                    inclusive: comparator.op == Op::Le,
                },
            ),
        }
    }

    if let (Some(lower), Some(upper)) = (&lower, &upper) {
        let order = compare_versions(&lower.version, &upper.version);
        if order == Ordering::Greater
            || (order == Ordering::Equal && !(lower.inclusive && upper.inclusive))
        {
            return false;
        }
    }

    if include_prerelease {
        return true;
    }

    let anchors_a: Vec<_> = a
        .iter()
        .filter(|comparator| comparator.anchor && !comparator.version.pre.is_empty())
        .map(|comparator| core(&comparator.version))
        .collect();
    let anchors_b: Vec<_> = b
        .iter()
        .filter(|comparator| comparator.anchor && !comparator.version.pre.is_empty())
        .map(|comparator| core(&comparator.version))
        .collect();

    for tuple in anchors_a.iter().filter(|tuple| anchors_b.contains(tuple)) {
        let witness = Version {
            major: tuple.0,
            minor: tuple.1,
            patch: tuple.2,
            pre: vec!["0".into()],
            build: Vec::new(),
        };
        if all.iter().all(|comparator| cmp_one(&witness, comparator)) {
            return true;
        }
    }

    if let Some(witness) = stable_witness(&lower, &upper) {
        return all
            .iter()
            .all(|comparator| cmp_one(&witness, comparator));
    }

    false
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    a.arms.iter().any(|arm_a| {
        b.arms
            .iter()
            .any(|arm_b| arms_intersect(arm_a, arm_b, include_prerelease))
    })
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
    fn public_version_comparators() {
        let version = pv("2.1.3-rc.1+build.7");
        assert_eq!((version.major, version.minor, version.patch), (2, 1, 3));
        assert_eq!(version.pre, vec!["rc", "1"]);
        assert_eq!(version.build, vec!["build", "7"]);
        assert_eq!(
            compare_versions(&pv("2.1.3-alpha"), &pv("2.1.3-beta")),
            Ordering::Less
        );

        let range = pr(">=2.1.0 <3.0.0");
        assert!(matches(&pv("2.5.0"), &range));
        assert!(!matches(&pv("3.0.0"), &range));
        assert!(matches(
            &pv("2.1.3-rc"),
            &pr(">=2.1.3-beta <2.1.3")
        ));
    }

    #[test]
    fn public_expanded_extrema() {
        for text in ["^2.1.0", "~2.5.0", "2.5.x", "2.1 - 2.8"] {
            assert!(matches(&pv("2.5.1"), &pr(text)));
        }
        assert!(matches(&pv("4.0.0"), &pr("2.x || 4.0.0")));

        let candidates = vec![
            "2.2.0".to_string(),
            "2.8.0".to_string(),
            "3.0.0".to_string(),
        ];
        let range = pr("2.x");
        assert_eq!(
            max_satisfying(&candidates, &range),
            Ok(Some("2.8.0".into()))
        );
        assert_eq!(
            min_satisfying(&candidates, &range),
            Ok(Some("2.2.0".into()))
        );
    }

    #[test]
    fn public_algebra() {
        let range = pr(">=2.1.0 <2.8.0");
        assert!(intersects(&range, &pr("2.5.x"), false));
        assert_eq!(canonical(&range), "[>=2.1.0;<2.8.0;pre=-]");
        assert!(matches_with(&pv("2.5.0-rc"), &range, true));
    }

    #[test]
    fn public_canonical_empty() {
        assert_eq!(canonical(&pr(">=2.0.0 <2.0.0")), "EMPTY");
        assert!(!intersects(&pr("2.x"), &pr("3.x"), true));
    }
}