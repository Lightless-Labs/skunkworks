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

fn valid_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

fn split_identifiers(
    s: &str,
    base: usize,
    prerelease: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;

    loop {
        let end = bytes[start..]
            .iter()
            .position(|b| *b == b'.')
            .map(|n| start + n)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(VersionError::EmptyIdentifier { at: base + start });
        }

        for i in start..end {
            if !valid_ident_byte(bytes[i]) {
                return Err(VersionError::InvalidChar { at: base + i });
            }
        }

        if prerelease
            && end - start > 1
            && bytes[start] == b'0'
            && bytes[start..end].iter().all(|b| is_digit(*b))
        {
            return Err(VersionError::LeadingZero { at: base + start });
        }

        out.push(s[start..end].to_string());

        if end == bytes.len() {
            break;
        }

        start = end + 1;
    }

    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let bytes = s.as_bytes();
    let mut pos = 0usize;
    let mut fields = [0u64; 3];

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

        if pos - start > 1 && bytes[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }

        fields[field] = s[start..pos]
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
            .map(|n| start + n)
            .unwrap_or(bytes.len());

        pre = split_identifiers(&s[start..end], start, true)?;
        pos = end;
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        let start = pos + 1;
        build = split_identifiers(&s[start..], start, false)?;
        pos = bytes.len();
    }

    if pos != bytes.len() {
        return Err(VersionError::InvalidChar { at: pos });
    }

    Ok(Version {
        major: fields[0],
        minor: fields[1],
        patch: fields[2],
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

fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }

    for (x, y) in a.iter().zip(b.iter()) {
        let xn = numeric_id(x);
        let yn = numeric_id(y);

        let ordering = match (xn, yn) {
            (true, true) => x
                .len()
                .cmp(&y.len())
                .then_with(|| x.as_bytes().cmp(y.as_bytes())),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => x.as_bytes().cmp(y.as_bytes()),
        };

        if ordering != Ordering::Equal {
            return ordering;
        }
    }

    a.len().cmp(&b.len())
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    core(a)
        .cmp(&core(b))
        .then_with(|| cmp_pre(&a.pre, &b.pre))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Debug)]
struct Cmp {
    op: String,
    v: Version,
}

#[derive(Clone, Debug)]
struct Arm {
    cmps: Vec<Cmp>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

fn ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

#[derive(Clone)]
struct Tok {
    s: String,
    at: usize,
}

fn tokenize(s: &str) -> Vec<Tok> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        while i < bytes.len() && ws(bytes[i]) {
            i += 1;
        }

        if i == bytes.len() {
            break;
        }

        let start = i;
        while i < bytes.len() && !ws(bytes[i]) {
            i += 1;
        }

        out.push(Tok {
            s: s[start..i].to_string(),
            at: start,
        });
    }

    out
}

fn mkver(a: u64, b: u64, c: u64, pre: Vec<String>, build: Vec<String>) -> Version {
    Version {
        major: a,
        minor: b,
        patch: c,
        pre,
        build,
    }
}

fn zero_pre() -> Vec<String> {
    vec!["0".to_string()]
}

fn parsed_full(offset: usize, operand: &str) -> Result<Version, RangeError> {
    parse_version(operand).map_err(|error| RangeError::Version {
        at: offset,
        error,
    })
}

#[derive(Clone)]
enum Partial {
    Full(Version),
    Part { nums: Vec<u64>, k: usize },
}

