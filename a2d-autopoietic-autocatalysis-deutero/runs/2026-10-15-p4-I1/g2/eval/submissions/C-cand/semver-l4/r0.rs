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

fn parse_core(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && is_digit(bytes[*pos]) {
        *pos += 1;
    }
    if *pos == start {
        if start == bytes.len() || matches!(bytes[start], b'.' | b'-' | b'+') {
            return Err(VersionError::MissingCore { at: start });
        }
        return Err(VersionError::InvalidChar { at: start });
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
    base: usize,
    prerelease: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    loop {
        let end = s[start..]
            .find('.')
            .map(|n| start + n)
            .unwrap_or(s.len());
        if end == start {
            return Err(VersionError::EmptyIdentifier { at: base + start });
        }
        let part = &s[start..end];
        for (i, b) in part.bytes().enumerate() {
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar {
                    at: base + start + i,
                });
            }
        }
        if prerelease
            && part.len() > 1
            && part.starts_with('0')
            && part.bytes().all(is_digit)
        {
            return Err(VersionError::LeadingZero { at: base + start });
        }
        out.push(part.to_string());
        if end == s.len() {
            break;
        }
        start = end + 1;
        if start == s.len() {
            return Err(VersionError::EmptyIdentifier { at: base + start });
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

    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        if pos == s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;
    let minor = parse_core(s, &mut pos)?;

    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        if pos == s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;
    let patch = parse_core(s, &mut pos)?;

    parse_tail(s, pos, major, minor, patch)
}

fn parse_tail(
    s: &str,
    mut pos: usize,
    major: u64,
    minor: u64,
    patch: u64,
) -> Result<Version, VersionError> {
    let bytes = s.as_bytes();
    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] == b'-' {
        pos += 1;
        let end = s[pos..].find('+').map(|n| pos + n).unwrap_or(s.len());
        pre = parse_identifiers(&s[pos..end], pos, true)?;
        pos = end;
        if pos < s.len() && bytes[pos] == b'+' {
            pos += 1;
            build = parse_identifiers(&s[pos..], pos, false)?;
        }
    } else if pos < bytes.len() && bytes[pos] == b'+' {
        pos += 1;
        build = parse_identifiers(&s[pos..], pos, false)?;
    } else if pos < bytes.len() {
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

fn numeric_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    for (x, y) in a.iter().zip(b) {
        let xn = numeric_id(x);
        let yn = numeric_id(y);
        let ord = match (xn, yn) {
            (true, true) => x.len().cmp(&y.len()).then_with(|| x.as_bytes().cmp(y.as_bytes())),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => x.as_bytes().cmp(y.as_bytes()),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    a.len().cmp(&b.len())
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch)
        .cmp(&(b.major, b.minor, b.patch))
        .then_with(|| cmp_pre(&a.pre, &b.pre))
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
}

#[derive(Clone, Debug)]
struct Arm {
    comps: Vec<Comparator>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

fn whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn split_tokens(s: &str, start: usize, end: usize) -> Vec<(usize, &str)> {
    let mut v = Vec::new();
    let mut i = start;
    while i < end {
        while i < end && whitespace(s.as_bytes()[i]) {
            i += 1;
        }
        if i == end {
            break;
        }
        let st = i;
        while i < end && !whitespace(s.as_bytes()[i]) {
            i += 1;
        }
        v.push((st, &s[st..i]));
    }
    v
}

fn full_or_partial(
    token: &str,
    at: usize,
) -> Result<(Version, usize, bool), RangeError> {
    let fields: Vec<&str> = token.split('.').collect();
    let has_suffix = token.contains('-') || token.contains('+');
    let has_wild = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));

    if has_suffix || (fields.len() >= 3 && !has_wild) {
        return parse_version(token)
            .map(|v| (v, 3, true))
            .map_err(|error| RangeError::Version { at, error });
    }
    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at });
    }

    let mut nums = Vec::new();
    let mut wildcard = false;
    for f in &fields {
        if matches!(*f, "*" | "x" | "X") {
            wildcard = true;
        } else {
            if wildcard
                || f.is_empty()
                || !f.bytes().all(is_digit)
                || (f.len() > 1 && f.starts_with('0'))
            {
                return Err(RangeError::Syntax { at });
            }
            let n = f.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
            nums.push(n);
        }
    }

    Ok((
        Version {
            major: *nums.first().unwrap_or(&0),
            minor: *nums.get(1).unwrap_or(&0),
            patch: *nums.get(2).unwrap_or(&0),
            pre: vec![],
            build: vec![],
        },
        nums.len(),
        false,
    ))
}

