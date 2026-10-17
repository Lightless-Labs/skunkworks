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

    for (field, value) in core.iter_mut().enumerate() {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }

        if start == pos {
            return Err(if pos == bytes.len() || matches!(bytes[pos], b'.' | b'-' | b'+') {
                VersionError::MissingCore { at: pos }
            } else {
                VersionError::InvalidChar { at: pos }
            });
        }

        if pos - start > 1 && bytes[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }
        *value = s[start..pos]
            .parse()
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
        let start = pos;
        while pos < bytes.len() && bytes[pos] != b'+' {
            pos += 1;
        }
        parse_identifiers(s, start, pos, true, &mut pre)?;
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        pos += 1;
        parse_identifiers(s, pos, bytes.len(), false, &mut build)?;
        pos = bytes.len();
    }

    if pos != bytes.len() {
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
    is_pre: bool,
    output: &mut Vec<String>,
) -> Result<(), VersionError> {
    let bytes = s.as_bytes();
    let mut field_start = start;

    loop {
        let field_end = (field_start..end)
            .find(|&i| bytes[i] == b'.')
            .unwrap_or(end);

        if field_start == field_end {
            return Err(VersionError::EmptyIdentifier { at: field_start });
        }

        for i in field_start..field_end {
            let c = bytes[i];
            if !(c.is_ascii_alphanumeric() || c == b'-') {
                return Err(VersionError::InvalidChar { at: i });
            }
        }

        let identifier = &bytes[field_start..field_end];
        if is_pre
            && identifier.iter().all(u8::is_ascii_digit)
            && identifier.len() > 1
            && identifier[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: field_start });
        }

        output.push(s[field_start..field_end].to_string());
        if field_end == end {
            break;
        }
        field_start = field_end + 1;
    }

    Ok(())
}

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn numeric(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    let core_order = core(a).cmp(&core(b));
    if core_order != Ordering::Equal {
        return core_order;
    }
    if a.pre.is_empty() {
        return if b.pre.is_empty() {
            Ordering::Equal
        } else {
            Ordering::Greater
        };
    }
    if b.pre.is_empty() {
        return Ordering::Less;
    }

    for (x, y) in a.pre.iter().zip(&b.pre) {
        let xn = numeric(x);
        let yn = numeric(y);
        let order = match (xn, yn) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (true, true) => x.len().cmp(&y.len()).then_with(|| x.as_bytes().cmp(y.as_bytes())),
            (false, false) => x.as_bytes().cmp(y.as_bytes()),
        };
        if order != Ordering::Equal {
            return order;
        }
    }

    a.pre.len().cmp(&b.pre.len())
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
struct Bound {
    version: Version,
    inclusive: bool,
}

#[derive(Clone, Debug)]
struct Comparison {
    op: u8,
    version: Version,
}

#[derive(Clone, Debug)]
struct Arm {
    comparisons: Vec<Comparison>,
    anchors: Vec<(u64, u64, u64)>,
}

fn is_range_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokenize(s: &str) -> Vec<(String, usize)> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut pos = 0;

    while pos < bytes.len() {
        while pos < bytes.len() && is_range_space(bytes[pos]) {
            pos += 1;
        }
        if pos == bytes.len() {
            break;
        }
        let start = pos;
        while pos < bytes.len() && !is_range_space(bytes[pos]) {
            pos += 1;
        }
        result.push((s[start..pos].to_string(), start));
    }

    result
}

fn version_error(at: usize, error: VersionError) -> RangeError {
    RangeError::Version { at, error }
}

fn parse_full_operand(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| version_error(at, error))
}

#[derive(Clone)]
struct Partial {
    numbers: Vec<u64>,
    full: Option<Version>,
    numeric_count: usize,
}

fn parse_partial(s: &str, at: usize) -> Result<Partial, RangeError> {
    let fields: Vec<&str> = s.split('.').collect();
    let has_wildcard = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));

    if s.contains('-') || s.contains('+') || (fields.len() >= 3 && !has_wildcard) {
        let version = parse_full_operand(s, at)?;
        return Ok(Partial {
            numbers: vec![version.major, version.minor, version.patch],
            full: Some(version),
            numeric_count: 3,
        });
    }

    if fields.len() > 3 {
        return Err(RangeError::Syntax { at });
    }

    let mut numbers = Vec::new();
    let mut wildcard_seen = false;

    for field in fields {
        if matches!(field, "*" | "x" | "X") {
            wildcard_seen = true;
            continue;
        }

        if field.is_empty()
            || wildcard_seen
            || !field.bytes().all(|b| b.is_ascii_digit())
            || (field.len() > 1 && field.starts_with('0'))
        {
            return Err(RangeError::Syntax { at });
        }

        let value = field.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
        numbers.push(value);
    }

    let numeric_count = numbers.len();
    Ok(Partial {
        numbers,
        full: None,
        numeric_count,
    })
}

