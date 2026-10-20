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

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

fn parse_core_field(bytes: &[u8], pos: &mut usize) -> Result<u64, VersionError> {
    let start = *pos;
    while *pos < bytes.len() && is_digit(bytes[*pos]) {
        *pos += 1;
    }
    if *pos == start {
        if *pos == bytes.len() || matches!(bytes[*pos], b'.' | b'-' | b'+') {
            return Err(VersionError::MissingCore { at: start });
        }
        return Err(VersionError::InvalidChar { at: start });
    }
    if *pos - start > 1 && bytes[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    let mut n = 0u64;
    for &b in &bytes[start..*pos] {
        n = n.checked_mul(10)
            .and_then(|x| x.checked_add((b - b'0') as u64))
            .ok_or(VersionError::Overflow { at: start })?;
    }
    Ok(n)
}

fn parse_identifiers(
    bytes: &[u8],
    start: usize,
    end: usize,
    prerelease: bool,
) -> Result<Vec<String>, VersionError> {
    let mut out = Vec::new();
    let mut i = start;
    loop {
        let id_start = i;
        while i < end && bytes[i] != b'.' {
            if !is_ident_byte(bytes[i]) {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }
        if i == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        if prerelease {
            let id = &bytes[id_start..i];
            if id.iter().all(|b| is_digit(*b)) && id.len() > 1 && id[0] == b'0' {
                return Err(VersionError::LeadingZero { at: id_start });
            }
        }
        out.push(String::from_utf8(bytes[id_start..i].to_vec()).unwrap());
        if i == end {
            break;
        }
        i += 1;
        if i == end {
            return Err(VersionError::EmptyIdentifier { at: i });
        }
    }
    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return Err(VersionError::Empty);
    }
    let mut p = 0usize;
    let major = parse_core_field(bytes, &mut p)?;
    for field in 0..2 {
        if p >= bytes.len() || bytes[p] != b'.' {
            if p >= bytes.len() || matches!(bytes[p], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: p });
            }
            return Err(VersionError::InvalidChar { at: p });
        }
        p += 1;
        let n = parse_core_field(bytes, &mut p)?;
        if field == 0 {
            let minor = n;
            // Continue below with the patch parsed in the second iteration.
            if p >= bytes.len() || bytes[p] != b'.' {
                if p >= bytes.len() || matches!(bytes[p], b'-' | b'+') {
                    return Err(VersionError::MissingCore { at: p });
                }
                return Err(VersionError::InvalidChar { at: p });
            }
            p += 1;
            let patch = parse_core_field(bytes, &mut p)?;
            let (pre, build) = parse_tail(bytes, p)?;
            return Ok(Version { major, minor, patch, pre, build });
        }
        let _ = n;
    }
    unreachable!()
}

fn parse_tail(bytes: &[u8], p: usize) -> Result<(Vec<String>, Vec<String>), VersionError> {
    if p == bytes.len() {
        return Ok((Vec::new(), Vec::new()));
    }
    match bytes[p] {
        b'-' => {
            let pre_start = p + 1;
            let plus = bytes[pre_start..]
                .iter()
                .position(|b| *b == b'+')
                .map(|x| pre_start + x);
            let pre_end = plus.unwrap_or(bytes.len());
            let pre = parse_identifiers(bytes, pre_start, pre_end, true)?;
            let build = if let Some(plus_pos) = plus {
                parse_identifiers(bytes, plus_pos + 1, bytes.len(), false)?
            } else {
                Vec::new()
            };
            Ok((pre, build))
        }
        b'+' => {
            let build = parse_identifiers(bytes, p + 1, bytes.len(), false)?;
            Ok((Vec::new(), build))
        }
        _ => Err(VersionError::InvalidChar { at: p }),
    }
}

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn numeric_id(s: &str) -> bool {
    !s.is_empty() && s.as_bytes().iter().all(|b| is_digit(*b))
}

