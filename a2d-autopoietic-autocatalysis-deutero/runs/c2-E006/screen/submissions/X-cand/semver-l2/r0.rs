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

fn digit(b: u8) -> bool {
    b.is_ascii_digit()
}

fn core_number(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && digit(bytes[*pos]) {
        *pos += 1;
    }

    if start == *pos {
        if *pos == bytes.len() || matches!(bytes[*pos], b'.' | b'-' | b'+') {
            return Err(VersionError::MissingCore { at: start });
        }
        return Err(VersionError::InvalidChar { at: start });
    }

    let digits = &s[start..*pos];
    if digits.len() > 1 && digits.starts_with('0') {
        return Err(VersionError::LeadingZero { at: start });
    }
    let value = digits
        .parse::<u64>()
        .map_err(|_| VersionError::Overflow { at: start })?;
    Ok(value)
}

fn is_numeric(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(digit)
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
            if !bytes[i].is_ascii_alphanumeric() && bytes[i] != b'-' {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }

        if i == ident_start {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }

        let ident = &s[ident_start..i];
        if prerelease && is_numeric(ident) && ident.len() > 1 && ident.starts_with('0') {
            return Err(VersionError::LeadingZero { at: ident_start });
        }
        result.push(ident.to_string());

        if i == end {
            break;
        }
        ident_start = i + 1;
        if ident_start > end {
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
    let major = core_number(s, &mut pos)?;
    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        if pos >= s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }

    pos += 1;
    let minor = core_number(s, &mut pos)?;
    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        if pos >= s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }

    pos += 1;
    let patch = core_number(s, &mut pos)?;

    if pos < s.len() && !matches!(s.as_bytes()[pos], b'-' | b'+') {
        return Err(VersionError::InvalidChar { at: pos });
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < s.len() && s.as_bytes()[pos] == b'-' {
        let pre_start = pos + 1;
        let build_sep = s.as_bytes()[pre_start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|i| pre_start + i);
        let pre_end = build_sep.unwrap_or(s.len());
        pre = parse_identifiers(s, pre_start, pre_end, true)?;
        pos = pre_end;
    }

    if pos < s.len() && s.as_bytes()[pos] == b'+' {
        build = parse_identifiers(s, pos + 1, s.len(), false)?;
    }

    Ok(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }

    for (x, y) in a.iter().zip(b.iter()) {
        let xn = is_numeric(x);
        let yn = is_numeric(y);
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
    a.major
        .cmp(&b.major)
        .then_with(|| a.minor.cmp(&b.minor))
        .then_with(|| a.patch.cmp(&b.patch))
        .then_with(|| compare_pre(&a.pre, &b.pre))
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

#[derive(Clone, Debug)]
struct Token {
    text: String,
    start: usize,
}

fn tokens(s: &str) -> Vec<Token> {
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
enum PartialOperand {
    Full(Version),
    Partial {
        nums: Vec<u64>,
        numeric_count: usize,
    },
}

fn parse_partial_operand(
    s: &str,
    at: usize,
) -> Result<PartialOperand, RangeError> {
    if s.contains('-') || s.contains('+') {
        return parse_version(s)
            .map(PartialOperand::Full)
            .map_err(|error| RangeError::Version { at, error });
    }

    let fields: Vec<&str> = s.split('.').collect();
    let has_wildcard = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));
    if fields.len() >= 3 && !has_wildcard {
        return parse_version(s)
            .map(PartialOperand::Full)
            .map_err(|error| RangeError::Version { at, error });
    }

    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|f| f.is_empty()) {
        return Err(RangeError::Syntax { at });
    }

    let mut nums = Vec::new();
    let mut wildcard_seen = false;
    for field in fields {
        if matches!(field, "*" | "x" | "X") {
            wildcard_seen = true;
        } else {
            if wildcard_seen
                || !field.bytes().all(digit)
                || (field.len() > 1 && field.starts_with('0'))
            {
                return Err(RangeError::Syntax { at });
            }
            let n = field.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
            nums.push(n);
        }
    }

    Ok(PartialOperand::Partial {
        numeric_count: nums.len(),
        nums,
    })
}

