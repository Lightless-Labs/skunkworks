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

fn is_numeric(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn parse_core_number(s: &str, start: usize, end: usize) -> Result<u64, VersionError> {
    if end - start > 1 && s.as_bytes()[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    s[start..end]
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
    let mut pos = start;

    loop {
        let mut stop = pos;
        while stop < end && bytes[stop] != b'.' {
            stop += 1;
        }

        if stop == pos {
            return Err(VersionError::EmptyIdentifier { at: pos });
        }

        for (i, byte) in bytes[pos..stop].iter().enumerate() {
            if !(byte.is_ascii_alphanumeric() || *byte == b'-') {
                return Err(VersionError::InvalidChar { at: pos + i });
            }
        }

        let ident = &s[pos..stop];
        if prerelease && is_numeric(ident) && ident.len() > 1 && ident.starts_with('0') {
            return Err(VersionError::LeadingZero { at: pos });
        }
        result.push(ident.to_owned());

        if stop == end {
            break;
        }
        pos = stop + 1;
    }

    Ok(result)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let bytes = s.as_bytes();
    let mut pos = 0;
    let mut core = [0u64; 3];

    for field in 0..3 {
        let start = pos;
        while pos < bytes.len() && is_digit(bytes[pos]) {
            pos += 1;
        }

        if pos == start {
            if pos == bytes.len() || matches!(bytes[pos], b'.' | b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            return Err(VersionError::InvalidChar { at: pos });
        }

        core[field] = parse_core_number(s, start, pos)?;

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
        let pre_start = pos + 1;
        let build_start = bytes[pre_start..]
            .iter()
            .position(|b| *b == b'+')
            .map(|offset| pre_start + offset);
        let pre_end = build_start.unwrap_or(bytes.len());
        pre = parse_identifiers(s, pre_start, pre_end, true)?;
        pos = pre_end;
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

fn cmp_numeric_ident(a: &str, b: &str) -> Ordering {
    a.len()
        .cmp(&b.len())
        .then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch)
        .cmp(&(b.major, b.minor, b.patch))
        .then_with(|| match (a.pre.is_empty(), b.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => {
                for (x, y) in a.pre.iter().zip(&b.pre) {
                    let x_numeric = is_numeric(x);
                    let y_numeric = is_numeric(y);
                    let order = match (x_numeric, y_numeric) {
                        (true, true) => cmp_numeric_ident(x, y),
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
        })
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
enum Bound {
    Unbounded,
    Finite(Version, bool),
}

#[derive(Clone, Debug)]
enum CmpOp {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
}

#[derive(Clone, Debug)]
struct Comparator {
    op: CmpOp,
    version: Version,
}

#[derive(Clone, Debug)]
struct Arm {
    comparators: Vec<Comparator>,
    anchors: Vec<(u64, u64, u64)>,
}

fn stable_version(major: u64, minor: u64, patch: u64) -> Version {
    Version {
        major,
        minor,
        patch,
        pre: Vec::new(),
        build: Vec::new(),
    }
}

fn prerelease_zero(major: u64, minor: u64, patch: u64) -> Version {
    Version {
        major,
        minor,
        patch,
        pre: vec!["0".to_owned()],
        build: Vec::new(),
    }
}

fn tuple(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn add_cmp(arm: &mut Arm, op: CmpOp, version: Version, is_anchor: bool) {
    if is_anchor && !version.pre.is_empty() {
        arm.anchors.push(tuple(&version));
    }
    arm.comparators.push(Comparator { op, version });
}

fn token_ranges(s: &str) -> Vec<(usize, usize)> {
    let bytes = s.as_bytes();
    let mut ranges = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        ranges.push((start, i));
    }

    ranges
}

fn parse_partial_or_full(
    token: &str,
    global_start: usize,
) -> Result<(Option<Version>, Vec<Option<u64>>, usize), RangeError> {
    let has_suffix = token.contains('-') || token.contains('+');
    let fields: Vec<&str> = token.split('.').collect();
    let has_wildcard = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));

    if has_suffix || (fields.len() >= 3 && !has_wildcard) {
        let version = parse_version(token).map_err(|error| RangeError::Version {
            at: global_start,
            error,
        })?;
        return Ok((Some(version), vec![Some(0), Some(0), Some(0)], 3));
    }

    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at: global_start });
    }

    let mut values = Vec::new();
    let mut saw_wildcard = false;
    let mut numeric_count = 0;

    for field in fields {
        if field.is_empty() {
            return Err(RangeError::Syntax { at: global_start });
        }
        if matches!(field, "*" | "x" | "X") {
            saw_wildcard = true;
            values.push(None);
        } else {
            if saw_wildcard
                || !field.bytes().all(is_digit)
                || (field.len() > 1 && field.starts_with('0'))
            {
                return Err(RangeError::Syntax { at: global_start });
            }
            let value = field
                .parse::<u64>()
                .map_err(|_| RangeError::Syntax { at: global_start })?;
            values.push(Some(value));
            numeric_count += 1;
        }
    }

    Ok((None, values, numeric_count))
}

fn padded(values: &[Option<u64>]) -> (u64, u64, u64) {
    let mut result = [0u64; 3];
    for (i, value) in values.iter().enumerate() {
        if let Some(value) = value {
            result[i] = *value;
        }
    }
    (result[0], result[1], result[2])
}

fn bump_tuple(
    value: (u64, u64, u64),
    field: usize,
    at: usize,
) -> Result<(u64, u64, u64), RangeError> {
    let (mut major, mut minor, mut patch) = value;
    match field {
        0 => {
            major = major.checked_add(1).ok_or(RangeError::Syntax { at })?;
            minor = 0;
            patch = 0;
        }
        1 => {
            minor = minor.checked_add(1).ok_or(RangeError::Syntax { at })?;
            patch = 0;
        }
        _ => patch = patch.checked_add(1).ok_or(RangeError::Syntax { at })?,
    }
    Ok((major, minor, patch))
}

fn operand_expansion(
    arm: &mut Arm,
    token: &str,
    start: usize,
    kind: char,
) -> Result<(), RangeError> {
    let operand_start = start + usize::from(kind != '\0');
    let operand = if kind == '\0' { token } else { &token[1..] };

    if operand.is_empty() {
        return Err(RangeError::Syntax { at: start });
    }

    let (full, values, count) = parse_partial_or_full(operand, operand_start)?;

    if kind == '\0' {
        if let Some(version) = full {
            add_cmp(arm, CmpOp::Eq, version, true);
        } else if count != 0 {
            let lower = padded(&values);
            match count {
                1 | 2 => {
                    add_cmp(
                        arm,
                        CmpOp::Ge,
                        stable_version(lower.0, lower.1, lower.2),
                        false,
                    );
                    let upper = bump_tuple(lower, count - 1, operand_start)?;
                    add_cmp(
                        arm,
                        CmpOp::Lt,
                        prerelease_zero(upper.0, upper.1, upper.2),
                        false,
                    );
                }
                _ => add_cmp(
                    arm,
                    CmpOp::Eq,
                    stable_version(lower.0, lower.1, lower.2),
                    false,
                ),
            }
        }
        return Ok(());
    }

    if count == 0 {
        return Err(RangeError::Syntax { at: start });
    }

    let lower = padded(&values);
    let lower_version = full
        .clone()
        .unwrap_or_else(|| stable_version(lower.0, lower.1, lower.2));
    add_cmp(arm, CmpOp::Ge, lower_version, true);

    let upper_field = match kind {
        '^' if lower.0 > 0 || count == 1 => 0,
        '^' if lower.1 > 0 || count == 2 => 1,
        '^' => 2,
        '~' if count == 1 => 0,
        '~' => 1,
        _ => unreachable!(),
    };

    let upper = bump_tuple(lower, upper_field, operand_start)?;
    add_cmp(
        arm,
        CmpOp::Lt,
        prerelease_zero(upper.0, upper.1, upper.2),
        false,
    );
    Ok(())
}

fn parse_explicit_comparator(
    arm: &mut Arm,
    token: &str,
    start: usize,
) -> Result<(), RangeError> {
    let (op, operand_len) = if token.starts_with(">=") {
        (CmpOp::Ge, 2)
    } else if token.starts_with("<=") {
        (CmpOp::Le, 2)
    } else if token.starts_with('>') {
        (CmpOp::Gt, 1)
    } else if token.starts_with('<') {
        (CmpOp::Lt, 1)
    } else {
        (CmpOp::Eq, 1)
    };

    let operand_start = start + operand_len;
    if operand_start >= start + token.len()
        || !token.as_bytes()[operand_len].is_ascii_digit()
    {
        return Err(RangeError::Syntax { at: start });
    }

    let version = parse_version(&token[operand_len..]).map_err(|error| RangeError::Version {
        at: operand_start,
        error,
    })?;
    add_cmp(arm, op, version, true);
    Ok(())
}

fn empty_arm() -> Arm {
    Arm {
        comparators: Vec::new(),
        anchors: Vec::new(),
    }
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let tokens = token_ranges(s);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms = Vec::new();
    let mut arm_tokens = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        let (start, end) = tokens[i];
        if &s[start..end] == "||" {
            if arm_tokens.is_empty() {
                return Err(RangeError::Empty { at: start });
            }
            arms.push(parse_arm(s, &arm_tokens)?);
            arm_tokens.clear();
            i += 1;
            if i == tokens.len() {
                return Err(RangeError::Empty { at: s.len() });
            }
        } else {
            arm_tokens.push((start, end));
            i += 1;
        }
    }

    if arm_tokens.is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(s, &arm_tokens)?);
    Ok(Range { arms })
}

fn parse_arm(s: &str, tokens: &[(usize, usize)]) -> Result<Arm, RangeError> {
    let mut arm = empty_arm();
    let hyphens: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, (start, end))| {
            (&s[*start..*end] == "-").then_some(i)
        })
        .collect();

    if !hyphens.is_empty() {
        if tokens.len() != 3 || hyphens != vec![1] {
            return Err(RangeError::Syntax {
                at: tokens[hyphens[0]].0,
            });
        }

        let (lower_start, lower_end) = tokens[0];
        let (upper_start, upper_end) = tokens[2];
        let (lower_full, lower_values, lower_count) =
            parse_partial_or_full(&s[lower_start..lower_end], lower_start)?;
        let (upper_full, upper_values, upper_count) =
            parse_partial_or_full(&s[upper_start..upper_end], upper_start)?;

        if lower_count == 0 {
            return Err(RangeError::Syntax { at: lower_start });
        }
        if upper_count == 0 {
            return Err(RangeError::Syntax { at: upper_start });
        }

        let lower_tuple = padded(&lower_values);
        let lower = lower_full
            .unwrap_or_else(|| stable_version(lower_tuple.0, lower_tuple.1, lower_tuple.2));
        add_cmp(&mut arm, CmpOp::Ge, lower, true);

        if let Some(upper) = upper_full {
            add_cmp(&mut arm, CmpOp::Le, upper, true);
        } else {
            let upper_tuple = padded(&upper_values);
            if upper_count == 3 {
                add_cmp(
                    &mut arm,
                    CmpOp::Le,
                    stable_version(upper_tuple.0, upper_tuple.1, upper_tuple.2),
                    false,
                );
            } else {
                let next = bump_tuple(upper_tuple, upper_count - 1, upper_start)?;
                add_cmp(
                    &mut arm,
                    CmpOp::Lt,
                    prerelease_zero(next.0, next.1, next.2),
                    false,
                );
            }
        }
        return Ok(arm);
    }

    for &(start, end) in tokens {
        let token = &s[start..end];
        match token.as_bytes()[0] {
            b'^' => operand_expansion(&mut arm, token, start, '^')?,
            b'~' => operand_expansion(&mut arm, token, start, '~')?,
            b'<' | b'>' | b'=' => parse_explicit_comparator(&mut arm, token, start)?,
            b'0'..=b'9' | b'*' | b'x' | b'X' => {
                operand_expansion(&mut arm, token, start, '\0')?
            }
            _ => return Err(RangeError::Syntax { at: start }),
        }
    }

    Ok(arm)
}