fn compare_id(a: &str, b: &str) -> Ordering {
    let an = numeric_id(a);
    let bn = numeric_id(b);
    match (an, bn) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    core(a)
        .cmp(&core(b))
        .then_with(|| match (a.pre.is_empty(), b.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => {
                for (x, y) in a.pre.iter().zip(&b.pre) {
                    let c = compare_id(x, y);
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

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn parse_full_operand(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

fn cmp_ok(v: &Version, c: &Comparator) -> bool {
    match c.op {
        Op::Eq => compare_versions(v, &c.version) == Ordering::Equal,
        Op::Lt => compare_versions(v, &c.version) == Ordering::Less,
        Op::Le => compare_versions(v, &c.version) != Ordering::Greater,
        Op::Gt => compare_versions(v, &c.version) == Ordering::Greater,
        Op::Ge => compare_versions(v, &c.version) != Ordering::Less,
    }
}

fn valid_arm(v: &Version, arm: &Arm, include_pre: bool) -> bool {
    if !arm.comparators.iter().all(|c| cmp_ok(v, c)) {
        return false;
    }
    v.pre.is_empty()
        || include_pre
        || arm.anchors.iter().any(|a| *a == core(v))
}

fn split_tokens(input: &str, base: usize) -> Vec<(usize, &str)> {
    let b = input.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && is_ws(b[i]) {
            i += 1;
        }
        if i == b.len() {
            break;
        }
        let start = i;
        while i < b.len() && !is_ws(b[i]) {
            i += 1;
        }
        out.push((base + start, &input[start..i]));
    }
    out
}

#[derive(Clone, Debug)]
enum Partial {
    Full(Version),
    Partial { fields: Vec<u64>, count: usize },
}

fn parse_partial(text: &str, at: usize) -> Result<Partial, RangeError> {
    let has_suffix = text.contains('-') || text.contains('+');
    let pieces: Vec<&str> = text.split('.').collect();
    if has_suffix || (pieces.len() >= 3 && !pieces.iter().any(|x| matches!(*x, "*" | "x" | "X"))) {
        return parse_full_operand(text, at).map(Partial::Full);
    }
    if pieces.is_empty() || pieces.len() > 3 {
        return Err(RangeError::Syntax { at });
    }
    let mut values = Vec::new();
    let mut wildcard = false;
    for p in pieces {
        if p.is_empty() {
            return Err(RangeError::Syntax { at });
        }
        if matches!(p, "*" | "x" | "X") {
            wildcard = true;
        } else {
            if wildcard || !p.bytes().all(is_digit) || (p.len() > 1 && p.starts_with('0')) {
                return Err(RangeError::Syntax { at });
            }
            let n = p.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
            values.push(n);
        }
    }
    let count = values.len();
    Ok(Partial::Partial { fields: values, count })
}

fn padded(p: &Partial) -> Option<Version> {
    match p {
        Partial::Full(v) => Some(v.clone()),
        Partial::Partial { fields, count } => {
            if *count == 0 {
                None
            } else {
                Some(Version {
                    major: fields.get(0).copied().unwrap_or(0),
                    minor: fields.get(1).copied().unwrap_or(0),
                    patch: fields.get(2).copied().unwrap_or(0),
                    pre: Vec::new(),
                    build: Vec::new(),
                })
            }
        }
    }
}

fn upper_bump(v: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let mut n = v.clone();
    n.pre = vec!["0".into()];
    n.build.clear();
    let target = match field {
        0 => &mut n.major,
        1 => &mut n.minor,
        _ => &mut n.patch,
    };
    *target = target.checked_add(1).ok_or(RangeError::Syntax { at })?;
    match field {
        0 => { n.minor = 0; n.patch = 0; }
        1 => n.patch = 0,
        _ => {}
    }
    Ok(n)
}

fn add_cmp(arm: &mut Arm, op: Op, version: Version, anchor: bool) {
    if anchor && !version.pre.is_empty() {
        arm.anchors.push(core(&version));
    }
    arm.comparators.push(Comparator { op, version });
}

fn parse_arm(tokens: &[(usize, &str)]) -> Result<Arm, RangeError> {
    let mut arm = Arm { comparators: Vec::new(), anchors: Vec::new() };
    let standalone_minus = tokens.iter().position(|(_, t)| *t == "-");
    if let Some(i) = standalone_minus {
        if tokens.len() != 3 || i != 1 {
            return Err(RangeError::Syntax { at: tokens[i].0 });
        }
        let (la, lt) = tokens[0];
        let (ua, ut) = tokens[2];
        let lower = parse_partial(lt, la)?;
        let upper = parse_partial(ut, ua)?;
        let lv = padded(&lower).filter(|_| !matches!(lower, Partial::Partial { count: 0, .. }))
            .ok_or(RangeError::Syntax { at: la })?;
        let uv = padded(&upper).filter(|_| !matches!(upper, Partial::Partial { count: 0, .. }))
            .ok_or(RangeError::Syntax { at: ua })?;
        add_cmp(&mut arm, Op::Ge, lv, true);
        match upper {
            Partial::Full(v) => add_cmp(&mut arm, Op::Le, v, true),
            Partial::Partial { count: 1, .. } => add_cmp(&mut arm, Op::Lt, upper_bump(&uv, 0, ua)?, false),
            Partial::Partial { count: 2, .. } => add_cmp(&mut arm, Op::Lt, upper_bump(&uv, 1, ua)?, false),
            Partial::Partial { .. } => add_cmp(&mut arm, Op::Le, uv, false),
        }
        return Ok(arm);
    }

    for &(at, token) in tokens {
        if token == "||" {
            return Err(RangeError::Syntax { at });
        }
        if token.is_empty() {
            return Err(RangeError::Syntax { at });
        }
        if token.starts_with('^') || token.starts_with('~') {
            let op = token.as_bytes()[0];
            let sub_at = at + 1;
            let part = parse_partial(&token[1..], sub_at)?;
            let count = match &part {
                Partial::Full(_) => 3,
                Partial::Partial { count, .. } => *count,
            };
            let lower = padded(&part).filter(|_| count > 0).ok_or(RangeError::Syntax { at })?;
            add_cmp(&mut arm, Op::Ge, lower.clone(), true);
            let field = if op == b'^' {
                if lower.major > 0 || count == 1 { 0 }
                else if lower.minor > 0 || count == 2 { 1 }
                else { 2 }
            } else if count == 1 { 0 } else { 1 };
            add_cmp(&mut arm, Op::Lt, upper_bump(&lower, field, at)?, false);
            continue;
        }
        if matches!(token.as_bytes()[0], b'<' | b'>' | b'=') {
            let (op, n) = if token.starts_with(">=") { (Op::Ge, 2) }
                else if token.starts_with("<=") { (Op::Le, 2) }
                else if token.starts_with('>') { (Op::Gt, 1) }
                else if token.starts_with('<') { (Op::Lt, 1) }
                else { (Op::Eq, 1) };
            if token.len() == n || !token.as_bytes()[n].is_ascii_digit() {
                return Err(RangeError::Syntax { at });
            }
            let v = parse_full_operand(&token[n..], at + n)?;
            add_cmp(&mut arm, op, v, true);
            continue;
        }
        if !(token.as_bytes()[0].is_ascii_digit() || matches!(token.as_bytes()[0], b'*' | b'x' | b'X')) {
            return Err(RangeError::Syntax { at });
        }
        let part = parse_partial(token, at)?;
        match part {
            Partial::Full(v) => add_cmp(&mut arm, Op::Eq, v, true),
            Partial::Partial { count: 0, .. } => {}
            Partial::Partial { count: 1, .. } => {
                let v = padded(&part).unwrap();
                add_cmp(&mut arm, Op::Ge, v.clone(), false);
                add_cmp(&mut arm, Op::Lt, upper_bump(&v, 0, at)?, false);
            }
            Partial::Partial { count: 2, .. } => {
                let v = padded(&part).unwrap();
                add_cmp(&mut arm, Op::Ge, v.clone(), false);
                add_cmp(&mut arm, Op::Lt, upper_bump(&v, 1, at)?, false);
            }
            Partial::Partial { .. } => {
                let v = padded(&part).unwrap();
                add_cmp(&mut arm, Op::Eq, v, false);
            }
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let tokens = split_tokens(s, 0);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    let mut arm_tokens: Vec<(usize, &str)> = Vec::new();
    let mut arms = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let (at, t) = tokens[i];
        if t == "||" {
            if arm_tokens.is_empty() {
                return Err(RangeError::Empty { at });
            }
            arms.push(parse_arm(&arm_tokens)?);
            arm_tokens.clear();
            i += 1;
            if i == tokens.len() {
                return Err(RangeError::Empty { at: bytes.len() });
            }
        } else {
            arm_tokens.push((at, t));
            i += 1;
        }
    }
    if arm_tokens.is_empty() {
        return Err(RangeError::Empty { at: bytes.len() });
    }
    arms.push(parse_arm(&arm_tokens)?);
    Ok(Range { arms })
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|a| valid_arm(v, a, include_prerelease))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

pub fn max_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, true)
}

pub fn min_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, false)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn extrema(candidates: &[String], r: &Range, max: bool) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::new();
    for (i, s) in candidates.iter().enumerate() {
        let v = parse_version(s).map_err(|error| CandidateError { index: i, error })?;
        parsed.push(v);
    }
    let mut best: Option<usize> = None;
    for i in 0..parsed.len() {
        if !matches(&parsed[i], r) {
            continue;
        }
        if let Some(j) = best {
            let c = compare_versions(&parsed[i], &parsed[j]);
            if (max && c == Ordering::Greater) || (!max && c == Ordering::Less) {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    if include_prerelease {
        // The general witness search is over candidate boundary representatives.
        return approximate_intersects(a, b, true);
    }
    approximate_intersects(a, b, false)
}

fn approximate_intersects(a: &Range, b: &Range, include_pre: bool) -> bool {
    for arm_a in &a.arms {
        for arm_b in &b.arms {
            let mut candidates = vec![
                Version { major: 0, minor: 0, patch: 0, pre: Vec::new(), build: Vec::new() },
                Version { major: u64::MAX, minor: u64::MAX, patch: u64::MAX, pre: Vec::new(), build: Vec::new() },
            ];
            for c in arm_a.comparators.iter().chain(&arm_b.comparators) {
                candidates.push(c.version.clone());
                candidates.push(Version { major: c.version.major, minor: c.version.minor, patch: c.version.patch, pre: Vec::new(), build: Vec::new() });
            }
            for v in &candidates {
                if valid_arm(v, arm_a, include_pre) && valid_arm(v, arm_b, include_pre) {
                    return true;
                }
            }
        }
    }
    false
}

pub fn canonical(r: &Range) -> String {
    let mut arms = Vec::new();
    for arm in &r.arms {
        let mut lower: Option<(Version, bool)> = None;
        let mut upper: Option<(Version, bool)> = None;
        for c in &arm.comparators {
            match c.op {
                Op::Eq => {
                    lower = Some((c.version.clone(), true));
                    upper = Some((c.version.clone(), true));
                }
                Op::Gt | Op::Ge => {
                    let inc = matches!(c.op, Op::Ge);
                    if lower.as_ref().map_or(true, |(v, old_inc)| {
                        compare_versions(&c.version, v) == Ordering::Greater
                            || (compare_versions(&c.version, v) == Ordering::Equal && !inc && *old_inc)
                    }) {
                        lower = Some((c.version.clone(), inc));
                    }
                }
                Op::Lt | Op::Le => {
                    let inc = matches!(c.op, Op::Le);
                    if upper.as_ref().map_or(true, |(v, old_inc)| {
                        compare_versions(&c.version, v) == Ordering::Less
                            || (compare_versions(&c.version, v) == Ordering::Equal && !inc && *old_inc)
                    }) {
                        upper = Some((c.version.clone(), inc));
                    }
                }
            }
        }
        let fmt = |v: &Version| {
            let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
            if !v.pre.is_empty() {
                s.push('-');
                s.push_str(&v.pre.join("."));
            }
            s
        };
        let lo = lower.map_or("*".to_string(), |(v, inc)| format!("{}{}", if inc { ">=" } else { ">" }, fmt(&v)));
        let hi = upper.map_or("*".to_string(), |(v, inc)| format!("{}{}", if inc { "<=" } else { "<" }, fmt(&v)));
        let mut anchors = arm.anchors.clone();
        anchors.sort();
        anchors.dedup();
        let a = if anchors.is_empty() { "-".into() } else {
            anchors.iter().map(|(x,y,z)| format!("{x}.{y}.{z}")).collect::<Vec<_>>().join(",")
        };
        arms.push(format!("[{lo};{hi};pre={a}]"));
    }
    arms.sort();
    arms.dedup();
    if arms.is_empty() { "EMPTY".into() } else { arms.join(" || ") }
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
    expr: &SetExpr,
    path: &mut Vec<usize>,
    package: &str,
    label: &str,
    out: &mut Vec<Range>,
) -> Result<(), SelectError> {
    match expr {
        SetExpr::Range(s) => {
            let r = parse_range(s).map_err(|error| SelectError::InvalidRange {
                package: package.into(),
                label: label.into(),
                path: path.clone(),
                error,
            })?;
            out.push(r);
        }
        SetExpr::Union(xs) | SetExpr::Intersection(xs) => {
            for (i, x) in xs.iter().enumerate() {
                path.push(i);
                validate_expr(x, path, package, label, out)?;
                path.pop();
            }
        }
    }
    Ok(())
}

fn eval_expr(expr: &SetExpr, v: &Version, include: bool) -> bool {
    match expr {
        SetExpr::Range(s) => parse_range(s).map(|r| matches_with(v, &r, include)).unwrap_or(false),
        SetExpr::Union(xs) => xs.iter().any(|x| eval_expr(x, v, include)),
        SetExpr::Intersection(xs) => xs.iter().all(|x| eval_expr(x, v, include)),
    }
}

fn conflict_labels(pkg: &Package, parsed: &[Version], include: bool) -> Vec<String> {
    if parsed.is_empty() {
        return Vec::new();
    }
    let n = pkg.constraints.len();
    for size in 1..=n {
        let mut found: Vec<String> = Vec::new();
        let mut indexes = Vec::new();
        fn search(
            start: usize,
            size: usize,
            n: usize,
            indexes: &mut Vec<usize>,
            found: &mut Vec<String>,
            pkg: &Package,
            parsed: &[Version],
            include: bool,
        ) {
            if indexes.len() == size {
                let satisfiable = parsed.iter().any(|v| {
                    indexes.iter().all(|&i| eval_expr(&pkg.constraints[i].expr, v, include))
                });
                if !satisfiable {
                    let mut labels: Vec<String> = indexes.iter().map(|&i| pkg.constraints[i].label.clone()).collect();
                    labels.sort_by(|a,b| a.as_bytes().cmp(b.as_bytes()));
                    found.push(labels);
                }
                return;
            }
            for i in start..n {
                indexes.push(i);
                search(i + 1, size, n, indexes, found, pkg, parsed, include);
                indexes.pop();
            }
        }
        search(0, size, n, &mut indexes, &mut found, pkg, parsed, include);
        if !found.is_empty() {
            found.sort_by(|a,b| {
                for (x,y) in a.iter().zip(b) {
                    let c = x.as_bytes().cmp(y.as_bytes());
                    if c != Ordering::Equal { return c; }
                }
                a.len().cmp(&b.len())
            });
            return found.remove(0);
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
            return Err(SelectError::DuplicatePackage { name: p.name.clone() });
        }
    }
    for p in packages {
        let mut labels = std::collections::HashSet::new();
        for c in &p.constraints {
            if !labels.insert(c.label.clone()) {
                return Err(SelectError::DuplicateLabel { package: p.name.clone(), label: c.label.clone() });
            }
        }
    }
    let mut parsed_packages: Vec<Vec<Version>> = Vec::new();
    for p in packages {
        let mut parsed = Vec::new();
        for (i, s) in p.candidates.iter().enumerate() {
            parsed.push(parse_version(s).map_err(|error| SelectError::InvalidCandidate {
                package: p.name.clone(), index: i, error
            })?);
        }
        parsed_packages.push(parsed);
    }
    for p in packages {
        for c in &p.constraints {
            validate_expr(&c.expr, &mut Vec::new(), &p.name, &c.label, &mut Vec::new())?;
        }
    }

    let mut selections = Vec::new();
    for (pi, p) in packages.iter().enumerate() {
        let parsed = &parsed_packages[pi];
        let mut qualifying: Vec<usize> = (0..parsed.len()).filter(|&i| {
            p.constraints.iter().all(|c| eval_expr(&c.expr, &parsed[i], include_prerelease))
        }).collect();
        if qualifying.is_empty() {
            return Err(SelectError::NoSolution {
                package: p.name.clone(),
                labels: conflict_labels(p, parsed, include_prerelease),
            });
        }
        if matches!(preference, Preference::StableFirst) {
            let stable: Vec<usize> = qualifying.iter().copied().filter(|&i| parsed[i].pre.is_empty()).collect();
            if !stable.is_empty() {
                qualifying = stable;
            }
        }
        let mut best = qualifying[0];
        for &i in qualifying.iter().skip(1) {
            let c = compare_versions(&parsed[i], &parsed[best]);
            let choose = match preference {
                Preference::Lowest => c == Ordering::Less,
                Preference::Highest | Preference::StableFirst => c == Ordering::Greater,
            };
            if choose {
                best = i;
            }
        }
        selections.push(Selection { package: p.name.clone(), version: p.candidates[best].clone() });
    }
    Ok(selections)
}