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
        if *pos == bytes.len() || matches!(bytes[*pos], b'.' | b'-' | b'+') {
            return Err(VersionError::MissingCore { at: start });
        }
        return Err(VersionError::InvalidChar { at: start });
    }

    let digits = &s[start..*pos];
    if digits.len() > 1 && bytes[start] == b'0' {
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
    numeric_pre: bool,
) -> Result<Vec<String>, VersionError> {
    if start == end {
        return Err(VersionError::EmptyIdentifier { at: start });
    }

    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut id_start = start;

    loop {
        let mut id_end = id_start;
        while id_end < end && bytes[id_end] != b'.' {
            let b = bytes[id_end];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: id_end });
            }
            id_end += 1;
        }

        if id_end == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }

        let ident = &s[id_start..id_end];
        if numeric_pre
            && ident.len() > 1
            && ident.as_bytes().iter().all(|b| is_digit(*b))
            && ident.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }
        result.push(ident.to_string());

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

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let mut pos = 0;
    let major = parse_core(s, &mut pos)?;
    for _field in 0..2 {
        if pos >= s.len() || s.as_bytes()[pos] != b'.' {
            if pos == s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            return Err(VersionError::InvalidChar { at: pos });
        }
        pos += 1;
        if _field == 0 {
            let minor = parse_core(s, &mut pos)?;
            if pos >= s.len() || s.as_bytes()[pos] != b'.' {
                if pos == s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
                    return Err(VersionError::MissingCore { at: pos });
                }
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
            let patch = parse_core(s, &mut pos)?;
            let (pre, build) = parse_version_tail(s, pos)?;
            return Ok(Version { major, minor, patch, pre, build });
        }
    }

    unreachable!()
}

fn parse_version_tail(s: &str, mut pos: usize) -> Result<(Vec<String>, Vec<String>), VersionError> {
    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos == s.len() {
        return Ok((pre, build));
    }

    match s.as_bytes()[pos] {
        b'-' => {
            pos += 1;
            let plus = s[pos..].find('+').map(|i| pos + i);
            let pre_end = plus.unwrap_or(s.len());
            pre = parse_identifiers(s, pos, pre_end, true)?;
            pos = pre_end;
            if pos < s.len() {
                pos += 1;
                build = parse_identifiers(s, pos, s.len(), false)?;
            }
        }
        b'+' => {
            pos += 1;
            build = parse_identifiers(s, pos, s.len(), false)?;
        }
        _ => return Err(VersionError::InvalidChar { at: pos }),
    }
    Ok((pre, build))
}

fn ident_numeric(s: &str) -> bool {
    !s.is_empty() && s.as_bytes().iter().all(|b| is_digit(*b))
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
                    let xn = ident_numeric(x);
                    let yn = ident_numeric(y);
                    let ord = match (xn, yn) {
                        (true, true) => cmp_numeric_ident(x, y),
                        (true, false) => Ordering::Less,
                        (false, true) => Ordering::Greater,
                        (false, false) => x.as_bytes().cmp(y.as_bytes()),
                    };
                    if ord != Ordering::Equal {
                        return ord;
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

#[derive(Clone, Debug)]
struct Bound {
    version: Version,
    inclusive: bool,
}

#[derive(Clone, Debug)]
struct Arm {
    lower: Option<Bound>,
    upper: Option<Bound>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Copy, Debug)]
struct Token<'a> {
    text: &'a str,
    start: usize,
}

#[derive(Clone, Debug)]
struct ParsedOperand {
    version: Option<Version>,
    nums: Vec<u64>,
    k: usize,
}

fn ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokenize(s: &str) -> Vec<Token<'_>> {
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
        out.push(Token { text: &s[start..i], start });
    }
    out
}

