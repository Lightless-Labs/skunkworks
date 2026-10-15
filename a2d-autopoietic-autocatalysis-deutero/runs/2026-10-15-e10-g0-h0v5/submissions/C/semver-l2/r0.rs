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

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

fn parse_core(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && is_digit(bytes[*pos]) {
        *pos += 1;
    }
    if *pos == start {
        return Err(if start == bytes.len() || matches!(bytes[start], b'.' | b'-' | b'+') {
            VersionError::MissingCore { at: start }
        } else {
            VersionError::InvalidChar { at: start }
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

fn require_dot(s: &str, pos: &mut usize) -> Result<(), VersionError> {
    let bytes = s.as_bytes();
    if *pos >= bytes.len() || matches!(bytes[*pos], b'-' | b'+') {
        return Err(VersionError::MissingCore { at: *pos });
    }
    if bytes[*pos] != b'.' {
        return Err(VersionError::InvalidChar { at: *pos });
    }
    *pos += 1;
    Ok(())
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
            if !is_ident_char(bytes[i]) {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }
        if i == ident_start {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }

        let ident = &s[ident_start..i];
        if prerelease
            && ident.bytes().all(is_digit)
            && ident.len() > 1
            && ident.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: ident_start });
        }
        result.push(ident.to_string());

        if i == end {
            break;
        }
        ident_start = i + 1;
        if ident_start == end {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
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
    require_dot(s, &mut pos)?;
    let minor = parse_core(s, &mut pos)?;
    require_dot(s, &mut pos)?;
    let patch = parse_core(s, &mut pos)?;

    let bytes = s.as_bytes();
    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] == b'-' {
        let pre_start = pos + 1;
        let plus = bytes[pre_start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|p| pre_start + p)
            .unwrap_or(bytes.len());
        pre = parse_identifiers(s, pre_start, plus, true)?;
        pos = plus;
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        let build_start = pos + 1;
        build = parse_identifiers(s, build_start, bytes.len(), false)?;
        pos = bytes.len();
    }

    if pos < bytes.len() {
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

fn core_cmp(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch))
}

fn numeric_ident(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }

    for (x, y) in a.iter().zip(b.iter()) {
        let xn = numeric_ident(x);
        let yn = numeric_ident(y);
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
    core_cmp(a, b).then_with(|| compare_pre(&a.pre, &b.pre))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

#[derive(Clone, Debug)]
struct Arm {
    checks: Vec<Check>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug)]
struct Check {
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

fn check_matches(v: &Version, c: &Check) -> bool {
    let ord = compare_versions(v, &c.version);
    match c.op {
        Op::Eq => ord == Ordering::Equal,
        Op::Lt => ord == Ordering::Less,
        Op::Le => ord != Ordering::Greater,
        Op::Gt => ord == Ordering::Greater,
        Op::Ge => ord != Ordering::Less,
    }
}

fn admitted(v: &Version, anchors: &[(u64, u64, u64)]) -> bool {
    v.pre.is_empty()
        || anchors
            .iter()
            .any(|&(a, b, c)| (v.major, v.minor, v.patch) == (a, b, c))
}

fn arm_matches(v: &Version, arm: &Arm) -> bool {
    admitted(v, &arm.anchors) && arm.checks.iter().all(|c| check_matches(v, c))
}

fn ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

#[derive(Clone, Debug)]
struct Token {
    text: String,
    start: usize,
}

fn tokenize(s: &str) -> Vec<Token> {
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
        out.push(Token {
            text: s[start..i].to_string(),
            start,
        });
    }
    out
}

#[derive(Clone, Debug)]
struct Partial {
    nums: Vec<u64>,
    full: Option<Version>,
}

fn wildcard(s: &str) -> bool {
    matches!(s, "*" | "x" | "X")
}

fn contains_suffix_or_full_shape(s: &str) -> bool {
    s.contains('-')
        || s.contains('+')
        || (s.split('.').count() >= 3 && !s.split('.').any(wildcard))
}

fn parse_partial(s: &str, start: usize) -> Result<Partial, RangeError> {
    if contains_suffix_or_full_shape(s) {
        return parse_version(s)
            .map(|v| Partial {
                nums: vec![v.major, v.minor, v.patch],
                full: Some(v),
            })
            .map_err(|error| RangeError::Version { at: start, error });
    }

    let fields: Vec<&str> = s.split('.').collect();
    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|f| f.is_empty()) {
        return Err(RangeError::Syntax { at: start });
    }

    let mut nums = Vec::new();
    let mut seen_wildcard = false;
    for field in fields {
        if wildcard(field) {
            seen_wildcard = true;
        } else if seen_wildcard
            || !field.bytes().all(is_digit)
            || (field.len() > 1 && field.as_bytes()[0] == b'0')
        {
            return Err(RangeError::Syntax { at: start });
        } else {
            let n = field
                .parse::<u64>()
                .map_err(|_| RangeError::Syntax { at: start })?;
            nums.push(n);
        }
    }
    Ok(Partial { nums, full: None })
}

