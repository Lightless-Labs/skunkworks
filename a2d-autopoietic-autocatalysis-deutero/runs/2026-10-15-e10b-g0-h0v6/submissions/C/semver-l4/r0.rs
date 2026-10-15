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

fn parse_core_number(bytes: &[u8], pos: &mut usize) -> Result<u64, VersionError> {
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

fn valid_identifier_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

fn parse_identifiers(
    s: &str,
    base: usize,
    validate_numeric_pre: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    loop {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'.')
            .map(|i| start + i)
            .unwrap_or(bytes.len());
        if start == end {
            return Err(VersionError::EmptyIdentifier { at: base + start });
        }
        for i in start..end {
            if !valid_identifier_char(bytes[i]) {
                return Err(VersionError::InvalidChar { at: base + i });
            }
        }
        let ident = &s[start..end];
        if validate_numeric_pre
            && ident.bytes().all(is_digit)
            && ident.len() > 1
            && ident.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: base + start });
        }
        out.push(ident.to_string());
        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(VersionError::EmptyIdentifier { at: base + start });
        }
    }
    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let bytes = s.as_bytes();
    let mut pos = 0;
    let major = parse_core_number(bytes, &mut pos)?;
    for _ in 0..1 {
        if pos >= bytes.len() {
            return Err(VersionError::MissingCore { at: pos });
        }
        match bytes[pos] {
            b'.' => pos += 1,
            b'-' | b'+' => return Err(VersionError::MissingCore { at: pos }),
            _ => return Err(VersionError::InvalidChar { at: pos }),
        }
    }
    let minor = parse_core_number(bytes, &mut pos)?;
    if pos >= bytes.len() {
        return Err(VersionError::MissingCore { at: pos });
    }
    match bytes[pos] {
        b'.' => pos += 1,
        b'-' | b'+' => return Err(VersionError::MissingCore { at: pos }),
        _ => return Err(VersionError::InvalidChar { at: pos }),
    }
    let patch = parse_core_number(bytes, &mut pos)?;
    if pos < bytes.len() && bytes[pos] != b'-' && bytes[pos] != b'+' {
        return Err(VersionError::InvalidChar { at: pos });
    }
    let mut pre = Vec::new();
    let mut build = Vec::new();
    if pos < bytes.len() && bytes[pos] == b'-' {
        pos += 1;
        let pre_start = pos;
        let plus = bytes[pos..]
            .iter()
            .position(|&b| b == b'+')
            .map(|i| pos + i)
            .unwrap_or(bytes.len());
        let pre_text = &s[pos..plus];
        pre = parse_identifiers(pre_text, pre_start, true)?;
        pos = plus;
    }
    if pos < bytes.len() && bytes[pos] == b'+' {
        pos += 1;
        build = parse_identifiers(&s[pos..], pos, false)?;
        pos = bytes.len();
    }
    if pos != bytes.len() {
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

fn cmp_numeric_ident(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

fn is_numeric_ident(s: &str) -> bool {
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
    for (x, y) in a.iter().zip(b.iter()) {
        let xn = is_numeric_ident(x);
        let yn = is_numeric_ident(y);
        let c = match (xn, yn) {
            (true, true) => cmp_numeric_ident(x, y),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => x.as_bytes().cmp(y.as_bytes()),
        };
        if c != Ordering::Equal {
            return c;
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

#[derive(Clone, Debug)]
struct Comparator {
    op: Op,
    version: Version,
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

fn whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tuple_version(t: (u64, u64, u64), pre: Vec<String>) -> Version {
    Version {
        major: t.0,
        minor: t.1,
        patch: t.2,
        pre,
        build: Vec::new(),
    }
}

fn version_tuple(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn add_cmp(arm: &mut Arm, op: Op, version: Version, anchor: bool) {
    if anchor && !version.pre.is_empty() {
        arm.anchors.push(version_tuple(&version));
    }
    arm.comparators.push(Comparator { op, version });
}

fn version_from_fields(fields: &[u64]) -> Version {
    tuple_version(
        (
            *fields.get(0).unwrap_or(&0),
            *fields.get(1).unwrap_or(&0),
            *fields.get(2).unwrap_or(&0),
        ),
        Vec::new(),
    )
}

#[derive(Clone, Debug)]
struct Operand {
    version: Version,
    k: usize,
    full: bool,
    wild: bool,
}

fn parse_partial_or_full(s: &str, at: usize) -> Result<Operand, RangeError> {
    let contains_suffix = s.contains('-') || s.contains('+');
    let fields: Vec<&str> = s.split('.').collect();
    let has_wild = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));
    let full_case = contains_suffix || (fields.len() >= 3 && !has_wild);
    if full_case {
        let version = parse_version(s).map_err(|error| RangeError::Version { at, error })?;
        return Ok(Operand {
            version,
            k: 3,
            full: true,
            wild: false,
        });
    }
    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at });
    }
    let mut nums = Vec::new();
    let mut wild = false;
    for field in &fields {
        if field.is_empty() {
            return Err(RangeError::Syntax { at });
        }
        if matches!(*field, "*" | "x" | "X") {
            wild = true;
            continue;
        }
        if wild
            || !field.bytes().all(is_digit)
            || (field.len() > 1 && field.as_bytes()[0] == b'0')
        {
            return Err(RangeError::Syntax { at });
        }
        let mut value = 0u64;
        for b in field.bytes() {
            value = value
                .checked_mul(10)
                .and_then(|x| x.checked_add((b - b'0') as u64))
                .ok_or(RangeError::Syntax { at })?;
        }
        nums.push(value);
    }
    let k = nums.len();
    let version = version_from_fields(&nums);
    Ok(Operand {
        version,
        k,
        full: false,
        wild,
    })
}

fn increment_upper(
    v: &Version,
    field: usize,
    at: usize,
) -> Result<Version, RangeError> {
    let mut t = version_tuple(v);
    match field {
        0 => {
            t.0 = t.0.checked_add(1).ok_or(RangeError::Syntax { at })?;
            t.1 = 0;
            t.2 = 0;
        }
        1 => {
            t.1 = t.1.checked_add(1).ok_or(RangeError::Syntax { at })?;
            t.2 = 0;
        }
        _ => t.2 = t.2.checked_add(1).ok_or(RangeError::Syntax { at })?,
    }
    Ok(tuple_version(t, vec!["0".to_string()]))
}

fn tokenize(s: &str, base: usize) -> Vec<(String, usize)> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && whitespace(bytes[i]) {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !whitespace(bytes[i]) {
            i += 1;
        }
        out.push((s[start..i].to_string(), base + start));
    }
    out
}

fn parse_full_comparator(token: &str, at: usize) -> Result<(Op, Version), RangeError> {
    let (op, prefix) = if token.starts_with(">=") {
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
        (Op::Eq, 0)
    };
    let operand = &token[prefix..];
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at });
    }
    let version = parse_version(operand).map_err(|error| RangeError::Version {
        at: at + prefix,
        error,
    })?;
    Ok((op, version))
}

