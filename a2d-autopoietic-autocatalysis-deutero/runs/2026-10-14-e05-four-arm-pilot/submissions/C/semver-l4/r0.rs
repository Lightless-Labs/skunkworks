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

/// Parses a strict SemVer version string.
pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let bytes = s.as_bytes();
    let mut pos = 0usize;
    let mut core = [0u64; 3];

    for field in 0..3 {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if start == pos {
            let missing = pos == bytes.len()
                || matches!(bytes[pos], b'.' | b'-' | b'+');
            return Err(if missing {
                VersionError::MissingCore { at: start }
            } else {
                VersionError::InvalidChar { at: start }
            });
        }
        if pos - start > 1 && bytes[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }
        core[field] = s[start..pos]
            .parse::<u64>()
            .map_err(|_| VersionError::Overflow { at: start })?;

        if field < 2 {
            if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            if bytes[pos] != b'.' {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
        } else if pos < bytes.len() && !matches!(bytes[pos], b'-' | b'+') {
            return Err(VersionError::InvalidChar { at: pos });
        }
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();
    if pos < bytes.len() && bytes[pos] == b'-' {
        let start = pos + 1;
        let end = bytes[start..]
            .iter()
            .position(|b| *b == b'+')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());
        pre = parse_identifiers(s, start, end, true)?;
        pos = end;
    }
    if pos < bytes.len() && bytes[pos] == b'+' {
        build = parse_identifiers(s, pos + 1, bytes.len(), false)?;
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
    check_numeric_zero: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut id_start = start;
    loop {
        if id_start >= end {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        let id_end = bytes[id_start..end]
            .iter()
            .position(|b| *b == b'.')
            .map(|offset| id_start + offset)
            .unwrap_or(end);
        if id_end == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        for index in id_start..id_end {
            let byte = bytes[index];
            if !(byte.is_ascii_alphanumeric() || byte == b'-') {
                return Err(VersionError::InvalidChar { at: index });
            }
        }
        let id = &s[id_start..id_end];
        if check_numeric_zero
            && id.len() > 1
            && id.bytes().all(|byte| byte.is_ascii_digit())
            && id.starts_with('0')
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

/// Compares SemVer precedence, ignoring build metadata.
pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    let core = (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch));
    if core != Ordering::Equal {
        return core;
    }
    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (left, right) in a.pre.iter().zip(&b.pre) {
                let order = compare_identifiers(left, right);
                if order != Ordering::Equal {
                    return order;
                }
            }
            a.pre.len().cmp(&b.pre.len())
        }
    }
}

fn compare_identifiers(a: &str, b: &str) -> Ordering {
    let a_numeric = a.bytes().all(|byte| byte.is_ascii_digit());
    let b_numeric = b.bytes().all(|byte| byte.is_ascii_digit());
    match (a_numeric, b_numeric) {
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
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
struct Check {
    op: Op,
    version: Version,
    anchor: bool,
}

#[derive(Clone, Debug)]
struct Arm {
    checks: Vec<Check>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

#[derive(Clone)]
struct Token<'a> {
    text: &'a str,
    at: usize,
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokens(s: &str) -> Vec<Token<'_>> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        while pos < bytes.len() && is_space(bytes[pos]) {
            pos += 1;
        }
        if pos == bytes.len() {
            break;
        }
        let start = pos;
        while pos < bytes.len() && !is_space(bytes[pos]) {
            pos += 1;
        }
        result.push(Token {
            text: &s[start..pos],
            at: start,
        });
    }
    result
}

fn split_arms<'a>(s: &'a str) -> Result<Vec<Vec<Token<'a>>>, RangeError> {
    let all = tokens(s);
    if all.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    let mut arms = vec![Vec::new()];
    let mut last_sep_end = None;
    for token in all {
        if token.text == "||" {
            if arms.last().is_some_and(Vec::is_empty) {
                return Err(RangeError::Empty { at: token.at });
            }
            arms.push(Vec::new());
            last_sep_end = Some(token.at + 2);
        } else {
            arms.last_mut().expect("an arm exists").push(token);
            last_sep_end = None;
        }
    }
    if arms.last().is_some_and(Vec::is_empty) {
        return Err(RangeError::Empty {
            at: last_sep_end.unwrap_or(s.len()),
        });
    }
    Ok(arms)
}