fn within_comparator(version: &Version, comparator: &Comparator) -> bool {
    let order = compare_versions(version, &comparator.version);
    match comparator.op {
        CmpOp::Lt => order == Ordering::Less,
        CmpOp::Le => order != Ordering::Greater,
        CmpOp::Eq => order == Ordering::Equal,
        CmpOp::Ge => order != Ordering::Less,
        CmpOp::Gt => order == Ordering::Greater,
    }
}

fn arm_matches(version: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    arm.comparators
        .iter()
        .all(|comparator| within_comparator(version, comparator))
        && (include_prerelease
            || version.pre.is_empty()
            || arm.anchors.iter().any(|anchor| *anchor == tuple(version)))
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
    greatest: bool,
) -> Result<Option<String>, CandidateError> {
    let parsed: Vec<Version> = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            parse_version(candidate).map_err(|error| CandidateError { index, error })
        })
        .collect::<Result<_, _>>()?;

    let mut best: Option<usize> = None;
    for (index, version) in parsed.iter().enumerate() {
        if !matches(version, range) {
            continue;
        }
        if let Some(previous) = best {
            let order = compare_versions(version, &parsed[previous]);
            if (greatest && order == Ordering::Greater)
                || (!greatest && order == Ordering::Less)
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
    NoSolution { package: String, labels: Vec<String> },
}

#[derive(Clone, Debug)]
enum ParsedExpr {
    Range(Range),
    Union(Vec<ParsedExpr>),
    Intersection(Vec<ParsedExpr>),
}

fn validate_expr(
    expr: &SetExpr,
    package: &str,
    label: &str,
    path: &mut Vec<usize>,
) -> Result<ParsedExpr, SelectError> {
    match expr {
        SetExpr::Range(source) => {
            let range = parse_range(source).map_err(|error| SelectError::InvalidRange {
                package: package.to_owned(),
                label: label.to_owned(),
                path: path.clone(),
                error,
            })?;
            Ok(ParsedExpr::Range(range))
        }
        SetExpr::Union(children) => {
            let mut parsed = Vec::new();
            for (index, child) in children.iter().enumerate() {
                path.push(index);
                let result = validate_expr(child, package, label, path);
                path.pop();
                parsed.push(result?);
            }
            Ok(ParsedExpr::Union(parsed))
        }
        SetExpr::Intersection(children) => {
            let mut parsed = Vec::new();
            for (index, child) in children.iter().enumerate() {
                path.push(index);
                let result = validate_expr(child, package, label, path);
                path.pop();
                parsed.push(result?);
            }
            Ok(ParsedExpr::Intersection(parsed))
        }
    }
}

fn expr_matches(version: &Version, expr: &ParsedExpr, include_prerelease: bool) -> bool {
    match expr {
        ParsedExpr::Range(range) => matches_with(version, range, include_prerelease),
        ParsedExpr::Union(children) => children
            .iter()
            .any(|child| expr_matches(version, child, include_prerelease)),
        ParsedExpr::Intersection(children) => children
            .iter()
            .all(|child| expr_matches(version, child, include_prerelease)),
    }
}

fn subset_conflict(
    candidates: &[Version],
    constraints: &[(String, ParsedExpr)],
    chosen: &mut Vec<usize>,
    next: usize,
    include_prerelease: bool,
    best: &mut Option<Vec<usize>>,
) {
    if next == constraints.len() {
        let satisfiable = candidates.iter().any(|version| {
            chosen.iter().all(|index| {
                expr_matches(
                    version,
                    &constraints[*index].1,
                    include_prerelease,
                )
            })
        });
        if satisfiable {
            return;
        }

        let mut current = chosen.clone();
        current.sort_by(|a, b| {
            constraints[*a]
                .0
                .as_bytes()
                .cmp(constraints[*b].0.as_bytes())
        });
        let replace = match best {
            None => true,
            Some(old) if current.len() < old.len() => true,
            Some(old) if current.len() == old.len() => {
                let current_labels: Vec<&[u8]> = current
                    .iter()
                    .map(|index| constraints[*index].0.as_bytes())
                    .collect();
                let old_labels: Vec<&[u8]> = old
                    .iter()
                    .map(|index| constraints[*index].0.as_bytes())
                    .collect();
                current_labels < old_labels
            }
            _ => false,
        };
        if replace {
            *best = Some(current);
        }
        return;
    }

    if best
        .as_ref()
        .is_none_or(|current| chosen.len() < current.len())
    {
        subset_conflict(
            candidates,
            constraints,
            chosen,
            next + 1,
            include_prerelease,
            best,
        );
        chosen.push(next);
        subset_conflict(
            candidates,
            constraints,
            chosen,
            next + 1,
            include_prerelease,
            best,
        );
        chosen.pop();
    }
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
        let mut versions = Vec::new();
        for (index, candidate) in package.candidates.iter().enumerate() {
            versions.push(parse_version(candidate).map_err(|error| {
                SelectError::InvalidCandidate {
                    package: package.name.clone(),
                    index,
                    error,
                }
            })?);
        }
        parsed_candidates.push(versions);
    }

    let mut parsed_constraints = Vec::new();
    for package in packages {
        let mut constraints = Vec::new();
        for constraint in &package.constraints {
            constraints.push((
                constraint.label.clone(),
                validate_expr(
                    &constraint.expr,
                    &package.name,
                    &constraint.label,
                    &mut Vec::new(),
                )?,
            ));
        }
        parsed_constraints.push(constraints);
    }

    let mut selections = Vec::new();
    for (package_index, package) in packages.iter().enumerate() {
        let versions = &parsed_candidates[package_index];
        let constraints = &parsed_constraints[package_index];
        let qualifies = |index: usize| {
            constraints
                .iter()
                .all(|(_, expr)| expr_matches(&versions[index], expr, include_prerelease))
        };

        let mut best: Option<usize> = None;
        for index in 0..versions.len() {
            if !qualifies(index) {
                continue;
            }

            let replace = match best {
                None => true,
                Some(previous) => match preference {
                    Preference::Highest => {
                        compare_versions(&versions[index], &versions[previous]) == Ordering::Greater
                    }
                    Preference::Lowest => {
                        compare_versions(&versions[index], &versions[previous]) == Ordering::Less
                    }
                    Preference::StableFirst => {
                        let stable = versions[index].pre.is_empty();
                        let previous_stable = versions[previous].pre.is_empty();
                        (stable && !previous_stable)
                            || (stable == previous_stable
                                && compare_versions(&versions[index], &versions[previous])
                                    == Ordering::Greater)
                    }
                },
            };
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
            let labels = if versions.is_empty() {
                Vec::new()
            } else {
                let mut best_conflict = None;
                subset_conflict(
                    versions,
                    constraints,
                    &mut Vec::new(),
                    0,
                    include_prerelease,
                    &mut best_conflict,
                );
                best_conflict
                    .unwrap_or_default()
                    .into_iter()
                    .map(|index| constraints[index].0.clone())
                    .collect()
            };

            return Err(SelectError::NoSolution {
                package: package.name.clone(),
                labels,
            });
        }
    }

    Ok(selections)
}