fn full_operand(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

fn parse_partial_or_full(token: &str, at: usize) -> Result<ParsedOperand, RangeError> {
    let has_suffix = token.as_bytes().iter().any(|b| matches!(b, b'-' | b'+'));
    let fields: Vec<&str> = token.split('.').collect();
    let has_wild = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));

    if has_suffix || (fields.len() >= 3 && !has_wild) {
        let v = full_operand(token, at)?;
        return Ok(ParsedOperand {
            nums: vec![v.major, v.minor, v.patch],
            k: 3,
            version: Some(v),
        });
    }

    if fields.len() > 3 {
        return Err(RangeError::Syntax { at });
    }

    let mut nums = Vec::new();
    let mut wildcard_seen = false;
    for f in &fields {
        if f.is_empty() {
            return Err(RangeError::Syntax { at });
        }
        if matches!(*f, "*" | "x" | "X") {
            wildcard_seen = true;
        } else {
            if wildcard_seen
                || !f.as_bytes().iter().all(|b| is_digit(*b))
                || (f.len() > 1 && f.as_bytes()[0] == b'0')
            {
                return Err(RangeError::Syntax { at });
            }
            nums.push(f.parse::<u64>().map_err(|_| RangeError::Syntax { at })?);
        }
    }

    let k = nums.len();
    Ok(ParsedOperand { version: None, nums, k })
}

