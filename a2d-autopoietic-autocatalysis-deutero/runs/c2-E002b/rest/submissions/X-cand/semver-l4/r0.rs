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

fn parse_core_field(bytes: &[u8], pos: &mut usize) -> Result<u64, VersionError> {
    let start = *pos;
    while *pos < bytes.len() && is_digit(bytes[*pos]) {
        *pos += 1;
    }
    if *pos == start {
        return Err(VersionError::MissingCore { at: start });
    }
    if *pos - start > 1 && bytes[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    let mut n = 0u64;
    for &b in &bytes[start..*pos] {
        n = n
            .checked_mul(10)
            .and_then(|x| x.checked_add((b - b'0') as u64))
            .ok_or(VersionError::Overflow { at: start })?;
    }
    Ok(n)
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    prerelease: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut p = start;
    loop {
        let ident_start = p;
        while p < end && bytes[p] != b'.' {
            let b = bytes[p];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: p });
            }
            p += 1;
        }
        if p == ident_start {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }
        if prerelease
            && bytes[ident_start..p].iter().all(|b| b.is_ascii_digit())
            && p - ident_start > 1
            && bytes[ident_start] == b'0'
        {
            return Err(VersionError::LeadingZero { at: ident_start });
        }
        out.push(s[ident_start..p].to_string());
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
    let bytes = s.as_bytes();
    let mut p = 0;
    let major = parse_core_field(bytes, &mut p)?;
    if p >= bytes.len() || bytes[p] != b'.' {
        return Err(match bytes.get(p) {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: p },
            _ => VersionError::InvalidChar { at: p },
        });
    }
    p += 1;
    let minor = parse_core_field(bytes, &mut p)?;
    if p >= bytes.len() || bytes[p] != b'.' {
        return Err(match bytes.get(p) {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: p },
            _ => VersionError::InvalidChar { at: p },
        });
    }
    p += 1;
    let patch = parse_core_field(bytes, &mut p)?;
    if p < bytes.len() && bytes[p] != b'-' && bytes[p] != b'+' {
        return Err(VersionError::InvalidChar { at: p });
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();
    if p < bytes.len() && bytes[p] == b'-' {
        let start = p + 1;
        let end = s[start..]
            .find('+')
            .map(|i| start + i)
            .unwrap_or(bytes.len());
        pre = parse_identifiers(s, start, end, true)?;
        p = end;
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

fn numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.as_bytes().iter().all(|b| b.is_ascii_digit())
}

fn compare_identifier(a: &str, b: &str) -> Ordering {
    let an = numeric_identifier(a);
    let bn = numeric_identifier(b);
    match (an, bn) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => {
            a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
        }
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
                for (x, y) in a.pre.iter().zip(&b.pre) {
                    let c = compare_identifier(x, y);
                    if c != Ordering::Equal {
                        return c;
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
struct Comparator {
    op: Op,
    version: Version,
    anchor: bool,
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
pub struct Range {
    arms: Vec<Arm>,
}

fn tuple(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn version_at_tuple(t: (u64, u64, u64), pre: Vec<String>) -> Version {
    Version {
        major: t.0,
        minor: t.1,
        patch: t.2,
        pre,
        build: Vec::new(),
    }
}

fn split_fields(s: &str) -> Vec<&str> {
    s.split('.').collect()
}

#[derive(Clone, Debug)]
struct Partial {
    version: Version,
    numeric: usize,
    full: bool,
}

fn parse_operand(s: &str, at: usize) -> Result<Partial, RangeError> {
    if s.is_empty() {
        return Err(RangeError::Syntax { at });
    }
    if s.contains('-') || s.contains('+') {
        return parse_version(s)
            .map(|v| Partial {
                version: v,
                numeric: 3,
                full: true,
            })
            .map_err(|error| RangeError::Version { at, error });
    }
    let fields = split_fields(s);
    let has_wild = fields.iter().any(|x| *x == "*" || *x == "x" || *x == "X");
    if fields.len() >= 3 && !has_wild {
        return parse_version(s)
            .map(|v| Partial {
                version: v,
                numeric: 3,
                full: true,
            })
            .map_err(|error| RangeError::Version { at, error });
    }
    if fields.len() > 3 || fields.iter().any(|x| x.is_empty()) {
        return Err(RangeError::Syntax { at });
    }
    let mut nums = Vec::new();
    let mut seen_wild = false;
    for field in fields {
        if field == "*" || field == "x" || field == "X" {
            seen_wild = true;
        } else {
            if seen_wild
                || !field.as_bytes().iter().all(|b| b.is_ascii_digit())
                || (field.len() > 1 && field.starts_with('0'))
                || field.parse::<u64>().is_err()
            {
                return Err(RangeError::Syntax { at });
            }
            nums.push(field.parse::<u64>().unwrap());
        }
    }
    let k = nums.len();
    let mut values = [0u64; 3];
    for (i, n) in nums.iter().enumerate() {
        values[i] = *n;
    }
    Ok(Partial {
        version: version_at_tuple((values[0], values[1], values[2]), Vec::new()),
        numeric: k,
        full: false,
    })
}

fn bump_tuple(
    v: &Version,
    field: usize,
    at: usize,
) -> Result<(u64, u64, u64), RangeError> {
    let mut t = tuple(v);
    let slot = match field {
        0 => &mut t.0,
        1 => &mut t.1,
        _ => &mut t.2,
    };
    *slot = slot.checked_add(1).ok_or(RangeError::Syntax { at })?;
    if field <= 1 {
        t.2 = 0;
    }
    if field == 0 {
        t.1 = 0;
    }
    Ok(t)
}

fn add_cmp(arm: &mut Arm, op: Op, version: Version, anchor: bool) {
    if anchor && !version.pre.is_empty() {
        arm.anchors.push(tuple(&version));
    }
    arm.comparators.push(Comparator {
        op,
        version,
        anchor,
    });
}

fn apply_partial(
    arm: &mut Arm,
    p: Partial,
    kind: char,
    at: usize,
) -> Result<(), RangeError> {
    if kind == '^' || kind == '~' {
        if p.numeric == 0 {
            return Err(RangeError::Syntax { at });
        }
        let lower = p.version.clone();
        add_cmp(arm, Op::Ge, lower.clone(), true);
        let field = if kind == '^' {
            if lower.major > 0 || p.numeric == 1 {
                0
            } else if lower.minor > 0 || p.numeric == 2 {
                1
            } else {
                2
            }
        } else if p.numeric == 1 {
            0
        } else {
            1
        };
        let upper = bump_tuple(&lower, field, at)?;
        add_cmp(
            arm,
            Op::Lt,
            version_at_tuple(upper, vec!["0".to_string()]),
            false,
        );
        return Ok(());
    }

    if p.full {
        add_cmp(arm, Op::Eq, p.version.clone(), true);
    } else if p.numeric == 1 || p.numeric == 2 {
        add_cmp(arm, Op::Ge, p.version.clone(), true);
        let field = if p.numeric == 1 { 0 } else { 1 };
        let upper = bump_tuple(&p.version, field, at)?;
        add_cmp(
            arm,
            Op::Lt,
            version_at_tuple(upper, vec!["0".to_string()]),
            false,
        );
    }
    Ok(())
}

fn whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokens_with_offsets(s: &str, base: usize) -> Vec<(&str, usize)> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut p = 0;
    while p < bytes.len() {
        while p < bytes.len() && whitespace(bytes[p]) {
            p += 1;
        }
        if p == bytes.len() {
            break;
        }
        let start = p;
        while p < bytes.len() && !whitespace(bytes[p]) {
            p += 1;
        }
        out.push((&s[start..p], base + start));
    }
    out
}

fn parse_comparator(token: &str, start: usize) -> Result<Comparator, RangeError> {
    let (op, n) = if token.starts_with(">=") {
        (Op::Ge, 2)
    } else if token.starts_with("<=") {
        (Op::Le, 2)
    } else if token.starts_with('>') {
        (Op::Gt, 1)
    } else if token.starts_with('<') {
        (Op::Lt, 1)
    } else if token.starts_with('=') {
        (Op::Eq, 1)
    } else {
        return Err(RangeError::Syntax { at: start });
    };
    let operand = &token[n..];
    let operand_at = start + n;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: start });
    }
    let version = parse_version(operand)
        .map_err(|error| RangeError::Version { at: operand_at, error })?;
    Ok(Comparator {
        op,
        version,
        anchor: true,
    })
}

fn parse_arm(s: &str, base: usize) -> Result<Arm, RangeError> {
    let toks = tokens_with_offsets(s, base);
    let mut arm = Arm {
        comparators: Vec::new(),
        anchors: Vec::new(),
    };
    if toks.is_empty() {
        return Err(RangeError::Empty { at: base });
    }
    if let Some((_, pos)) = toks.iter().find(|(t, _)| *t == "-") {
        if toks.len() != 3 || toks[1].0 != "-" {
            return Err(RangeError::Syntax { at: *pos });
        }
        let (lt, lp) = toks[0];
        let (ut, up) = toks[2];
        let lower = parse_operand(lt, lp)?;
        let upper = parse_operand(ut, up)?;
        if lower.numeric == 0 {
            return Err(RangeError::Syntax { at: lp });
        }
        if upper.numeric == 0 {
            return Err(RangeError::Syntax { at: up });
        }
        add_cmp(&mut arm, Op::Ge, lower.version.clone(), true);
        if upper.full {
            add_cmp(&mut arm, Op::Le, upper.version.clone(), true);
        } else {
            let field = if upper.numeric == 1 { 0 } else { 1 };
            let bound = bump_tuple(&upper.version, field, up)?;
            add_cmp(
                &mut arm,
                Op::Lt,
                version_at_tuple(bound, vec!["0".to_string()]),
                false,
            );
        }
        return Ok(arm);
    }

    for (token, start) in toks {
        if token.starts_with(">=")
            || token.starts_with("<=")
            || token.starts_with('>')
            || token.starts_with('<')
            || token.starts_with('=')
        {
            let cmp = parse_comparator(token, start)?;
            if cmp.anchor && !cmp.version.pre.is_empty() {
                arm.anchors.push(tuple(&cmp.version));
            }
            arm.comparators.push(cmp);
            continue;
        }
        let first = token.as_bytes()[0];
        if first == b'^' || first == b'~' {
            let p = parse_operand(&token[1..], start + 1)?;
            apply_partial(&mut arm, p, first as char, start + 1)?;
        } else if first.is_ascii_digit() || first == b'*' || first == b'x' || first == b'X' {
            let p = parse_operand(token, start)?;
            apply_partial(&mut arm, p, ' ', start)?;
        } else {
            return Err(RangeError::Syntax { at: start });
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = Vec::new();
    let mut segment_start = 0usize;
    let mut p = 0usize;
    let mut pending_empty = None;
    while p < bytes.len() {
        if p + 1 < bytes.len() && bytes[p] == b'|' && bytes[p + 1] == b'|' {
            let before = &s[segment_start..p];
            if tokens_with_offsets(before, segment_start).is_empty() {
                return Err(RangeError::Empty { at: p });
            }
            arms.push(parse_arm(before, segment_start)?);
            p += 2;
            while p < bytes.len() && whitespace(bytes[p]) {
                p += 1;
            }
            segment_start = p;
            pending_empty = Some(p);
        } else {
            p += 1;
        }
    }
    let last = &s[segment_start..];
    if tokens_with_offsets(last, segment_start).is_empty() {
        if arms.is_empty() {
            return Err(RangeError::Empty { at: 0 });
        }
        return Err(RangeError::Empty {
            at: pending_empty.unwrap_or(s.len()),
        });
    }
    arms.push(parse_arm(last, segment_start)?);
    Ok(Range { arms })
}

fn cmp_satisfies(v: &Version, c: &Comparator) -> bool {
    let o = compare_versions(v, &c.version);
    match c.op {
        Op::Eq => o == Ordering::Equal,
        Op::Lt => o == Ordering::Less,
        Op::Le => o != Ordering::Greater,
        Op::Gt => o == Ordering::Greater,
        Op::Ge => o != Ordering::Less,
    }
}

fn arm_matches(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !arm.comparators.iter().all(|c| cmp_satisfies(v, c)) {
        return false;
    }
    include_prerelease
        || v.pre.is_empty()
        || arm.anchors.iter().any(|a| *a == tuple(v))
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms
        .iter()
        .any(|arm| arm_matches(v, arm, include_prerelease))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn extrema(
    candidates: &[String],
    r: &Range,
    greatest: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, s) in candidates.iter().enumerate() {
        parsed.push(
            parse_version(s).map_err(|error| CandidateError { index, error })?,
        );
    }
    let mut best: Option<usize> = None;
    for (i, v) in parsed.iter().enumerate() {
        if !matches(v, r) {
            continue;
        }
        if let Some(j) = best {
            let o = compare_versions(v, &parsed[j]);
            if (greatest && o == Ordering::Greater) || (!greatest && o == Ordering::Less) {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

#[derive(Clone, Debug)]
enum Bound {
    Unbounded,
    Finite(Version, bool),
}

fn arm_bounds(arm: &Arm) -> (Bound, Bound) {
    let mut lower = Bound::Unbounded;
    let mut upper = Bound::Unbounded;
    for c in &arm.comparators {
        match c.op {
            Op::Eq => {
                lower = strongest_lower(lower, Bound::Finite(c.version.clone(), true));
                upper = strongest_upper(upper, Bound::Finite(c.version.clone(), true));
            }
            Op::Gt | Op::Ge => {
                lower = strongest_lower(
                    lower,
                    Bound::Finite(c.version.clone(), matches!(c.op, Op::Ge)),
                );
            }
            Op::Lt | Op::Le => {
                upper = strongest_upper(
                    upper,
                    Bound::Finite(c.version.clone(), matches!(c.op, Op::Le)),
                );
            }
        }
    }
    (lower, upper)
}

fn strongest_lower(a: Bound, b: Bound) -> Bound {
    match (a, b) {
        (Bound::Unbounded, x) | (x, Bound::Unbounded) => x,
        (Bound::Finite(av, ai), Bound::Finite(bv, bi)) => match compare_versions(&av, &bv) {
            Ordering::Less => Bound::Finite(bv, bi),
            Ordering::Greater => Bound::Finite(av, ai),
            Ordering::Equal => Bound::Finite(av, ai && bi),
        },
    }
}

fn strongest_upper(a: Bound, b: Bound) -> Bound {
    match (a, b) {
        (Bound::Unbounded, x) | (x, Bound::Unbounded) => x,
        (Bound::Finite(av, ai), Bound::Finite(bv, bi)) => match compare_versions(&av, &bv) {
            Ordering::Greater => Bound::Finite(bv, bi),
            Ordering::Less => Bound::Finite(av, ai),
            Ordering::Equal => Bound::Finite(av, ai && bi),
        },
    }
}

fn core_text(t: (u64, u64, u64)) -> String {
    format!("{}.{}.{}", t.0, t.1, t.2)
}

fn version_text(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

pub fn canonical(r: &Range) -> String {
    let mut out = Vec::new();
    for arm in &r.arms {
        let (lower, upper) = arm_bounds(arm);
        let anchors = {
            let mut a = arm.anchors.clone();
            a.sort();
            a.dedup();
            a
        };
        let lower_text = match lower {
            Bound::Unbounded => "*".to_string(),
            Bound::Finite(v, inclusive) => {
                format!("{}{}", if inclusive { ">=" } else { ">" }, version_text(&v))
            }
        };
        let upper_text = match upper {
            Bound::Unbounded => "*".to_string(),
            Bound::Finite(v, inclusive) => {
                format!("{}{}", if inclusive { "<=" } else { "<" }, version_text(&v))
            }
        };
        let anchor_text = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors
                .iter()
                .map(|x| core_text(*x))
                .collect::<Vec<_>>()
                .join(",")
        };
        out.push(format!("[{};{};pre={}]", lower_text, upper_text, anchor_text));
    }
    out.sort();
    out.dedup();
    if out.is_empty() {
        "EMPTY".to_string()
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

fn validate_expr(
    expr: &SetExpr,
    package: &str,
    label: &str,
    path: &mut Vec<usize>,
    ranges: &mut Vec<Range>,
) -> Result<(), SelectError> {
    match expr {
        SetExpr::Range(s) => {
            let r = parse_range(s).map_err(|error| SelectError::InvalidRange {
                package: package.to_string(),
                label: label.to_string(),
                path: path.clone(),
                error,
            })?;
            ranges.push(r);
        }
        SetExpr::Union(children) | SetExpr::Intersection(children) => {
            for (i, child) in children.iter().enumerate() {
                path.push(i);
                validate_expr(child, package, label, path, ranges)?;
                path.pop();
            }
        }
    }
    Ok(())
}

fn eval_expr(expr: &SetExpr, v: &Version, include: bool, ranges: &mut usize, parsed: &[Range]) -> bool {
    match expr {
        SetExpr::Range(_) => {
            let result = matches_with(v, &parsed[*ranges], include);
            *ranges += 1;
            result
        }
        SetExpr::Union(children) => {
            let start = *ranges;
            let mut result = false;
            for child in children {
                let child_result = eval_expr(child, v, include, ranges, parsed);
                result |= child_result;
            }
            let _ = start;
            result
        }
        SetExpr::Intersection(children) => {
            let mut result = true;
            for child in children {
                let child_result = eval_expr(child, v, include, ranges, parsed);
                result &= child_result;
            }
            result
        }
    }
}

fn expr_accepts(expr: &SetExpr, v: &Version, include: bool, ranges: &[Range]) -> bool {
    fn walk(expr: &SetExpr, v: &Version, include: bool, it: &mut impl Iterator<Item = &Range>) -> bool {
        match expr {
            SetExpr::Range(_) => it.next().map(|r| matches_with(v, r, include)).unwrap_or(false),
            SetExpr::Union(xs) => xs.iter().any(|x| walk(x, v, include, it)),
            SetExpr::Intersection(xs) => xs.iter().all(|x| walk(x, v, include, it)),
        }
    }
    walk(expr, v, include, &mut ranges.iter())
}

fn subset_conflict(
    constraints: &[Constraint],
    candidates: &[Version],
    include: bool,
    subset: &[usize],
) -> bool {
    candidates.iter().all(|v| {
        !subset.iter().all(|&i| {
            let mut ranges = Vec::new();
            let mut path = Vec::new();
            let _ = validate_expr(
                &constraints[i].expr,
                "",
                "",
                &mut path,
                &mut ranges,
            );
            expr_accepts(&constraints[i].expr, v, include, &ranges)
        })
    })
}

pub fn select_packages(
    packages: &[Package],
    preference: Preference,
    include_prerelease: bool,
) -> Result<Vec<Selection>, SelectError> {
    let mut names = Vec::<String>::new();
    for p in packages {
        if names.iter().any(|n| n == &p.name) {
            return Err(SelectError::DuplicatePackage {
                name: p.name.clone(),
            });
        }
        names.push(p.name.clone());
    }

    for p in packages {
        let mut labels = Vec::<String>::new();
        for c in &p.constraints {
            if labels.iter().any(|l| l == &c.label) {
                return Err(SelectError::DuplicateLabel {
                    package: p.name.clone(),
                    label: c.label.clone(),
                });
            }
            labels.push(c.label.clone());
        }
    }

    let mut parsed_candidates = Vec::new();
    for p in packages {
        let mut versions = Vec::new();
        for (index, candidate) in p.candidates.iter().enumerate() {
            versions.push(parse_version(candidate).map_err(|error| {
                SelectError::InvalidCandidate {
                    package: p.name.clone(),
                    index,
                    error,
                }
            })?);
        }
        parsed_candidates.push(versions);
    }

    let mut parsed_ranges = Vec::new();
    for p in packages {
        let mut package_ranges = Vec::new();
        for c in &p.constraints {
            let mut path = Vec::new();
            let mut ranges = Vec::new();
            validate_expr(&c.expr, &p.name, &c.label, &mut path, &mut ranges)?;
            package_ranges.push(ranges);
        }
        parsed_ranges.push(package_ranges);
    }

    let mut selections = Vec::new();
    for (pi, p) in packages.iter().enumerate() {
        let candidates = &parsed_candidates[pi];
        let mut qualifying = Vec::new();
        for (ci, v) in candidates.iter().enumerate() {
            let accepted = p.constraints.iter().enumerate().all(|(ki, c)| {
                expr_accepts(&c.expr, v, include_prerelease, &parsed_ranges[pi][ki])
            });
            if accepted {
                qualifying.push(ci);
            }
        }
        if qualifying.is_empty() {
            let labels = if candidates.is_empty() {
                Vec::new()
            } else {
                let n = p.constraints.len();
                let mut answer = None;
                for size in 1..=n {
                    let total = 1usize.checked_shl(n as u32).unwrap_or(0);
                    for mask in 1..total {
                        if mask.count_ones() as usize != size {
                            continue;
                        }
                        let subset: Vec<usize> =
                            (0..n).filter(|i| (mask & (1usize << i)) != 0).collect();
                        if subset_conflict(&p.constraints, candidates, include_prerelease, &subset) {
                            let mut ls: Vec<String> = subset
                                .iter()
                                .map(|&i| p.constraints[i].label.clone())
                                .collect();
                            ls.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
                            if answer.as_ref().map_or(true, |old: &Vec<String>| {
                                ls.as_slice() < old.as_slice()
                            }) {
                                answer = Some(ls);
                            }
                        }
                    }
                    if answer.is_some() {
                        break;
                    }
                }
                answer.unwrap_or_default()
            };
            return Err(SelectError::NoSolution {
                package: p.name.clone(),
                labels,
            });
        }

        let mut best = qualifying[0];
        for &i in qualifying.iter().skip(1) {
            let current = &candidates[i];
            let chosen = &candidates[best];
            let current_stable = current.pre.is_empty();
            let chosen_stable = chosen.pre.is_empty();
            let ordering = compare_versions(current, chosen);
            let replace = match preference {
                Preference::Highest => ordering == Ordering::Greater,
                Preference::Lowest => ordering == Ordering::Less,
                Preference::StableFirst => {
                    if current_stable != chosen_stable {
                        current_stable
                    } else {
                        ordering == Ordering::Greater
                    }
                }
            };
            if replace {
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

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    // A stable witness in an intersecting interval is sufficient whenever the
    // interval reaches a release point. Exact finite prerelease intervals are
    // checked at their explicit endpoints.
    for aa in &a.arms {
        for bb in &b.arms {
            let mut cs = aa.comparators.clone();
            cs.extend(bb.comparators.clone());
            let anchors_a = &aa.anchors;
            let anchors_b = &bb.anchors;
            if include_prerelease {
                if interval_has_witness(&cs) {
                    return true;
                }
            } else {
                let shared: Vec<_> = anchors_a
                    .iter()
                    .filter(|x| anchors_b.contains(x))
                    .copied()
                    .collect();
                for t in shared {
                    let v = version_at_tuple(t, vec!["0".to_string()]);
                    if cs.iter().all(|c| cmp_satisfies(&v, c)) {
                        return true;
                    }
                }
                if interval_has_stable_witness(&cs) {
                    return true;
                }
            }
        }
    }
    false
}

fn interval_has_witness(cs: &[Comparator]) -> bool {
    let (lo, hi) = {
        let mut arm = Arm {
            comparators: cs.to_vec(),
            anchors: Vec::new(),
        };
        arm_bounds(&arm)
    };
    let candidate = match lo {
        Bound::Unbounded => version_at_tuple((0, 0, 0), Vec::new()),
        Bound::Finite(v, inclusive) => {
            if inclusive {
                v
            } else if v.pre.is_empty() {
                match next_stable(&v) {
                    Some(x) => x,
                    None => return false,
                }
            } else {
                let mut x = v.clone();
                x.pre.push("0".to_string());
                x
            }
        }
    };
    if cs.iter().all(|c| cmp_satisfies(&candidate, c)) {
        return true;
    }
    match hi {
        Bound::Finite(v, _) if compare_versions(&candidate, &v) == Ordering::Greater => false,
        _ => false,
    }
}

fn interval_has_stable_witness(cs: &[Comparator]) -> bool {
    let (lo, hi) = {
        let arm = Arm {
            comparators: cs.to_vec(),
            anchors: Vec::new(),
        };
        arm_bounds(&arm)
    };
    let mut candidate = match lo {
        Bound::Unbounded => version_at_tuple((0, 0, 0), Vec::new()),
        Bound::Finite(v, inclusive) => {
            if v.pre.is_empty() {
                if inclusive {
                    v
                } else {
                    match next_stable(&v) {
                        Some(x) => x,
                        None => return false,
                    }
                }
            } else {
                version_at_tuple(tuple(&v), Vec::new())
            }
        }
    };
    if !candidate.pre.is_empty() {
        candidate.pre.clear();
    }
    if cs.iter().all(|c| cmp_satisfies(&candidate, c)) {
        return true;
    }
    if let Bound::Finite(upper, _) = hi {
        if compare_versions(&candidate, &upper) != Ordering::Less
            && compare_versions(&candidate, &upper) != Ordering::Equal
        {
            return false;
        }
    }
    false
}

fn next_stable(v: &Version) -> Option<Version> {
    if v.patch < u64::MAX {
        Some(version_at_tuple((v.major, v.minor, v.patch + 1), Vec::new()))
    } else if v.minor < u64::MAX {
        Some(version_at_tuple((v.major, v.minor + 1, 0), Vec::new()))
    } else if v.major < u64::MAX {
        Some(version_at_tuple((v.major + 1, 0, 0), Vec::new()))
    } else {
        None
    }
}