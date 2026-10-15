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

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    prerelease: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut ident_start = start;

    loop {
        let mut i = ident_start;
        while i < end && bytes[i] != b'.' {
            let b = bytes[i];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }

        if i == ident_start {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }

        let ident = &s[ident_start..i];
        if prerelease
            && ident.len() > 1
            && ident.starts_with('0')
            && ident.bytes().all(is_digit)
        {
            return Err(VersionError::LeadingZero { at: ident_start });
        }

        result.push(ident.to_owned());
        if i == end {
            break;
        }
        ident_start = i + 1;
    }

    Ok(result)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let bytes = s.as_bytes();
    let mut pos = 0usize;
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

        let digits = &s[start..pos];
        if digits.len() > 1 && digits.starts_with('0') {
            return Err(VersionError::LeadingZero { at: start });
        }
        core[field] = digits
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
        }
    }

    if pos < bytes.len() && !matches!(bytes[pos], b'-' | b'+') {
        return Err(VersionError::InvalidChar { at: pos });
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] == b'-' {
        let start = pos + 1;
        let plus = bytes[start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|i| start + i)
            .unwrap_or(bytes.len());
        pre = parse_identifiers(s, start, plus, true)?;
        pos = plus;
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

fn is_numeric_ident(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }

    for (x, y) in a.iter().zip(b) {
        let xn = is_numeric_ident(x);
        let yn = is_numeric_ident(y);
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
        .then_with(|| compare_pre(&a.pre, &b.pre))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Debug)]
struct Comparator {
    op: Op,
    version: Version,
    generated: bool,
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

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn has_pre(v: &Version) -> bool {
    !v.pre.is_empty()
}

fn cmp_bound(v: &Version, c: &Comparator) -> Ordering {
    compare_versions(v, &c.version)
}

fn satisfies_comparator(v: &Version, c: &Comparator) -> bool {
    match c.op {
        Op::Eq => cmp_bound(v, c) == Ordering::Equal,
        Op::Lt => cmp_bound(v, c) == Ordering::Less,
        Op::Le => cmp_bound(v, c) != Ordering::Greater,
        Op::Gt => cmp_bound(v, c) == Ordering::Greater,
        Op::Ge => cmp_bound(v, c) != Ordering::Less,
    }
}

fn arm_matches(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !include_prerelease
        && has_pre(v)
        && !arm.anchors.contains(&core(v))
    {
        return false;
    }
    arm.comparators
        .iter()
        .all(|c| satisfies_comparator(v, c))
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

#[derive(Clone, Debug)]
struct Token {
    text: String,
    at: usize,
}

fn tokenize(s: &str) -> Vec<Token> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut i = 0usize;
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
        result.push(Token {
            text: s[start..i].to_owned(),
            at: start,
        });
    }
    result
}

fn next_core(mut v: Version, k: usize, at: usize) -> Result<Version, RangeError> {
    let err = || RangeError::Syntax { at };
    match k {
        1 => {
            v.major = v.major.checked_add(1).ok_or_else(err)?;
            v.minor = 0;
            v.patch = 0;
        }
        2 => {
            v.minor = v.minor.checked_add(1).ok_or_else(err)?;
            v.patch = 0;
        }
        _ => {
            v.patch = v.patch.checked_add(1).ok_or_else(err)?;
        }
    }
    v.pre = vec!["0".to_owned()];
    v.build.clear();
    Ok(v)
}

#[derive(Clone, Debug)]
struct Partial {
    version: Version,
    numeric_fields: usize,
    full: bool,
}

fn parse_full_operand(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

fn parse_partial_operand(s: &str, at: usize) -> Result<Partial, RangeError> {
    let bytes = s.as_bytes();
    let has_sign = bytes.iter().any(|&b| matches!(b, b'-' | b'+'));
    let fields: Vec<&str> = s.split('.').collect();
    let has_wild = fields
        .iter()
        .any(|f| matches!(*f, "*" | "x" | "X"));

    if has_sign || (fields.len() >= 3 && !has_wild) {
        let v = parse_full_operand(s, at)?;
        return Ok(Partial {
            version: v,
            numeric_fields: 3,
            full: true,
        });
    }

    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at });
    }

    let mut vals = [0u64; 3];
    let mut numeric_fields = 0usize;
    let mut saw_wild = false;

    for (i, f) in fields.iter().enumerate() {
        if f.is_empty() {
            return Err(RangeError::Syntax { at });
        }
        if matches!(*f, "*" | "x" | "X") {
            saw_wild = true;
            continue;
        }
        if saw_wild || !f.bytes().all(is_digit) || (f.len() > 1 && f.starts_with('0')) {
            return Err(RangeError::Syntax { at });
        }
        vals[i] = f.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
        numeric_fields += 1;
    }

    Ok(Partial {
        version: Version {
            major: vals[0],
            minor: vals[1],
            patch: vals[2],
            pre: Vec::new(),
            build: Vec::new(),
        },
        numeric_fields,
        full: false,
    })
}

