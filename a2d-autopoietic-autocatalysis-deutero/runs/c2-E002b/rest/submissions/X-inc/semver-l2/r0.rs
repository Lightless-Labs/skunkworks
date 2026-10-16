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

fn byte(s: &str, at: usize) -> Option<u8> {
    s.as_bytes().get(at).copied()
}

fn parse_core_field(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let start = *pos;
    while byte(s, *pos).is_some_and(|b| b.is_ascii_digit()) {
        *pos += 1;
    }

    if *pos == start {
        return match byte(s, start) {
            None | Some(b'.' | b'-' | b'+') => Err(VersionError::MissingCore { at: start }),
            _ => Err(VersionError::InvalidChar { at: start }),
        };
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
    start: usize,
    end: usize,
    is_pre: bool,
) -> Result<Vec<String>, VersionError> {
    let mut result = Vec::new();
    let mut ident_start = start;

    loop {
        let mut i = ident_start;
        while i < end && byte(s, i) != Some(b'.') {
            let b = s.as_bytes()[i];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }

        if i == ident_start {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }

        let ident = &s[ident_start..i];
        if is_pre
            && ident.bytes().all(|b| b.is_ascii_digit())
            && ident.len() > 1
            && ident.starts_with('0')
        {
            return Err(VersionError::LeadingZero { at: ident_start });
        }
        result.push(ident.to_owned());

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
    let major = parse_core_field(s, &mut pos)?;

    if byte(s, pos) != Some(b'.') {
        return match byte(s, pos) {
            None | Some(b'-' | b'+') => Err(VersionError::MissingCore { at: pos }),
            _ => Err(VersionError::InvalidChar { at: pos }),
        };
    }
    pos += 1;

    let minor = parse_core_field(s, &mut pos)?;

    if byte(s, pos) != Some(b'.') {
        return match byte(s, pos) {
            None | Some(b'-' | b'+') => Err(VersionError::MissingCore { at: pos }),
            _ => Err(VersionError::InvalidChar { at: pos }),
        };
    }
    pos += 1;

    let patch = parse_core_field(s, &mut pos)?;

    let mut pre = Vec::new();
    let mut build = Vec::new();

    match byte(s, pos) {
        None => {}
        Some(b'-') => {
            let pre_start = pos + 1;
            let build_start = s.as_bytes()[pre_start..]
                .iter()
                .position(|&b| b == b'+')
                .map(|offset| pre_start + offset);

            let pre_end = build_start.unwrap_or(s.len());
            pre = parse_identifiers(s, pre_start, pre_end, true)?;

            if let Some(build_start) = build_start {
                let identifiers_start = build_start + 1;
                build = parse_identifiers(s, identifiers_start, s.len(), false)?;
            }
        }
        Some(b'+') => {
            build = parse_identifiers(s, pos + 1, s.len(), false)?;
        }
        _ => return Err(VersionError::InvalidChar { at: pos }),
    }

    Ok(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch)
        .cmp(&(b.major, b.minor, b.patch))
        .then_with(|| compare_pre(&a.pre, &b.pre))
}

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }

    for (x, y) in a.iter().zip(b) {
        let xn = x.bytes().all(|b| b.is_ascii_digit());
        let yn = y.bytes().all(|b| b.is_ascii_digit());
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

fn same_core(a: &Version, b: &Version) -> bool {
    (a.major, a.minor, a.patch) == (b.major, b.minor, b.patch)
}

fn ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

#[derive(Clone, Debug)]
struct Token<'a> {
    text: &'a str,
    start: usize,
}

fn tokenize(s: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < s.len() {
        while i < s.len() && ws(s.as_bytes()[i]) {
            i += 1;
        }
        if i == s.len() {
            break;
        }
        let start = i;
        while i < s.len() && !ws(s.as_bytes()[i]) {
            i += 1;
        }
        tokens.push(Token {
            text: &s[start..i],
            start,
        });
    }
    tokens
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
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
struct Comparator {
    op: Op,
    version: Version,
    anchor: bool,
}

#[derive(Clone, Debug)]
struct Arm {
    comparators: Vec<Comparator>,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let all = tokenize(s);
    if all.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut groups: Vec<Vec<Token<'_>>> = vec![Vec::new()];
    let mut separators = Vec::new();
    for token in all {
        if token.text == "||" {
            separators.push(token.start);
            groups.push(Vec::new());
        } else {
            groups.last_mut().unwrap().push(token);
        }
    }

    for (i, group) in groups.iter().enumerate() {
        if group.is_empty() {
            let at = if i == groups.len() - 1 {
                s.len()
            } else {
                separators[i.saturating_sub(1)]
            };
            return Err(RangeError::Empty { at });
        }
    }

    let mut arms = Vec::new();
    for group in groups {
        arms.push(parse_arm(&group)?);
    }
    Ok(Range { arms })
}

fn parse_arm(tokens: &[Token<'_>]) -> Result<Arm, RangeError> {
    if let Some(minus) = tokens.iter().find(|t| t.text == "-") {
        if tokens.len() != 3 || tokens[1].text != "-" {
            return Err(RangeError::Syntax { at: minus.start });
        }
        return parse_hyphen(&tokens[0], &tokens[2]);
    }

    let mut comparators = Vec::new();
    for token in tokens {
        parse_token(token, &mut comparators)?;
    }
    Ok(Arm { comparators })
}

fn parse_token(token: &Token<'_>, out: &mut Vec<Comparator>) -> Result<(), RangeError> {
    let text = token.text;
    let first = text.as_bytes()[0];

    if first == b'^' || first == b'~' {
        let operand = &text[1..];
        let start = token.start + 1;
        let (lower, k, full) = parse_partial_or_full(operand, start)?;
        if k == 0 {
            return Err(RangeError::Syntax { at: token.start });
        }

        let upper = if first == b'^' {
            caret_upper(&lower, k)
        } else {
            tilde_upper(&lower, k)
        }
        .ok_or(RangeError::Syntax { at: start })?;

        out.push(Comparator {
            op: Op::Ge,
            anchor: full && !lower.pre.is_empty(),
            version: lower.clone(),
        });
        out.push(Comparator {
            op: Op::Lt,
            anchor: false,
            version: upper,
        });
        return Ok(());
    }

    if matches!(first, b'<' | b'>' | b'=') {
        let (op, n) = if text.starts_with("<=") {
            (Op::Le, 2)
        } else if text.starts_with(">=") {
            (Op::Ge, 2)
        } else if text.starts_with('<') {
            (Op::Lt, 1)
        } else if text.starts_with('>') {
            (Op::Gt, 1)
        } else {
            (Op::Eq, 1)
        };
        let operand = &text[n..];
        if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
            return Err(RangeError::Syntax { at: token.start });
        }
        let version = parse_version(operand).map_err(|error| RangeError::Version {
            at: token.start + n,
            error,
        })?;
        out.push(Comparator {
            op,
            anchor: !version.pre.is_empty(),
            version,
        });
        return Ok(());
    }

    if !(first.is_ascii_digit() || matches!(first, b'*' | b'x' | b'X')) {
        return Err(RangeError::Syntax { at: token.start });
    }

    let (version, k, full) = parse_partial_or_full(text, token.start)?;
    if full {
        out.push(Comparator {
            op: Op::Eq,
            anchor: !version.pre.is_empty(),
            version,
        });
    } else if k == 0 {
        // A wildcard-only operand adds no constraints.
    } else {
        out.push(Comparator {
            op: Op::Ge,
            anchor: false,
            version: version.clone(),
        });
        if k < 3 {
            let upper = partial_upper(&version, k)
                .ok_or(RangeError::Syntax { at: token.start })?;
            out.push(Comparator {
                op: Op::Lt,
                anchor: false,
                version: upper,
            });
        }
    }
    Ok(())
}

fn parse_partial_or_full(
    operand: &str,
    start: usize,
) -> Result<(Version, usize, bool), RangeError> {
    let has_sign = operand.contains('-') || operand.contains('+');
    let fields: Vec<&str> = operand.split('.').collect();
    let full_candidate = fields.len() >= 3
        && !fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));

    if has_sign || full_candidate {
        let version = parse_version(operand).map_err(|error| RangeError::Version {
            at: start,
            error,
        })?;
        return Ok((version, 3, true));
    }

    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at: start });
    }

    let mut nums = Vec::new();
    let mut wildcard_seen = false;
    for field in fields {
        if field.is_empty() {
            return Err(RangeError::Syntax { at: start });
        }
        if matches!(field, "*" | "x" | "X") {
            wildcard_seen = true;
        } else {
            if wildcard_seen
                || !field.bytes().all(|b| b.is_ascii_digit())
                || (field.len() > 1 && field.starts_with('0'))
            {
                return Err(RangeError::Syntax { at: start });
            }
            let n = field
                .parse::<u64>()
                .map_err(|_| RangeError::Syntax { at: start })?;
            nums.push(n);
        }
    }

    let k = nums.len();
    let version = Version {
        major: nums.first().copied().unwrap_or(0),
        minor: nums.get(1).copied().unwrap_or(0),
        patch: nums.get(2).copied().unwrap_or(0),
        pre: Vec::new(),
        build: Vec::new(),
    };
    Ok((version, k, false))
}

