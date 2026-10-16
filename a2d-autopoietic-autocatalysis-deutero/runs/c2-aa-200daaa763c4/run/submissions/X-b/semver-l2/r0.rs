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

fn parse_core(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let start = *pos;
    let bytes = s.as_bytes();
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if *pos == start {
        return Err(match byte(s, start) {
            None | Some(b'.' | b'-' | b'+') => VersionError::MissingCore { at: start },
            _ => VersionError::InvalidChar { at: start },
        });
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
    prerelease: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut ident_start = start;
    let mut i = start;

    loop {
        if i == end || bytes[i] == b'.' {
            if i == ident_start {
                return Err(VersionError::EmptyIdentifier { at: ident_start });
            }
            let ident = &s[ident_start..i];
            if prerelease
                && ident.len() > 1
                && ident.bytes().all(|b| b.is_ascii_digit())
                && ident.starts_with('0')
            {
                return Err(VersionError::LeadingZero { at: ident_start });
            }
            result.push(ident.to_owned());
            if i == end {
                break;
            }
            ident_start = i + 1;
            i += 1;
            continue;
        }
        let b = bytes[i];
        if !(b.is_ascii_alphanumeric() || b == b'-') {
            return Err(VersionError::InvalidChar { at: i });
        }
        i += 1;
    }
    Ok(result)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let mut pos = 0;
    let major = parse_core(s, &mut pos)?;
    if byte(s, pos) != Some(b'.') {
        return Err(match byte(s, pos) {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: pos },
            _ => VersionError::InvalidChar { at: pos },
        });
    }

    pos += 1;
    let minor = parse_core(s, &mut pos)?;
    if byte(s, pos) != Some(b'.') {
        return Err(match byte(s, pos) {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: pos },
            _ => VersionError::InvalidChar { at: pos },
        });
    }

    pos += 1;
    let patch = parse_core(s, &mut pos)?;

    let bytes = s.as_bytes();
    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() {
        match bytes[pos] {
            b'-' => {
                let start = pos + 1;
                let plus = bytes[start..]
                    .iter()
                    .position(|&b| b == b'+')
                    .map(|n| start + n)
                    .unwrap_or(s.len());
                pre = parse_identifiers(s, start, plus, true)?;
                pos = plus;
                if pos < s.len() {
                    let build_start = pos + 1;
                    build = parse_identifiers(s, build_start, s.len(), false)?;
                }
            }
            b'+' => {
                let build_start = pos + 1;
                build = parse_identifiers(s, build_start, s.len(), false)?;
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

fn is_numeric(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn compare_numeric_ident(a: &str, b: &str) -> Ordering {
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
                    let xn = is_numeric(x);
                    let yn = is_numeric(y);
                    let ord = match (xn, yn) {
                        (true, true) => compare_numeric_ident(x, y),
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Debug)]
enum Bound {
    Compare { op: Op, version: Version },
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
    bounds: Vec<Bound>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

#[derive(Clone, Copy)]
struct Token<'a> {
    text: &'a str,
    start: usize,
}

fn whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokenize(s: &str, base: usize) -> Vec<Token<'_>> {
    let bytes = s.as_bytes();
    let mut tokens = Vec::new();
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
        tokens.push(Token {
            text: &s[start..i],
            start: base + start,
        });
    }
    tokens
}

#[derive(Clone, Debug)]
struct Partial {
    nums: Vec<u64>,
    full: Option<Version>,
}

fn wildcard(s: &str) -> bool {
    matches!(s, "*" | "x" | "X")
}

fn parse_partial_or_full(
    operand: &str,
    at: usize,
    allow_full: bool,
) -> Result<Partial, RangeError> {
    let has_sign = operand.contains('-') || operand.contains('+');
    let fields: Vec<&str> = operand.split('.').collect();
    let has_three_without_wild = fields.len() >= 3 && !fields.iter().any(|f| wildcard(f));

    if allow_full && (has_sign || has_three_without_wild) {
        let v = parse_version(operand).map_err(|error| RangeError::Version { at, error })?;
        return Ok(Partial {
            nums: vec![v.major, v.minor, v.patch],
            full: Some(v),
        });
    }

    if fields.len() > 3 || fields.iter().any(|f| f.is_empty()) {
        return Err(RangeError::Syntax { at });
    }

    let mut nums = Vec::new();
    let mut saw_wild = false;
    for field in fields {
        if wildcard(field) {
            saw_wild = true;
        } else {
            if saw_wild
                || !field.bytes().all(|b| b.is_ascii_digit())
                || (field.len() > 1 && field.starts_with('0'))
            {
                return Err(RangeError::Syntax { at });
            }
            let n = field.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
            nums.push(n);
        }
    }
    Ok(Partial { nums, full: None })
}

fn version_tuple(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn endpoint(nums: &[u64]) -> Version {
    Version {
        major: nums.first().copied().unwrap_or(0),
        minor: nums.get(1).copied().unwrap_or(0),
        patch: nums.get(2).copied().unwrap_or(0),
        pre: Vec::new(),
        build: Vec::new(),
    }
}

fn add_bound(arm: &mut Arm, op: Op, v: Version, explicit: bool) {
    if explicit && !v.pre.is_empty() {
        arm.anchors.push(version_tuple(&v));
    }
    arm.bounds.push(Bound::Compare { op, version: v });
}

fn next_endpoint(nums: &[u64], which: usize, at: usize) -> Result<Version, RangeError> {
    let mut fields = [0u64; 3];
    for (i, n) in nums.iter().enumerate().take(3) {
        fields[i] = *n;
    }
    fields[which] = fields[which]
        .checked_add(1)
        .ok_or(RangeError::Syntax { at })?;
    for field in fields.iter_mut().skip(which + 1) {
        *field = 0;
    }
    Ok(Version {
        major: fields[0],
        minor: fields[1],
        patch: fields[2],
        pre: vec!["0".to_owned()],
        build: Vec::new(),
    })
}

fn all_wild(partial: &Partial) -> bool {
    partial.nums.is_empty()
}

fn parse_bare(token: Token<'_>, arm: &mut Arm) -> Result<(), RangeError> {
    let p = parse_partial_or_full(token.text, token.start, true)?;
    if let Some(v) = p.full {
        add_bound(arm, Op::Eq, v, true);
        return Ok(());
    }

    let k = p.nums.len();
    if k == 0 {
        return Ok(());
    }
    add_bound(arm, Op::Ge, endpoint(&p.nums), false);
    if k <= 2 {
        let upper = next_endpoint(&p.nums, if k == 1 { 0 } else { 1 }, token.start)?;
        add_bound(arm, Op::Lt, upper, false);
    }
    Ok(())
}

fn parse_caret_tilde(
    token: Token<'_>,
    arm: &mut Arm,
    caret: bool,
) -> Result<(), RangeError> {
    let operand = &token.text[1..];
    let at = token.start + 1;
    let p = parse_partial_or_full(operand, at, true)?;
    if all_wild(&p) {
        return Err(RangeError::Syntax { at: token.start });
    }

    let k = p.nums.len();
    let lower = p.full.clone().unwrap_or_else(|| endpoint(&p.nums));
    let fields = [lower.major, lower.minor, lower.patch];
    add_bound(arm, Op::Ge, lower, p.full.is_some());

    let bump = if caret {
        if fields[0] > 0 || k == 1 {
            0
        } else if fields[1] > 0 || k == 2 {
            1
        } else {
            2
        }
    } else if k == 1 {
        0
    } else {
        1
    };

    let upper = next_endpoint(&fields, bump, at)?;
    add_bound(arm, Op::Lt, upper, false);
    Ok(())
}

fn parse_hyphen(tokens: &[Token<'_>], arm: &mut Arm) -> Result<(), RangeError> {
    let minus = tokens.iter().find(|t| t.text == "-").unwrap();
    if tokens.len() != 3 || tokens[1].text != "-" {
        return Err(RangeError::Syntax { at: minus.start });
    }

    let lower = parse_partial_or_full(tokens[0].text, tokens[0].start, true)?;
    let upper = parse_partial_or_full(tokens[2].text, tokens[2].start, true)?;

    if all_wild(&lower) {
        return Err(RangeError::Syntax { at: tokens[0].start });
    }
    if all_wild(&upper) {
        return Err(RangeError::Syntax { at: tokens[2].start });
    }

    let lower_version = lower
        .full
        .clone()
        .unwrap_or_else(|| endpoint(&lower.nums));
    add_bound(arm, Op::Ge, lower_version, lower.full.is_some());

    if let Some(upper_version) = upper.full {
        add_bound(arm, Op::Le, upper_version, true);
    } else {
        let k = upper.nums.len();
        let upper_bound = next_endpoint(
            &upper.nums,
            if k == 1 { 0 } else { 1 },
            tokens[2].start,
        )?;
        add_bound(arm, Op::Lt, upper_bound, false);
    }
    Ok(())
}

fn parse_comparator(token: Token<'_>, arm: &mut Arm) -> Result<(), RangeError> {
    let (op, n) = if token.text.starts_with("<=") {
        (Op::Le, 2)
    } else if token.text.starts_with(">=") {
        (Op::Ge, 2)
    } else {
        match token.text.as_bytes().first().copied() {
            Some(b'=') => (Op::Eq, 1),
            Some(b'<') => (Op::Lt, 1),
            Some(b'>') => (Op::Gt, 1),
            _ => return Err(RangeError::Syntax { at: token.start }),
        }
    };

    let operand = &token.text[n..];
    let at = token.start + n;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: token.start });
    }

    let version =
        parse_version(operand).map_err(|error| RangeError::Version { at, error })?;
    add_bound(arm, op, version, true);
    Ok(())
}

fn parse_arm(tokens: &[Token<'_>]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        bounds: Vec::new(),
        anchors: Vec::new(),
    };

    if tokens.iter().any(|t| t.text == "-") {
        parse_hyphen(tokens, &mut arm)?;
        return Ok(arm);
    }

    for token in tokens {
        match token.text.as_bytes().first().copied() {
            Some(b'^') => parse_caret_tilde(*token, &mut arm, true)?,
            Some(b'~') => parse_caret_tilde(*token, &mut arm, false)?,
            Some(b'<' | b'>' | b'=') => parse_comparator(*token, &mut arm)?,
            Some(b'0'..=b'9' | b'*' | b'x' | b'X') => parse_bare(*token, &mut arm)?,
            _ => return Err(RangeError::Syntax { at: token.start }),
        }
    }
    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let tokens = tokenize(s, 0);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms = Vec::new();
    let mut current = Vec::new();

    for token in tokens {
        if token.text == "||" {
            if current.is_empty() {
                return Err(RangeError::Empty { at: token.start });
            }
            arms.push(parse_arm(&current)?);
            current.clear();
        } else {
            current.push(token);
        }
    }

    if current.is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(&current)?);
    Ok(Range { arms })
}