fn bump(v: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let mut n = v.clone();
    let target = match field {
        0 => &mut n.major,
        1 => &mut n.minor,
        _ => &mut n.patch,
    };
    *target = target
        .checked_add(1)
        .ok_or(RangeError::Syntax { at })?;
    match field {
        0 => {
            n.minor = 0;
            n.patch = 0;
        }
        1 => n.patch = 0,
        _ => {}
    }
    n.pre = vec!["0".into()];
    n.build.clear();
    Ok(n)
}

fn add(arm: &mut Arm, op: Op, v: Version, anchor: bool) {
    if anchor && !v.pre.is_empty() {
        arm.anchors.push((v.major, v.minor, v.patch));
    }
    arm.comps.push(Comparator { op, version: v });
}

fn parse_arm(tokens: &[(usize, &str)]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        comps: vec![],
        anchors: vec![],
    };

    if let Some((idx, _)) = tokens.iter().find(|(_, t)| *t == "-") {
        if tokens.len() != 3 || tokens[1].1 != "-" {
            return Err(RangeError::Syntax { at: *idx });
        }
        let (lv, lk, _) = full_or_partial(tokens[0].1, tokens[0].0)?;
        let (uv, uk, uf) = full_or_partial(tokens[2].1, tokens[2].0)?;
        if lk == 0 {
            return Err(RangeError::Syntax { at: tokens[0].0 });
        }
        if uk == 0 {
            return Err(RangeError::Syntax { at: tokens[2].0 });
        }
        add(&mut arm, Op::Ge, lv, true);
        if uf {
            add(&mut arm, Op::Le, uv, true);
        } else {
            let upper = if uk == 1 {
                bump(&uv, 0, tokens[2].0)?
            } else {
                bump(&uv, 1, tokens[2].0)?
            };
            add(&mut arm, Op::Lt, upper, false);
        }
        return Ok(arm);
    }

    for (at, token) in tokens {
        if *token == "||" {
            return Err(RangeError::Syntax { at: *at });
        }
        if let Some(rest) = token.strip_prefix('^').or_else(|| token.strip_prefix('~')) {
            let caret = token.starts_with('^');
            if rest.is_empty() {
                return Err(RangeError::Syntax { at: *at });
            }
            let (v, k, _) = full_or_partial(rest, at + 1)?;
            if k == 0 {
                return Err(RangeError::Syntax { at: *at });
            }
            add(&mut arm, Op::Ge, v.clone(), true);
            let field = if caret {
                if v.major > 0 || k == 1 {
                    0
                } else if v.minor > 0 || k == 2 {
                    1
                } else {
                    2
                }
            } else if k == 1 {
                0
            } else {
                1
            };
            let upper = bump(&v, field, *at)?;
            add(&mut arm, Op::Lt, upper, false);
        } else {
            let (op, rest, op_len) = if let Some(x) = token.strip_prefix(">=") {
                (Op::Ge, x, 2)
            } else if let Some(x) = token.strip_prefix("<=") {
                (Op::Le, x, 2)
            } else if let Some(x) = token.strip_prefix('>') {
                (Op::Gt, x, 1)
            } else if let Some(x) = token.strip_prefix('<') {
                (Op::Lt, x, 1)
            } else if let Some(x) = token.strip_prefix('=') {
                (Op::Eq, x, 1)
            } else {
                (Op::Eq, *token, 0)
            };

            if op_len > 0 {
                if rest.is_empty() || !rest.as_bytes()[0].is_ascii_digit() {
                    return Err(RangeError::Syntax { at: *at });
                }
                let v = parse_version(rest).map_err(|error| RangeError::Version {
                    at: *at + op_len,
                    error,
                })?;
                add(&mut arm, op, v, true);
            } else {
                let first = token.as_bytes()[0];
                if !(first.is_ascii_digit() || matches!(first, b'*' | b'x' | b'X')) {
                    return Err(RangeError::Syntax { at: *at });
                }
                let (v, k, full) = full_or_partial(token, *at)?;
                if full {
                    add(&mut arm, Op::Eq, v, true);
                } else if k > 0 {
                    add(&mut arm, Op::Ge, v.clone(), true);
                    if k < 3 {
                        let upper = bump(&v, if k == 1 { 0 } else { 1 }, *at)?;
                        add(&mut arm, Op::Lt, upper, false);
                    }
                }
            }
        }
    }

    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() && whitespace(bytes[cursor]) {
        cursor += 1;
    }
    if cursor == bytes.len() {
        return Err(RangeError::Empty { at: 0 });
    }

    loop {
        let arm_start = cursor;
        let mut sep = None;
        while cursor < bytes.len() {
            if whitespace(bytes[cursor]) {
                let mut j = cursor;
                while j < bytes.len() && whitespace(bytes[j]) {
                    j += 1;
                }
                if j + 1 < bytes.len()
                    && &s[j..j + 2] == "||"
                    && (j == 0 || whitespace(bytes[j - 1]))
                    && (j + 2 == bytes.len() || whitespace(bytes[j + 2]))
                {
                    sep = Some((j, j + 2));
                    break;
                }
                cursor = j;
            } else {
                cursor += 1;
            }
        }

        let end = sep.map(|x| x.0).unwrap_or(bytes.len());
        let tokens = split_tokens(s, arm_start, end);
        if tokens.is_empty() {
            return Err(RangeError::Empty {
                at: if arms.is_empty() { 0 } else { arm_start },
            });
        }
        arms.push(parse_arm(&tokens)?);

        if let Some((_, after)) = sep {
            cursor = after;
            while cursor < bytes.len() && whitespace(bytes[cursor]) {
                cursor += 1;
            }
            if cursor == bytes.len() {
                return Err(RangeError::Empty { at: bytes.len() });
            }
        } else {
            break;
        }
    }

    Ok(Range { arms })
}

