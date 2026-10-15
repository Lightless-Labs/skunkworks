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

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
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
        return Err(match bytes.get(start).copied() {
            None | Some(b'.' | b'-' | b'+') => VersionError::MissingCore { at: start },
            _ => VersionError::InvalidChar { at: start },
        });
    }

    let digits = &s[start..*pos];
    if digits.len() > 1 && digits.as_bytes()[0] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    digits
        .parse::<u64>()
        .map_err(|_| VersionError::Overflow { at: start })
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    pre: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut p = start;

    loop {
        let id_start = p;
        while p < end && bytes[p] != b'.' {
            let b = bytes[p];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: p });
            }
            p += 1;
        }

        if p == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }

        let id = &s[id_start..p];
        if pre && id.bytes().all(is_digit) && id.len() > 1 && id.as_bytes()[0] == b'0' {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        out.push(id.to_string());

        if p == end {
            break;
        }
        p += 1;
        if p == end {
            return Err(VersionError::EmptyIdentifier { at: p });
        }
    }

    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let mut p = 0;
    let major = parse_core(s, &mut p)?;
    if p >= s.len() || s.as_bytes()[p] != b'.' {
        return Err(match s.as_bytes().get(p).copied() {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: p },
            _ => VersionError::InvalidChar { at: p },
        });
    }

    p += 1;
    let minor = parse_core(s, &mut p)?;
    if p >= s.len() || s.as_bytes()[p] != b'.' {
        return Err(match s.as_bytes().get(p).copied() {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: p },
            _ => VersionError::InvalidChar { at: p },
        });
    }

    p += 1;
    let patch = parse_core(s, &mut p)?;
    let bytes = s.as_bytes();
    let mut pre = Vec::new();
    let mut build = Vec::new();

    if p < bytes.len() && bytes[p] == b'-' {
        let pre_start = p + 1;
        let plus = bytes[pre_start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|x| pre_start + x);
        let pre_end = plus.unwrap_or(bytes.len());
        pre = parse_identifiers(s, pre_start, pre_end, true)?;
        p = pre_end;
    }

    if p < bytes.len() && bytes[p] == b'+' {
        build = parse_identifiers(s, p + 1, bytes.len(), false)?;
        p = bytes.len();
    }

    if p != bytes.len() {
        return Err(VersionError::InvalidChar { at: p });
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
    if a.is_empty() && b.is_empty() {
        return Ordering::Equal;
    }
    if a.is_empty() {
        return Ordering::Greater;
    }
    if b.is_empty() {
        return Ordering::Less;
    }

    for (x, y) in a.iter().zip(b) {
        let xn = numeric_id(x);
        let yn = numeric_id(y);
        let ord = match (xn, yn) {
            (true, true) => x
                .len()
                .cmp(&y.len())
                .then_with(|| x.as_bytes().cmp(y.as_bytes())),
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
struct Cmp {
    op: Op,
    v: Version,
    anchor: bool,
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
    cmps: Vec<Cmp>,
    anchors: Vec<(u64, u64, u64)>,
}

fn mkver(a: u64, b: u64, c: u64, pre: Vec<String>) -> Version {
    Version {
        major: a,
        minor: b,
        patch: c,
        pre,
        build: Vec::new(),
    }
}

fn endpoint(v: Version, op: Op, anchor: bool) -> Cmp {
    Cmp { op, v, anchor }
}

fn ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokens_with_offsets(s: &str, base: usize) -> Vec<(String, usize)> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut p = 0;

    while p < bytes.len() {
        while p < bytes.len() && ws(bytes[p]) {
            p += 1;
        }
        if p == bytes.len() {
            break;
        }

        let start = p;
        while p < bytes.len() && !ws(bytes[p]) {
            p += 1;
        }
        out.push((s[start..p].to_string(), base + start));
    }

    out
}

fn operand_version(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

#[derive(Clone, Debug)]
struct Partial {
    nums: Vec<u64>,
    full: Option<Version>,
}

fn parse_partial_operand(s: &str, at: usize) -> Result<Partial, RangeError> {
    let has_suffix = s.contains('-') || s.contains('+');
    let fields: Vec<&str> = s.split('.').collect();
    let has_wild = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));

    if has_suffix || (fields.len() >= 3 && !has_wild) {
        return Ok(Partial {
            nums: Vec::new(),
            full: Some(operand_version(s, at)?),
        });
    }

    if fields.len() > 3 {
        return Err(RangeError::Syntax { at });
    }

    let mut nums = Vec::new();
    let mut wildcard_seen = false;
    for field in fields {
        if matches!(field, "*" | "x" | "X") {
            wildcard_seen = true;
        } else {
            if wildcard_seen
                || field.is_empty()
                || !field.bytes().all(is_digit)
                || (field.len() > 1 && field.as_bytes()[0] == b'0')
            {
                return Err(RangeError::Syntax { at });
            }
            match field.parse::<u64>() {
                Ok(n) => nums.push(n),
                Err(_) => return Err(RangeError::Syntax { at }),
            }
        }
    }

    Ok(Partial { nums, full: None })
}

fn padded(p: &Partial) -> Version {
    mkver(
        *p.nums.first().unwrap_or(&0),
        *p.nums.get(1).unwrap_or(&0),
        *p.nums.get(2).unwrap_or(&0),
        Vec::new(),
    )
}

fn increment_upper(v: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let (mut major, mut minor, mut patch) = core(v);
    let target = match field {
        0 => &mut major,
        1 => &mut minor,
        _ => &mut patch,
    };
    *target = target
        .checked_add(1)
        .ok_or(RangeError::Syntax { at })?;

    if field == 0 {
        minor = 0;
        patch = 0;
    } else if field == 1 {
        patch = 0;
    }

    Ok(mkver(major, minor, patch, vec!["0".into()]))
}

fn partial_expansion(p: &Partial, at: usize) -> Result<Vec<Cmp>, RangeError> {
    if p.nums.is_empty() {
        return Ok(Vec::new());
    }

    if let Some(v) = &p.full {
        return Ok(vec![endpoint(v.clone(), Op::Eq, !v.pre.is_empty())]);
    }

    let lower = padded(p);
    if p.nums.len() == 3 {
        return Ok(vec![endpoint(lower, Op::Eq, false)]);
    }

    let upper = increment_upper(&lower, p.nums.len() - 1, at)?;
    Ok(vec![
        endpoint(lower, Op::Ge, false),
        endpoint(upper, Op::Lt, false),
    ])
}

fn parse_general_token(tok: &str, at: usize) -> Result<Vec<Cmp>, RangeError> {
    if tok.starts_with('^') || tok.starts_with('~') {
        let op = tok.as_bytes()[0];
        let operand = &tok[1..];
        let p = parse_partial_operand(operand, at + 1)?;
        if p.nums.is_empty() && p.full.is_none() {
            return Err(RangeError::Syntax { at });
        }

        let lower = p.full.clone().unwrap_or_else(|| padded(&p));
        let k = if p.full.is_some() { 3 } else { p.nums.len() };
        let field = if op == b'^' {
            if lower.major > 0 || k == 1 {
                0
            } else if lower.minor > 0 || k == 2 {
                1
            } else {
                2
            }
        } else if k == 1 {
            0
        } else {
            1
        };

        let upper = increment_upper(&lower, field, at + 1)?;
        return Ok(vec![
            endpoint(
                lower.clone(),
                Op::Ge,
                p.full.as_ref().map_or(false, |v| !v.pre.is_empty()),
            ),
            endpoint(upper, Op::Lt, false),
        ]);
    }

    if tok.starts_with('<') || tok.starts_with('>') || tok.starts_with('=') {
        let (op, n) = if tok.starts_with("<=") {
            (Op::Le, 2)
        } else if tok.starts_with(">=") {
            (Op::Ge, 2)
        } else {
            (
                match tok.as_bytes()[0] {
                    b'<' => Op::Lt,
                    b'>' => Op::Gt,
                    _ => Op::Eq,
                },
                1,
            )
        };

        if n == tok.len() || !tok.as_bytes()[n].is_ascii_digit() {
            return Err(RangeError::Syntax { at });
        }

        let v = operand_version(&tok[n..], at + n)?;
        return Ok(vec![endpoint(v.clone(), op, !v.pre.is_empty())]);
    }

    if !(tok.as_bytes()[0].is_ascii_digit() || matches!(tok, "*" | "x" | "X")) {
        return Err(RangeError::Syntax { at });
    }

    let p = parse_partial_operand(tok, at)?;
    partial_expansion(&p, at)
}

fn parse_hyphen(tokens: &[(String, usize)]) -> Result<Arm, RangeError> {
    let dashes: Vec<usize> = tokens
        .iter()
        .filter(|(token, _)| token == "-")
        .map(|(_, at)| *at)
        .collect();

    if !dashes.is_empty() && (tokens.len() != 3 || tokens[1].0 != "-") {
        return Err(RangeError::Syntax { at: dashes[0] });
    }
    if tokens.len() != 3 || tokens[1].0 != "-" {
        return Err(RangeError::Syntax { at: tokens[0].1 });
    }

    let (lower_token, lower_at) = (&tokens[0].0, tokens[0].1);
    let (upper_token, upper_at) = (&tokens[2].0, tokens[2].1);
    let lower_partial = parse_partial_operand(lower_token, lower_at)?;
    let upper_partial = parse_partial_operand(upper_token, upper_at)?;

    if lower_partial.nums.is_empty() && lower_partial.full.is_none() {
        return Err(RangeError::Syntax { at: lower_at });
    }
    if upper_partial.nums.is_empty() && upper_partial.full.is_none() {
        return Err(RangeError::Syntax { at: upper_at });
    }

    let mut cmps = Vec::new();
    let lower = lower_partial
        .full
        .clone()
        .unwrap_or_else(|| padded(&lower_partial));
    cmps.push(endpoint(
        lower,
        Op::Ge,
        lower_partial
            .full
            .as_ref()
            .map_or(false, |v| !v.pre.is_empty()),
    ));

    if let Some(v) = upper_partial.full {
        cmps.push(endpoint(v.clone(), Op::Le, !v.pre.is_empty()));
    } else if upper_partial.nums.len() == 3 {
        cmps.push(endpoint(padded(&upper_partial), Op::Le, false));
    } else {
        let upper = increment_upper(
            &padded(&upper_partial),
            upper_partial.nums.len() - 1,
            upper_at,
        )?;
        cmps.push(endpoint(upper, Op::Lt, false));
    }

    let anchors = cmps
        .iter()
        .filter(|c| c.anchor)
        .map(|c| core(&c.v))
        .collect();
    Ok(Arm { cmps, anchors })
}

fn parse_arm(s: &str, base: usize) -> Result<Arm, RangeError> {
    let tokens = tokens_with_offsets(s, base);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: base });
    }

    if tokens.iter().any(|(token, _)| token == "-") {
        return parse_hyphen(&tokens);
    }

    let mut cmps = Vec::new();
    for (token, at) in tokens {
        cmps.extend(parse_general_token(&token, at)?);
    }
    let anchors = cmps
        .iter()
        .filter(|c| c.anchor)
        .map(|c| core(&c.v))
        .collect();

    Ok(Arm { cmps, anchors })
}