fn parse_general_token(
    arm: &mut Arm,
    token: &str,
    at: usize,
) -> Result<(), RangeError> {
    let bytes = token.as_bytes();
    if bytes.starts_with(b"^") || bytes.starts_with(b"~") {
        let caret = bytes[0] == b'^';
        let op_at = at;
        let operand_at = at + 1;
        if token.len() == 1 {
            return Err(RangeError::Syntax { at: op_at });
        }
        let operand = parse_partial_or_full(&token[1..], operand_at)?;
        if operand.k == 0 {
            return Err(RangeError::Syntax { at: op_at });
        }
        add_cmp(arm, Op::Ge, operand.version.clone(), true);
        let field = if caret {
            if operand.version.major > 0 || operand.k == 1 {
                0
            } else if operand.version.minor > 0 || operand.k == 2 {
                1
            } else {
                2
            }
        } else if operand.k == 1 {
            0
        } else {
            1
        };
        let upper = increment_upper(&operand.version, field, operand_at)?;
        add_cmp(arm, Op::Lt, upper, false);
        return Ok(());
    }
    if bytes.first().map_or(false, |b| matches!(b, b'<' | b'>' | b'=')) {
        let (op, version) = parse_full_comparator(token, at)?;
        add_cmp(arm, op, version, true);
        return Ok(());
    }
    if token.starts_with('*') || token.starts_with('x') || token.starts_with('X')
        || bytes.first().map_or(false, |b| b.is_ascii_digit())
    {
        let operand = parse_partial_or_full(token, at)?;
        if operand.full {
            add_cmp(arm, Op::Eq, operand.version, true);
        } else if operand.k == 1 {
            add_cmp(arm, Op::Ge, operand.version.clone(), false);
            let upper = increment_upper(&operand.version, 0, at)?;
            add_cmp(arm, Op::Lt, upper, false);
        } else if operand.k == 2 {
            add_cmp(arm, Op::Ge, operand.version.clone(), false);
            let upper = increment_upper(&operand.version, 1, at)?;
            add_cmp(arm, Op::Lt, upper, false);
        }
        return Ok(());
    }
    Err(RangeError::Syntax { at })
}