fn push_cmp(arm: &mut Arm, op: Op, version: Version, generated: bool) {
    if has_pre(&version) && !generated {
        arm.anchors.push(core(&version));
    }
    arm.comparators.push(Comparator {
        op,
        version,
        generated,
    });
}

fn parse_comparator_token(token: &Token, arm: &mut Arm) -> Result<(), RangeError> {
    let (op, operand_offset, explicit) = if token.text.starts_with(">=") {
        (Op::Ge, 2, true)
    } else if token.text.starts_with("<=") {
        (Op::Le, 2, true)
    } else if token.text.starts_with('>') {
        (Op::Gt, 1, true)
    } else if token.text.starts_with('<') {
        (Op::Lt, 1, true)
    } else if token.text.starts_with('=') {
        (Op::Eq, 1, true)
    } else {
        (Op::Eq, 0, false)
    };

    if explicit {
        let rest = &token.text[operand_offset..];
        if rest.is_empty() || !rest.as_bytes()[0].is_ascii_digit() {
            return Err(RangeError::Syntax { at: token.at });
        }
        let at = token.at + operand_offset;
        let v = parse_full_operand(rest, at)?;
        push_cmp(arm, op, v, false);
        return Ok(());
    }

    let part = parse_partial_operand(&token.text, token.at)?;
    if part.full {
        push_cmp(arm, Op::Eq, part.version, false);
    } else if part.numeric_fields == 0 {
        // A bare wildcard contributes no comparisons.
    } else if part.numeric_fields == 3 {
        push_cmp(arm, Op::Eq, part.version, false);
    } else {
        let lo = part.version.clone();
        push_cmp(arm, Op::Ge, lo, false);
        let hi = next_core(part.version, part.numeric_fields, token.at)?;
        push_cmp(arm, Op::Lt, hi, true);
    }
    Ok(())
}

fn parse_caret_tilde(token: &Token, arm: &mut Arm, caret: bool) -> Result<(), RangeError> {
    let operand = &token.text[1..];
    let at = token.at + 1;
    let part = parse_partial_operand(operand, at)?;
    if part.numeric_fields == 0 {
        return Err(RangeError::Syntax { at: token.at });
    }
    let k = part.numeric_fields;
    let lo = part.version.clone();
    push_cmp(arm, Op::Ge, lo, false);
    let bump = if caret {
        if part.version.major > 0 || k == 1 {
            1
        } else if part.version.minor > 0 || k == 2 {
            2
        } else {
            3
        }
    } else if k == 1 {
        1
    } else {
        2
    };
    let hi = next_core(part.version, bump, at)?;
    push_cmp(arm, Op::Lt, hi, true);
    Ok(())
}

fn parse_hyphen_arm(tokens: &[Token], arm: &mut Arm) -> Result<(), RangeError> {
    let dashes: Vec<&Token> = tokens.iter().filter(|t| t.text == "-").collect();
    if !dashes.is_empty()
        && (tokens.len() != 3 || tokens[1].text != "-")
    {
        return Err(RangeError::Syntax { at: dashes[0].at });
    }

    let lower = parse_partial_operand(&tokens[0].text, tokens[0].at)?;
    let upper = parse_partial_operand(&tokens[2].text, tokens[2].at)?;
    if lower.numeric_fields == 0 {
        return Err(RangeError::Syntax { at: tokens[0].at });
    }
    if upper.numeric_fields == 0 {
        return Err(RangeError::Syntax { at: tokens[2].at });
    }

    push_cmp(arm, Op::Ge, lower.version, false);
    if upper.full {
        push_cmp(arm, Op::Le, upper.version, false);
    } else {
        let hi = next_core(upper.version, upper.numeric_fields, tokens[2].at)?;
        push_cmp(arm, Op::Lt, hi, true);
    }
    Ok(())
}