fn partial(tok: &Tok, offset: usize, s: &str) -> Result<Partial, RangeError> {
    if s.contains('-') || s.contains('+') {
        return parsed_full(offset, s).map(Partial::Full);
    }

    let fields: Vec<&str> = s.split('.').collect();

    if fields.len() >= 3 && !fields.iter().any(|x| matches!(*x, "*" | "x" | "X")) {
        return parsed_full(offset, s).map(Partial::Full);
    }

    if fields.len() > 3 || fields.iter().any(|x| x.is_empty()) {
        return Err(RangeError::Syntax { at: tok.at });
    }

    let mut nums = Vec::new();
    let mut wildcard = false;

    for field in &fields {
        if matches!(*field, "*" | "x" | "X") {
            wildcard = true;
        } else {
            if wildcard
                || !field.bytes().all(is_digit)
                || (field.len() > 1 && field.starts_with('0'))
            {
                return Err(RangeError::Syntax { at: tok.at });
            }

            let n = field
                .parse::<u64>()
                .map_err(|_| RangeError::Syntax { at: tok.at })?;
            nums.push(n);
        }
    }

    Ok(Partial::Part {
        k: nums.len(),
        nums,
    })
}

fn padded(nums: &[u64]) -> Version {
    mkver(
        *nums.first().unwrap_or(&0),
        *nums.get(1).unwrap_or(&0),
        *nums.get(2).unwrap_or(&0),
        Vec::new(),
        Vec::new(),
    )
}

fn bump(v: &Version, field: usize, tok: &Tok) -> Result<Version, RangeError> {
    let mut a = v.major;
    let mut b = v.minor;
    let mut c = v.patch;

    match field {
        0 => {
            a = a
                .checked_add(1)
                .ok_or(RangeError::Syntax { at: tok.at })?;
            b = 0;
            c = 0;
        }
        1 => {
            b = b
                .checked_add(1)
                .ok_or(RangeError::Syntax { at: tok.at })?;
            c = 0;
        }
        _ => {
            c = c
                .checked_add(1)
                .ok_or(RangeError::Syntax { at: tok.at })?;
        }
    }

    Ok(mkver(a, b, c, zero_pre(), Vec::new()))
}

fn add(arm: &mut Arm, op: &str, v: Version, anchor: bool) {
    if anchor && !v.pre.is_empty() {
        arm.anchors.push(core(&v));
    }
    arm.cmps.push(Cmp {
        op: op.to_string(),
        v,
    });
}

fn parse_operand(tok: &Tok, arm: &mut Arm) -> Result<(), RangeError> {
    let s = tok.s.as_str();
    let (prefix, body) = if s.starts_with('^') || s.starts_with('~') {
        (&s[..1], &s[1..])
    } else {
        ("", s)
    };
    let offset = tok.at + prefix.len();
    let parsed = partial(tok, offset, body)?;

    match (prefix, parsed) {
        ("", Partial::Full(v)) => add(arm, "=", v, true),
        ("", Partial::Part { k: 0, .. }) => {}
        ("", Partial::Part { nums, k }) => {
            let lower = padded(&nums);
            add(arm, ">=", lower.clone(), true);

            if k < 3 {
                let upper = if k == 1 {
                    bump(&lower, 0, tok)?
                } else {
                    bump(&lower, 1, tok)?
                };
                add(arm, "<", upper, false);
            }
        }
        ("^", Partial::Part { k: 0, .. }) | ("~", Partial::Part { k: 0, .. }) => {
            return Err(RangeError::Syntax { at: tok.at });
        }
        ("^", Partial::Part { nums, k }) => {
            let lower = padded(&nums);
            add(arm, ">=", lower.clone(), true);

            let field = if lower.major > 0 || k == 1 {
                0
            } else if lower.minor > 0 || k == 2 {
                1
            } else {
                2
            };

            add(arm, "<", bump(&lower, field, tok)?, false);
        }
        ("~", Partial::Part { nums, k }) => {
            let lower = padded(&nums);
            add(arm, ">=", lower.clone(), true);
            add(
                arm,
                "<",
                bump(&lower, if k == 1 { 0 } else { 1 }, tok)?,
                false,
            );
        }
        ("^", Partial::Full(v)) | ("~", Partial::Full(v)) => {
            let lower = v.clone();
            add(arm, ">=", lower.clone(), true);

            let field = if prefix == "~" {
                1
            } else if lower.major > 0 {
                0
            } else if lower.minor > 0 {
                1
            } else {
                2
            };

            add(arm, "<", bump(&lower, field, tok)?, false);
        }
        _ => return Err(RangeError::Syntax { at: tok.at }),
    }

    Ok(())
}