fn parse_hyphen_arm(tokens: &[(String, usize)]) -> Result<Arm, RangeError> {
    let minus_positions: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, (t, _))| if t == "-" { Some(i) } else { None })
        .collect();
    if !minus_positions.is_empty() {
        if tokens.len() != 3 || minus_positions != [1] {
            return Err(RangeError::Syntax {
                at: tokens[minus_positions[0]].1,
            });
        }
        let (lo, lo_at) = (&tokens[0].0, tokens[0].1);
        let (hi, hi_at) = (&tokens[2].0, tokens[2].1);
        let lower = parse_partial_or_full(lo, lo_at)?;
        let upper = parse_partial_or_full(hi, hi_at)?;
        if lower.k == 0 {
            return Err(RangeError::Syntax { at: lo_at });
        }
        if upper.k == 0 {
            return Err(RangeError::Syntax { at: hi_at });
        }
        let mut arm = Arm {
            comparators: Vec::new(),
            anchors: Vec::new(),
        };
        add_cmp(&mut arm, Op::Ge, lower.version.clone(), true);
        if upper.full {
            add_cmp(&mut arm, Op::Le, upper.version, true);
        } else if upper.k == 1 {
            let bound = increment_upper(&upper.version, 0, hi_at)?;
            add_cmp(&mut arm, Op::Lt, bound, false);
        } else if upper.k == 2 {
            let bound = increment_upper(&upper.version, 1, hi_at)?;
            add_cmp(&mut arm, Op::Lt, bound, false);
        } else {
            add_cmp(&mut arm, Op::Le, upper.version, false);
        }
        return Ok(arm);
    }
    let mut arm = Arm {
        comparators: Vec::new(),
        anchors: Vec::new(),
    };
    for (token, at) in tokens {
        parse_general_token(&mut arm, token, *at)?;
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut tokens = Vec::<(String, usize)>::new();
    let mut separators = Vec::<usize>::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && whitespace(bytes[i]) {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !whitespace(bytes[i]) {
            i += 1;
        }
        let token = &s[start..i];
        if token == "||" {
            separators.push(start);
            tokens.push((token.to_string(), start));
        } else {
            tokens.push((token.to_string(), start));
        }
    }
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    let mut arms = Vec::new();
    let mut group = Vec::new();
    for (token, at) in tokens {
        if token == "||" {
            if group.is_empty() {
                return Err(RangeError::Empty { at });
            }
            arms.push(parse_hyphen_arm(&group)?);
            group.clear();
        } else {
            group.push((token, at));
        }
    }
    if group.is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_hyphen_arm(&group)?);
    let _ = separators;
    Ok(Range { arms })
}

fn comparator_matches(v: &Version, c: &Comparator) -> bool {
    match c.op {
        Op::Eq => compare_versions(v, &c.version) == Ordering::Equal,
        Op::Lt => compare_versions(v, &c.version) == Ordering::Less,
        Op::Le => compare_versions(v, &c.version) != Ordering::Greater,
        Op::Gt => compare_versions(v, &c.version) == Ordering::Greater,
        Op::Ge => compare_versions(v, &c.version) != Ordering::Less,
    }
}

fn arm_matches(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    if !arm.comparators.iter().all(|c| comparator_matches(v, c)) {
        return false;
    }
    if include_prerelease || v.pre.is_empty() {
        return true;
    }
    let tuple = version_tuple(v);
    arm.anchors.contains(&tuple)
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
    max: bool,
) -> Result<Option<String>, CandidateError> {
    let parsed: Vec<Version> = candidates
        .iter()
        .enumerate()
        .map(|(index, s)| {
            parse_version(s).map_err(|error| CandidateError { index, error })
        })
        .collect::<Result<_, _>>()?;
    let mut best: Option<usize> = None;
    for (i, v) in parsed.iter().enumerate() {
        if !matches(v, r) {
            continue;
        }
        best = match best {
            None => Some(i),
            Some(j) => {
                let cmp = compare_versions(v, &parsed[j]);
                if (max && cmp == Ordering::Greater) || (!max && cmp == Ordering::Less) {
                    Some(i)
                } else {
                    Some(j)
                }
            }
        };
    }
    Ok(best.map(|i| candidates[i].clone()))
}