fn parse_arm(tokens: &[Token]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        comparators: Vec::new(),
        anchors: Vec::new(),
    };

    if tokens.iter().any(|t| t.text == "-") {
        parse_hyphen_arm(tokens, &mut arm)?;
        return Ok(arm);
    }

    for token in tokens {
        let first = token.text.as_bytes().first().copied();
        match first {
            Some(b'^') => parse_caret_tilde(token, &mut arm, true)?,
            Some(b'~') => parse_caret_tilde(token, &mut arm, false)?,
            Some(b'<') | Some(b'>') | Some(b'=') => parse_comparator_token(token, &mut arm)?,
            Some(b'0'..=b'9') | Some(b'*') | Some(b'x') | Some(b'X') => {
                parse_comparator_token(token, &mut arm)?
            }
            _ => return Err(RangeError::Syntax { at: token.at }),
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let tokens = tokenize(s);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms = Vec::new();
    let mut current = Vec::new();
    let mut saw_separator = false;

    for token in tokens {
        if token.text == "||" {
            if current.is_empty() {
                return Err(RangeError::Empty { at: token.at });
            }
            arms.push(parse_arm(&current)?);
            current.clear();
            saw_separator = true;
        } else {
            current.push(token);
            saw_separator = false;
        }
    }

    if current.is_empty() {
        if saw_separator {
            return Err(RangeError::Empty { at: s.len() });
        }
        return Err(RangeError::Empty { at: 0 });
    }
    arms.push(parse_arm(&current)?);
    Ok(Range { arms })
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
    for (index, candidate) in candidates.iter().enumerate() {
        let version = parse_version(candidate).map_err(|error| CandidateError { index, error })?;
        parsed.push(version);
    }

    let mut best: Option<usize> = None;
    for i in 0..candidates.len() {
        if !matches(&parsed[i], r) {
            continue;
        }
        if let Some(j) = best {
            let order = compare_versions(&parsed[i], &parsed[j]);
            if (maximum && order == Ordering::Greater) || (!maximum && order == Ordering::Less) {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

fn bump_core(v: &Version, field: usize) -> Option<Version> {
    let mut out = v.clone();
    match field {
        0 => {
            out.major = out.major.checked_add(1)?;
            out.minor = 0;
            out.patch = 0;
        }
        1 => {
            out.minor = out.minor.checked_add(1)?;
            out.patch = 0;
        }
        _ => out.patch = out.patch.checked_add(1)?,
    }
    out.pre.clear();
    out.build.clear();
    Some(out)
}

#[derive(Clone, Debug)]
struct Bound {
    version: Version,
    inclusive: bool,
}

fn arm_bounds(arm: &Arm) -> (Option<Bound>, Option<Bound>, Option<Version>) {
    let mut lower: Option<Bound> = None;
    let mut upper: Option<Bound> = None;
    let mut equality: Option<Version> = None;

    for c in &arm.comparators {
        match c.op {
            Op::Eq => {
                equality = Some(c.version.clone());
                let b = Bound {
                    version: c.version.clone(),
                    inclusive: true,
                };
                lower = Some(b.clone());
                upper = Some(b);
            }
            Op::Ge | Op::Gt => {
                let b = Bound {
                    version: c.version.clone(),
                    inclusive: c.op == Op::Ge,
                };
                lower = Some(match lower {
                    None => b,
                    Some(prev) => match compare_versions(&b.version, &prev.version) {
                        Ordering::Greater => b,
                        Ordering::Less => prev,
                        Ordering::Equal if !b.inclusive => b,
                        Ordering::Equal => prev,
                    },
                });
            }
            Op::Le | Op::Lt => {
                let b = Bound {
                    version: c.version.clone(),
                    inclusive: c.op == Op::Le,
                };
                upper = Some(match upper {
                    None => b,
                    Some(prev) => match compare_versions(&b.version, &prev.version) {
                        Ordering::Less => b,
                        Ordering::Greater => prev,
                        Ordering::Equal if !b.inclusive => b,
                        Ordering::Equal => prev,
                    },
                });
            }
        }
    }
    (lower, upper, equality)
}

fn version_string(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

fn arm_is_satisfiable(arm: &Arm) -> bool {
    let (lower, upper, equality) = arm_bounds(arm);
    if let Some(eq) = equality {
        return arm_matches(&eq, arm, false);
    }

    if let (Some(l), Some(u)) = (&lower, &upper) {
        let ord = compare_versions(&l.version, &u.version);
        if ord == Ordering::Greater || (ord == Ordering::Equal && !(l.inclusive && u.inclusive)) {
            return false;
        }

        if ord == Ordering::Equal {
            return arm_matches(&l.version, arm, false);
        }

        if !arm.anchors.is_empty() {
            for anchor in &arm.anchors {
                let v = Version {
                    major: anchor.0,
                    minor: anchor.1,
                    patch: anchor.2,
                    pre: vec!["0".to_owned()],
                    build: Vec::new(),
                };
                if arm_matches(&v, arm, false) {
                    return true;
                }
            }
        }

        // Stable versions exist only if the interval includes at least one release.
        if !has_pre(&l.version) || !has_pre(&u.version) {
            let candidate = if has_pre(&l.version) {
                let mut v = l.version.clone();
                v.pre.clear();
                v
            } else if l.inclusive {
                l.version.clone()
            } else {
                match bump_core(&l.version, 2) {
                    Some(v) => v,
                    None => return false,
                }
            };
            if !has_pre(&candidate)
                && compare_versions(&candidate, &l.version) != Ordering::Less
                && compare_versions(&candidate, &u.version) != Ordering::Greater
                && (compare_versions(&candidate, &u.version) != Ordering::Equal || u.inclusive)
                && arm_matches(&candidate, arm, false)
            {
                return true;
            }
        }
        return false;
    }

    true
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    if include_prerelease {
        return a.arms.iter().any(|x| {
            b.arms.iter().any(|y| {
                let combined = Arm {
                    comparators: x
                        .comparators
                        .iter()
                        .chain(&y.comparators)
                        .cloned()
                        .collect(),
                    anchors: Vec::new(),
                };
                arm_is_satisfiable(&combined)
            })
        });
    }

    a.arms.iter().any(|x| {
        b.arms.iter().any(|y| {
            let combined = Arm {
                comparators: x
                    .comparators
                    .iter()
                    .chain(&y.comparators)
                    .cloned()
                    .collect(),
                anchors: x
                    .anchors
                    .iter()
                    .copied()
                    .filter(|z| y.anchors.contains(z))
                    .collect(),
            };
            arm_is_satisfiable(&combined)
        })
    })
}

pub fn canonical(r: &Range) -> String {
    let mut strings = Vec::new();
    for arm in &r.arms {
        if !arm_is_satisfiable(arm) {
            continue;
        }
        let (lower, upper, _) = arm_bounds(arm);
        let mut anchors = arm.anchors.clone();
        anchors.sort_unstable();
        anchors.dedup();
        let anchor_text = if anchors.is_empty() {
            "-".to_owned()
        } else {
            anchors
                .iter()
                .map(|(a, b, c)| format!("{a}.{b}.{c}"))
                .collect::<Vec<_>>()
                .join(",")
        };

        let lower_text = lower
            .map(|b| {
                format!(
                    "{}{}",
                    if b.inclusive { ">=" } else { ">" },
                    version_string(&b.version)
                )
            })
            .unwrap_or_else(|| "*".to_owned());
        let upper_text = upper
            .map(|b| {
                format!(
                    "{}{}",
                    if b.inclusive { "<=" } else { "<" },
                    version_string(&b.version)
                )
            })
            .unwrap_or_else(|| "*".to_owned());

        strings.push(format!(
            "[{lower_text};{upper_text};pre={anchor_text}]"
        ));
    }

    strings.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    strings.dedup();
    if strings.is_empty() {
        "EMPTY".to_owned()
    } else {
        strings.join(" || ")
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

fn eval_expr(
    expr: &SetExpr,
    path: &mut Vec<usize>,
    leaves: &mut Vec<(Vec<usize>, Result<Range, RangeError>)>,
) {
    match expr {
        SetExpr::Range(s) => leaves.push((path.clone(), parse_range(s))),
        SetExpr::Union(children) | SetExpr::Intersection(children) => {
            for (i, child) in children.iter().enumerate() {
                path.push(i);
                eval_expr(child, path, leaves);
                path.pop();
            }
        }
    }
}

fn expr_matches(expr: &SetExpr, candidate: &Version, include: bool) -> bool {
    match expr {
        SetExpr::Range(s) => parse_range(s)
            .map(|r| matches_with(candidate, &r, include))
            .unwrap_or(false),
        SetExpr::Union(children) => children
            .iter()
            .any(|e| expr_matches(e, candidate, include)),
        SetExpr::Intersection(children) => children
            .iter()
            .all(|e| expr_matches(e, candidate, include)),
    }
}

fn conflict_labels(
    constraints: &[Constraint],
    candidates: &[Version],
    include: bool,
) -> Vec<String> {
    if candidates.is_empty() {
        return Vec::new();
    }

    let n = constraints.len();
    for size in 1..=n {
        let mut chosen = Vec::new();
        if let Some(indices) = find_conflict(
            constraints,
            candidates,
            include,
            size,
            0,
            &mut chosen,
        ) {
            let mut labels = indices
                .iter()
                .map(|&i| constraints[i].label.clone())
                .collect::<Vec<_>>();
            labels.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            return labels;
        }
    }
    Vec::new()
}

fn find_conflict(
    constraints: &[Constraint],
    candidates: &[Version],
    include: bool,
    remaining: usize,
    start: usize,
    chosen: &mut Vec<usize>,
) -> Option<Vec<usize>> {
    if remaining == 0 {
        let any = candidates.iter().any(|candidate| {
            chosen
                .iter()
                .all(|&i| expr_matches(&constraints[i].expr, candidate, include))
        });
        return if any { None } else { Some(chosen.clone()) };
    }

    if constraints.len().saturating_sub(start) < remaining {
        return None;
    }

    for i in start..=constraints.len() - remaining {
        chosen.push(i);
        if let Some(found) = find_conflict(
            constraints,
            candidates,
            include,
            remaining - 1,
            i + 1,
            chosen,
        ) {
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

    let mut parsed_candidates = Vec::with_capacity(packages.len());
    for package in packages {
        let mut parsed = Vec::with_capacity(package.candidates.len());
        for (index, candidate) in package.candidates.iter().enumerate() {
            parsed.push(parse_version(candidate).map_err(|error| {
                SelectError::InvalidCandidate {
                    package: package.name.clone(),
                    index,
                    error,
                }
            })?);
        }
        parsed_candidates.push(parsed);
    }

    for package in packages {
        for constraint in &package.constraints {
            let mut leaves = Vec::new();
            eval_expr(&constraint.expr, &mut Vec::new(), &mut leaves);
            for (path, result) in leaves {
                if let Err(error) = result {
                    return Err(SelectError::InvalidRange {
                        package: package.name.clone(),
                        label: constraint.label.clone(),
                        path,
                        error,
                    });
                }
            }
        }
    }

    let mut selections = Vec::with_capacity(packages.len());
    for (package_index, package) in packages.iter().enumerate() {
        let parsed = &parsed_candidates[package_index];
        let mut qualifying: Vec<usize> = parsed
            .iter()
            .enumerate()
            .filter_map(|(i, candidate)| {
                package
                    .constraints
                    .iter()
                    .all(|constraint| expr_matches(&constraint.expr, candidate, include_prerelease))
                    .then_some(i)
            })
            .collect();

        if qualifying.is_empty() {
            return Err(SelectError::NoSolution {
                package: package.name.clone(),
                labels: conflict_labels(&package.constraints, parsed, include_prerelease),
            });
        }

        let stable_first = matches!(preference, Preference::StableFirst)
            && qualifying.iter().any(|&i| !has_pre(&parsed[i]));

        qualifying.sort_by(|&i, &j| {
            let i_stable = !has_pre(&parsed[i]);
            let j_stable = !has_pre(&parsed[j]);
            if stable_first && i_stable != j_stable {
                return if i_stable {
                    Ordering::Less
                } else {
                    Ordering::Greater
                };
            }

            match preference {
                Preference::Lowest => compare_versions(&parsed[i], &parsed[j]),
                Preference::Highest | Preference::StableFirst => {
                    compare_versions(&parsed[j], &parsed[i])
                }
            }
            .then_with(|| i.cmp(&j))
        });

        let index = qualifying[0];
        selections.push(Selection {
            package: package.name.clone(),
            version: package.candidates[index].clone(),
        });
    }

    Ok(selections)
}

fn main() {}