fn split_ranges(s: &str) -> Result<Vec<(String, usize)>, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = Vec::new();
    let mut start = 0;
    let mut p = 0;
    let mut had_separator = false;

    while p < bytes.len() {
        if p + 1 < bytes.len() && bytes[p] == b'|' && bytes[p + 1] == b'|' {
            let left = &s[start..p];
            if tokens_with_offsets(left, start).is_empty() {
                return Err(RangeError::Empty {
                    at: if had_separator { start } else { 0 },
                });
            }
            arms.push((left.to_string(), start));
            p += 2;
            while p < bytes.len() && ws(bytes[p]) {
                p += 1;
            }
            start = p;
            had_separator = true;
        } else {
            p += 1;
        }
    }

    let tail = &s[start..];
    if tokens_with_offsets(tail, start).is_empty() {
        if had_separator {
            return Err(RangeError::Empty { at: s.len() });
        }
        if tokens_with_offsets(s, 0).is_empty() {
            return Err(RangeError::Empty { at: 0 });
        }
    }

    arms.push((tail.to_string(), start));
    Ok(arms)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let mut arms = Vec::new();
    for (arm, at) in split_ranges(s)? {
        arms.push(parse_arm(&arm, at)?);
    }
    Ok(Range { arms })
}

fn arm_matches(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !include_prerelease
        && !v.pre.is_empty()
        && !arm.anchors.contains(&core(v))
    {
        return false;
    }

    arm.cmps.iter().all(|c| {
        let ord = compare_versions(v, &c.v);
        match c.op {
            Op::Eq => ord == Ordering::Equal,
            Op::Lt => ord == Ordering::Less,
            Op::Le => ord != Ordering::Greater,
            Op::Gt => ord == Ordering::Greater,
            Op::Ge => ord != Ordering::Less,
        }
    })
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
    let mut parsed = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        parsed.push(
            parse_version(candidate)
                .map_err(|error| CandidateError { index, error })?,
        );
    }

    let mut best: Option<usize> = None;
    for i in 0..parsed.len() {
        if !matches(&parsed[i], r) {
            continue;
        }
        if let Some(j) = best {
            let ord = compare_versions(&parsed[i], &parsed[j]);
            if (maximum && ord == Ordering::Greater)
                || (!maximum && ord == Ordering::Less)
            {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }

    Ok(best.map(|index| candidates[index].clone()))
}

fn strip_build(v: &Version) -> String {
    let mut out = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        out.push('-');
        out.push_str(&v.pre.join("."));
    }
    out
}