fn upper_bound(arm: &Arm) -> Option<(Version, bool)> {
    let mut best: Option<(Version, bool)> = None;
    for c in &arm.comparators {
        let candidate = match c.op {
            Op::Eq => Some((c.version.clone(), true)),
            Op::Lt => Some((c.version.clone(), false)),
            Op::Le => Some((c.version.clone(), true)),
            _ => None,
        };
        if let Some((v, inclusive)) = candidate {
            best = match best {
                None => Some((v, inclusive)),
                Some((old, old_inc)) => match compare_versions(&v, &old) {
                    Ordering::Less => Some((v, inclusive)),
                    Ordering::Equal => Some((old, old_inc && inclusive)),
                    Ordering::Greater => Some((old, old_inc)),
                },
            };
        }
    }
    best
}

fn lower_bound(arm: &Arm) -> Option<(Version, bool)> {
    let mut best: Option<(Version, bool)> = None;
    for c in &arm.comparators {
        let candidate = match c.op {
            Op::Eq => Some((c.version.clone(), true)),
            Op::Gt => Some((c.version.clone(), false)),
            Op::Ge => Some((c.version.clone(), true)),
            _ => None,
        };
        if let Some((v, inclusive)) = candidate {
            best = match best {
                None => Some((v, inclusive)),
                Some((old, old_inc)) => match compare_versions(&v, &old) {
                    Ordering::Greater => Some((v, inclusive)),
                    Ordering::Equal => Some((old, old_inc && inclusive)),
                    Ordering::Less => Some((old, old_inc)),
                },
            };
        }
    }
    best
}

fn version_display(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

fn arm_has_any_default(arm: &Arm) -> bool {
    if !arm
        .comparators
        .iter()
        .any(|c| matches!(c.op, Op::Eq | Op::Lt | Op::Le | Op::Gt | Op::Ge))
    {
        return true;
    }
    if let Some((lo, li)) = lower_bound(arm) {
        if let Some((hi, hi_i)) = upper_bound(arm) {
            match compare_versions(&lo, &hi) {
                Ordering::Greater => return false,
                Ordering::Equal => {
                    if !li || !hi_i {
                        return false;
                    }
                    if !lo.pre.is_empty() && !arm.anchors.contains(&version_tuple(&lo)) {
                        return false;
                    }
                }
                Ordering::Less => {}
            }
        }
    }
    true
}

pub fn canonical(r: &Range) -> String {
    let mut strings = Vec::new();
    for arm in &r.arms {
        if !arm_has_any_default(arm) {
            continue;
        }
        let lower = lower_bound(arm)
            .map(|(v, inc)| format!("{}{}", if inc { ">=" } else { ">" }, version_display(&v)))
            .unwrap_or_else(|| "*".to_string());
        let upper = upper_bound(arm)
            .map(|(v, inc)| format!("{}{}", if inc { "<=" } else { "<" }, version_display(&v)))
            .unwrap_or_else(|| "*".to_string());
        let mut anchors = arm.anchors.clone();
        anchors.sort();
        anchors.dedup();
        let anchor_text = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors
                .iter()
                .map(|(a, b, c)| format!("{a}.{b}.{c}"))
                .collect::<Vec<_>>()
                .join(",")
        };
        strings.push(format!("[{lower};{upper};pre={anchor_text}]"));
    }
    strings.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    strings.dedup();
    if strings.is_empty() {
        "EMPTY".to_string()
    } else {
        strings.join(" || ")
    }
}