fn padded(nums: &[u64]) -> Version {
    Version {
        major: *nums.first().unwrap_or(&0),
        minor: *nums.get(1).unwrap_or(&0),
        patch: *nums.get(2).unwrap_or(&0),
        pre: Vec::new(),
        build: Vec::new(),
    }
}

fn upper_version(mut v: Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let target = match field {
        0 => &mut v.major,
        1 => &mut v.minor,
        _ => &mut v.patch,
    };
    *target = target.checked_add(1).ok_or(RangeError::Syntax { at })?;
    match field {
        0 => {
            v.minor = 0;
            v.patch = 0;
        }
        1 => v.patch = 0,
        _ => {}
    }
    v.pre = vec!["0".to_string()];
    v.build.clear();
    Ok(v)
}

fn add_cmp(arm: &mut Arm, op: Op, version: Version, anchor: bool) {
    if anchor && !version.pre.is_empty() {
        arm.anchors
            .push((version.major, version.minor, version.patch));
    }
    arm.comparators.push(Comparator { op, version });
}

fn operand_version(op: PartialOperand) -> Version {
    match op {
        PartialOperand::Full(v) => v,
        PartialOperand::Partial { nums, .. } => padded(&nums),
    }
}

fn parse_expanded_token(
    token: &Token,
    arm: &mut Arm,
) -> Result<(), RangeError> {
    let text = token.text.as_str();

    if text.starts_with('^') || text.starts_with('~') {
        let kind = text.as_bytes()[0];
        let operand_start = token.start + 1;
        let operand_text = &text[1..];
        let parsed = parse_partial_operand(operand_text, operand_start)?;
        let (lower, k) = match parsed {
            PartialOperand::Full(v) => {
                let k = 3;
                (v, k)
            }
            PartialOperand::Partial { nums, numeric_count } => {
                (padded(&nums), numeric_count)
            }
        };
        if k == 0 {
            return Err(RangeError::Syntax { at: token.start });
        }

        add_cmp(arm, Op::Ge, lower.clone(), true);
        let field = if kind == b'^' {
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
        let upper = upper_version(lower, field, operand_start)?;
        add_cmp(arm, Op::Lt, upper, false);
        return Ok(());
    }

    let parsed = parse_partial_operand(text, token.start)?;
    match parsed {
        PartialOperand::Full(v) => add_cmp(arm, Op::Eq, v, true),
        PartialOperand::Partial { nums, numeric_count } => {
            if numeric_count == 0 {
                return Ok(());
            }
            let lower = padded(&nums);
            add_cmp(arm, Op::Ge, lower.clone(), true);
            let field = if numeric_count == 1 { 0 } else { 1 };
            let upper = upper_version(lower, field, token.start)?;
            add_cmp(arm, Op::Lt, upper, false);
        }
    }
    Ok(())
}

fn parse_comparator(token: &Token, arm: &mut Arm) -> Result<(), RangeError> {
    let bytes = token.text.as_bytes();
    let (op, n) = if token.text.starts_with(">=") {
        (Op::Ge, 2)
    } else if token.text.starts_with("<=") {
        (Op::Le, 2)
    } else if token.text.starts_with('>') {
        (Op::Gt, 1)
    } else if token.text.starts_with('<') {
        (Op::Lt, 1)
    } else {
        (Op::Eq, 1)
    };

    let explicit = matches!(bytes[0], b'<' | b'>' | b'=');
    let operand_start = token.start + if explicit { n } else { 0 };
    let operand = if explicit { &token.text[n..] } else { &token.text };
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: token.start });
    }
    let version = parse_version(operand)
        .map_err(|error| RangeError::Version { at: operand_start, error })?;
    add_cmp(arm, op, version, true);
    Ok(())
}