pub fn canonical(r: &Range) -> String {
    let mut rows = Vec::new();

    for arm in &r.arms {
        let mut lower: Option<(Version, bool)> = None;
        let mut upper: Option<(Version, bool)> = None;
        let mut equality: Option<Version> = None;

        for cmp in &arm.cmps {
            match cmp.op {
                Op::Eq => equality = Some(cmp.v.clone()),
                Op::Gt | Op::Ge => {
                    let inclusive = cmp.op == Op::Ge;
                    let replace = lower.as_ref().map_or(true, |(v, old_inclusive)| {
                        let ord = compare_versions(&cmp.v, v);
                        ord == Ordering::Greater
                            || (ord == Ordering::Equal && !inclusive && *old_inclusive)
                    });
                    if replace {
                        lower = Some((cmp.v.clone(), inclusive));
                    }
                }
                Op::Lt | Op::Le => {
                    let inclusive = cmp.op == Op::Le;
                    let replace = upper.as_ref().map_or(true, |(v, old_inclusive)| {
                        let ord = compare_versions(&cmp.v, v);
                        ord == Ordering::Less
                            || (ord == Ordering::Equal && !inclusive && *old_inclusive)
                    });
                    if replace {
                        upper = Some((cmp.v.clone(), inclusive));
                    }
                }
            }
        }

        if let Some(v) = equality {
            lower = Some((v.clone(), true));
            upper = Some((v, true));
        }

        let lower_text = lower.map_or_else(
            || "*".to_string(),
            |(v, inclusive)| {
                format!(
                    "{}{}",
                    if inclusive { ">=" } else { ">" },
                    strip_build(&v)
                )
            },
        );
        let upper_text = upper.map_or_else(
            || "*".to_string(),
            |(v, inclusive)| {
                format!(
                    "{}{}",
                    if inclusive { "<=" } else { "<" },
                    strip_build(&v)
                )
            },
        );

        let mut anchors = arm.anchors.clone();
        anchors.sort();
        anchors.dedup();
        let anchors_text = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors
                .iter()
                .map(|(a, b, c)| format!("{a}.{b}.{c}"))
                .collect::<Vec<_>>()
                .join(",")
        };

        rows.push(format!(
            "[{lower_text};{upper_text};pre={anchors_text}]"
        ));
    }

    rows.sort();
    rows.dedup();
    if rows.is_empty() {
        "EMPTY".to_string()
    } else {
        rows.join(" || ")
    }
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for arm_a in &a.arms {
        for arm_b in &b.arms {
            if include_prerelease {
                let mut candidates = vec![mkver(0, 0, 0, Vec::new())];
                for cmp in arm_a.cmps.iter().chain(arm_b.cmps.iter()) {
                    candidates.push(cmp.v.clone());
                    let (major, minor, patch) = core(&cmp.v);
                    candidates.push(mkver(major, minor, patch, Vec::new()));
                }
                if candidates.iter().any(|v| {
                    v.pre.is_empty()
                        && arm_matches(v, arm_a, true)
                        && arm_matches(v, arm_b, true)
                }) {
                    return true;
                }
            } else {
                let common_anchors = arm_a
                    .anchors
                    .iter()
                    .copied()
                    .filter(|tuple| arm_b.anchors.contains(tuple));
                for (major, minor, patch) in common_anchors {
                    let v = mkver(major, minor, patch, vec!["0".into()]);
                    if arm_matches(&v, arm_a, true) && arm_matches(&v, arm_b, true) {
                        return true;
                    }
                }

                let mut candidates = vec![mkver(0, 0, 0, Vec::new())];
                for cmp in arm_a.cmps.iter().chain(arm_b.cmps.iter()) {
                    candidates.push(cmp.v.clone());
                    let (major, minor, patch) = core(&cmp.v);
                    candidates.push(mkver(major, minor, patch, Vec::new()));
                }
                if candidates.iter().any(|v| {
                    v.pre.is_empty()
                        && arm_matches(v, arm_a, true)
                        && arm_matches(v, arm_b, true)
                }) {
                    return true;
                }
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

fn validate_expr(
    expr: &SetExpr,
    path: &mut Vec<usize>,
    package: &str,
    label: &str,
    out: &mut Vec<Range>,
) -> Result<(), SelectError> {
    match expr {
        SetExpr::Range(s) => {
            out.push(parse_range(s).map_err(|error| SelectError::InvalidRange {
                package: package.into(),
                label: label.into(),
                path: path.clone(),
                error,
            })?);
        }
        SetExpr::Union(children) | SetExpr::Intersection(children) => {
            for (index, child) in children.iter().enumerate() {
                path.push(index);
                validate_expr(child, path, package, label, out)?;
                path.pop();
            }
        }
    }
    Ok(())
}

fn eval_expr<'a>(
    expr: &SetExpr,
    v: &Version,
    include: bool,
    ranges: &mut impl Iterator<Item = &'a Range>,
) -> bool {
    match expr {
        SetExpr::Range(_) => matches_with(v, ranges.next().unwrap(), include),
        SetExpr::Union(children) => {
            children.iter().any(|child| eval_expr(child, v, include, ranges))
        }
        SetExpr::Intersection(children) => children
            .iter()
            .all(|child| eval_expr(child, v, include, ranges)),
    }
}

fn accepted_subset(
    mask: u64,
    package: &Package,
    parsed: &[Version],
    include: bool,
    ranges: &[Vec<Range>],
) -> bool {
    parsed.iter().enumerate().any(|(candidate_index, version)| {
        package
            .constraints
            .iter()
            .enumerate()
            .all(|(constraint_index, constraint)| {
                mask & (1u64 << constraint_index) == 0
                    || eval_expr(
                        &constraint.expr,
                        version,
                        include,
                        &mut ranges[constraint_index].iter(),
                    )
            })
    })
}

fn minimum_conflict(
    package: &Package,
    parsed: &[Version],
    include: bool,
    ranges: &[Vec<Range>],
) -> Vec<String> {
    if parsed.is_empty() {
        return Vec::new();
    }

    let count = package.constraints.len();
    let mut best: Option<Vec<String>> = None;

    fn search(
        start: usize,
        need: usize,
        chosen: &mut Vec<usize>,
        package: &Package,
        parsed: &[Version],
        include: bool,
        ranges: &[Vec<Range>],
        best: &mut Option<Vec<String>>,
    ) {
        if need == 0 {
            let mut mask = 0u64;
            for &index in chosen.iter() {
                if index < 64 {
                    mask |= 1u64 << index;
                }
            }
            if !accepted_subset(mask, package, parsed, include, ranges) {
                let mut labels = chosen
                    .iter()
                    .map(|&index| package.constraints[index].label.clone())
                    .collect::<Vec<_>>();
                labels.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
                let replace = best.as_ref().map_or(true, |old| {
                    labels.len() < old.len()
                        || (labels.len() == old.len()
                            && labels
                                .iter()
                                .map(String::as_bytes)
                                .cmp(old.iter().map(String::as_bytes))
                                == Ordering::Less)
                });
                if replace {
                    *best = Some(labels);
                }
            }
            return;
        }

        if package.constraints.len().saturating_sub(start) < need {
            return;
        }
        for index in start..package.constraints.len() {
            chosen.push(index);
            search(
                index + 1,
                need - 1,
                chosen,
                package,
                parsed,
                include,
                ranges,
                best,
            );
            chosen.pop();
        }
    }

    for size in 1..=count {
        let mut chosen = Vec::new();
        search(
            0,
            size,
            &mut chosen,
            package,
            parsed,
            include,
            ranges,
            &mut best,
        );
        if best.is_some() {
            break;
        }
    }

    best.unwrap_or_default()
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
        let mut parsed = Vec::new();
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

    let mut all_ranges = Vec::new();
    for package in packages {
        let mut package_ranges = Vec::new();
        for constraint in &package.constraints {
            let mut ranges = Vec::new();
            validate_expr(
                &constraint.expr,
                &mut Vec::new(),
                &package.name,
                &constraint.label,
                &mut ranges,
            )?;
            package_ranges.push(ranges);
        }
        all_ranges.push(package_ranges);
    }

    let mut selections = Vec::new();
    for (package_index, package) in packages.iter().enumerate() {
        let mut accepted = Vec::new();

        for (candidate_index, version) in parsed_candidates[package_index].iter().enumerate() {
            let qualifies = package.constraints.iter().enumerate().all(
                |(constraint_index, constraint)| {
                    eval_expr(
                        &constraint.expr,
                        version,
                        include_prerelease,
                        &mut all_ranges[package_index][constraint_index].iter(),
                    )
                },
            );
            if qualifies {
                accepted.push(candidate_index);
            }
        }

        if accepted.is_empty() {
            return Err(SelectError::NoSolution {
                package: package.name.clone(),
                labels: minimum_conflict(
                    package,
                    &parsed_candidates[package_index],
                    include_prerelease,
                    &all_ranges[package_index],
                ),
            });
        }

        let is_better = |i: usize, j: usize| {
            let a = &parsed_candidates[package_index][i];
            let b = &parsed_candidates[package_index][j];
            match preference {
                Preference::Highest => compare_versions(a, b) == Ordering::Greater,
                Preference::Lowest => compare_versions(a, b) == Ordering::Less,
                Preference::StableFirst => match (a.pre.is_empty(), b.pre.is_empty()) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => compare_versions(a, b) == Ordering::Greater,
                },
            }
        };

        let mut best = accepted[0];
        for &index in &accepted[1..] {
            if is_better(index, best) {
                best = index;
            }
        }
        selections.push(Selection {
            package: package.name.clone(),
            version: package.candidates[best].clone(),
        });
    }

    Ok(selections)
}

fn main() {}