fn parse_full(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

fn is_wildcard(field: &str) -> bool {
    matches!(field, "*" | "x" | "X")
}

#[derive(Clone)]
struct Operand {
    version: Version,
    k: usize,
    full: bool,
    anchor: bool,
}

fn parse_operand(text: &str, at: usize) -> Result<Operand, RangeError> {
    let fields: Vec<&str> = text.split('.').collect();
    let full_form = text.contains('-')
        || text.contains('+')
        || (fields.len() >= 3 && !fields.iter().any(|field| is_wildcard(field)));
    if full_form {
        let version = parse_full(text, at)?;
        let numeric_fields = 3;
        return Ok(Operand {
            anchor: !version.pre.is_empty(),
            version,
            k: numeric_fields,
            full: true,
        });
    }

    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|field| field.is_empty()) {
        return Err(RangeError::Syntax { at });
    }
    let mut core = [0u64; 3];
    let mut numeric_count = 0usize;
    let mut saw_wildcard = false;
    for (index, field) in fields.iter().enumerate() {
        if is_wildcard(field) {
            saw_wildcard = true;
        } else {
            if saw_wildcard
                || !field.bytes().all(|byte| byte.is_ascii_digit())
                || (field.len() > 1 && field.starts_with('0'))
            {
                return Err(RangeError::Syntax { at });
            }
            core[index] = field.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
            numeric_count += 1;
        }
    }
    Ok(Operand {
        version: Version {
            major: core[0],
            minor: core[1],
            patch: core[2],
            pre: Vec::new(),
            build: Vec::new(),
        },
        k: numeric_count,
        full: false,
        anchor: false,
    })
}

fn endpoint(core: (u64, u64, u64), pre: Vec<String>) -> Version {
    Version {
        major: core.0,
        minor: core.1,
        patch: core.2,
        pre,
        build: Vec::new(),
    }
}

fn bump_for_partial(op: &Operand, at: usize, mode: &str) -> Result<Version, RangeError> {
    let mut core = (
        op.version.major,
        op.version.minor,
        op.version.patch,
    );
    let field = match mode {
        "caret" => {
            if core.0 > 0 || op.k == 1 {
                0
            } else if core.1 > 0 || op.k == 2 {
                1
            } else {
                2
            }
        }
        "tilde" => {
            if op.k == 1 {
                0
            } else {
                1
            }
        }
        _ => {
            if op.k == 1 {
                0
            } else {
                1
            }
        }
    };
    let incremented = match field {
        0 => core.0.checked_add(1).map(|value| core.0 = value),
        1 => core.1.checked_add(1).map(|value| core.1 = value),
        _ => core.2.checked_add(1).map(|value| core.2 = value),
    };
    if incremented.is_none() {
        return Err(RangeError::Syntax { at });
    }
    if field == 0 {
        core.1 = 0;
        core.2 = 0;
    } else if field == 1 {
        core.2 = 0;
    }
    Ok(endpoint(core, vec!["0".to_string()]))
}

fn check(op: Op, version: Version, anchor: bool) -> Check {
    Check { op, version, anchor }
}