fn endpoint(major: u64, minor: u64, patch: u64) -> Version {
    Version {
        major,
        minor,
        patch,
        pre: vec!["0".to_owned()],
        build: Vec::new(),
    }
}

fn partial_upper(v: &Version, k: usize) -> Option<Version> {
    match k {
        1 => Some(endpoint(v.major.checked_add(1)?, 0, 0)),
        2 => Some(endpoint(v.major, v.minor.checked_add(1)?, 0)),
        _ => None,
    }
}

fn caret_upper(v: &Version, k: usize) -> Option<Version> {
    if v.major > 0 || k == 1 {
        Some(endpoint(v.major.checked_add(1)?, 0, 0))
    } else if v.minor > 0 || k == 2 {
        Some(endpoint(v.major, v.minor.checked_add(1)?, 0))
    } else {
        Some(endpoint(v.major, v.minor, v.patch.checked_add(1)?))
    }
}

fn tilde_upper(v: &Version, k: usize) -> Option<Version> {
    if k == 1 {
        Some(endpoint(v.major.checked_add(1)?, 0, 0))
    } else {
        Some(endpoint(v.major, v.minor.checked_add(1)?, 0))
    }
}

fn parse_hyphen(
    lower_token: &Token<'_>,
    upper_token: &Token<'_>,
) -> Result<Arm, RangeError> {
    let (lower, lower_k, lower_full) =
        parse_partial_or_full(lower_token.text, lower_token.start)?;
    let (upper, upper_k, upper_full) =
        parse_partial_or_full(upper_token.text, upper_token.start)?;

    if lower_k == 0 {
        return Err(RangeError::Syntax {
            at: lower_token.start,
        });
    }
    if upper_k == 0 {
        return Err(RangeError::Syntax {
            at: upper_token.start,
        });
    }

    let upper_bound = if upper_full {
        upper.clone()
    } else if upper_k == 1 || upper_k == 2 {
        partial_upper(&upper, upper_k).ok_or(RangeError::Syntax {
            at: upper_token.start,
        })?
    } else {
        upper.clone()
    };

    let comparators = vec![
        Comparator {
            op: Op::Ge,
            anchor: lower_full && !lower.pre.is_empty(),
            version: lower,
        },
        Comparator {
            op: if upper_full { Op::Le } else { Op::Lt },
            anchor: upper_full && !upper.pre.is_empty(),
            version: upper_bound,
        },
    ];
    Ok(Arm { comparators })
}

fn test_comparator(v: &Version, c: &Comparator) -> bool {
    match compare_versions(v, &c.version) {
        Ordering::Less => matches!(c.op, Op::Lt | Op::Le),
        Ordering::Equal => matches!(c.op, Op::Eq | Op::Le | Op::Ge),
        Ordering::Greater => matches!(c.op, Op::Gt | Op::Ge),
    }
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        if !arm
            .comparators
            .iter()
            .all(|c| test_comparator(v, c))
        {
            return false;
        }
        if v.pre.is_empty() {
            return true;
        }
        arm.comparators
            .iter()
            .any(|c| c.anchor && !c.version.pre.is_empty() && same_core(v, &c.version))
    })
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
    want_max: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let version =
            parse_version(candidate).map_err(|error| CandidateError { index, error })?;
        parsed.push(version);
    }

    let mut best: Option<usize> = None;
    for (i, version) in parsed.iter().enumerate() {
        if !matches(version, r) {
            continue;
        }
        if let Some(j) = best {
            let order = compare_versions(version, &parsed[j]);
            if (want_max && order == Ordering::Greater)
                || (!want_max && order == Ordering::Less)
            {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}