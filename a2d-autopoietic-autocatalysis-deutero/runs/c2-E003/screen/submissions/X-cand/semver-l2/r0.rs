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

fn parse_core_number(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && is_digit(bytes[*pos]) {
        *pos += 1;
    }
    if *pos == start {
        return Err(match bytes.get(*pos).copied() {
            None | Some(b'.' | b'-' | b'+') => VersionError::MissingCore { at: *pos },
            _ => VersionError::InvalidChar { at: *pos },
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

    loop {
        let mut i = ident_start;
        while i < end && bytes[i] != b'.' {
            if !bytes[i].is_ascii_alphanumeric() && bytes[i] != b'-' {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }
        if ident_start == i {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }
        let ident = &s[ident_start..i];
        if prerelease
            && ident.len() > 1
            && ident.bytes().all(is_digit)
            && ident.starts_with('0')
        {
            return Err(VersionError::LeadingZero { at: ident_start });
        }
        result.push(ident.to_string());
        if i == end {
            break;
        }
        ident_start = i + 1;
    }

    Ok(result)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let mut pos = 0;
    let major = parse_core_number(s, &mut pos)?;
    if s.as_bytes().get(pos) != Some(&b'.') {
        return Err(match s.as_bytes().get(pos).copied() {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: pos },
            _ => VersionError::InvalidChar { at: pos },
        });
    }
    pos += 1;
    let minor = parse_core_number(s, &mut pos)?;
    if s.as_bytes().get(pos) != Some(&b'.') {
        return Err(match s.as_bytes().get(pos).copied() {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: pos },
            _ => VersionError::InvalidChar { at: pos },
        });
    }
    pos += 1;
    let patch = parse_core_number(s, &mut pos)?;

    let bytes = s.as_bytes();
    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] != b'-' && bytes[pos] != b'+' {
        return Err(VersionError::InvalidChar { at: pos });
    }
    if pos < bytes.len() && bytes[pos] == b'-' {
        let pre_start = pos + 1;
        let plus = bytes[pre_start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|n| pre_start + n)
            .unwrap_or(bytes.len());
        pre = parse_identifiers(s, pre_start, plus, true)?;
        pos = plus;
    }
    if pos < bytes.len() && bytes[pos] == b'+' {
        let build_start = pos + 1;
        build = parse_identifiers(s, build_start, bytes.len(), false)?;
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

fn numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    for (x, y) in a.iter().zip(b) {
        let xn = numeric_identifier(x);
        let yn = numeric_identifier(y);
        let order = match (xn, yn) {
            (true, true) => x.len().cmp(&y.len()).then_with(|| x.as_bytes().cmp(y.as_bytes())),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => x.as_bytes().cmp(y.as_bytes()),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    a.len().cmp(&b.len())
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch)
        .cmp(&(b.major, b.minor, b.patch))
        .then_with(|| compare_pre(&a.pre, &b.pre))
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Vec<Comparator>>,
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

#[derive(Clone, Debug)]
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
    admission: bool,
}

fn compare_op(candidate: &Version, cmp: &Comparator) -> bool {
    match cmp.op {
        Op::Eq => compare_versions(candidate, &cmp.version) == Ordering::Equal,
        Op::Lt => compare_versions(candidate, &cmp.version) == Ordering::Less,
        Op::Le => compare_versions(candidate, &cmp.version) != Ordering::Greater,
        Op::Gt => compare_versions(candidate, &cmp.version) == Ordering::Greater,
        Op::Ge => compare_versions(candidate, &cmp.version) != Ordering::Less,
    }
}

fn whitespace(b: u8) -> bool {
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
        out.push(Token {
            text: s[start..i].to_string(),
            start,
        });
    }
    out
}

fn full_version(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

fn parse_full_comparator(token: &Token) -> Result<Comparator, RangeError> {
    let (op, offset) = if token.text.starts_with(">=") {
        (Op::Ge, 2)
    } else if token.text.starts_with("<=") {
        (Op::Le, 2)
    } else if token.text.starts_with('>') {
        (Op::Gt, 1)
    } else if token.text.starts_with('<') {
        (Op::Lt, 1)
    } else if token.text.starts_with('=') {
        (Op::Eq, 1)
    } else {
        (Op::Eq, 0)
    };
    let operand = &token.text[offset..];
    let operand_start = token.start + offset;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: token.start });
    }
    let version = full_version(operand, operand_start)?;
    let admission = !version.pre.is_empty();
    Ok(Comparator {
        op,
        version,
        admission,
    })
}