fn parse_arm(tokens: &[Token<'_>]) -> Result<Arm, RangeError> {
    let mut checks = Vec::new();
    let mut anchors = Vec::new();
    let standalone_minus = tokens.iter().find(|token| token.text == "-");
    if let Some(minus) = standalone_minus {
        if tokens.len() != 3 || tokens[1].text != "-" {
            return Err(RangeError::Syntax { at: minus.at });
        }
        let lower = parse_operand(tokens[0].text, tokens[0].at)?;
        let upper = parse_operand(tokens[2].text, tokens[2].at)?;
        if lower.k == 0 {
            return Err(RangeError::Syntax { at: tokens[0].at });
        }
        if upper.k == 0 {
            return Err(RangeError::Syntax { at: tokens[2].at });
        }
        if lower.anchor {
            anchors.push((lower.version.major, lower.version.minor, lower.version.patch));
        }
        if upper.anchor {
            anchors.push((upper.version.major, upper.version.minor, upper.version.patch));
        }
        checks.push(check(Op::Ge, lower.version, lower.anchor));
        if upper.full {
            checks.push(check(Op::Le, upper.version, upper.anchor));
        } else if upper.k == 1 || upper.k == 2 {
            checks.push(check(
                Op::Lt,
                bump_for_partial(&upper, tokens[2].at, "hyphen")?,
                false,
            ));
        } else {
            checks.push(check(Op::Le, upper.version, false));
        }
        return Ok(Arm { checks, anchors });
    }

    for token in tokens {
        let text = token.text;
        let first = text.as_bytes()[0];
        if !first.is_ascii_digit() && !matches!(first, b'*' | b'x' | b'X' | b'^' | b'~' | b'<' | b'>' | b'=') {
            return Err(RangeError::Syntax { at: token.at });
        }
        if matches!(first, b'<' | b'>' | b'=') {
            let (op, length) = if text.starts_with("<=") {
                (Op::Le, 2)
            } else if text.starts_with(">=") {
                (Op::Ge, 2)
            } else if text.starts_with('<') {
                (Op::Lt, 1)
            } else if text.starts_with('>') {
                (Op::Gt, 1)
            } else {
                (Op::Eq, 1)
            };
            let operand = &text[length..];
            if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
                return Err(RangeError::Syntax { at: token.at });
            }
            let version = parse_full(operand, token.at + length)?;
            if !version.pre.is_empty() {
                anchors.push((version.major, version.minor, version.patch));
            }
            checks.push(check(op, version.clone(), !version.pre.is_empty()));
            continue;
        }
        let (mode, operand_text, operand_at) = if first == b'^' || first == b'~' {
            (&text[..1], &text[1..], token.at + 1)
        } else {
            ("", text, token.at)
        };
        let operand = parse_operand(operand_text, operand_at)?;
        if !mode.is_empty() && operand.k == 0 {
            return Err(RangeError::Syntax { at: token.at });
        }
        if operand.anchor {
            anchors.push((operand.version.major, operand.version.minor, operand.version.patch));
        }
        if mode == "^" || mode == "~" {
            checks.push(check(Op::Ge, operand.version.clone(), operand.anchor));
            checks.push(check(
                Op::Lt,
                bump_for_partial(
                    &operand,
                    operand_at,
                    if mode == "^" { "caret" } else { "tilde" },
                )?,
                false,
            ));
        } else if operand.full {
            checks.push(check(Op::Eq, operand.version.clone(), operand.anchor));
        } else if operand.k == 1 || operand.k == 2 {
            checks.push(check(Op::Ge, operand.version.clone(), false));
            checks.push(check(
                Op::Lt,
                bump_for_partial(&operand, operand_at, "partial")?,
                false,
            ));
        }
    }
    Ok(Arm { checks, anchors })
}

fn compare_check(version: &Version, check: &Check) -> bool {
    let order = compare_versions(version, &check.version);
    match check.op {
        Op::Eq => order == Ordering::Equal,
        Op::Lt => order == Ordering::Less,
        Op::Le => order != Ordering::Greater,
        Op::Gt => order == Ordering::Greater,
        Op::Ge => order != Ordering::Less,
    }
}

fn arm_matches(version: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !arm.checks.iter().all(|item| compare_check(version, item)) {
        return false;
    }
    version.pre.is_empty()
        || include_prerelease
        || arm.anchors.contains(&(version.major, version.minor, version.patch))
}

/// Parses a range expression with whitespace-separated conjunctions and OR arms.
pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let split = split_arms(s)?;
    let mut arms = Vec::new();
    for arm_tokens in split {
        arms.push(parse_arm(&arm_tokens)?);
    }
    Ok(Range { arms })
}

/// Tests a version against a range using default prerelease admission.
pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

/// Tests a version against a range, optionally admitting prereleases at any tuple.
pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|arm| arm_matches(v, arm, include_prerelease))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn satisfying_extreme(
    candidates: &[String],
    range: &Range,
    want_max: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let version = parse_version(candidate).map_err(|error| CandidateError { index, error })?;
        parsed.push((version, candidate));
    }
    let mut selected: Option<(Version, &String)> = None;
    for (version, spelling) in parsed {
        if !matches(&version, range) {
            continue;
        }
        let replace = selected.as_ref().is_none_or(|(current, _)| {
            let order = compare_versions(&version, current);
            if want_max {
                order == Ordering::Greater
            } else {
                order == Ordering::Less
            }
        });
        if replace {
            selected = Some((version, spelling));
        }
    }
    Ok(selected.map(|(_, spelling)| spelling.clone()))
}

/// Returns the highest-precedence matching candidate.
pub fn max_satisfying(
    candidates: &[String],
    r: &Range,
) -> Result<Option<String>, CandidateError> {
    satisfying_extreme(candidates, r, true)
}