fn make_version(tuple: (u64, u64, u64), pre: Vec<String>) -> Version {
    Version {
        major: tuple.0,
        minor: tuple.1,
        patch: tuple.2,
        pre,
        build: Vec::new(),
    }
}

fn padded(partial: &Partial) -> Version {
    if let Some(version) = &partial.full {
        return version.clone();
    }
    make_version(
        (
            partial.numbers.first().copied().unwrap_or(0),
            partial.numbers.get(1).copied().unwrap_or(0),
            partial.numbers.get(2).copied().unwrap_or(0),
        ),
        Vec::new(),
    )
}

fn expanded_upper(partial: &Partial, mode: u8, at: usize) -> Result<Version, RangeError> {
    let mut tuple = (
        partial.numbers.first().copied().unwrap_or(0),
        partial.numbers.get(1).copied().unwrap_or(0),
        partial.numbers.get(2).copied().unwrap_or(0),
    );
    let increment = |n: u64| n.checked_add(1).ok_or(RangeError::Syntax { at });

    match mode {
        0 => {
            if partial.numeric_count == 1 {
                tuple.0 = increment(tuple.0)?;
                tuple.1 = 0;
                tuple.2 = 0;
            } else {
                tuple.1 = increment(tuple.1)?;
                tuple.2 = 0;
            }
        }
        1 => {
            if tuple.0 > 0 || partial.numeric_count == 1 {
                tuple.0 = increment(tuple.0)?;
                tuple.1 = 0;
                tuple.2 = 0;
            } else if tuple.1 > 0 || partial.numeric_count == 2 {
                tuple.1 = increment(tuple.1)?;
                tuple.2 = 0;
            } else {
                tuple.2 = increment(tuple.2)?;
            }
        }
        _ => {
            if partial.numeric_count == 1 {
                tuple.0 = increment(tuple.0)?;
                tuple.1 = 0;
                tuple.2 = 0;
            } else {
                tuple.1 = increment(tuple.1)?;
                tuple.2 = 0;
            }
        }
    }

    Ok(make_version(tuple, vec!["0".to_string()]))
}

fn add_comparison(arm: &mut Arm, op: u8, version: Version) {
    arm.comparisons.push(Comparison { op, version });
}

fn parse_arm(tokens: &[(String, usize)]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        comparisons: Vec::new(),
        anchors: Vec::new(),
    };

    if let Some((minus_index, (_, minus_at))) =
        tokens.iter().enumerate().find(|(_, (token, _))| token == "-")
    {
        if minus_index != 1 || tokens.len() != 3 {
            return Err(RangeError::Syntax { at: *minus_at });
        }

        let lower = parse_partial(&tokens[0].0, tokens[0].1)?;
        let upper = parse_partial(&tokens[2].0, tokens[2].1)?;

        if lower.numeric_count == 0 {
            return Err(RangeError::Syntax { at: tokens[0].1 });
        }
        if upper.numeric_count == 0 {
            return Err(RangeError::Syntax { at: tokens[2].1 });
        }

        let low_version = padded(&lower);
        add_comparison(&mut arm, b'>', low_version.clone());
        if !low_version.pre.is_empty() {
            arm.anchors.push(core(&low_version));
        }

        if let Some(version) = upper.full {
            add_comparison(&mut arm, b'=', version.clone());
            if !version.pre.is_empty() {
                arm.anchors.push(core(&version));
            }
        } else if upper.numeric_count == 3 {
            let version = padded(&upper);
            add_comparison(&mut arm, b'=', version.clone());
        } else {
            let version = expanded_upper(&upper, 0, tokens[2].1)?;
            add_comparison(&mut arm, b'<', version);
        }

        return Ok(arm);
    }

    for (token, at) in tokens {
        let bytes = token.as_bytes();
        if bytes.is_empty() {
            continue;
        }

        if matches!(bytes[0], b'<' | b'>' | b'=') {
            let mut operator_len = 1;
            if bytes.len() > 1 && bytes[1] == b'=' {
                operator_len = 2;
            }

            if bytes.len() == operator_len || !bytes[operator_len].is_ascii_digit() {
                return Err(RangeError::Syntax { at: *at });
            }

            let op = match (bytes[0], operator_len) {
                (b'<', 1) => b'<',
                (b'<', _) => b'(',
                (b'>', 1) => b'>',
                (b'>', _) => b')',
                _ => b'=',
            };
            let version = parse_full_operand(&token[operator_len..], at + operator_len)?;
            if !version.pre.is_empty() {
                arm.anchors.push(core(&version));
            }
            add_comparison(&mut arm, op, version);
            continue;
        }

        let (expansion, body, operand_at) = if bytes[0] == b'^' || bytes[0] == b'~' {
            (bytes[0], &token[1..], at + 1)
        } else {
            (0, token.as_str(), *at)
        };

        if !matches!(bytes[0], b'^' | b'~' | b'*' | b'x' | b'X')
            && !bytes[0].is_ascii_digit()
        {
            return Err(RangeError::Syntax { at: *at });
        }

        let partial = parse_partial(body, operand_at)?;

        if expansion == 0 {
            if let Some(version) = partial.full {
                if !version.pre.is_empty() {
                    arm.anchors.push(core(&version));
                }
                add_comparison(&mut arm, b'=', version);
            } else if partial.numeric_count == 1 || partial.numeric_count == 2 {
                let lower = padded(&partial);
                add_comparison(&mut arm, b'>', lower);
                let upper = expanded_upper(&partial, 0, operand_at)?;
                add_comparison(&mut arm, b'<', upper);
            }
        } else {
            if partial.numeric_count == 0 {
                return Err(RangeError::Syntax { at: *at });
            }
            let lower = padded(&partial);
            if !lower.pre.is_empty() {
                arm.anchors.push(core(&lower));
            }
            add_comparison(&mut arm, b'>', lower);
            let mode = if expansion == b'^' { 1 } else { 2 };
            let upper = expanded_upper(&partial, mode, operand_at)?;
            add_comparison(&mut arm, b'<', upper);
        }
    }

    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let tokens = tokenize(s);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut groups: Vec<Vec<(String, usize)>> = vec![Vec::new()];
    for (token, at) in tokens {
        if token == "||" {
            if groups.last().unwrap().is_empty() {
                return Err(RangeError::Empty { at });
            }
            groups.push(Vec::new());
        } else {
            groups.last_mut().unwrap().push((token, at));
        }
    }

    if groups.last().unwrap().is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }

    let mut arms = Vec::with_capacity(groups.len());
    for group in groups {
        arms.push(parse_arm(&group)?);
    }
    Ok(Range { arms })
}