fn candidate_witness_for_arm(arm: &Arm, include_prerelease: bool) -> Option<Version> {
    let lo = lower_bound(arm);
    let hi = upper_bound(arm);
    let mut candidates = Vec::new();
    if let Some((v, _)) = &lo {
        candidates.push(v.clone());
        if v.pre.is_empty() {
            candidates.push(tuple_version(version_tuple(v), vec!["0".to_string()]));
        }
    }
    if let Some((v, _)) = &hi {
        candidates.push(v.clone());
        if v.pre.is_empty() {
            candidates.push(tuple_version(version_tuple(v), vec!["0".to_string()]));
        }
    }
    candidates.push(tuple_version((0, 0, 0), Vec::new()));
    for c in candidates {
        if arm_matches(&c, arm, include_prerelease) {
            return Some(c);
        }
    }
    None
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for aa in &a.arms {
        for bb in &b.arms {
            let mut combined = Arm {
                comparators: aa.comparators.clone(),
                anchors: aa.anchors.iter().copied().filter(|x| bb.anchors.contains(x)).collect(),
            };
            combined.comparators.extend(bb.comparators.clone());
            if candidate_witness_for_arm(&combined, include_prerelease).is_some() {
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
    package: &str,
    label: &str,
    expr: &SetExpr,
    path: &mut Vec<usize>,
) -> Result<(), SelectError> {
    match expr {
        SetExpr::Range(s) => parse_range(s).map(|_| ()).map_err(|error| {
            SelectError::InvalidRange {
                package: package.to_string(),
                label: label.to_string(),
                path: path.clone(),
                error,
            }
        }),
        SetExpr::Union(children) | SetExpr::Intersection(children) => {
            for (i, child) in children.iter().enumerate() {
                path.push(i);
                validate_expr(package, label, child, path)?;
                path.pop();
            }
            Ok(())
        }
    }
}

fn expr_accepts(expr: &SetExpr, v: &Version, include_prerelease: bool) -> bool {
    match expr {
        SetExpr::Range(s) => parse_range(s)
            .map(|r| matches_with(v, &r, include_prerelease))
            .unwrap_or(false),
        SetExpr::Union(children) => children
            .iter()
            .any(|child| expr_accepts(child, v, include_prerelease)),
        SetExpr::Intersection(children) => children
            .iter()
            .all(|child| expr_accepts(child, v, include_prerelease)),
    }
}

fn conflict_labels(
    package: &Package,
    versions: &[Version],
    include_prerelease: bool,
) -> Vec<String> {
    let n = package.constraints.len();
    if versions.is_empty() {
        return Vec::new();
    }
    for size in 1..=n {
        let mut best: Option<Vec<String>> = None;
        let mut idxs = Vec::new();
        fn search(
            start: usize,
            need: usize,
            idxs: &mut Vec<usize>,
            package: &Package,
            versions: &[Version],
            include_prerelease: bool,
            best: &mut Option<Vec<String>>,
        ) {
            if need == 0 {
                let mut accepted_any = false;
                'cand: for v in versions {
                    for &i in idxs.iter() {
                        if !expr_accepts(
                            &package.constraints[i].expr,
                            v,
                            include_prerelease,
                        ) {
                            continue 'cand;
                        }
                    }
                    accepted_any = true;
                    break;
                }
                if !accepted_any {
                    let mut labels: Vec<String> = idxs
                        .iter()
                        .map(|&i| package.constraints[i].label.clone())
                        .collect();
                    labels.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
                    if best.as_ref().map_or(true, |old| {
                        labels
                            .iter()
                            .map(String::as_bytes)
                            .cmp(old.iter().map(String::as_bytes))
                            == Ordering::Less
                    }) {
                        *best = Some(labels);
                    }
                }
                return;
            }
            if package.constraints.len().saturating_sub(start) < need {
                return;
            }
            for i in start..package.constraints.len() {
                idxs.push(i);
                search(
                    i + 1,
                    need - 1,
                    idxs,
                    package,
                    versions,
                    include_prerelease,
                    best,
                );
                idxs.pop();
            }
        }
        search(
            0,
            size,
            &mut idxs,
            package,
            versions,
            include_prerelease,
            &mut best,
        );
        if let Some(labels) = best {
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
    let mut all_versions = Vec::new();
    for p in packages {
        let mut parsed = Vec::new();
        for (index, s) in p.candidates.iter().enumerate() {
            match parse_version(s) {
                Ok(v) => parsed.push(v),
                Err(error) => {
                    return Err(SelectError::InvalidCandidate {
                        package: p.name.clone(),
                        index,
                        error,
                    })
                }
            }
        }
        all_versions.push(parsed);
    }
    for p in packages {
        for c in &p.constraints {
            validate_expr(&p.name, &c.label, &c.expr, &mut Vec::new())?;
        }
    }
    let mut selections = Vec::new();
    for (pi, p) in packages.iter().enumerate() {
        let versions = &all_versions[pi];
        let mut qualifying = Vec::new();
        'candidate: for (i, v) in versions.iter().enumerate() {
            for c in &p.constraints {
                if !expr_accepts(&c.expr, v, include_prerelease) {
                    continue 'candidate;
                }
            }
            qualifying.push(i);
        }
        if qualifying.is_empty() {
            return Err(SelectError::NoSolution {
                package: p.name.clone(),
                labels: conflict_labels(p, versions, include_prerelease),
            });
        }
        let is_better = |i: usize, j: usize| -> bool {
            let vi = &versions[i];
            let vj = &versions[j];
            match preference {
                Preference::Highest => compare_versions(vi, vj) == Ordering::Greater,
                Preference::Lowest => compare_versions(vi, vj) == Ordering::Less,
                Preference::StableFirst => {
                    let si = vi.pre.is_empty();
                    let sj = vj.pre.is_empty();
                    if si != sj {
                        si
                    } else {
                        compare_versions(vi, vj) == Ordering::Greater
                    }
                }
            }
        };
        let mut best = qualifying[0];
        for &i in qualifying.iter().skip(1) {
            if is_better(i, best) {
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

fn main() {}