/// Returns the lowest-precedence matching candidate.
pub fn min_satisfying(
    candidates: &[String],
    r: &Range,
) -> Result<Option<String>, CandidateError> {
    satisfying_extreme(candidates, r, false)
}

fn core_successor(core: (u64, u64, u64)) -> Option<(u64, u64, u64)> {
    if core.2 < u64::MAX {
        Some((core.0, core.1, core.2 + 1))
    } else if core.1 < u64::MAX {
        Some((core.0, core.1 + 1, 0))
    } else if core.0 < u64::MAX {
        Some((core.0 + 1, 0, 0))
    } else {
        None
    }
}

/// Returns whether the two ranges have a common matching version.
pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for left in &a.arms {
        for right in &b.arms {
            if arms_intersect(left, right, include_prerelease) {
                return true;
            }
        }
    }
    false
}

fn arms_intersect(a: &Arm, b: &Arm, include_prerelease: bool) -> bool {
    let mut lower: Option<(Version, bool)> = None;
    let mut upper: Option<(Version, bool)> = None;
    for item in a.checks.iter().chain(b.checks.iter()) {
        match item.op {
            Op::Eq => {
                tighten_lower(&mut lower, item.version.clone(), true);
                tighten_upper(&mut upper, item.version.clone(), true);
            }
            Op::Ge => tighten_lower(&mut lower, item.version.clone(), true),
            Op::Gt => tighten_lower(&mut lower, item.version.clone(), false),
            Op::Le => tighten_upper(&mut upper, item.version.clone(), true),
            Op::Lt => tighten_upper(&mut upper, item.version.clone(), false),
        }
    }
    if let (Some((lo, lo_inclusive)), Some((hi, hi_inclusive))) = (&lower, &upper) {
        let order = compare_versions(lo, hi);
        if order == Ordering::Greater
            || (order == Ordering::Equal && !(*lo_inclusive && *hi_inclusive))
        {
            return false;
        }
    }
    if include_prerelease {
        return interval_has_witness(lower.as_ref(), upper.as_ref());
    }
    let shared_anchors: Vec<(u64, u64, u64)> = a
        .anchors
        .iter()
        .copied()
        .filter(|anchor| b.anchors.contains(anchor))
        .collect();
    if shared_anchors.iter().any(|core| {
        let probe = endpoint(*core, vec!["0".to_string()]);
        interval_contains(&probe, lower.as_ref(), upper.as_ref())
    }) {
        return true;
    }
    interval_has_witness(lower.as_ref(), upper.as_ref())
}

fn tighten_lower(bound: &mut Option<(Version, bool)>, value: Version, inclusive: bool) {
    let replace = bound.as_ref().is_none_or(|(current, current_inclusive)| {
        let order = compare_versions(&value, current);
        order == Ordering::Greater || (order == Ordering::Equal && !inclusive && *current_inclusive)
    });
    if replace {
        *bound = Some((value, inclusive));
    }
}

fn tighten_upper(bound: &mut Option<(Version, bool)>, value: Version, inclusive: bool) {
    let replace = bound.as_ref().is_none_or(|(current, current_inclusive)| {
        let order = compare_versions(&value, current);
        order == Ordering::Less || (order == Ordering::Equal && !inclusive && *current_inclusive)
    });
    if replace {
        *bound = Some((value, inclusive));
    }
}

fn interval_contains(
    version: &Version,
    lower: Option<&(Version, bool)>,
    upper: Option<&(Version, bool)>,
) -> bool {
    if let Some((bound, inclusive)) = lower {
        let order = compare_versions(version, bound);
        if order == Ordering::Less || (order == Ordering::Equal && !inclusive) {
            return false;
        }
    }
    if let Some((bound, inclusive)) = upper {
        let order = compare_versions(version, bound);
        if order == Ordering::Greater || (order == Ordering::Equal && !inclusive) {
            return false;
        }
    }
    true
}

