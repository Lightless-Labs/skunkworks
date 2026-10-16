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
    if start == *pos {
        if *pos == bytes.len() || matches!(bytes[*pos], b'.' | b'-' | b'+') {
            return Err(VersionError::MissingCore { at: start });
        }
        return Err(VersionError::InvalidChar { at: start });
    }
    if *pos - start > 1 && bytes[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    s[start..*pos]
        .parse::<u64>()
        .map_err(|_| VersionError::Overflow { at: start })
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    numeric_pre: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut id_start = start;
    let mut pos = start;
    loop {
        if pos == end || bytes[pos] == b'.' {
            if pos == id_start {
                return Err(VersionError::EmptyIdentifier { at: id_start });
            }
            let id = &s[id_start..pos];
            for (i, b) in id.bytes().enumerate() {
                if !(b.is_ascii_alphanumeric() || b == b'-') {
                    return Err(VersionError::InvalidChar {
                        at: id_start + i,
                    });
                }
            }
            if numeric_pre
                && id.len() > 1
                && id.as_bytes()[0] == b'0'
                && id.bytes().all(is_digit)
            {
                return Err(VersionError::LeadingZero { at: id_start });
            }
            out.push(id.to_string());
            if pos == end {
                break;
            }
            pos += 1;
            id_start = pos;
        } else {
            pos += 1;
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
    if pos >= s.len() {
        return Err(VersionError::MissingCore { at: pos });
    }
    if s.as_bytes()[pos] != b'.' {
        return if matches!(s.as_bytes()[pos], b'-' | b'+') {
            Err(VersionError::MissingCore { at: pos })
        } else {
            Err(VersionError::InvalidChar { at: pos })
        };
    }
    pos += 1;

    let minor = parse_core(s, &mut pos)?;
    if pos >= s.len() {
        return Err(VersionError::MissingCore { at: pos });
    }
    if s.as_bytes()[pos] != b'.' {
        return if matches!(s.as_bytes()[pos], b'-' | b'+') {
            Err(VersionError::MissingCore { at: pos })
        } else {
            Err(VersionError::InvalidChar { at: pos })
        };
    }
    pos += 1;

    let patch = parse_core(s, &mut pos)?;
    let bytes = s.as_bytes();
    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() {
        match bytes[pos] {
            b'-' => {
                let pre_start = pos + 1;
                let build_at = bytes[pre_start..]
                    .iter()
                    .position(|&b| b == b'+')
                    .map(|n| pre_start + n);
                let pre_end = build_at.unwrap_or(bytes.len());
                pre = parse_identifiers(s, pre_start, pre_end, true)?;
                if let Some(at) = build_at {
                    build = parse_identifiers(s, at + 1, bytes.len(), false)?;
                }
            }
            b'+' => {
                build = parse_identifiers(s, pos + 1, bytes.len(), false)?;
            }
            _ => return Err(VersionError::InvalidChar { at: pos }),
        }
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

fn numeric_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn cmp_numeric_str(a: &str, b: &str) -> Ordering {
    a.len()
        .cmp(&b.len())
        .then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    if a.is_empty() || b.is_empty() {
        return match (a.is_empty(), b.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            _ => unreachable!(),
        };
    }
    for (x, y) in a.iter().zip(b) {
        let xn = numeric_id(x);
        let yn = numeric_id(y);
        let ord = match (xn, yn) {
            (true, true) => cmp_numeric_str(x, y),
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
    core(a)
        .cmp(&core(b))
        .then_with(|| cmp_pre(&a.pre, &b.pre))
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn cmp_matches(v: &Version, c: &Comparator) -> bool {
    match compare_versions(v, &c.version) {
        Ordering::Equal => matches!(c.op, Op::Eq | Op::Le | Op::Ge),
        Ordering::Less => matches!(c.op, Op::Lt | Op::Le),
        Ordering::Greater => matches!(c.op, Op::Gt | Op::Ge),
    }
}

fn parse_full_operand(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

fn partial_or_full(s: &str, at: usize) -> Result<(Version, usize, bool), RangeError> {
    if s.contains('-') || s.contains('+') {
        return parse_full_operand(s, at).map(|v| (v, 3, true));
    }

    let fields: Vec<&str> = s.split('.').collect();
    let has_wild = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));
    if fields.len() >= 3 && !has_wild {
        return parse_full_operand(s, at).map(|v| (v, 3, true));
    }
    if fields.len() > 3 || fields.iter().any(|f| f.is_empty()) {
        return Err(RangeError::Syntax { at });
    }

    let mut nums = Vec::new();
    let mut wildcard_seen = false;
    for f in &fields {
        if matches!(*f, "*" | "x" | "X") {
            wildcard_seen = true;
        } else if wildcard_seen
            || !f.bytes().all(is_digit)
            || (f.len() > 1 && f.as_bytes()[0] == b'0')
        {
            return Err(RangeError::Syntax { at });
        } else {
            let n = f.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
            nums.push(n);
        }
    }

    let k = nums.len();
    let v = Version {
        major: *nums.first().unwrap_or(&0),
        minor: *nums.get(1).unwrap_or(&0),
        patch: *nums.get(2).unwrap_or(&0),
        pre: Vec::new(),
        build: Vec::new(),
    };
    Ok((v, k, false))
}

fn inc_bound(v: &Version, which: usize, at: usize) -> Result<Version, RangeError> {
    let mut r = Version {
        major: v.major,
        minor: v.minor,
        patch: v.patch,
        pre: vec!["0".into()],
        build: Vec::new(),
    };
    let field = match which {
        0 => &mut r.major,
        1 => &mut r.minor,
        _ => &mut r.patch,
    };
    *field = field.checked_add(1).ok_or(RangeError::Syntax { at })?;
    if which == 0 {
        r.minor = 0;
        r.patch = 0;
    } else if which == 1 {
        r.patch = 0;
    }
    Ok(r)
}

fn push(arm: &mut Vec<Comparator>, op: Op, version: Version, anchor: bool) {
    arm.push(Comparator {
        op,
        version,
        anchor,
    });
}

fn parse_partial_token(
    token: &str,
    start: usize,
    mode: char,
    arm: &mut Vec<Comparator>,
) -> Result<(), RangeError> {
    let operand = if mode == '\0' {
        token
    } else {
        &token[1..]
    };
    let operand_at = if mode == '\0' { start } else { start + 1 };
    if operand.is_empty() {
        return Err(RangeError::Syntax { at: start });
    }

    let (v, k, full) = partial_or_full(operand, operand_at)?;
    if mode == '\0' {
        if full {
            push(arm, Op::Eq, v.clone(), !v.pre.is_empty());
        } else if k == 1 || k == 2 {
            push(arm, Op::Ge, v.clone(), false);
            let upper = inc_bound(&v, k - 1, start)?;
            push(arm, Op::Lt, upper, false);
        } else if k == 3 {
            push(arm, Op::Eq, v, false);
        }
        return Ok(());
    }

    if k == 0 {
        return Err(RangeError::Syntax { at: start });
    }

    push(arm, Op::Ge, v.clone(), full && !v.pre.is_empty());
    let which = if mode == '^' {
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
    let upper = inc_bound(&v, which, start)?;
    push(arm, Op::Lt, upper, false);
    Ok(())
}

fn token_ranges(s: &str) -> Vec<(usize, usize)> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && is_ws(bytes[i]) {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !is_ws(bytes[i]) {
            i += 1;
        }
        out.push((start, i));
    }
    out
}

fn parse_hyphen_arm(
    s: &str,
    tokens: &[(usize, usize)],
    minus_index: usize,
    arm: &mut Vec<Comparator>,
) -> Result<(), RangeError> {
    if tokens.len() != 3 || minus_index != 1 {
        return Err(RangeError::Syntax {
            at: tokens[minus_index].0,
        });
    }

    let (ls, le) = tokens[0];
    let (us, ue) = tokens[2];
    let (lv, lk, lf) = partial_or_full(&s[ls..le], ls)?;
    let (uv, uk, uf) = partial_or_full(&s[us..ue], us)?;
    if lk == 0 {
        return Err(RangeError::Syntax { at: ls });
    }
    if uk == 0 {
        return Err(RangeError::Syntax { at: us });
    }

    push(arm, Op::Ge, lv.clone(), lf && !lv.pre.is_empty());
    if uf {
        push(arm, Op::Le, uv.clone(), !uv.pre.is_empty());
    } else if uk == 1 || uk == 2 {
        let upper = inc_bound(&uv, uk - 1, us)?;
        push(arm, Op::Lt, upper, false);
    } else {
        push(arm, Op::Le, uv, false);
    }
    Ok(())
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let tokens = token_ranges(s);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms = Vec::new();
    let mut current = Vec::new();
    let mut arm_tokens: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        let (st, en) = tokens[i];
        let token = &s[st..en];

        if token == "||" {
            if arm_tokens.is_empty() {
                return Err(RangeError::Empty { at: st });
            }
            if let Some(minus) = arm_tokens.iter().position(|&(a, b)| &s[a..b] == "-") {
                parse_hyphen_arm(s, &arm_tokens, minus, &mut current)?;
            } else {
                parse_arm_tokens(s, &arm_tokens, &mut current)?;
            }
            arms.push(current);
            current = Vec::new();
            arm_tokens.clear();
            i += 1;
            if i == tokens.len() {
                return Err(RangeError::Empty { at: s.len() });
            }
            continue;
        }
        arm_tokens.push((st, en));
        i += 1;
    }

    if let Some(minus) = arm_tokens.iter().position(|&(a, b)| &s[a..b] == "-") {
        parse_hyphen_arm(s, &arm_tokens, minus, &mut current)?;
    } else {
        parse_arm_tokens(s, &arm_tokens, &mut current)?;
    }
    arms.push(current);
    Ok(Range { arms })
}

fn parse_arm_tokens(
    s: &str,
    tokens: &[(usize, usize)],
    arm: &mut Vec<Comparator>,
) -> Result<(), RangeError> {
    for &(st, en) in tokens {
        let token = &s[st..en];
        let bytes = token.as_bytes();

        if token.starts_with('^') || token.starts_with('~') {
            parse_partial_token(token, st, bytes[0] as char, arm)?;
            continue;
        }

        let (op, n) = if bytes.starts_with(b">=") {
            (Op::Ge, 2)
        } else if bytes.starts_with(b"<=") {
            (Op::Le, 2)
        } else if bytes.starts_with(b">") {
            (Op::Gt, 1)
        } else if bytes.starts_with(b"<") {
            (Op::Lt, 1)
        } else if bytes.starts_with(b"=") {
            (Op::Eq, 1)
        } else {
            (Op::Eq, 0)
        };

        if n == 0 && (bytes[0].is_ascii_digit() || matches!(bytes[0], b'*' | b'x' | b'X')) {
            parse_partial_token(token, st, '\0', arm)?;
        } else if n > 0 {
            let operand = &token[n..];
            let operand_at = st + n;
            if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
                return Err(RangeError::Syntax { at: st });
            }
            let v = parse_full_operand(operand, operand_at)?;
            push(arm, op, v, false);
        } else {
            return Err(RangeError::Syntax { at: st });
        }
    }
    Ok(())
}

fn arm_admits(v: &Version, arm: &[Comparator]) -> bool {
    arm.iter()
        .any(|c| c.anchor && !c.version.pre.is_empty() && core(v) == core(&c.version))
}

fn arm_matches(v: &Version, arm: &[Comparator], include_pre: bool) -> bool {
    if !include_pre && !v.pre.is_empty() && !arm_admits(v, arm) {
        return false;
    }
    arm.iter().all(|c| cmp_matches(v, c))
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
    highest: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, s) in candidates.iter().enumerate() {
        parsed.push(parse_version(s).map_err(|error| CandidateError { index, error })?);
    }

    let mut best: Option<usize> = None;
    for (i, v) in parsed.iter().enumerate() {
        if !matches(v, r) {
            continue;
        }
        if let Some(j) = best {
            let cmp = compare_versions(v, &parsed[j]);
            if (highest && cmp == Ordering::Greater) || (!highest && cmp == Ordering::Less) {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
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

fn expr_matches(expr: &SetExpr, v: &Version, include_pre: bool) -> bool {
    match expr {
        SetExpr::Range(s) => parse_range(s)
            .map(|r| matches_with(v, &r, include_pre))
            .unwrap_or(false),
        SetExpr::Union(children) => children.iter().any(|x| expr_matches(x, v, include_pre)),
        SetExpr::Intersection(children) => {
            children.iter().all(|x| expr_matches(x, v, include_pre))
        }
    }
}

fn conflict_labels(
    constraints: &[Constraint],
    candidates: &[Version],
    include_pre: bool,
) -> Vec<String> {
    if candidates.is_empty() {
        return Vec::new();
    }
    for size in 1..=constraints.len() {
        let mut chosen = Vec::new();
        if let Some(indices) =
            find_conflict(constraints, candidates, include_pre, size, 0, &mut chosen)
        {
            let mut labels: Vec<String> = indices
                .iter()
                .map(|&i| constraints[i].label.clone())
                .collect();
            labels.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            return labels;
        }
    }
    Vec::new()
}

fn find_conflict(
    constraints: &[Constraint],
    candidates: &[Version],
    include_pre: bool,
    need: usize,
    start: usize,
    chosen: &mut Vec<usize>,
) -> Option<Vec<usize>> {
    if chosen.len() == need {
        let any = candidates.iter().any(|v| {
            chosen
                .iter()
                .all(|&i| expr_matches(&constraints[i].expr, v, include_pre))
        });
        return if any { None } else { Some(chosen.clone()) };
    }

    let remaining = need - chosen.len();
    if constraints.len().saturating_sub(start) < remaining {
        return None;
    }
    for i in start..=constraints.len() - remaining {
        chosen.push(i);
        if let Some(found) =
            find_conflict(constraints, candidates, include_pre, need, i + 1, chosen)
        {
            return Some(found);
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
    for p in packages {
        if !names.insert(p.name.clone()) {
            return Err(SelectError::DuplicatePackage {
                name: p.name.clone(),
            });
        }
    }

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

    let mut parsed: Vec<Vec<Version>> = Vec::new();
    for p in packages {
        let mut versions = Vec::new();
        for (index, s) in p.candidates.iter().enumerate() {
            versions.push(
                parse_version(s).map_err(|error| SelectError::InvalidCandidate {
                    package: p.name.clone(),
                    index,
                    error,
                })?,
            );
        }
        parsed.push(versions);
    }

    for p in packages {
        for c in &p.constraints {
            validate_expr(&c.expr, &p.name, &c.label, &mut Vec::new())?;
        }
    }

    let mut selections = Vec::new();
    for (pi, p) in packages.iter().enumerate() {
        let versions = &parsed[pi];
        let valid: Vec<usize> = versions
            .iter()
            .enumerate()
            .filter(|(_, v)| {
                p.constraints
                    .iter()
                    .all(|c| expr_matches(&c.expr, v, include_prerelease))
            })
            .map(|(i, _)| i)
            .collect();

        if valid.is_empty() {
            return Err(SelectError::NoSolution {
                package: p.name.clone(),
                labels: conflict_labels(&p.constraints, versions, include_prerelease),
            });
        }

        let stable_available = matches!(preference, Preference::StableFirst)
            && valid.iter().any(|&i| versions[i].pre.is_empty());
        let mut best = valid[0];
        for &i in valid.iter().skip(1) {
            let vi = &versions[i];
            let vb = &versions[best];
            let stable_i = vi.pre.is_empty();
            let stable_b = vb.pre.is_empty();
            let better = match preference {
                Preference::Highest => compare_versions(vi, vb) == Ordering::Greater,
                Preference::Lowest => compare_versions(vi, vb) == Ordering::Less,
                Preference::StableFirst => {
                    if stable_available && stable_i != stable_b {
                        stable_i
                    } else {
                        compare_versions(vi, vb) == Ordering::Greater
                    }
                }
            };
            if better {
                best = i;
            }
        }

        selections.push(Selection {
            package: p.name.clone(),
            version: p.candidates[best].clone(),
        });
    }
    Ok(selections)
}

fn make_core_version(t: (u64, u64, u64), pre: Vec<String>) -> Version {
    Version {
        major: t.0,
        minor: t.1,
        patch: t.2,
        pre,
        build: Vec::new(),
    }
}

fn increment_tuple(t: (u64, u64, u64)) -> Option<(u64, u64, u64)> {
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

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for aa in &a.arms {
        for bb in &b.arms {
            let mut all = aa.clone();
            all.extend(bb.clone());
            if arm_intersects(&all, aa, bb, include_prerelease) {
                return true;
            }
        }
    }
    false
}

fn arm_intersects(
    all: &[Comparator],
    aa: &[Comparator],
    bb: &[Comparator],
    include_pre: bool,
) -> bool {
    let mut lower: Option<(Version, bool)> = None;
    let mut upper: Option<(Version, bool)> = None;
    let mut equal: Option<Version> = None;

    for c in all {
        match c.op {
            Op::Eq => {
                if let Some(old) = &equal {
                    if compare_versions(old, &c.version) != Ordering::Equal {
                        return false;
                    }
                }
                equal = Some(c.version.clone());
            }
            Op::Ge | Op::Gt => {
                let inclusive = c.op == Op::Ge;
                if lower.as_ref().map_or(true, |(v, inc)| {
                    compare_versions(&c.version, v) == Ordering::Greater
                        || (compare_versions(&c.version, v) == Ordering::Equal
                            && !inclusive
                            && *inc)
                }) {
                    lower = Some((c.version.clone(), inclusive));
                }
            }
            Op::Le | Op::Lt => {
                let inclusive = c.op == Op::Le;
                if upper.as_ref().map_or(true, |(v, inc)| {
                    compare_versions(&c.version, v) == Ordering::Less
                        || (compare_versions(&c.version, v) == Ordering::Equal
                            && !inclusive
                            && *inc)
                }) {
                    upper = Some((c.version.clone(), inclusive));
                }
            }
        }
    }

    if let Some(v) = equal {
        return arm_matches(&v, aa, include_pre) && arm_matches(&v, bb, include_pre);
    }

    if let (Some((l, li)), Some((u, ui))) = (&lower, &upper) {
        let c = compare_versions(l, u);
        if c == Ordering::Greater || (c == Ordering::Equal && !(*li && *ui)) {
            return false;
        }
    }

    if include_pre {
        return true;
    }

    let mut anchors = Vec::new();
    for arm in [aa, bb] {
        for c in arm {
            if c.anchor && !c.version.pre.is_empty() {
                anchors.push(core(&c.version));
            }
        }
    }
    anchors.sort();
    anchors.dedup();

    for t in anchors {
        let v = make_core_version(t, vec!["0".into()]);
        if in_bounds(&v, &lower, &upper) && all.iter().all(|c| cmp_matches(&v, c)) {
            return true;
        }
    }

    if let Some((l, li)) = &lower {
        let candidate = if !l.pre.is_empty() {
            if *li {
                l.clone()
            } else {
                let mut p = l.pre.clone();
                p.push("0".into());
                make_core_version(core(l), p)
            }
        } else if *li {
            l.clone()
        } else if let Some(t) = increment_tuple(core(l)) {
            make_core_version(t, vec![])
        } else {
            return false;
        };

        if in_bounds(&candidate, &lower, &upper) && all.iter().all(|c| cmp_matches(&candidate, c)) {
            return true;
        }
        if candidate.pre.is_empty() {
            if let Some(t) = increment_tuple(core(&candidate)) {
                let least = make_core_version(t, vec!["0".into()]);
                if in_bounds(&least, &lower, &upper) && all.iter().all(|c| cmp_matches(&least, c)) {
                    return true;
                }
            }
        }
        return false;
    }

    let candidate = make_core_version((0, 0, 0), Vec::new());
    in_bounds(&candidate, &lower, &upper) && all.iter().all(|c| cmp_matches(&candidate, c))
}

fn in_bounds(
    v: &Version,
    lower: &Option<(Version, bool)>,
    upper: &Option<(Version, bool)>,
) -> bool {
    if let Some((b, incl)) = lower {
        let o = compare_versions(v, b);
        if o == Ordering::Less || (o == Ordering::Equal && !incl) {
            return false;
        }
    }
    if let Some((b, incl)) = upper {
        let o = compare_versions(v, b);
        if o == Ordering::Greater || (o == Ordering::Equal && !incl) {
            return false;
        }
    }
    true
}

pub fn canonical(r: &Range) -> String {
    let mut forms = Vec::new();

    for arm in &r.arms {
        let mut lower: Option<(Version, bool)> = None;
        let mut upper: Option<(Version, bool)> = None;
        let mut anchors = Vec::new();

        for c in arm {
            if c.anchor && !c.version.pre.is_empty() {
                anchors.push(core(&c.version));
            }
            match c.op {
                Op::Eq => {
                    update_lower(&mut lower, c.version.clone(), true);
                    update_upper(&mut upper, c.version.clone(), true);
                }
                Op::Ge | Op::Gt => {
                    update_lower(&mut lower, c.version.clone(), c.op == Op::Ge)
                }
                Op::Le | Op::Lt => {
                    update_upper(&mut upper, c.version.clone(), c.op == Op::Le)
                }
            }
        }

        if !arm_intersects(arm, arm, arm, false) {
            continue;
        }

        anchors.sort();
        anchors.dedup();

        let low = lower.map_or_else(
            || "*".to_string(),
            |(v, inc)| format!("{}{}", if inc { ">=" } else { ">" }, version_no_build(&v)),
        );
        let high = upper.map_or_else(
            || "*".to_string(),
            |(v, inc)| format!("{}{}", if inc { "<=" } else { "<" }, version_no_build(&v)),
        );
        let pre = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors
                .iter()
                .map(|t| format!("{}.{}.{}", t.0, t.1, t.2))
                .collect::<Vec<_>>()
                .join(",")
        };
        forms.push(format!("[{};{};pre={}]", low, high, pre));
    }

    forms.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    forms.dedup();
    if forms.is_empty() {
        "EMPTY".to_string()
    } else {
        forms.join(" || ")
    }
}

fn update_lower(slot: &mut Option<(Version, bool)>, v: Version, inclusive: bool) {
    if slot.as_ref().map_or(true, |(old, inc)| {
        compare_versions(&v, old) == Ordering::Greater
            || (compare_versions(&v, old) == Ordering::Equal && !inclusive && *inc)
    }) {
        *slot = Some((v, inclusive));
    }
}

fn update_upper(slot: &mut Option<(Version, bool)>, v: Version, inclusive: bool) {
    if slot.as_ref().map_or(true, |(old, inc)| {
        compare_versions(&v, old) == Ordering::Less
            || (compare_versions(&v, old) == Ordering::Equal && !inclusive && *inc)
    }) {
        *slot = Some((v, inclusive));
    }
}

fn version_no_build(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}