fn tuple_of(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn version_from_tuple(t: (u64, u64, u64), pre: Vec<String>) -> Version {
    Version { major: t.0, minor: t.1, patch: t.2, pre, build: Vec::new() }
}

fn padded(nums: &[u64]) -> (u64, u64, u64) {
    (nums.first().copied().unwrap_or(0), nums.get(1).copied().unwrap_or(0), nums.get(2).copied().unwrap_or(0))
}

fn next_tuple(nums: &[u64], k: usize, mode: &str) -> Option<(u64, u64, u64)> {
    let mut t = [0u64; 3];
    for (i, n) in nums.iter().enumerate().take(3) {
        t[i] = *n;
    }

    if mode == "bare" || mode == "hyphen" {
        match k {
            1 => { t[0] = t[0].checked_add(1)?; t[1] = 0; t[2] = 0; }
            2 => { t[1] = t[1].checked_add(1)?; t[2] = 0; }
            _ => return None,
        }
    } else if mode == "tilde" {
        if k == 1 { t[0] = t[0].checked_add(1)?; t[1] = 0; t[2] = 0; }
        else { t[1] = t[1].checked_add(1)?; t[2] = 0; }
    } else if t[0] > 0 || k == 1 {
        t[0] = t[0].checked_add(1)?; t[1] = 0; t[2] = 0;
    } else if t[1] > 0 || k == 2 {
        t[1] = t[1].checked_add(1)?; t[2] = 0;
    } else {
        t[2] = t[2].checked_add(1)?;
    }
    Some((t[0], t[1], t[2]))
}

fn add_lower(arm: &mut Arm, version: Version, inclusive: bool) {
    let bound = Bound { version, inclusive };
    match &arm.lower {
        None => arm.lower = Some(bound),
        Some(old) => {
            let ord = compare_versions(&bound.version, &old.version);
            if ord == Ordering::Greater || (ord == Ordering::Equal && !inclusive && old.inclusive) {
                arm.lower = Some(bound);
            }
        }
    }
}

fn add_upper(arm: &mut Arm, version: Version, inclusive: bool) {
    let bound = Bound { version, inclusive };
    match &arm.upper {
        None => arm.upper = Some(bound),
        Some(old) => {
            let ord = compare_versions(&bound.version, &old.version);
            if ord == Ordering::Less || (ord == Ordering::Equal && !inclusive && old.inclusive) {
                arm.upper = Some(bound);
            }
        }
    }
}

fn add_anchor(arm: &mut Arm, version: &Version) {
    if !version.pre.is_empty() {
        arm.anchors.push(tuple_of(version));
    }
}

fn parse_bare(token: Token<'_>, arm: &mut Arm) -> Result<(), RangeError> {
    let op = parse_partial_or_full(token.text, token.start)?;
    if let Some(v) = op.version {
        add_anchor(arm, &v);
        add_lower(arm, v.clone(), true);
        add_upper(arm, v, true);
    } else if op.k > 0 {
        add_lower(arm, version_from_tuple(padded(&op.nums), Vec::new()), true);
        if op.k < 3 {
            let upper = next_tuple(&op.nums, op.k, "bare")
                .ok_or(RangeError::Syntax { at: token.start })?;
            add_upper(arm, version_from_tuple(upper, vec!["0".into()]), false);
        }
    }
    Ok(())
}

fn parse_caret_tilde(token: Token<'_>, arm: &mut Arm, caret: bool) -> Result<(), RangeError> {
    let operand = &token.text[1..];
    let at = token.start + 1;
    let op = parse_partial_or_full(operand, at)?;
    if op.k == 0 {
        return Err(RangeError::Syntax { at: token.start });
    }

    let lower = if let Some(v) = op.version.clone() {
        v
    } else {
        version_from_tuple(padded(&op.nums), Vec::new())
    };
    add_anchor(arm, &lower);
    add_lower(arm, lower, true);

    let upper = next_tuple(&op.nums, op.k, if caret { "caret" } else { "tilde" })
        .ok_or(RangeError::Syntax { at })?;
    add_upper(arm, version_from_tuple(upper, vec!["0".into()]), false);
    Ok(())
}

fn parse_comparator(token: Token<'_>, arm: &mut Arm) -> Result<(), RangeError> {
    let bytes = token.text.as_bytes();
    let (op_len, op) = if token.text.starts_with(">=") {
        (2, ">=")
    } else if token.text.starts_with("<=") {
        (2, "<=")
    } else if token.text.starts_with('>') {
        (1, ">")
    } else if token.text.starts_with('<') {
        (1, "<")
    } else if token.text.starts_with('=') {
        (1, "=")
    } else {
        return Err(RangeError::Syntax { at: token.start });
    };

    if bytes.len() == op_len || !is_digit(bytes[op_len]) {
        return Err(RangeError::Syntax { at: token.start });
    }
    let at = token.start + op_len;
    let v = full_operand(&token.text[op_len..], at)?;
    add_anchor(arm, &v);
    match op {
        ">" => add_lower(arm, v, false),
        ">=" => add_lower(arm, v, true),
        "<" => add_upper(arm, v, false),
        "<=" => add_upper(arm, v, true),
        "=" => { add_lower(arm, v.clone(), true); add_upper(arm, v, true); }
        _ => {}
    }
    Ok(())
}

fn parse_hyphen_arm(tokens: &[Token<'_>], arm: &mut Arm) -> Result<(), RangeError> {
    let minus = tokens.iter().find(|t| t.text == "-").unwrap();
    if tokens.len() != 3 || tokens[1].text != "-" {
        return Err(RangeError::Syntax { at: minus.start });
    }

    let lower = parse_partial_or_full(tokens[0].text, tokens[0].start)?;
    let upper = parse_partial_or_full(tokens[2].text, tokens[2].start)?;
    if lower.k == 0 {
        return Err(RangeError::Syntax { at: tokens[0].start });
    }
    if upper.k == 0 {
        return Err(RangeError::Syntax { at: tokens[2].start });
    }

    let lower_v = lower.version.clone().unwrap_or_else(|| version_from_tuple(padded(&lower.nums), Vec::new()));
    add_anchor(arm, &lower_v);
    add_lower(arm, lower_v, true);

    if let Some(v) = upper.version {
        add_anchor(arm, &v);
        add_upper(arm, v, true);
    } else if upper.k == 3 {
        add_upper(arm, version_from_tuple(padded(&upper.nums), Vec::new()), true);
    } else {
        let tuple = next_tuple(&upper.nums, upper.k, "hyphen")
            .ok_or(RangeError::Syntax { at: tokens[2].start })?;
        add_upper(arm, version_from_tuple(tuple, vec!["0".into()]), false);
    }
    Ok(())
}

fn parse_arm_tokens(tokens: &[Token<'_>]) -> Result<Arm, RangeError> {
    let mut arm = Arm { lower: None, upper: None, anchors: Vec::new() };
    if tokens.iter().any(|t| t.text == "-") {
        parse_hyphen_arm(tokens, &mut arm)?;
        return Ok(arm);
    }

    for token in tokens {
        match token.text.as_bytes()[0] {
            b'^' => parse_caret_tilde(*token, &mut arm, true)?,
            b'~' => parse_caret_tilde(*token, &mut arm, false)?,
            b'<' | b'>' | b'=' => parse_comparator(*token, &mut arm)?,
            b'*' | b'x' | b'X' | b'0'..=b'9' => parse_bare(*token, &mut arm)?,
            _ => return Err(RangeError::Syntax { at: token.start }),
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
    let mut last_separator = false;
    for token in tokens {
        if token.text == "||" {
            if current.is_empty() {
                return Err(RangeError::Empty { at: token.start });
            }
            arms.push(parse_arm_tokens(&current)?);
            current.clear();
            last_separator = true;
        } else {
            current.push(token);
            last_separator = false;
        }
    }

    if current.is_empty() {
        if last_separator {
            return Err(RangeError::Empty { at: s.len() });
        }
        return Err(RangeError::Empty { at: 0 });
    }
    arms.push(parse_arm_tokens(&current)?);
    Ok(Range { arms })
}

fn arm_matches(v: &Version, arm: &Arm, include_pre: bool) -> bool {
    if !v.pre.is_empty() && !include_pre && !arm.anchors.contains(&tuple_of(v)) {
        return false;
    }
    if let Some(lower) = &arm.lower {
        let ord = compare_versions(v, &lower.version);
        if ord == Ordering::Less || (ord == Ordering::Equal && !lower.inclusive) {
            return false;
        }
    }
    if let Some(upper) = &arm.upper {
        let ord = compare_versions(v, &upper.version);
        if ord == Ordering::Greater || (ord == Ordering::Equal && !upper.inclusive) {
            return false;
        }
    }
    true
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|arm| arm_matches(v, arm, include_prerelease))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

pub fn max_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, true)
}

pub fn min_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, false)
}

fn extrema(candidates: &[String], r: &Range, maximum: bool) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, s) in candidates.iter().enumerate() {
        parsed.push(parse_version(s).map_err(|error| CandidateError { index, error })?);
    }

    let mut best: Option<usize> = None;
    for i in 0..parsed.len() {
        if !matches(&parsed[i], r) {
            continue;
        }
        best = match best {
            None => Some(i),
            Some(j) => {
                let ord = compare_versions(&parsed[i], &parsed[j]);
                if (maximum && ord == Ordering::Greater) || (!maximum && ord == Ordering::Less) {
                    Some(i)
                } else {
                    Some(j)
                }
            }
        };
    }
    Ok(best.map(|i| candidates[i].clone()))
}