fn padded(p: &Partial) -> Version {
    Version {
        major: *p.nums.first().unwrap_or(&0),
        minor: *p.nums.get(1).unwrap_or(&0),
        patch: *p.nums.get(2).unwrap_or(&0),
        pre: p.full.as_ref().map(|v| v.pre.clone()).unwrap_or_default(),
        build: p.full.as_ref().map(|v| v.build.clone()).unwrap_or_default(),
    }
}

fn bump(v: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let mut out = Version {
        major: v.major,
        minor: v.minor,
        patch: v.patch,
        pre: vec!["0".to_string()],
        build: Vec::new(),
    };

    let overflow = match field {
        0 => {
            if out.major == u64::MAX {
                true
            } else {
                out.major += 1;
                out.minor = 0;
                out.patch = 0;
                false
            }
        }
        1 => {
            if out.minor == u64::MAX {
                true
            } else {
                out.minor += 1;
                out.patch = 0;
                false
            }
        }
        _ => {
            if out.patch == u64::MAX {
                true
            } else {
                out.patch += 1;
                false
            }
        }
    };

    if overflow {
        Err(RangeError::Syntax { at })
    } else {
        Ok(out)
    }
}

fn push_check(arm: &mut Arm, op: Op, version: Version, explicit: bool) {
    if explicit && !version.pre.is_empty() {
        arm.anchors
            .push((version.major, version.minor, version.patch));
    }
    arm.checks.push(Check { op, version });
}

fn process_expanded(arm: &mut Arm, token: &Token) -> Result<(), RangeError> {
    let (kind, operand, operand_start) = if token.text.starts_with('^') {
        ('^', &token.text[1..], token.start + 1)
    } else if token.text.starts_with('~') {
        ('~', &token.text[1..], token.start + 1)
    } else {
        ('b', token.text.as_str(), token.start)
    };

    let partial = parse_partial(operand, operand_start)?;
    if kind != 'b' && partial.nums.is_empty() {
        return Err(RangeError::Syntax { at: token.start });
    }
    let lower = padded(&partial);

    if let Some(full) = partial.full.clone() {
        if kind == 'b' {
            push_check(arm, Op::Eq, full, true);
            return Ok(());
        }
    }

    if kind == 'b' {
        let k = partial.nums.len();
        if k == 0 {
            return Ok(());
        }
        push_check(arm, Op::Ge, lower.clone(), false);
        let upper = bump(&lower, if k == 1 { 0 } else { 1 }, operand_start)?;
        push_check(arm, Op::Lt, upper, false);
    } else {
        push_check(arm, Op::Ge, lower.clone(), true);
        let k = partial.nums.len();
        let field = if kind == '^' {
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
        let upper = bump(&lower, field, operand_start)?;
        push_check(arm, Op::Lt, upper, false);
    }
    Ok(())
}

fn comparator(token: &str) -> Option<(Op, usize)> {
    if token.starts_with(">=") {
        Some((Op::Ge, 2))
    } else if token.starts_with("<=") {
        Some((Op::Le, 2))
    } else if token.starts_with('>') {
        Some((Op::Gt, 1))
    } else if token.starts_with('<') {
        Some((Op::Lt, 1))
    } else if token.starts_with('=') {
        Some((Op::Eq, 1))
    } else {
        None
    }
}

fn parse_full_comparator(arm: &mut Arm, token: &Token) -> Result<(), RangeError> {
    let (op, oplen) = comparator(&token.text).unwrap_or((Op::Eq, 0));
    let operand = &token.text[oplen..];
    let at = token.start + oplen;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: token.start });
    }
    let version = parse_version(operand)
        .map_err(|error| RangeError::Version { at, error })?;
    push_check(arm, op, version, true);
    Ok(())
}