fn cmp_lower(a: &Bound, b: &Bound) -> Ordering {
    match (a, b) {
        (Bound::Unbounded, Bound::Unbounded) => Ordering::Equal,
        (Bound::Unbounded, _) => Ordering::Less,
        (_, Bound::Unbounded) => Ordering::Greater,
        (Bound::Finite(av, ai), Bound::Finite(bv, bi)) => {
            compare_versions(av, bv).then_with(|| bi.cmp(ai))
        }
    }
}

fn cmp_upper(a: &Bound, b: &Bound) -> Ordering {
    match (a, b) {
        (Bound::Unbounded, Bound::Unbounded) => Ordering::Equal,
        (Bound::Unbounded, _) => Ordering::Greater,
        (_, Bound::Unbounded) => Ordering::Less,
        (Bound::Finite(av, ai), Bound::Finite(bv, bi)) => {
            compare_versions(av, bv).then_with(|| ai.cmp(bi))
        }
    }
}

fn arm_bounds(arm: &Arm) -> (Bound, Bound) {
    let mut lower = Bound::Unbounded;
    let mut upper = Bound::Unbounded;

    for comparator in &arm.comparators {
        match comparator.op {
            CmpOp::Eq => {
                let lo = Bound::Finite(comparator.version.clone(), true);
                let hi = Bound::Finite(comparator.version.clone(), true);
                if cmp_lower(&lo, &lower) == Ordering::Greater {
                    lower = lo;
                }
                if cmp_upper(&hi, &upper) == Ordering::Less {
                    upper = hi;
                }
            }
            CmpOp::Ge | CmpOp::Gt => {
                let bound = Bound::Finite(
                    comparator.version.clone(),
                    matches!(comparator.op, CmpOp::Ge),
                );
                if cmp_lower(&bound, &lower) == Ordering::Greater {
                    lower = bound;
                }
            }
            CmpOp::Le | CmpOp::Lt => {
                let bound = Bound::Finite(
                    comparator.version.clone(),
                    matches!(comparator.op, CmpOp::Le),
                );
                if cmp_upper(&bound, &upper) == Ordering::Less {
                    upper = bound;
                }
            }
        }
    }

    (lower, upper)
}