fn cmp_token(tok: &Tok, arm: &mut Arm) -> Result<(), RangeError> {
    let s = tok.s.as_str();

    let (op, n) = if s.starts_with(">=") {
        (">=", 2)
    } else if s.starts_with("<=") {
        ("<=", 2)
    } else if s.starts_with('>') {
        (">", 1)
    } else if s.starts_with('<') {
        ("<", 1)
    } else if s.starts_with('=') {
        ("=", 1)
    } else {
        return Err(RangeError::Syntax { at: tok.at });
    };

    if s.len() <= n || !s.as_bytes()[n].is_ascii_digit() {
        return Err(RangeError::Syntax { at: tok.at });
    }

    let v = parsed_full(tok.at + n, &s[n..])?;
    add(arm, op, v, true);
    Ok(())
}

fn parse_hyphen(tokens: &[Tok]) -> Result<Arm, RangeError> {
    if tokens.len() != 3 || tokens[1].s != "-" {
        let at = tokens.iter().find(|t| t.s == "-").unwrap().at;
        return Err(RangeError::Syntax { at });
    }

    let lower = partial(&tokens[0], tokens[0].at, &tokens[0].s)?;
    let upper = partial(&tokens[2], tokens[2].at, &tokens[2].s)?;

    let unpack = |p: Partial| -> Option<(Version, usize, bool)> {
        match p {
            Partial::Full(v) => Some((v, 3, true)),
            Partial::Part { nums, k } if k > 0 => Some((padded(&nums), k, false)),
            _ => None,
        }
    };

    let (lower_v, _, _) = unpack(lower).ok_or(RangeError::Syntax {
        at: tokens[0].at,
    })?;
    let (upper_v, upper_k, upper_full) =
        unpack(upper).ok_or(RangeError::Syntax { at: tokens[2].at })?;

    let mut arm = Arm {
        cmps: Vec::new(),
        anchors: Vec::new(),
    };

    add(&mut arm, ">=", lower_v, true);

    if upper_full {
        add(&mut arm, "<=", upper_v, true);
    } else {
        let field = if upper_k == 1 { 0 } else { 1 };
        add(&mut arm, "<", bump(&upper_v, field, &tokens[2])?, false);
    }

    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let tokens = tokenize(s);

    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut segments: Vec<Vec<Tok>> = vec![Vec::new()];

    for token in tokens {
        if token.s == "||" {
            if segments.last().unwrap().is_empty() {
                return Err(RangeError::Empty { at: token.at });
            }
            segments.push(Vec::new());
        } else {
            segments.last_mut().unwrap().push(token);
        }
    }

    if segments.last().unwrap().is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }

    let mut arms = Vec::new();

    for segment in segments {
        if segment.iter().any(|t| t.s == "-") {
            arms.push(parse_hyphen(&segment)?);
            continue;
        }

        let mut arm = Arm {
            cmps: Vec::new(),
            anchors: Vec::new(),
        };

        for tok in &segment {
            let first = tok.s.as_bytes()[0];

            if matches!(first, b'<' | b'>' | b'=') {
                cmp_token(tok, &mut arm)?;
            } else if matches!(first, b'^' | b'~')
                || first.is_ascii_digit()
                || matches!(first, b'*' | b'x' | b'X')
            {
                parse_operand(tok, &mut arm)?;
            } else {
                return Err(RangeError::Syntax { at: tok.at });
            }
        }

        arms.push(arm);
    }

    Ok(Range { arms })
}

fn compare(c: &Cmp, v: &Version) -> bool {
    let ordering = compare_versions(v, &c.v);
    match c.op.as_str() {
        "=" => ordering == Ordering::Equal,
        "<" => ordering == Ordering::Less,
        "<=" => ordering != Ordering::Greater,
        ">" => ordering == Ordering::Greater,
        ">=" => ordering != Ordering::Less,
        _ => false,
    }
}