#[derive(Clone, Debug)]
enum ParsedOperand {
    Full(Version),
    Partial { fields: Vec<u64>, k: usize },
}

fn is_wildcard(field: &str) -> bool {
    matches!(field, "*" | "x" | "X")
}

fn parse_operand(
    s: &str,
    at: usize,
    token_start: usize,
) -> Result<ParsedOperand, RangeError> {
    let fields: Vec<&str> = s.split('.').collect();
    let looks_full = s.contains('-')
        || s.contains('+')
        || (fields.len() >= 3 && !fields.iter().any(|f| is_wildcard(f)));
    if looks_full {
        return full_version(s, at).map(ParsedOperand::Full);
    }
    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|f| f.is_empty()) {
        return Err(RangeError::Syntax { at: token_start });
    }

    let mut numbers = Vec::new();
    let mut wildcard_seen = false;
    for field in fields {
        if is_wildcard(field) {
            wildcard_seen = true;
            continue;
        }
        if wildcard_seen
            || !field.bytes().all(is_digit)
            || (field.len() > 1 && field.starts_with('0'))
        {
            return Err(RangeError::Syntax { at: token_start });
        }
        let value = field
            .parse::<u64>()
            .map_err(|_| RangeError::Syntax { at: token_start })?;
        numbers.push(value);
    }
    Ok(ParsedOperand::Partial {
        k: numbers.len(),
        fields: numbers,
    })
}