fn comparison_matches(comparison: &Comparison, version: &Version) -> bool {
    let order = compare_versions(version, &comparison.version);
    match comparison.op {
        b'=' => order == Ordering::Equal,
        b'<' => order == Ordering::Less,
        b'>' => order == Ordering::Greater,
        b'(' => order != Ordering::Greater,
        b')' => order != Ordering::Less,
        _ => true,
    }
}

fn admitted(version: &Version, arm: &Arm) -> bool {
    version.pre.is_empty() || arm.anchors.contains(&core(version))
}

fn arm_matches(version: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    arm.comparisons
        .iter()
        .all(|comparison| comparison_matches(comparison, version))
        && (include_prerelease || admitted(version, arm))
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

fn find_extreme(
    candidates: &[String],
    range: &Range,
    greatest: bool,
) -> Result<Option<String>, CandidateError> {
    let mut best: Option<(String, Version)> = None;

    for (index, candidate) in candidates.iter().enumerate() {
        let version = parse_version(candidate).map_err(|error| CandidateError { index, error })?;
        if !matches(&version, range) {
            continue;
        }

        let replace = best.as_ref().map_or(true, |(_, previous)| {
            let order = compare_versions(&version, previous);
            if greatest {
                order == Ordering::Greater
            } else {
                order == Ordering::Less
            }
        });

        if replace {
            best = Some((candidate.clone(), version));
        }
    }

    Ok(best.map(|(candidate, _)| candidate))
}

pub fn max_satisfying(
    candidates: &[String],
    range: &Range,
) -> Result<Option<String>, CandidateError> {
    find_extreme(candidates, range, true)
}

pub fn min_satisfying(
    candidates: &[String],
    range: &Range,
) -> Result<Option<String>, CandidateError> {
    find_extreme(candidates, range, false)
}

#[derive(Clone)]
struct Interval {
    lower: Option<Bound>,
    upper: Option<Bound>,
}

fn set_lower(interval: &mut Interval, bound: Bound) {
    let replace = interval.lower.as_ref().map_or(true, |old| {
        let order = compare_versions(&bound.version, &old.version);
        order == Ordering::Greater || (order == Ordering::Equal && !bound.inclusive && old.inclusive)
    });
    if replace {
        interval.lower = Some(bound);
    }
}

fn set_upper(interval: &mut Interval, bound: Bound) {
    let replace = interval.upper.as_ref().map_or(true, |old| {
        let order = compare_versions(&bound.version, &old.version);
        order == Ordering::Less || (order == Ordering::Equal && !bound.inclusive && old.inclusive)
    });
    if replace {
        interval.upper = Some(bound);
    }
}

fn interval_of(arm: &Arm) -> Interval {
    let mut interval = Interval {
        lower: None,
        upper: None,
    };

    for comparison in &arm.comparisons {
        let version = comparison.version.clone();
        match comparison.op {
            b'=' => {
                set_lower(
                    &mut interval,
                    Bound {
                        version: version.clone(),
                        inclusive: true,
                    },
                );
                set_upper(
                    &mut interval,
                    Bound {
                        version,
                        inclusive: true,
                    },
                );
            }
            b'>' | b'(' => set_lower(
                &mut interval,
                Bound {
                    version,
                    inclusive: comparison.op == b'(',
                },
            ),
            b'<' | b')' => set_upper(
                &mut interval,
                Bound {
                    version,
                    inclusive: comparison.op == b')',
                },
            ),
            _ => {}
        }
    }

    interval
}

fn above_lower(version: &Version, lower: &Option<Bound>) -> bool {
    lower.as_ref().map_or(true, |bound| {
        let order = compare_versions(version, &bound.version);
        order == Ordering::Greater || (order == Ordering::Equal && bound.inclusive)
    })
}

fn below_upper(version: &Version, upper: &Option<Bound>) -> bool {
    upper.as_ref().map_or(true, |bound| {
        let order = compare_versions(version, &bound.version);
        order == Ordering::Less || (order == Ordering::Equal && bound.inclusive)
    })
}

fn stable_witness(interval: &Interval) -> Option<Version> {
    let candidate = if let Some(lower) = &interval.lower {
        if lower.inclusive && lower.version.pre.is_empty() {
            lower.version.clone()
        } else if lower.version.pre.is_empty() {
            let (mut major, mut minor, mut patch) = core(&lower.version);
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
            make_version((major, minor, patch), Vec::new())
        } else {
            make_version(core(&lower.version), Vec::new())
        }
    } else {
        make_version((0, 0, 0), Vec::new())
    };

    if above_lower(&candidate, &interval.lower) && below_upper(&candidate, &interval.upper) {
        Some(candidate)
    } else {
        None
    }
}

fn has_prerelease_witness(interval: &Interval, anchors: &[(u64, u64, u64)]) -> bool {
    anchors.iter().any(|tuple| {
        let candidate = make_version(*tuple, vec!["0".to_string()]);
        above_lower(&candidate, &interval.lower) && below_upper(&candidate, &interval.upper)
    })
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for left in &a.arms {
        for right in &b.arms {
            let mut interval = interval_of(left);
            let other = interval_of(right);

            if let Some(lower) = other.lower {
                set_lower(&mut interval, lower);
            }
            if let Some(upper) = other.upper {
                set_upper(&mut interval, upper);
            }

            if stable_witness(&interval).is_some() {
                return true;
            }

            if include_prerelease {
                if has_prerelease_witness(&interval, &[]) {
                    return true;
                }
            } else {
                let shared: Vec<_> = left
                    .anchors
                    .iter()
                    .filter(|tuple| right.anchors.contains(tuple))
                    .copied()
                    .collect();
                if has_prerelease_witness(&interval, &shared) {
                    return true;
                }
            }
        }
    }

    false
}

fn display_version(version: &Version) -> String {
    let mut result = format!("{}.{}.{}", version.major, version.minor, version.patch);
    if !version.pre.is_empty() {
        result.push('-');
        result.push_str(&version.pre.join("."));
    }
    result
}

fn canonical_arm(arm: &Arm) -> Option<String> {
    let interval = interval_of(arm);

    // A nonempty real-valued interval may still contain no SemVer version.
    // Check both stable releases and the explicitly admitted prerelease tuples.
    if stable_witness(&interval).is_none()
        && !has_prerelease_witness(&interval, &arm.anchors)
    {
        return None;
    }

    let lower = interval.lower.map_or_else(
        || "*".to_string(),
        |bound| {
            format!(
                "{}{}",
                if bound.inclusive { ">=" } else { ">" },
                display_version(&bound.version)
            )
        },
    );
    let upper = interval.upper.map_or_else(
        || "*".to_string(),
        |bound| {
            format!(
                "{}{}",
                if bound.inclusive { "<=" } else { "<" },
                display_version(&bound.version)
            )
        },
    );

    let mut anchors = arm.anchors.clone();
    anchors.sort();
    anchors.dedup();

    let anchor_text = if anchors.is_empty() {
        "-".to_string()
    } else {
        anchors
            .iter()
            .map(|(major, minor, patch)| format!("{major}.{minor}.{patch}"))
            .collect::<Vec<_>>()
            .join(",")
    };

    Some(format!("[{lower};{upper};pre={anchor_text}]"))
}

pub fn canonical(range: &Range) -> String {
    let mut arms: Vec<String> = range.arms.iter().filter_map(canonical_arm).collect();
    arms.sort();
    arms.dedup();

    if arms.is_empty() {
        "EMPTY".to_string()
    } else {
        arms.join(" || ")
    }
}