fn parse_hyphen_arm(ts: &[Token], arm: &mut Arm) -> Result<(), RangeError> {
    let dash = ts.iter().find(|t| t.text == "-").unwrap();
    if ts.len() != 3 || ts[1].text != "-" {
        return Err(RangeError::Syntax { at: dash.start });
    }

    let lower = parse_partial_operand(&ts[0].text, ts[0].start)?;
    let upper = parse_partial_operand(&ts[2].text, ts[2].start)?;

    let lower_k = match &lower {
        PartialOperand::Full(_) => 3,
        PartialOperand::Partial { numeric_count, .. } => *numeric_count,
    };
    let upper_k = match &upper {
        PartialOperand::Full(_) => 3,
        PartialOperand::Partial { numeric_count, .. } => *numeric_count,
    };
    if lower_k == 0 {
        return Err(RangeError::Syntax { at: ts[0].start });
    }
    if upper_k == 0 {
        return Err(RangeError::Syntax { at: ts[2].start });
    }

    let lower_v = operand_version(lower);
    add_cmp(arm, Op::Ge, lower_v, true);

    match upper {
        PartialOperand::Full(v) => add_cmp(arm, Op::Le, v, true),
        PartialOperand::Partial { nums, numeric_count } => {
            let base = padded(&nums);
            let field = if numeric_count == 1 { 0 } else { 1 };
            let upper_v = upper_version(base, field, ts[2].start)?;
            add_cmp(arm, Op::Lt, upper_v, false);
        }
    }
    Ok(())
}

fn parse_arm(ts: &[Token]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        comparators: Vec::new(),
        anchors: Vec::new(),
    };
    if ts.iter().any(|t| t.text == "-") {
        parse_hyphen_arm(ts, &mut arm)?;
        return Ok(arm);
    }

    for token in ts {
        let first = token.text.as_bytes()[0];
        if matches!(first, b'<' | b'>' | b'=') {
            parse_comparator(token, &mut arm)?;
        } else if first.is_ascii_digit() || matches!(first, b'*' | b'x' | b'X' | b'^' | b'~') {
            parse_expanded_token(token, &mut arm)?;
        } else {
            return Err(RangeError::Syntax { at: token.start });
        }
    }

    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let ts = tokens(s);
    if ts.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms = Vec::new();
    let mut current = Vec::new();
    let mut expect_arm = true;
    let mut first_empty_separator = None;

    for token in ts {
        if token.text == "||" {
            if current.is_empty() {
                return Err(RangeError::Empty { at: token.start });
            }
            arms.push(parse_arm(&current)?);
            current.clear();
            expect_arm = true;
            first_empty_separator = Some(token.start);
        } else {
            current.push(token);
            expect_arm = false;
        }
    }

    if !current.is_empty() {
        arms.push(parse_arm(&current)?);
    } else if expect_arm {
        return Err(RangeError::Empty {
            at: s.len(),
        });
    }

    let _ = first_empty_separator;
    Ok(Range { arms })
}

fn comparison_matches(v: &Version, c: &Comparator) -> bool {
    let ord = compare_versions(v, &c.version);
    match c.op {
        Op::Eq => ord == Ordering::Equal,
        Op::Lt => ord == Ordering::Less,
        Op::Le => ord != Ordering::Greater,
        Op::Gt => ord == Ordering::Greater,
        Op::Ge => ord != Ordering::Less,
    }
}

fn arm_matches(v: &Version, arm: &Arm) -> bool {
    if !v.pre.is_empty()
        && !arm.anchors.iter().any(|&(major, minor, patch)| {
            (v.major, v.minor, v.patch) == (major, minor, patch)
        })
    {
        return false;
    }
    arm.comparators
        .iter()
        .all(|c| comparison_matches(v, c))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| arm_matches(v, arm))
}

fn satisfying(
    candidates: &[String],
    r: &Range,
    want_max: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let version = parse_version(candidate).map_err(|error| CandidateError { index, error })?;
        parsed.push(version);
    }

    let mut best: Option<usize> = None;
    for (index, version) in parsed.iter().enumerate() {
        if !matches(version, r) {
            continue;
        }
        if let Some(current) = best {
            let order = compare_versions(version, &parsed[current]);
            if (want_max && order == Ordering::Greater) || (!want_max && order == Ordering::Less) {
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
    satisfying(candidates, r, true)
}

pub fn min_satisfying(
    candidates: &[String],
    r: &Range,
) -> Result<Option<String>, CandidateError> {
    satisfying(candidates, r, false)
}