fn version_tuple(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn make_version(tuple: (u64, u64, u64)) -> Version {
    Version {
        major: tuple.0,
        minor: tuple.1,
        patch: tuple.2,
        pre: Vec::new(),
        build: Vec::new(),
    }
}

fn partial_lower(fields: &[u64]) -> Version {
    make_version((
        fields.first().copied().unwrap_or(0),
        fields.get(1).copied().unwrap_or(0),
        fields.get(2).copied().unwrap_or(0),
    ))
}

fn bump_upper(fields: &[u64], mode: &str, at: usize) -> Result<Version, RangeError> {
    let k = fields.len();
    let major = fields.first().copied().unwrap_or(0);
    let minor = fields.get(1).copied().unwrap_or(0);
    let patch = fields.get(2).copied().unwrap_or(0);
    let tuple = match mode {
        "bare" => {
            if k == 1 {
                (major.checked_add(1), Some(0), Some(0))
            } else {
                (Some(major), minor.checked_add(1), Some(0))
            }
        }
        "caret" => {
            if major > 0 || k == 1 {
                (major.checked_add(1), Some(0), Some(0))
            } else if minor > 0 || k == 2 {
                (Some(major), minor.checked_add(1), Some(0))
            } else {
                (Some(major), Some(minor), patch.checked_add(1))
            }
        }
        "tilde" => {
            if k == 1 {
                (major.checked_add(1), Some(0), Some(0))
            } else {
                (Some(major), minor.checked_add(1), Some(0))
            }
        }
        _ => unreachable!(),
    };
    match tuple {
        (Some(a), Some(b), Some(c)) => Ok(make_version((a, b, c))),
        _ => Err(RangeError::Syntax { at }),
    }
}

fn lower_comparator(version: Version, admission: bool) -> Comparator {
    Comparator {
        op: Op::Ge,
        version,
        admission,
    }
}

fn upper_comparator(version: Version) -> Comparator {
    Comparator {
        op: Op::Lt,
        version,
        admission: false,
    }
}

fn parse_expanded(token: &Token) -> Result<Vec<Comparator>, RangeError> {
    let first = token.text.as_bytes()[0];
    let (kind, operand_offset) = match first {
        b'^' => ("caret", 1),
        b'~' => ("tilde", 1),
        _ => ("bare", 0),
    };
    let operand = &token.text[operand_offset..];
    let operand_start = token.start + operand_offset;
    if operand.is_empty() {
        return Err(RangeError::Syntax { at: token.start });
    }
    let parsed = parse_operand(operand, operand_start, token.start)?;
    match parsed {
        ParsedOperand::Full(v) if kind == "bare" => Ok(vec![Comparator {
            op: Op::Eq,
            admission: !v.pre.is_empty(),
            version: v,
        }]),
        ParsedOperand::Full(v) => {
            if v.pre.is_empty() {
                let fields = vec![v.major, v.minor, v.patch];
                let upper = bump_upper(&fields, kind, operand_start)?;
                Ok(vec![
                    lower_comparator(v, false),
                    upper_comparator(upper),
                ])
            } else {
                let admission = true;
                let fields = vec![v.major, v.minor, v.patch];
                let upper = bump_upper(&fields, kind, operand_start)?;
                Ok(vec![
                    lower_comparator(v, admission),
                    upper_comparator(upper),
                ])
            }
        }
        ParsedOperand::Partial { fields, k } => {
            if kind != "bare" && k == 0 {
                return Err(RangeError::Syntax { at: token.start });
            }
            if kind == "bare" && k == 0 {
                return Ok(Vec::new());
            }
            let lower = partial_lower(&fields);
            let upper = bump_upper(&fields, kind, operand_start)?;
            Ok(vec![
                lower_comparator(lower, false),
                upper_comparator(upper),
            ])
        }
    }
}

fn parse_hyphen_arm(
    arm: &[Token],
    first_minus: usize,
) -> Result<Vec<Comparator>, RangeError> {
    if arm.len() != 3 || arm[1].text != "-" {
        return Err(RangeError::Syntax { at: first_minus });
    }
    let lower_parsed = parse_operand(&arm[0].text, arm[0].start, arm[0].start)?;
    let upper_parsed = parse_operand(&arm[2].text, arm[2].start, arm[2].start)?;
    let lower = match lower_parsed {
        ParsedOperand::Full(v) => v,
        ParsedOperand::Partial { fields, k } => {
            if k == 0 {
                return Err(RangeError::Syntax { at: arm[0].start });
            }
            partial_lower(&fields)
        }
    };
    let upper = match upper_parsed {
        ParsedOperand::Full(v) => v,
        ParsedOperand::Partial { fields, k } => {
            if k == 0 {
                return Err(RangeError::Syntax { at: arm[2].start });
            }
            let bound = bump_upper(&fields, "bare", arm[2].start)?;
            return Ok(vec![
                lower_comparator(lower.clone(), !lower.pre.is_empty()),
                upper_comparator(bound),
            ]);
        }
    };
    Ok(vec![
        lower_comparator(lower.clone(), !lower.pre.is_empty()),
        Comparator {
            op: Op::Le,
            admission: !upper.pre.is_empty(),
            version: upper,
        },
    ])
}

fn parse_arm(arm: &[Token]) -> Result<Vec<Comparator>, RangeError> {
    if let Some(minus) = arm.iter().find(|t| t.text == "-") {
        return parse_hyphen_arm(arm, minus.start);
    }
    let mut comparators = Vec::new();
    for token in arm {
        let first = token.text.as_bytes()[0];
        match first {
            b'<' | b'>' | b'=' => comparators.push(parse_full_comparator(token)?),
            b'^' | b'~' | b'0'..=b'9' | b'*' | b'x' | b'X' => {
                comparators.extend(parse_expanded(token)?);
            }
            _ => return Err(RangeError::Syntax { at: token.start }),
        }
    }
    Ok(comparators)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let all_tokens = tokens(s);
    if all_tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    let mut arms: Vec<Vec<Token>> = vec![Vec::new()];
    let mut separators = Vec::new();
    for token in all_tokens {
        if token.text == "||" {
            separators.push(token.start);
            arms.push(Vec::new());
        } else {
            arms.last_mut().unwrap().push(token);
        }
    }

    let mut parsed_arms = Vec::new();
    for (i, arm) in arms.iter().enumerate() {
        if arm.is_empty() {
            let at = if i == arms.len() - 1 {
                s.len()
            } else {
                separators[i.saturating_sub(1)]
            };
            return Err(RangeError::Empty { at });
        }
        parsed_arms.push(parse_arm(arm)?);
    }
    Ok(Range { arms: parsed_arms })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        if !arm.iter().all(|cmp| compare_op(v, cmp)) {
            return false;
        }
        if v.pre.is_empty() {
            return true;
        }
        arm.iter().any(|cmp| {
            cmp.admission && version_tuple(v) == version_tuple(&cmp.version)
        })
    })
}

fn extrema(
    candidates: &[String],
    r: &Range,
    want_max: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, s) in candidates.iter().enumerate() {
        let version = parse_version(s).map_err(|error| CandidateError { index, error })?;
        parsed.push(version);
    }

    let mut best: Option<usize> = None;
    for (i, version) in parsed.iter().enumerate() {
        if !matches(version, r) {
            continue;
        }
        best = match best {
            None => Some(i),
            Some(j) => {
                let order = compare_versions(version, &parsed[j]);
                if (want_max && order == Ordering::Greater)
                    || (!want_max && order == Ordering::Less)
                {
                    Some(i)
                } else {
                    Some(j)
                }
            }
        };
    }
    Ok(best.map(|i| candidates[i].clone()))
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