fn core_successor(version: &Version) -> Option<Version> {
    if version.patch < u64::MAX {
        Some(prerelease_zero(
            version.major,
            version.minor,
            version.patch + 1,
        ))
    } else if version.minor < u64::MAX {
        Some(prerelease_zero(version.major, version.minor + 1, 0))
    } else if version.major < u64::MAX {
        Some(prerelease_zero(version.major + 1, 0, 0))
    } else {
        None
    }
}

fn least_after(version: &Version) -> Option<Version> {
    if !version.pre.is_empty() {
        let mut next = version.clone();
        next.build.clear();
        next.pre.push("0".to_owned());
        Some(next)
    } else {
        core_successor(version)
    }
}

fn version_from_core_text(version: &Version) -> String {
    let mut text = format!(
        "{}.{}.{}",
        version.major, version.minor, version.patch
    );
    if !version.pre.is_empty() {
        text.push('-');
        text.push_str(&version.pre.join("."));
    }
    text
}

fn bound_text(bound: &Bound, lower: bool) -> String {
    match bound {
        Bound::Unbounded => "*".to_owned(),
        Bound::Finite(version, inclusive) => {
            let operator = match (lower, inclusive) {
                (true, true) => ">=",
                (true, false) => ">",
                (false, true) => "<=",
                (false, false) => "<",
            };
            format!("{operator}{}", version_from_core_text(version))
        }
    }
}