fn interval_has_witness(
    lower: Option<&(Version, bool)>,
    upper: Option<&(Version, bool)>,
) -> bool {
    if let (Some((lo, lo_inclusive)), Some((hi, hi_inclusive))) = (lower, upper) {
        let order = compare_versions(lo, hi);
        if order == Ordering::Greater {
            return false;
        }
        if order == Ordering::Equal {
            return *lo_inclusive && *hi_inclusive;
        }
        let candidate = if !lo.pre.is_empty() {
            let mut pre = lo.pre.clone();
            pre.push("0".to_string());
            endpoint((lo.major, lo.minor, lo.patch), pre)
        } else if !lo_inclusive {
            if let Some(next) = core_successor((lo.major, lo.minor, lo.patch)) {
                endpoint(next, vec!["0".to_string()])
            } else {
                return false;
            }
        } else if lo.pre.is_empty() {
            lo.clone()
        } else {
            return false;
        };
        return interval_contains(&candidate, lower, upper)
            || (candidate.pre.is_empty()
                && interval_contains(&candidate, lower, upper));
    }
    true
}

/// Serializes each satisfiable original arm in canonical display form.
pub fn canonical(r: &Range) -> String {
    let mut rendered = Vec::new();
    for arm in &r.arms {
        let (lower, upper) = arm_bounds(arm);
        if !interval_has_witness(lower.as_ref(), upper.as_ref()) {
            continue;
        }
        let mut anchors = arm.anchors.clone();
        anchors.sort_unstable();
        anchors.dedup();
        let anchor_text = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors
                .iter()
                .map(|core| format!("{}.{}.{}", core.0, core.1, core.2))
                .collect::<Vec<_>>()
                .join(",")
        };
        rendered.push(format!(
            "[{};{};pre={}]",
            lower.as_ref().map_or("*".to_string(), |(version, inclusive)| {
                format!("{}{}", if *inclusive { ">=" } else { ">" }, display_version(version))
            }),
            upper.as_ref().map_or("*".to_string(), |(version, inclusive)| {
                format!("{}{}", if *inclusive { "<=" } else { "<" }, display_version(version))
            }),
            anchor_text
        ));
    }
    rendered.sort();
    rendered.dedup();
    if rendered.is_empty() {
        "EMPTY".to_string()
    } else {
        rendered.join(" || ")
    }
}

fn arm_bounds(arm: &Arm) -> (Option<(Version, bool)>, Option<(Version, bool)>) {
    let mut lower = None;
    let mut upper = None;
    for item in &arm.checks {
        match item.op {
            Op::Eq => {
                tighten_lower(&mut lower, item.version.clone(), true);
                tighten_upper(&mut upper, item.version.clone(), true);
            }
            Op::Ge => tighten_lower(&mut lower, item.version.clone(), true),
            Op::Gt => tighten_lower(&mut lower, item.version.clone(), false),
            Op::Le => tighten_upper(&mut upper, item.version.clone(), true),
            Op::Lt => tighten_upper(&mut upper, item.version.clone(), false),
        }
    }
    (lower, upper)
}

fn display_version(version: &Version) -> String {
    let mut result = format!("{}.{}.{}", version.major, version.minor, version.patch);
    if !version.pre.is_empty() {
        result.push('-');
        result.push_str(&version.pre.join("."));
    }
    result
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
    DuplicatePackage { name: String },
    DuplicateLabel { package: String, label: String },
    InvalidCandidate { package: String, index: usize, error: VersionError },
    InvalidRange { package: String, label: String, path: Vec<usize>, error: RangeError },
    NoSolution { package: String, labels: Vec<String> },
}

fn validate_expr(
    package: &str,
    constraint: &str,
    expression: &SetExpr,
    path: &mut Vec<usize>,
    parsed: &mut Vec<Range>,
) -> Result<(), SelectError> {
    match expression {
        SetExpr::Range(source) => {
            let range = parse_range(source).map_err(|error| SelectError::InvalidRange {
                package: package.to_string(),
                label: constraint.to_string(),
                path: path.clone(),
                error,
            })?;
            parsed.push(range);
        }
        SetExpr::Union(children) | SetExpr::Intersection(children) => {
            for (index, child) in children.iter().enumerate() {
                path.push(index);
                validate_expr(package, constraint, child, path, parsed)?;
                path.pop();
            }
        }
    }
    Ok(())
}

fn expr_matches(expression: &SetExpr, parsed: &mut usize, version: &Version, include: bool, ranges: &[Range]) -> bool {
    match expression {
        SetExpr::Range(_) => {
            let result = matches_with(version, &ranges[*parsed], include);
            *parsed += 1;
            result
        }
        SetExpr::Union(children) => children
            .iter()
            .any(|child| expr_matches(child, parsed, version, include, ranges)),
        SetExpr::Intersection(children) => children
            .iter()
            .all(|child| expr_matches(child, parsed, version, include, ranges)),
    }
}