fn check_bound(v: &Version, bound: &Bound) -> bool {
    match bound {
        Bound::Compare { op, version } => {
            let ord = compare_versions(v, version);
            match op {
                Op::Eq => ord == Ordering::Equal,
                Op::Lt => ord == Ordering::Less,
                Op::Le => ord != Ordering::Greater,
                Op::Gt => ord == Ordering::Greater,
                Op::Ge => ord != Ordering::Less,
            }
        }
    }
}

fn arm_matches(v: &Version, arm: &Arm) -> bool {
    if !v.pre.is_empty() && !arm.anchors.contains(&(v.major, v.minor, v.patch)) {
        return false;
    }
    arm.bounds.iter().all(|bound| check_bound(v, bound))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| arm_matches(v, arm))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn satisfying(
    candidates: &[String],
    r: &Range,
    greatest: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, text) in candidates.iter().enumerate() {
        let version =
            parse_version(text).map_err(|error| CandidateError { index, error })?;
        parsed.push(version);
    }

    let mut best: Option<usize> = None;
    for (i, version) in parsed.iter().enumerate() {
        if !matches(version, r) {
            continue;
        }
        if let Some(j) = best {
            let ord = compare_versions(version, &parsed[j]);
            if (greatest && ord == Ordering::Greater) || (!greatest && ord == Ordering::Less) {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

pub fn max_satisfying(
    candidates: &[String],
    r: &Range,
) -> Result<Option<String>, CandidateError> {
    satisfying(candidates, r, true)
}

pub fn min_satisfying(
    candidates: &[String],
    r: &Range,
) -> Result<Option<String>, CandidateError> {
    satisfying(candidates, r, false)
}