fn arm_has_default_witness(arm: &Arm) -> bool {
    let (lower, upper) = arm_bounds(arm);
    let mut candidate = match lower {
        Bound::Unbounded => stable_version(0, 0, 0),
        Bound::Finite(version, inclusive) => {
            if inclusive {
                version
            } else if let Some(next) = least_after(&version) {
                next
            } else {
                return false;
            }
        }
    };

    loop {
        if let Bound::Finite(ref upper_version, inclusive) = upper {
            let order = compare_versions(&candidate, upper_version);
            if order == Ordering::Greater || (order == Ordering::Equal && !inclusive) {
                return false;
            }
        }

        if candidate.pre.is_empty() {
            return true;
        }
        if arm.anchors.iter().any(|anchor| *anchor == tuple(&candidate)) {
            return true;
        }
        let Some(next) = least_after(&candidate) else {
            return false;
        };
        candidate = next;
    }
}

pub fn canonical(range: &Range) -> String {
    let mut items = Vec::new();

    for arm in &range.arms {
        if !arm_has_default_witness(arm) {
            continue;
        }

        let (lower, upper) = arm_bounds(arm);
        let mut anchors = arm.anchors.clone();
        anchors.sort();
        anchors.dedup();

        let anchor_text = if anchors.is_empty() {
            "-".to_owned()
        } else {
            anchors
                .iter()
                .map(|(major, minor, patch)| format!("{major}.{minor}.{patch}"))
                .collect::<Vec<_>>()
                .join(",")
        };

        items.push(format!(
            "[{};{};pre={}]",
            bound_text(&lower, true),
            bound_text(&upper, false),
            anchor_text
        ));
    }

    items.sort();
    items.dedup();

    if items.is_empty() {
        "EMPTY".to_owned()
    } else {
        items.join(" || ")
    }
}