fn expr_matches_indexed(
    expression: &SetExpr,
    version: &Version,
    include: bool,
    range_map: &mut Vec<&Range>,
) -> bool {
    match expression {
        SetExpr::Range(_) => {
            let range = range_map.remove(0);
            matches_with(version, range, include)
        }
        SetExpr::Union(children) => {
            let mut result = false;
            for child in children {
                let child_result = expr_matches_indexed(child, version, include, range_map);
                result = result || child_result;
            }
            result
        }
        SetExpr::Intersection(children) => {
            let mut result = true;
            for child in children {
                let child_result = expr_matches_indexed(child, version, include, range_map);
                result = result && child_result;
            }
            result
        }
    }
}

fn collect_range_refs<'a>(expression: &'a SetExpr, parsed: &'a [Range], index: &mut usize, output: &mut Vec<&'a Range>) {
    match expression {
        SetExpr::Range(_) => {
            output.push(&parsed[*index]);
            *index += 1;
        }
        SetExpr::Union(children) | SetExpr::Intersection(children) => {
            for child in children {
                collect_range_refs(child, parsed, index, output);
            }
        }
    }
}

/// Validates packages and selects one candidate for each package.
pub fn select_packages(
    packages: &[Package],
    preference: Preference,
    include_prerelease: bool,
) -> Result<Vec<Selection>, SelectError> {
    for (index, package) in packages.iter().enumerate() {
        if packages[..index].iter().any(|previous| previous.name == package.name) {
            return Err(SelectError::DuplicatePackage {
                name: package.name.clone(),
            });
        }
    }
    for package in packages {
        for (index, constraint) in package.constraints.iter().enumerate() {
            if package.constraints[..index]
                .iter()
                .any(|previous| previous.label == constraint.label)
            {
                return Err(SelectError::DuplicateLabel {
                    package: package.name.clone(),
                    label: constraint.label.clone(),
                });
            }
        }
    }

    let mut candidates = Vec::new();
    for package in packages {
        let mut parsed = Vec::new();
        for (index, candidate) in package.candidates.iter().enumerate() {
            let version = parse_version(candidate).map_err(|error| SelectError::InvalidCandidate {
                package: package.name.clone(),
                index,
                error,
            })?;
            parsed.push(version);
        }
        candidates.push(parsed);
    }

    let mut all_ranges = Vec::new();
    for package in packages {
        let mut package_ranges = Vec::new();
        for constraint in &package.constraints {
            let mut path = Vec::new();
            let mut parsed = Vec::new();
            validate_expr(
                &package.name,
                &constraint.label,
                &constraint.expr,
                &mut path,
                &mut parsed,
            )?;
            package_ranges.push(parsed);
        }
        all_ranges.push(package_ranges);
    }

    let mut selections = Vec::new();
    for (package_index, package) in packages.iter().enumerate() {
        let versions = &candidates[package_index];
        let mut qualifiers = vec![false; versions.len()];
        for candidate_index in 0..versions.len() {
            let mut accepted = true;
            for (constraint_index, constraint) in package.constraints.iter().enumerate() {
                let parsed_ranges = &all_ranges[package_index][constraint_index];
                let mut refs = Vec::new();
                let mut parsed_index = 0usize;
                collect_range_refs(&constraint.expr, parsed_ranges, &mut parsed_index, &mut refs);
                let result = expr_matches_indexed(
                    &constraint.expr,
                    &versions[candidate_index],
                    include_prerelease,
                    &mut refs,
                );
                accepted = accepted && result;
            }
            qualifiers[candidate_index] = accepted;
        }

        let mut best: Option<usize> = None;
        for index in 0..versions.len() {
            if !qualifiers[index] {
                continue;
            }
            let replace = best.is_none_or(|current| {
                let candidate = &versions[index];
                let incumbent = &versions[current];
                match preference {
                    Preference::Highest => compare_versions(candidate, incumbent) == Ordering::Greater,
                    Preference::Lowest => compare_versions(candidate, incumbent) == Ordering::Less,
                    Preference::StableFirst => {
                        let candidate_stable = candidate.pre.is_empty();
                        let incumbent_stable = incumbent.pre.is_empty();
                        if candidate_stable != incumbent_stable {
                            candidate_stable
                        } else {
                            compare_versions(candidate, incumbent) == Ordering::Greater
                        }
                    }
                }
            });
            if replace {
                best = Some(index);
            }
        }
        if let Some(index) = best {
            selections.push(Selection {
                package: package.name.clone(),
                version: package.candidates[index].clone(),
            });
        } else {
            let labels = minimum_conflict(
                package,
                versions,
                &all_ranges[package_index],
                include_prerelease,
            );
            return Err(SelectError::NoSolution {
                package: package.name.clone(),
                labels,
            });
        }
    }
    Ok(selections)
}