fn version_without_build(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

pub fn canonical(r: &Range) -> String {
    let mut arms = Vec::new();
    for arm in &r.arms {
        let lower = arm.lower.as_ref().map(|b| format!("{}{}", if b.inclusive { ">=" } else { ">" }, version_without_build(&b.version)));
        let upper = arm.upper.as_ref().map(|b| format!("{}{}", if b.inclusive { "<=" } else { "<" }, version_without_build(&b.version)));
        let mut anchors = arm.anchors.clone();
        anchors.sort();
        anchors.dedup();
        let anchor_text = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors.iter().map(|(a,b,c)| format!("{a}.{b}.{c}")).collect::<Vec<_>>().join(",")
        };
        arms.push(format!("[{};{};pre={}]", lower.unwrap_or_else(|| "*".into()), upper.unwrap_or_else(|| "*".into()), anchor_text));
    }
    arms.sort();
    arms.dedup();
    if arms.is_empty() { "EMPTY".into() } else { arms.join(" || ") }
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for aa in &a.arms {
        for bb in &b.arms {
            let mut points = Vec::<Version>::new();
            for arm in [aa, bb] {
                if let Some(bound) = &arm.lower {
                    points.push(bound.version.clone());
                    let mut stable = bound.version.clone();
                    stable.pre.clear();
                    stable.build.clear();
                    points.push(stable);
                }
                if let Some(bound) = &arm.upper {
                    points.push(bound.version.clone());
                    let mut stable = bound.version.clone();
                    stable.pre.clear();
                    stable.build.clear();
                    points.push(stable);
                }
                for tuple in &arm.anchors {
                    points.push(version_from_tuple(*tuple, vec!["0".into()]));
                    points.push(version_from_tuple(*tuple, Vec::new()));
                }
            }
            points.push(version_from_tuple((0, 0, 0), Vec::new()));
            points.push(version_from_tuple((u64::MAX, u64::MAX, u64::MAX), Vec::new()));
            for v in points {
                if arm_matches(&v, aa, include_prerelease) && arm_matches(&v, bb, include_prerelease) {
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
    DuplicatePackage { name: String },
    DuplicateLabel { package: String, label: String },
    InvalidCandidate { package: String, index: usize, error: VersionError },
    InvalidRange { package: String, label: String, path: Vec<usize>, error: RangeError },
    NoSolution { package: String, labels: Vec<String> },
}

fn validate_expr(expr: &SetExpr, package: &str, label: &str, path: &mut Vec<usize>) -> Result<(), SelectError> {
    match expr {
        SetExpr::Range(s) => parse_range(s).map(|_| ()).map_err(|error| SelectError::InvalidRange {
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
        SetExpr::Range(s) => parse_range(s).map(|r| matches_with(v, &r, include_pre)).unwrap_or(false),
        SetExpr::Union(children) => children.iter().any(|c| expr_matches(c, v, include_pre)),
        SetExpr::Intersection(children) => children.iter().all(|c| expr_matches(c, v, include_pre)),
    }
}

fn minimum_conflict(constraints: &[Constraint], candidates: &[Version], include_pre: bool) -> Vec<String> {
    if candidates.is_empty() {
        return Vec::new();
    }

    fn choose(
        start: usize,
        need: usize,
        idx: &mut Vec<usize>,
        constraints: &[Constraint],
        candidates: &[Version],
        include_pre: bool,
    ) -> Option<Vec<String>> {
        if need == 0 {
            let satisfiable = candidates.iter().any(|v| idx.iter().all(|i| expr_matches(&constraints[*i].expr, v, include_pre)));
            if !satisfiable {
                let mut labels = idx.iter().map(|i| constraints[*i].label.clone()).collect::<Vec<_>>();
                labels.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
                return Some(labels);
            }
            return None;
        }
        if constraints.len().saturating_sub(start) < need {
            return None;
        }
        for i in start..=(constraints.len() - need) {
            idx.push(i);
            if let Some(found) = choose(i + 1, need - 1, idx, constraints, candidates, include_pre) {
                return Some(found);
            }
            idx.pop();
        }
        None
    }

    for size in 1..=constraints.len() {
        if let Some(labels) = choose(0, size, &mut Vec::new(), constraints, candidates, include_pre) {
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

    let mut parsed_candidates = Vec::new();
    for p in packages {
        let mut parsed = Vec::new();
        for (index, s) in p.candidates.iter().enumerate() {
            parsed.push(parse_version(s).map_err(|error| SelectError::InvalidCandidate {
                package: p.name.clone(), index, error,
            })?);
        }
        parsed_candidates.push(parsed);
    }

    for p in packages {
        for c in &p.constraints {
            validate_expr(&c.expr, &p.name, &c.label, &mut Vec::new())?;
        }
    }

    let mut selections = Vec::new();
    for (pi, p) in packages.iter().enumerate() {
        let mut best: Option<usize> = None;
        for i in 0..p.candidates.len() {
            let version = &parsed_candidates[pi][i];
            if !p.constraints.iter().all(|c| expr_matches(&c.expr, version, include_prerelease)) {
                continue;
            }
            best = match best {
                None => Some(i),
                Some(j) => {
                    let vi = &parsed_candidates[pi][i];
                    let vj = &parsed_candidates[pi][j];
                    let stable_pref = matches!(preference, Preference::StableFirst);
                    if stable_pref && vi.pre.is_empty() != vj.pre.is_empty() {
                        if vi.pre.is_empty() { Some(i) } else { Some(j) }
                    } else {
                        let ord = compare_versions(vi, vj);
                        match preference {
                            Preference::Lowest if ord == Ordering::Less => Some(i),
                            Preference::Highest | Preference::StableFirst if ord == Ordering::Greater => Some(i),
                            _ => Some(j),
                        }
                    }
                }
            };
        }

        if let Some(i) = best {
            selections.push(Selection { package: p.name.clone(), version: p.candidates[i].clone() });
        } else {
            let labels = minimum_conflict(&p.constraints, &parsed_candidates[pi], include_prerelease);
            return Err(SelectError::NoSolution { package: p.name.clone(), labels });
        }
    }
    Ok(selections)
}