fn bounds_overlap(a: &Arm, b: &Arm) -> bool {
    let (a_lower, a_upper) = arm_bounds(a);
    let (b_lower, b_upper) = arm_bounds(b);

    let lower = if cmp_lower(&a_lower, &b_lower) == Ordering::Greater {
        a_lower
    } else {
        b_lower
    };
    let upper = if cmp_upper(&a_upper, &b_upper) == Ordering::Less {
        a_upper
    } else {
        b_upper
    };

    match (&lower, &upper) {
        (Bound::Finite(lower_version, lower_inclusive), Bound::Finite(upper_version, upper_inclusive)) => {
            match compare_versions(lower_version, upper_version) {
                Ordering::Less => true,
                Ordering::Greater => false,
                Ordering::Equal => *lower_inclusive && *upper_inclusive,
            }
        }
        _ => true,
    }
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for a_arm in &a.arms {
        for b_arm in &b.arms {
            if !bounds_overlap(a_arm, b_arm) {
                continue;
            }
            if include_prerelease {
                return true;
            }

            let (a_lower, a_upper) = arm_bounds(a_arm);
            let (b_lower, b_upper) = arm_bounds(b_arm);
            let lower = if cmp_lower(&a_lower, &b_lower) == Ordering::Greater {
                a_lower
            } else {
                b_lower
            };
            let upper = if cmp_upper(&a_upper, &b_upper) == Ordering::Less {
                a_upper
            } else {
                b_upper
            };

            let mut candidate = match lower {
                Bound::Unbounded => stable_version(0, 0, 0),
                Bound::Finite(version, inclusive) => {
                    if inclusive {
                        version
                    } else if let Some(next) = least_after(&version) {
                        next
                    } else {
                        continue;
                    }
                }
            };

            loop {
                if let Bound::Finite(ref upper_version, inclusive) = upper {
                    let order = compare_versions(&candidate, upper_version);
                    if order == Ordering::Greater || (order == Ordering::Equal && !inclusive) {
                        break;
                    }
                }
                if candidate.pre.is_empty() {
                    return true;
                }
                if a_arm.anchors.iter().any(|anchor| *anchor == tuple(&candidate))
                    && b_arm.anchors.iter().any(|anchor| *anchor == tuple(&candidate))
                {
                    return true;
                }
                let Some(next) = least_after(&candidate) else {
                    break;
                };
                candidate = next;
            }
        }
    }
    false
}

fn main() {}