fn constraint_accepts(
    constraint: &Constraint,
    ranges: &[Range],
    version: &Version,
    include: bool,
) -> bool {
    let mut refs = Vec::new();
    let mut index = 0usize;
    collect_range_refs(&constraint.expr, ranges, &mut index, &mut refs);
    expr_matches_indexed(&constraint.expr, version, include, &mut refs)
}

fn minimum_conflict(
    package: &Package,
    versions: &[Version],
    ranges: &[Vec<Range>],
    include: bool,
) -> Vec<String> {
    if versions.is_empty() {
        return Vec::new();
    }
    let count = package.constraints.len();
    if count >= usize::BITS as usize {
        return package
            .constraints
            .iter()
            .map(|constraint| constraint.label.clone())
            .collect();
    }
    let limit = 1usize << count;
    let mut best: Option<Vec<String>> = None;
    for mask in 0..limit {
        let cardinality = mask.count_ones() as usize;
        if cardinality == 0 || best.as_ref().is_some_and(|current| cardinality > current.len()) {
            continue;
        }
        let conflict = versions.iter().all(|version| {
            package
                .constraints
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1usize << index) != 0)
                .all(|(index, constraint)| {
                    constraint_accepts(constraint, &ranges[index], version, include)
                })
                == false
        });
        if conflict {
            let mut labels: Vec<String> = package
                .constraints
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1usize << index) != 0)
                .map(|(_, constraint)| constraint.label.clone())
                .collect();
            labels.sort();
            let replace = best.as_ref().is_none_or(|current| {
                labels.len() < current.len() || (labels.len() == current.len() && labels < *current)
            });
            if replace {
                best = Some(labels);
            }
        }
    }
    best.unwrap_or_default()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn pv(source: &str) -> Version {
        parse_version(source).unwrap()
    }

    fn pr(source: &str) -> Range {
        parse_range(source).unwrap()
    }

    #[test]
    fn versions_and_precedence() {
        let version = pv("2.1.3-rc.1+build.7");
        assert_eq!((version.major, version.minor, version.patch), (2, 1, 3));
        assert_eq!(version.pre, vec!["rc", "1"]);
        assert_eq!(version.build, vec!["build", "7"]);
        assert_eq!(
            compare_versions(&pv("1.0.0-alpha.2"), &pv("1.0.0-alpha.10")),
            Ordering::Less
        );
    }

    #[test]
    fn invalid_versions_and_partial_ranges() {
        assert_eq!(parse_version("01.0.0"), Err(VersionError::LeadingZero { at: 0 }));
        assert_eq!(parse_version("1.0"), Err(VersionError::MissingCore { at: 3 }));
        assert!(matches(&pv("2.5.1"), &pr("2.5.x")));
        assert!(!matches(&pv("2.6.0"), &pr("2.5.x")));
    }

    #[test]
    fn ranges_extrema_and_packages() {
        let range = pr(">=2.1.0 <3.0.0");
        assert!(matches(&pv("2.5.0"), &range));
        assert!(!matches(&pv("3.0.0"), &range));
        assert!(intersects(&range, &pr("2.5.x"), false));
        let candidates = vec!["2.2.0".to_string(), "2.8.0".to_string()];
        assert_eq!(max_satisfying(&candidates, &pr("2.x")), Ok(Some("2.8.0".into())));
        assert_eq!(canonical(&range), "[>=2.1.0;<3.0.0;pre=-]");

        let package = Package {
            name: "demo".into(),
            candidates: vec!["2.1.0".into(), "2.7.0".into()],
            constraints: vec![Constraint {
                label: "range".into(),
                expr: SetExpr::Range("2.x".into()),
            }],
        };
        assert_eq!(
            select_packages(&[package], Preference::Highest, false).unwrap()[0].version,
            "2.7.0"
        );
    }
}