fn arm_matches(v: &Version, arm: &Arm, include: bool) -> bool {
    if !include
        && !v.pre.is_empty()
        && !arm.anchors.contains(&(v.major, v.minor, v.patch))
    {
        return false;
    }
    arm.comps.iter().all(|c| {
        let o = compare_versions(v, &c.version);
        match c.op {
            Op::Eq => o == Ordering::Equal,
            Op::Lt => o == Ordering::Less,
            Op::Le => o != Ordering::Greater,
            Op::Gt => o == Ordering::Greater,
            Op::Ge => o != Ordering::Less,
        }
    })
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms
        .iter()
        .any(|a| arm_matches(v, a, include_prerelease))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn extrema(
    candidates: &[String],
    r: &Range,
    max: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::new();
    for (i, s) in candidates.iter().enumerate() {
        parsed.push(
            parse_version(s).map_err(|error| CandidateError { index: i, error })?,
        );
    }

    let mut best: Option<usize> = None;
    for (i, v) in parsed.iter().enumerate() {
        if matches(v, r)
            && best.map_or(true, |b| {
                let c = compare_versions(v, &parsed[b]);
                if max {
                    c == Ordering::Greater
                } else {
                    c == Ordering::Less
                }
            })
        {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
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

fn next_core(v: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    bump(v, field, at)
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    let mut candidates = Vec::new();
    for arm in a.arms.iter().chain(b.arms.iter()) {
        for c in &arm.comps {
            let v = &c.version;
            candidates.push(v.clone());
            if !v.pre.is_empty() {
                let mut successor = v.clone();
                successor.pre.push("0".into());
                candidates.push(successor);
            }
            let mut stable = v.clone();
            stable.pre.clear();
            candidates.push(stable);
        }
    }
    candidates.push(Version {
        major: 0,
        minor: 0,
        patch: 0,
        pre: vec![],
        build: vec![],
    });

    candidates
        .iter()
        .any(|x| matches_with(x, a, include_prerelease) && matches_with(x, b, include_prerelease))
}

fn version_without_build(v: &Version) -> String {
    format!(
        "{}.{}.{}{}",
        v.major,
        v.minor,
        v.patch,
        if v.pre.is_empty() {
            String::new()
        } else {
            format!("-{}", v.pre.join("."))
        }
    )
}

pub fn canonical(r: &Range) -> String {
    let mut out = Vec::new();
    for arm in &r.arms {
        let mut lower: Option<(Version, bool)> = None;
        let mut upper: Option<(Version, bool)> = None;
        for c in &arm.comps {
            match c.op {
                Op::Eq => {
                    lower = Some((c.version.clone(), true));
                    upper = Some((c.version.clone(), true));
                }
                Op::Gt | Op::Ge => {
                    let inc = c.op == Op::Ge;
                    if lower.as_ref().map_or(true, |(v, old)| {
                        compare_versions(&c.version, v) == Ordering::Greater
                            || (compare_versions(&c.version, v) == Ordering::Equal
                                && !inc
                                && *old)
                    }) {
                        lower = Some((c.version.clone(), inc));
                    }
                }
                Op::Lt | Op::Le => {
                    let inc = c.op == Op::Le;
                    if upper.as_ref().map_or(true, |(v, old)| {
                        compare_versions(&c.version, v) == Ordering::Less
                            || (compare_versions(&c.version, v) == Ordering::Equal
                                && !inc
                                && *old)
                    }) {
                        upper = Some((c.version.clone(), inc));
                    }
                }
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
                .map(|(x, y, z)| format!("{x}.{y}.{z}"))
                .collect::<Vec<_>>()
                .join(",")
        };

        let format_bound = |bound: &Option<(Version, bool)>, is_lower: bool| -> String {
            if let Some((v, inclusive)) = bound {
                let op = match (is_lower, inclusive) {
                    (true, true) => ">=",
                    (true, false) => ">",
                    (false, true) => "<=",
                    (false, false) => "<",
                };
                format!("{op}{}", version_without_build(v))
            } else {
                "*".into()
            }
        };

        out.push(format!(
            "[{};{};pre={}]",
            format_bound(&lower, true),
            format_bound(&upper, false),
            anchor_text
        ));
    }
    out.sort();
    out.dedup();
    if out.is_empty() {
        "EMPTY".into()
    } else {
        out.join(" || ")
    }
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

fn visit_ranges<F: FnMut(&str, Vec<usize>)>(
    e: &SetExpr,
    path: &mut Vec<usize>,
    f: &mut F,
) {
    match e {
        SetExpr::Range(s) => f(s, path.clone()),
        SetExpr::Union(xs) | SetExpr::Intersection(xs) => {
            for (i, x) in xs.iter().enumerate() {
                path.push(i);
                visit_ranges(x, path, f);
                path.pop();
            }
        }
    }
}

fn parse_expr_ranges(
    expr: &SetExpr,
    package: &str,
    label: &str,
    path: &mut Vec<usize>,
    out: &mut Vec<Range>,
) -> Result<(), SelectError> {
    match expr {
        SetExpr::Range(s) => {
            let r = parse_range(s).map_err(|error| SelectError::InvalidRange {
                package: package.to_string(),
                label: label.to_string(),
                path: path.clone(),
                error,
            })?;
            out.push(r);
        }
        SetExpr::Union(xs) | SetExpr::Intersection(xs) => {
            for (i, child) in xs.iter().enumerate() {
                path.push(i);
                parse_expr_ranges(child, package, label, path, out)?;
                path.pop();
            }
        }
    }
    Ok(())
}

fn expr_matches(
    e: &SetExpr,
    v: &Version,
    include: bool,
    ranges: &[Range],
    cursor: &mut usize,
) -> bool {
    match e {
        SetExpr::Range(_) => {
            let r = &ranges[*cursor];
            *cursor += 1;
            matches_with(v, r, include)
        }
        SetExpr::Union(xs) => {
            let mut result = false;
            for x in xs {
                let child = expr_matches(x, v, include, ranges, cursor);
                result |= child;
            }
            result
        }
        SetExpr::Intersection(xs) => {
            let mut result = true;
            for x in xs {
                let child = expr_matches(x, v, include, ranges, cursor);
                result &= child;
            }
            result
        }
    }
}

fn subset_satisfiable(
    indexes: &[usize],
    candidates: &[Version],
    exprs: &[SetExpr],
    parsed: &[Vec<Range>],
    include: bool,
) -> bool {
    candidates.iter().any(|v| {
        indexes.iter().all(|&i| {
            let mut cursor = 0;
            expr_matches(&exprs[i], v, include, &parsed[i], &mut cursor)
        })
    })
}

fn minimum_conflict(
    candidates: &[Version],
    constraints: &[Constraint],
    parsed: &[Vec<Range>],
    include: bool,
) -> Vec<String> {
    if candidates.is_empty() {
        return vec![];
    }

    fn search(
        start: usize,
        need: usize,
        chosen: &mut Vec<usize>,
        candidates: &[Version],
        constraints: &[Constraint],
        parsed: &[Vec<Range>],
        include: bool,
    ) -> Option<Vec<usize>> {
        if chosen.len() == need {
            let exprs: Vec<SetExpr> = constraints.iter().map(|c| c.expr.clone()).collect();
            return if subset_satisfiable(chosen, candidates, &exprs, parsed, include) {
                None
            } else {
                Some(chosen.clone())
            };
        }
        let remaining = need - chosen.len();
        if constraints.len().saturating_sub(start) < remaining {
            return None;
        }
        for i in start..=constraints.len() - remaining {
            chosen.push(i);
            if let Some(found) = search(
                i + 1,
                need,
                chosen,
                candidates,
                constraints,
                parsed,
                include,
            ) {
                return Some(found);
            }
            chosen.pop();
        }
        None
    }

    for size in 1..=constraints.len() {
        if let Some(indexes) = search(
            0,
            size,
            &mut Vec::new(),
            candidates,
            constraints,
            parsed,
            include,
        ) {
            let mut labels: Vec<String> = indexes
                .iter()
                .map(|&i| constraints[i].label.clone())
                .collect();
            labels.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            return labels;
        }
    }
    vec![]
}

pub fn select_packages(
    packages: &[Package],
    preference: Preference,
    include_prerelease: bool,
) -> Result<Vec<Selection>, SelectError> {
    // Global phase 1: package names.
    let mut names = std::collections::HashSet::new();
    for p in packages {
        if !names.insert(p.name.clone()) {
            return Err(SelectError::DuplicatePackage {
                name: p.name.clone(),
            });
        }
    }

    // Global phase 2: labels.
    for p in packages {
        let mut labels = std::collections::HashSet::new();
        for c in &p.constraints {
            if !labels.insert(c.label.clone()) {
                return Err(SelectError::DuplicateLabel {
                    package: p.name.clone(),
                    label: c.label.clone(),
                });
            }
        }
    }

    // Global phase 3: candidate versions.
    let mut versions = Vec::new();
    for p in packages {
        let mut parsed = Vec::new();
        for (i, s) in p.candidates.iter().enumerate() {
            parsed.push(parse_version(s).map_err(|error| {
                SelectError::InvalidCandidate {
                    package: p.name.clone(),
                    index: i,
                    error,
                }
            })?);
        }
        versions.push(parsed);
    }

    // Global phase 4: every range leaf.
    let mut all_ranges = Vec::new();
    for p in packages {
        let mut package_ranges = Vec::new();
        for c in &p.constraints {
            let mut parsed = Vec::new();
            parse_expr_ranges(
                &c.expr,
                &p.name,
                &c.label,
                &mut Vec::new(),
                &mut parsed,
            )?;
            package_ranges.push(parsed);
        }
        all_ranges.push(package_ranges);
    }

    let mut selections = Vec::new();
    for (pi, p) in packages.iter().enumerate() {
        let mut qualifying = Vec::new();
        for (ci, v) in versions[pi].iter().enumerate() {
            let mut ok = true;
            for (ki, c) in p.constraints.iter().enumerate() {
                let mut cursor = 0;
                if !expr_matches(
                    &c.expr,
                    v,
                    include_prerelease,
                    &all_ranges[pi][ki],
                    &mut cursor,
                ) {
                    ok = false;
                }
            }
            if ok {
                qualifying.push(ci);
            }
        }

        if qualifying.is_empty() {
            let labels = minimum_conflict(
                &versions[pi],
                &p.constraints,
                &all_ranges[pi],
                include_prerelease,
            );
            return Err(SelectError::NoSolution {
                package: p.name.clone(),
                labels,
            });
        }

        let chosen = match preference {
            Preference::Highest => *qualifying
                .iter()
                .max_by(|&&a, &&b| compare_versions(&versions[pi][a], &versions[pi][b]))
                .unwrap(),
            Preference::Lowest => *qualifying
                .iter()
                .min_by(|&&a, &&b| compare_versions(&versions[pi][a], &versions[pi][b]))
                .unwrap(),
            Preference::StableFirst => {
                let stable: Vec<usize> = qualifying
                    .iter()
                    .copied()
                    .filter(|&i| versions[pi][i].pre.is_empty())
                    .collect();
                let pool = if stable.is_empty() {
                    &qualifying
                } else {
                    &stable
                };
                *pool.iter()
                    .max_by(|&&a, &&b| compare_versions(&versions[pi][a], &versions[pi][b]))
                    .unwrap()
            }
        };

        selections.push(Selection {
            package: p.name.clone(),
            version: p.candidates[chosen].clone(),
        });
    }

    Ok(selections)
}

fn main() {}