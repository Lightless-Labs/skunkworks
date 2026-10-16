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

fn parse_core(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if *pos == start {
        if *pos == bytes.len() || matches!(bytes[*pos], b'.' | b'-' | b'+') {
            return Err(VersionError::MissingCore { at: start });
        }
        return Err(VersionError::InvalidChar { at: start });
    }

    let digits = &bytes[start..*pos];
    if digits.len() > 1 && digits[0] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }

    let mut value = 0u64;
    for &digit in digits {
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add((digit - b'0') as u64))
            .ok_or(VersionError::Overflow { at: start })?;
    }
    Ok(value)
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    pre: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut field_start = start;

    loop {
        let mut i = field_start;
        while i < end && bytes[i] != b'.' {
            let c = bytes[i];
            if !(c.is_ascii_alphanumeric() || c == b'-') {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }

        if i == field_start {
            return Err(VersionError::EmptyIdentifier { at: field_start });
        }

        let ident = &s[field_start..i];
        if pre
            && ident.len() > 1
            && ident.as_bytes()[0] == b'0'
            && ident.bytes().all(|c| c.is_ascii_digit())
        {
            return Err(VersionError::LeadingZero { at: field_start });
        }

        result.push(ident.to_owned());
        if i == end {
            break;
        }
        field_start = i + 1;
    }

    Ok(result)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let mut pos = 0;
    let major = parse_core(s, &mut pos)?;

    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        if pos >= s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;

    let minor = parse_core(s, &mut pos)?;

    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        if pos >= s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;

    let patch = parse_core(s, &mut pos)?;

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < s.len() {
        match s.as_bytes()[pos] {
            b'-' => {
                let pre_start = pos + 1;
                let plus = s[pre_start..].find('+').map(|i| pre_start + i);
                let pre_end = plus.unwrap_or(s.len());
                pre = parse_identifiers(s, pre_start, pre_end, true)?;
                if let Some(plus_pos) = plus {
                    build = parse_identifiers(s, plus_pos + 1, s.len(), false)?;
                }
            }
            b'+' => {
                build = parse_identifiers(s, pos + 1, s.len(), false)?;
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

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }

    for (x, y) in a.iter().zip(b) {
        let x_numeric = is_numeric(x);
        let y_numeric = is_numeric(y);
        let ord = match (x_numeric, y_numeric) {
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
pub struct Range {
    arms: Vec<Arm>,
}

#[derive(Clone, Debug)]
struct Arm {
    comps: Vec<Comparator>,
    anchors: Vec<(u64, u64, u64)>,
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

impl Comparator {
    fn accepts(&self, v: &Version) -> bool {
        let ord = compare_versions(v, &self.version);
        match self.op {
            Op::Eq => ord == Ordering::Equal,
            Op::Lt => ord == Ordering::Less,
            Op::Le => ord != Ordering::Greater,
            Op::Gt => ord == Ordering::Greater,
            Op::Ge => ord != Ordering::Less,
        }
    }
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokenize(s: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;

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
        out.push((start, &s[start..i]));
    }

    out
}

fn is_wild(s: &str) -> bool {
    matches!(s, "*" | "x" | "X")
}

fn syntax(at: usize) -> RangeError {
    RangeError::Syntax { at }
}

fn version_at(at: usize, operand: &str) -> Result<Version, RangeError> {
    parse_version(operand).map_err(|error| RangeError::Version { at, error })
}

fn full_or_partial(
    at: usize,
    operand: &str,
) -> Result<(Version, usize, bool), RangeError> {
    let fields: Vec<&str> = operand.split('.').collect();
    let looks_full = operand.contains('-')
        || operand.contains('+')
        || (fields.len() >= 3 && !fields.iter().any(|f| is_wild(f)));

    if looks_full {
        return Ok((version_at(at, operand)?, 3, true));
    }

    if fields.len() > 3 {
        return Err(syntax(at));
    }

    let mut nums = Vec::new();
    let mut saw_wild = false;

    for field in &fields {
        if field.is_empty() {
            return Err(syntax(at));
        }
        if is_wild(field) {
            saw_wild = true;
        } else {
            if saw_wild
                || !field.bytes().all(|b| b.is_ascii_digit())
                || (field.len() > 1 && field.as_bytes()[0] == b'0')
            {
                return Err(syntax(at));
            }
            let n = field.parse::<u64>().map_err(|_| syntax(at))?;
            nums.push(n);
        }
    }

    let k = nums.len();
    let mut core = [0u64; 3];
    for (i, n) in nums.iter().enumerate() {
        core[i] = *n;
    }

    Ok((
        Version {
            major: core[0],
            minor: core[1],
            patch: core[2],
            pre: Vec::new(),
            build: Vec::new(),
        },
        k,
        false,
    ))
}

fn add_comp(arm: &mut Arm, op: Op, version: Version, anchor: bool) {
    if anchor && !version.pre.is_empty() {
        arm.anchors
            .push((version.major, version.minor, version.patch));
    }
    arm.comps.push(Comparator { op, version });
}

fn upper_version(core: [u64; 3], at: usize, bump: usize) -> Result<Version, RangeError> {
    let mut bumped = core;
    bumped[bump] = bumped[bump].checked_add(1).ok_or_else(|| syntax(at))?;
    for field in bumped.iter_mut().skip(bump + 1) {
        *field = 0;
    }

    Ok(Version {
        major: bumped[0],
        minor: bumped[1],
        patch: bumped[2],
        pre: vec!["0".to_owned()],
        build: Vec::new(),
    })
}

fn padded_core(version: &Version) -> [u64; 3] {
    [version.major, version.minor, version.patch]
}

fn parse_hyphen_arm(tokens: &[(usize, &str)], arm: &mut Arm) -> Result<(), RangeError> {
    let minus = tokens.iter().position(|(_, token)| *token == "-").unwrap();
    if tokens.len() != 3 || minus != 1 {
        return Err(syntax(tokens[minus].0));
    }

    let (lower_at, lower_text) = tokens[0];
    let (upper_at, upper_text) = tokens[2];
    let (lower, lower_k, _) = full_or_partial(lower_at, lower_text)?;
    let (upper, upper_k, upper_full) = full_or_partial(upper_at, upper_text)?;

    if lower_k == 0 {
        return Err(syntax(lower_at));
    }
    if upper_k == 0 {
        return Err(syntax(upper_at));
    }

    add_comp(arm, Op::Ge, lower, true);

    if upper_full {
        add_comp(arm, Op::Le, upper, true);
    } else {
        let bump = if upper_k == 1 { 0 } else { 1 };
        let upper_bound = upper_version(padded_core(&upper), upper_at, bump)?;
        add_comp(arm, Op::Lt, upper_bound, false);
    }

    Ok(())
}

fn parse_arm(tokens: &[(usize, &str)]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        comps: Vec::new(),
        anchors: Vec::new(),
    };

    if tokens.iter().any(|(_, token)| *token == "-") {
        parse_hyphen_arm(tokens, &mut arm)?;
        return Ok(arm);
    }

    for &(at, token) in tokens {
        let first = token.as_bytes()[0];

        if matches!(first, b'^' | b'~') {
            let operand = &token[1..];
            let operand_at = at + 1;
            let (lower, k, _) = full_or_partial(operand_at, operand)?;
            if k == 0 {
                return Err(syntax(at));
            }

            add_comp(&mut arm, Op::Ge, lower.clone(), true);
            let core = padded_core(&lower);
            let bump = if first == b'^' {
                if core[0] > 0 || k == 1 {
                    0
                } else if core[1] > 0 || k == 2 {
                    1
                } else {
                    2
                }
            } else if k == 1 {
                0
            } else {
                1
            };

            let upper = upper_version(core, operand_at, bump)?;
            add_comp(&mut arm, Op::Lt, upper, false);
            continue;
        }

        if matches!(first, b'=' | b'<' | b'>') {
            let (op, length) = if token.starts_with("<=") {
                (Op::Le, 2)
            } else if token.starts_with(">=") {
                (Op::Ge, 2)
            } else if token.starts_with('<') {
                (Op::Lt, 1)
            } else if token.starts_with('>') {
                (Op::Gt, 1)
            } else {
                (Op::Eq, 1)
            };

            let operand = &token[length..];
            let operand_at = at + length;
            if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
                return Err(syntax(at));
            }

            let version = version_at(operand_at, operand)?;
            add_comp(&mut arm, op, version, true);
            continue;
        }

        if !(first.is_ascii_digit() || is_wild(token)) {
            return Err(syntax(at));
        }

        let (version, k, full) = full_or_partial(at, token)?;
        if full {
            add_comp(&mut arm, Op::Eq, version, true);
        } else if k == 0 {
            // An all-wildcard operand adds no comparison.
        } else if k == 1 || k == 2 {
            add_comp(&mut arm, Op::Ge, version.clone(), true);
            let upper = upper_version(padded_core(&version), at, if k == 1 { 0 } else { 1 })?;
            add_comp(&mut arm, Op::Lt, upper, false);
        } else {
            add_comp(&mut arm, Op::Eq, version, true);
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

    for (at, token) in tokens {
        if token == "||" {
            if current.is_empty() {
                return Err(RangeError::Empty { at });
            }
            arms.push(parse_arm(&current)?);
            current.clear();
        } else {
            current.push((at, token));
        }
    }

    if current.is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(&current)?);

    Ok(Range { arms })
}

fn arm_matches(version: &Version, arm: &Arm) -> bool {
    if !version.pre.is_empty()
        && !arm
            .anchors
            .iter()
            .any(|core| *core == (version.major, version.minor, version.patch))
    {
        return false;
    }
    arm.comps.iter().all(|comparator| comparator.accepts(version))
}

pub fn matches(version: &Version, range: &Range) -> bool {
    range
        .arms
        .iter()
        .any(|arm| arm_matches(version, arm))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

pub fn max_satisfying(
    candidates: &[String],
    range: &Range,
) -> Result<Option<String>, CandidateError> {
    extrema(candidates, range, true)
}

pub fn min_satisfying(
    candidates: &[String],
    range: &Range,
) -> Result<Option<String>, CandidateError> {
    extrema(candidates, range, false)
}

fn extrema(
    candidates: &[String],
    range: &Range,
    maximum: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());

    for (index, input) in candidates.iter().enumerate() {
        let version =
            parse_version(input).map_err(|error| CandidateError { index, error })?;
        parsed.push(version);
    }

    let mut best: Option<usize> = None;
    for (index, version) in parsed.iter().enumerate() {
        if !matches(version, range) {
            continue;
        }

        if let Some(best_index) = best {
            let ordering = compare_versions(version, &parsed[best_index]);
            if (maximum && ordering == Ordering::Greater)
                || (!maximum && ordering == Ordering::Less)
            {
                best = Some(index);
            }
        } else {
            best = Some(index);
        }
    }

    Ok(best.map(|index| candidates[index].clone()))
}