fn arm_match(v: &Version, arm: &Arm, include: bool) -> bool {
    if !arm.cmps.iter().all(|c| compare(c, v)) {
        return false;
    }

    include || v.pre.is_empty() || arm.anchors.contains(&core(v))
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms
        .iter()
        .any(|arm| arm_match(v, arm, include_prerelease))
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
    high: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::new();

    for (index, s) in candidates.iter().enumerate() {
        parsed.push(parse_version(s).map_err(|error| CandidateError { index, error })?);
    }

    let mut best: Option<usize> = None;

    for i in 0..parsed.len() {
        if !matches(&parsed[i], r) {
            continue;
        }

        if let Some(j) = best {
            let ordering = compare_versions(&parsed[i], &parsed[j]);
            if (high && ordering == Ordering::Greater)
                || (!high && ordering == Ordering::Less)
            {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }

    Ok(best.map(|i| candidates[i].clone()))
}

fn core_string(c: (u64, u64, u64)) -> String {
    format!("{}.{}.{}", c.0, c.1, c.2)
}

fn version_string(v: &Version) -> String {
    let mut s = core_string(core(v));
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

fn arm_bounds(
    arm: &Arm,
) -> (Option<(Version, bool)>, Option<(Version, bool)>) {
    let mut lower: Option<(Version, bool)> = None;
    let mut upper: Option<(Version, bool)> = None;

    for cmp in &arm.cmps {
        match cmp.op.as_str() {
            "=" => {
                lower = Some((cmp.v.clone(), true));
                upper = Some((cmp.v.clone(), true));
            }
            ">" | ">=" => {
                let inclusive = cmp.op == ">=";
                let replace = lower
                    .as_ref()
                    .map(|(v, old_inclusive)| {
                        let ordering = compare_versions(&cmp.v, v);
                        ordering == Ordering::Greater
                            || (ordering == Ordering::Equal && !inclusive && *old_inclusive)
                    })
                    .unwrap_or(true);

                if replace {
                    lower = Some((cmp.v.clone(), inclusive));
                }
            }
            "<" | "<=" => {
                let inclusive = cmp.op == "<=";
                let replace = upper
                    .as_ref()
                    .map(|(v, old_inclusive)| {
                        let ordering = compare_versions(&cmp.v, v);
                        ordering == Ordering::Less
                            || (ordering == Ordering::Equal && !inclusive && *old_inclusive)
                    })
                    .unwrap_or(true);

                if replace {
                    upper = Some((cmp.v.clone(), inclusive));
                }
            }
            _ => {}
        }
    }

    (lower, upper)
}

fn render_bound(bound: &Option<(Version, bool)>, lower: bool) -> String {
    match bound {
        None => "*".into(),
        Some((v, inclusive)) => {
            let op = if lower {
                if *inclusive { ">=" } else { ">" }
            } else if *inclusive {
                "<="
            } else {
                "<"
            };
            format!("{op}{}", version_string(v))
        }
    }
}

pub fn canonical(r: &Range) -> String {
    let mut rows = Vec::new();

    for arm in &r.arms {
        let (lower, upper) = arm_bounds(arm);
        let mut anchors = arm.anchors.clone();
        anchors.sort();
        anchors.dedup();

        let anchor_text = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors
                .iter()
                .map(|c| core_string(*c))
                .collect::<Vec<_>>()
                .join(",")
        };

        let row = format!(
            "[{};{};pre={}]",
            render_bound(&lower, true),
            render_bound(&upper, false),
            anchor_text
        );

        if !rows.contains(&row) {
            rows.push(row);
        }
    }

    rows.sort();

    if rows.is_empty() {
        "EMPTY".into()
    } else {
        rows.join(" || ")
    }
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for arm_a in &a.arms {
        for arm_b in &b.arms {
            let (a_lower, a_upper) = arm_bounds(arm_a);
            let (b_lower, b_upper) = arm_bounds(arm_b);

            let lower = match (&a_lower, &b_lower) {
                (Some(x), Some(y)) => Some(if compare_versions(&x.0, &y.0)
                    == Ordering::Greater
                {
                    x.clone()
                } else {
                    y.clone()
                }),
                (Some(x), None) | (None, Some(x)) => Some(x.clone()),
                (None, None) => None,
            };

            let upper = match (&a_upper, &b_upper) {
                (Some(x), Some(y)) => Some(if compare_versions(&x.0, &y.0) == Ordering::Less {
                    x.clone()
                } else {
                    y.clone()
                }),
                (Some(x), None) | (None, Some(x)) => Some(x.clone()),
                (None, None) => None,
            };

            if let (Some((lo, lo_inc)), Some((hi, hi_inc))) = (&lower, &upper) {
                let ordering = compare_versions(lo, hi);
                if ordering == Ordering::Greater
                    || (ordering == Ordering::Equal && !(*lo_inc && *hi_inc))
                {
                    continue;
                }
            }

            if include_prerelease {
                return true;
            }

            let shared_anchor = arm_a
                .anchors
                .iter()
                .any(|anchor| arm_b.anchors.contains(anchor));

            if shared_anchor {
                for anchor in &arm_a.anchors {
                    if !arm_b.anchors.contains(anchor) {
                        continue;
                    }
                    let candidate = mkver(anchor.0, anchor.1, anchor.2, zero_pre(), Vec::new());
                    if lower.as_ref().map(|(v, inc)| {
                        let ordering = compare_versions(&candidate, v);
                        ordering == Ordering::Greater || (*inc && ordering == Ordering::Equal)
                    }).unwrap_or(true)
                        && upper.as_ref().map(|(v, inc)| {
                            let ordering = compare_versions(&candidate, v);
                            ordering == Ordering::Less || (*inc && ordering == Ordering::Equal)
                        }).unwrap_or(true)
                    {
                        return true;
                    }
                }
            }

            let mut candidate = lower
                .as_ref()
                .map(|bound| bound.0.clone())
                .unwrap_or_else(|| mkver(0, 0, 0, Vec::new(), Vec::new()));

            if let Some((_, false)) = &lower {
                if candidate.pre.is_empty() {
                    candidate.pre = zero_pre();
                }
            }

            if candidate.pre.is_empty() {
                let lower_satisfied = lower
                    .as_ref()
                    .map(|(v, inclusive)| {
                        let ordering = compare_versions(&candidate, v);
                        ordering == Ordering::Greater
                            || (*inclusive && ordering == Ordering::Equal)
                    })
                    .unwrap_or(true);

                if !lower_satisfied {
                    let (major, minor, patch) = core(&candidate);
                    candidate = if patch < u64::MAX {
                        mkver(major, minor, patch + 1, Vec::new(), Vec::new())
                    } else if minor < u64::MAX {
                        mkver(major, minor + 1, 0, Vec::new(), Vec::new())
                    } else if major < u64::MAX {
                        mkver(major + 1, 0, 0, Vec::new(), Vec::new())
                    } else {
                        continue;
                    };
                }
            }

            if upper
                .as_ref()
                .map(|(v, inclusive)| {
                    let ordering = compare_versions(&candidate, v);
                    ordering == Ordering::Less || (*inclusive && ordering == Ordering::Equal)
                })
                .unwrap_or(true)
            {
                return true;
            }
        }
    }

    false
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

fn expr_valid(
    expr: &SetExpr,
    package: &str,
    label: &str,
    path: &mut Vec<usize>,
) -> Result<(), SelectError> {
    match expr {
        SetExpr::Range(s) => parse_range(s).map(|_| ()).map_err(|error| {
            SelectError::InvalidRange {
                package: package.into(),
                label: label.into(),
                path: path.clone(),
                error,
            }
        }),
        SetExpr::Union(children) | SetExpr::Intersection(children) => {
            for (index, child) in children.iter().enumerate() {
                path.push(index);
                expr_valid(child, package, label, path)?;
                path.pop();
            }
            Ok(())
        }
    }
}

fn eval(expr: &SetExpr, v: &Version, include: bool) -> bool {
    match expr {
        SetExpr::Range(s) => {
            let range = parse_range(s).expect("range was validated");
            matches_with(v, &range, include)
        }
        SetExpr::Union(children) => children.iter().any(|child| eval(child, v, include)),
        SetExpr::Intersection(children) => {
            children.iter().all(|child| eval(child, v, include))
        }
    }
}

fn subset_satisfiable(
    indices: &[usize],
    constraints: &[Constraint],
    versions: &[Version],
    include: bool,
) -> bool {
    versions.iter().any(|v| {
        indices
            .iter()
            .all(|i| eval(&constraints[*i].expr, v, include))
    })
}

fn choose_conflict(
    constraints: &[Constraint],
    versions: &[Version],
    include: bool,
) -> Vec<String> {
    if versions.is_empty() {
        return Vec::new();
    }

    let n = constraints.len();

    for size in 1..=n {
        let mut answer: Option<Vec<String>> = None;

        fn walk(
            start: usize,
            size: usize,
            current: &mut Vec<usize>,
            n: usize,
            constraints: &[Constraint],
            versions: &[Version],
            include: bool,
            answer: &mut Option<Vec<String>>,
        ) {
            if current.len() == size {
                if !subset_satisfiable(current, constraints, versions, include) {
                    let mut labels: Vec<String> = current
                        .iter()
                        .map(|i| constraints[*i].label.clone())
                        .collect();
                    labels.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

                    if answer
                        .as_ref()
                        .map(|existing| labels.as_slice() < existing.as_slice())
                        .unwrap_or(true)
                    {
                        *answer = Some(labels);
                    }
                }
                return;
            }

            for i in start..n {
                current.push(i);
                walk(
                    i + 1,
                    size,
                    current,
                    n,
                    constraints,
                    versions,
                    include,
                    answer,
                );
                current.pop();
            }
        }

        walk(
            0,
            size,
            &mut Vec::new(),
            n,
            constraints,
            versions,
            include,
            &mut answer,
        );

        if let Some(labels) = answer {
            return labels;
        }
    }

    Vec::new()
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

    let mut all_versions = Vec::new();

    for package in packages {
        let mut versions = Vec::new();

        for (index, s) in package.candidates.iter().enumerate() {
            versions.push(parse_version(s).map_err(|error| SelectError::InvalidCandidate {
                package: package.name.clone(),
                index,
                error,
            })?);
        }

        all_versions.push(versions);
    }

    for package in packages {
        for constraint in &package.constraints {
            expr_valid(
                &constraint.expr,
                &package.name,
                &constraint.label,
                &mut Vec::new(),
            )?;
        }
    }

    let mut result = Vec::new();

    for (package_index, package) in packages.iter().enumerate() {
        let versions = &all_versions[package_index];
        let mut best: Option<usize> = None;

        for i in 0..versions.len() {
            if !package
                .constraints
                .iter()
                .all(|constraint| eval(&constraint.expr, &versions[i], include_prerelease))
            {
                continue;
            }

            if let Some(j) = best {
                let stable_preference = matches!(preference, Preference::StableFirst)
                    && versions[i].pre.is_empty() != versions[j].pre.is_empty();

                let better = if stable_preference {
                    versions[i].pre.is_empty()
                } else {
                    match preference {
                        Preference::Lowest => {
                            compare_versions(&versions[i], &versions[j]) == Ordering::Less
                        }
                        Preference::Highest | Preference::StableFirst => {
                            compare_versions(&versions[i], &versions[j]) == Ordering::Greater
                        }
                    }
                };

                if better {
                    best = Some(i);
                }
            } else {
                best = Some(i);
            }
        }

        if let Some(index) = best {
            result.push(Selection {
                package: package.name.clone(),
                version: package.candidates[index].clone(),
            });
        } else {
            let labels = choose_conflict(
                &package.constraints,
                versions,
                include_prerelease,
            );
            return Err(SelectError::NoSolution {
                package: package.name.clone(),
                labels,
            });
        }
    }

    Ok(result)
}