fn parse_hyphen_arm(tokens: &[Token], arm: &mut Arm) -> Result<(), RangeError> {
    let minus_positions: Vec<usize> = tokens
        .iter()
        .filter(|token| token.text == "-")
        .map(|token| token.start)
        .collect();

    if !minus_positions.is_empty() && (tokens.len() != 3 || tokens[1].text != "-") {
        return Err(RangeError::Syntax {
            at: minus_positions[0],
        });
    }
    if tokens.len() != 3 || tokens[1].text != "-" {
        return Err(RangeError::Syntax {
            at: tokens.first().map(|token| token.start).unwrap_or(0),
        });
    }

    let lower_partial = parse_partial(&tokens[0].text, tokens[0].start)?;
    let upper_partial = parse_partial(&tokens[2].text, tokens[2].start)?;

    if lower_partial.nums.is_empty() {
        return Err(RangeError::Syntax {
            at: tokens[0].start,
        });
    }
    if upper_partial.nums.is_empty() {
        return Err(RangeError::Syntax {
            at: tokens[2].start,
        });
    }

    let lower = padded(&lower_partial);
    let lower_explicit_pre = lower_partial
        .full
        .as_ref()
        .map_or(false, |version| !version.pre.is_empty());
    push_check(arm, Op::Ge, lower, lower_explicit_pre);

    if let Some(full) = upper_partial.full.clone() {
        let explicit_pre = !full.pre.is_empty();
        push_check(arm, Op::Le, full, explicit_pre);
    } else {
        let upper = padded(&upper_partial);
        let k = upper_partial.nums.len();
        let bound = bump(&upper, if k == 1 { 0 } else { 1 }, tokens[2].start)?;
        push_check(arm, Op::Lt, bound, false);
    }
    Ok(())
}

fn parse_arm(tokens: &[Token]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        checks: Vec::new(),
        anchors: Vec::new(),
    };

    if tokens.iter().any(|token| token.text == "-") {
        parse_hyphen_arm(tokens, &mut arm)?;
        return Ok(arm);
    }

    for token in tokens {
        let first = token.text.as_bytes().first().copied().unwrap_or(0);
        if matches!(first, b'<' | b'>' | b'=') {
            parse_full_comparator(&mut arm, token)?;
        } else if matches!(first, b'^' | b'~')
            || first.is_ascii_digit()
            || matches!(first, b'*' | b'x' | b'X')
        {
            process_expanded(&mut arm, token)?;
        } else {
            return Err(RangeError::Syntax { at: token.start });
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
    let mut previous_separator: Option<usize> = None;

    for token in &tokens {
        if token.text == "||" {
            if current.is_empty() {
                return Err(RangeError::Empty {
                    at: previous_separator.unwrap_or(token.start),
                });
            }
            arms.push(parse_arm(&current)?);
            current.clear();
            previous_separator = Some(token.start);
        } else {
            current.push(token.clone());
        }
    }

    if current.is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(&current)?);
    Ok(Range { arms })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| arm_matches(v, arm))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn satisfying_extreme(
    candidates: &[String],
    range: &Range,
    greatest: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let version = parse_version(candidate).map_err(|error| CandidateError { index, error })?;
        parsed.push(version);
    }

    let mut best: Option<usize> = None;
    for (index, version) in parsed.iter().enumerate() {
        if !matches(version, range) {
            continue;
        }

        if let Some(best_index) = best {
            let ordering = compare_versions(version, &parsed[best_index]);
            if (greatest && ordering == Ordering::Greater)
                || (!greatest && ordering == Ordering::Less)
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
    r: &Range,
) -> Result<Option<String>, CandidateError> {
    satisfying_extreme(candidates, r, true)
}

pub fn min_satisfying(
    candidates: &[String],
    r: &Range,
) -> Result<Option<String>, CandidateError> {
    satisfying_extreme(candidates, r, false)
}

fn main() {}