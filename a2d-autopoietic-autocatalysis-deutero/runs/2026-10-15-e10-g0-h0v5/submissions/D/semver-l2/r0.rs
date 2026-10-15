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

fn parse_core_field(bytes: &[u8], pos: &mut usize) -> Result<u64, VersionError> {
    let start = *pos;
    while *pos < bytes.len() && is_digit(bytes[*pos]) {
        *pos += 1;
    }
    if *pos == start {
        return Err(match bytes.get(start) {
            None | Some(b'.' | b'-' | b'+') => VersionError::MissingCore { at: start },
            _ => VersionError::InvalidChar { at: start },
        });
    }
    if *pos - start > 1 && bytes[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    let mut value = 0u64;
    for &b in &bytes[start..*pos] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u64))
            .ok_or(VersionError::Overflow { at: start })?;
    }
    Ok(value)
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
        let text = &s[ident_start..i];
        if prerelease
            && text.len() > 1
            && text.bytes().all(is_digit)
            && text.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: ident_start });
        }
        result.push(text.to_owned());
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
    let bytes = s.as_bytes();
    let mut pos = 0;
    let major = parse_core_field(bytes, &mut pos)?;
    if pos >= bytes.len() || bytes[pos] != b'.' {
        return Err(match bytes.get(pos) {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: pos },
            _ => VersionError::InvalidChar { at: pos },
        });
    }
    pos += 1;
    let minor = parse_core_field(bytes, &mut pos)?;
    if pos >= bytes.len() || bytes[pos] != b'.' {
        return Err(match bytes.get(pos) {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: pos },
            _ => VersionError::InvalidChar { at: pos },
        });
    }
    pos += 1;
    let patch = parse_core_field(bytes, &mut pos)?;

    let mut pre = Vec::new();
    let mut build = Vec::new();
    if pos < bytes.len() && bytes[pos] == b'-' {
        let start = pos + 1;
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|n| start + n)
            .unwrap_or(bytes.len());
        pre = parse_identifiers(s, start, end, true)?;
        pos = end;
    }
    if pos < bytes.len() && bytes[pos] == b'+' {
        build = parse_identifiers(s, pos + 1, bytes.len(), false)?;
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

fn numeric_ident(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn compare_numeric_ident(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
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
                    let xn = numeric_ident(x);
                    let yn = numeric_ident(y);
                    let order = match (xn, yn) {
                        (true, true) => compare_numeric_ident(x, y),
                        (true, false) => Ordering::Less,
                        (false, true) => Ordering::Greater,
                        (false, false) => x.as_bytes().cmp(y.as_bytes()),
                    };
                    if order != Ordering::Equal {
                        return order;
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
pub struct Range {
    arms: Vec<Vec<Comparator>>,
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

fn cmp_matches(v: &Version, c: &Comparator) -> bool {
    match c.op {
        Op::Eq => compare_versions(v, &c.version) == Ordering::Equal,
        Op::Lt => compare_versions(v, &c.version) == Ordering::Less,
        Op::Le => compare_versions(v, &c.version) != Ordering::Greater,
        Op::Gt => compare_versions(v, &c.version) == Ordering::Greater,
        Op::Ge => compare_versions(v, &c.version) != Ordering::Less,
    }
}

fn candidate_admitted(v: &Version, arm: &[Comparator]) -> bool {
    v.pre.is_empty()
        || arm.iter().any(|c| {
            c.anchor
                && !c.version.pre.is_empty()
                && (v.major, v.minor, v.patch)
                    == (c.version.major, c.version.minor, c.version.patch)
        })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        candidate_admitted(v, arm) && arm.iter().all(|c| cmp_matches(v, c))
    })
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

#[derive(Clone, Copy)]
struct Token<'a> {
    text: &'a str,
    start: usize,
}

fn tokenize(s: &str) -> Vec<Token<'_>> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
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
        out.push(Token {
            text: &s[start..i],
            start,
        });
    }
    out
}

fn syntax(at: usize) -> RangeError {
    RangeError::Syntax { at }
}

fn version_wrap(at: usize, e: VersionError) -> RangeError {
    RangeError::Version { at, error: e }
}

fn parse_full_operand(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|e| version_wrap(at, e))
}

#[derive(Clone, Debug)]
enum ParsedOperand {
    Full(Version),
    Partial { fields: Vec<u64> },
}

fn parse_operand(s: &str, at: usize) -> Result<ParsedOperand, RangeError> {
    let bytes = s.as_bytes();
    let has_sign = bytes.iter().any(|&b| b == b'-' || b == b'+');
    let fields: Vec<&str> = s.split('.').collect();
    let has_wildcard = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));
    if has_sign || (fields.len() >= 3 && !has_wildcard) {
        return parse_full_operand(s, at).map(ParsedOperand::Full);
    }
    if fields.is_empty() || fields.len() > 3 {
        return Err(syntax(at));
    }
    let mut nums = Vec::new();
    let mut wildcard_seen = false;
    for f in fields {
        if matches!(f, "*" | "x" | "X") {
            wildcard_seen = true;
            continue;
        }
        if f.is_empty() || wildcard_seen || !f.bytes().all(is_digit) {
            return Err(syntax(at));
        }
        if f.len() > 1 && f.as_bytes()[0] == b'0' {
            return Err(syntax(at));
        }
        let mut n = 0u64;
        for b in f.bytes() {
            n = n
                .checked_mul(10)
                .and_then(|v| v.checked_add((b - b'0') as u64))
                .ok_or_else(|| syntax(at))?;
        }
        nums.push(n);
    }
    Ok(ParsedOperand::Partial { fields: nums })
}

fn make_version(fields: &[u64], pre: Vec<String>, build: Vec<String>) -> Version {
    Version {
        major: fields.first().copied().unwrap_or(0),
        minor: fields.get(1).copied().unwrap_or(0),
        patch: fields.get(2).copied().unwrap_or(0),
        pre,
        build,
    }
}

fn next_bound(v: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let mut f = [v.major, v.minor, v.patch];
    f[field] = f[field].checked_add(1).ok_or_else(|| syntax(at))?;
    for x in f.iter_mut().skip(field + 1) {
        *x = 0;
    }
    Ok(make_version(&f, vec!["0".to_owned()], Vec::new()))
}

fn add_cmp(arm: &mut Vec<Comparator>, op: Op, version: Version, anchor: bool) {
    arm.push(Comparator {
        op,
        version,
        anchor,
    });
}

fn parse_simple_token(token: Token<'_>, arm: &mut Vec<Comparator>) -> Result<(), RangeError> {
    let t = token.text;
    let start = token.start;
    let bytes = t.as_bytes();

    if matches!(bytes.first(), Some(b'<' | b'>' | b'=')) {
        let (op, n) = if t.starts_with(">=") {
            (Op::Ge, 2)
        } else if t.starts_with("<=") {
            (Op::Le, 2)
        } else if t.starts_with('>') {
            (Op::Gt, 1)
        } else if t.starts_with('<') {
            (Op::Lt, 1)
        } else {
            (Op::Eq, 1)
        };
        let operand = &t[n..];
        let operand_start = start + n;
        if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
            return Err(syntax(start));
        }
        let v = parse_full_operand(operand, operand_start)?;
        let anchor = !v.pre.is_empty();
        add_cmp(arm, op, v, anchor);
        return Ok(());
    }

    if t.starts_with('^') || t.starts_with('~') {
        let caret = t.starts_with('^');
        let op_at = start;
        let operand_start = start + 1;
        let parsed = parse_operand(&t[1..], operand_start)?;
        let (lower, k) = match parsed {
            ParsedOperand::Full(v) => (v, 3),
            ParsedOperand::Partial { fields } if !fields.is_empty() => {
                let k = fields.len();
                (make_version(&fields, Vec::new(), Vec::new()), k)
            }
            _ => return Err(syntax(op_at)),
        };
        let anchor = !lower.pre.is_empty();
        add_cmp(arm, Op::Ge, lower.clone(), anchor);
        let field = if caret {
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
        let upper = next_bound(&lower, field, operand_start)?;
        add_cmp(arm, Op::Lt, upper, false);
        return Ok(());
    }

    if !bytes
        .first()
        .is_some_and(|b| b.is_ascii_digit() || matches!(b, b'*' | b'x' | b'X'))
    {
        return Err(syntax(start));
    }
    let parsed = parse_operand(t, start)?;
    match parsed {
        ParsedOperand::Full(v) => {
            let anchor = !v.pre.is_empty();
            add_cmp(arm, Op::Eq, v, anchor);
        }
        ParsedOperand::Partial { fields } => {
            if fields.is_empty() {
                return Ok(());
            }
            let k = fields.len();
            let lower = make_version(&fields, Vec::new(), Vec::new());
            add_cmp(arm, Op::Ge, lower.clone(), false);
            if k < 3 {
                let upper = next_bound(&lower, k - 1, start)?;
                add_cmp(arm, Op::Lt, upper, false);
            } else {
                add_cmp(arm, Op::Eq, lower, false);
            }
        }
    }
    Ok(())
}

fn parse_hyphen_arm(tokens: &[Token<'_>], arm: &mut Vec<Comparator>) -> Result<(), RangeError> {
    let minus = tokens.iter().find(|t| t.text == "-").unwrap();
    if tokens.len() != 3 || tokens[1].text != "-" {
        return Err(syntax(minus.start));
    }
    let lower_parsed = parse_operand(tokens[0].text, tokens[0].start)?;
    let upper_parsed = parse_operand(tokens[2].text, tokens[2].start)?;
    let (lower, lower_k) = match lower_parsed {
        ParsedOperand::Full(v) => (v, 3),
        ParsedOperand::Partial { fields } => {
            let k = fields.len();
            (make_version(&fields, Vec::new(), Vec::new()), k)
        }
    };
    let (upper, upper_k, upper_full) = match upper_parsed {
        ParsedOperand::Full(v) => (v, 3, true),
        ParsedOperand::Partial { fields } => {
            let k = fields.len();
            (make_version(&fields, Vec::new(), Vec::new()), k, false)
        }
    };
    if lower_k == 0 {
        return Err(syntax(tokens[0].start));
    }
    if upper_k == 0 {
        return Err(syntax(tokens[2].start));
    }

    let lower_anchor = !lower.pre.is_empty();
    add_cmp(arm, Op::Ge, lower, lower_anchor);
    if upper_full {
        let upper_anchor = !upper.pre.is_empty();
        add_cmp(arm, Op::Le, upper, upper_anchor);
    } else {
        let upper_bound = next_bound(&upper, upper_k - 1, tokens[2].start)?;
        add_cmp(arm, Op::Lt, upper_bound, false);
    }
    Ok(())
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let tokens = tokenize(s);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arms: Vec<Vec<Comparator>> = Vec::new();
    let mut arm_tokens: Vec<Token<'_>> = Vec::new();
    let mut saw_separator = false;

    for token in tokens {
        if token.text == "||" {
            saw_separator = true;
            if arm_tokens.is_empty() {
                return Err(RangeError::Empty { at: token.start });
            }
            let mut arm = Vec::new();
            if arm_tokens.iter().any(|t| t.text == "-") {
                parse_hyphen_arm(&arm_tokens, &mut arm)?;
            } else {
                for t in &arm_tokens {
                    parse_simple_token(*t, &mut arm)?;
                }
            }
            arms.push(arm);
            arm_tokens.clear();
        } else {
            arm_tokens.push(token);
        }
    }

    if arm_tokens.is_empty() {
        if saw_separator {
            return Err(RangeError::Empty { at: s.len() });
        }
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arm = Vec::new();
    if arm_tokens.iter().any(|t| t.text == "-") {
        parse_hyphen_arm(&arm_tokens, &mut arm)?;
    } else {
        for t in &arm_tokens {
            parse_simple_token(*t, &mut arm)?;
        }
    }
    arms.push(arm);

    Ok(Range { arms })
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
        let v = parse_version(s).map_err(|error| CandidateError { index, error })?;
        parsed.push(v);
    }

    let mut best: Option<usize> = None;
    for i in 0..candidates.len() {
        if !matches(&parsed[i], r) {
            continue;
        }
        if let Some(j) = best {
            let order = compare_versions(&parsed[i], &parsed[j]);
            if (greatest && order == Ordering::Greater)
                || (!greatest && order == Ordering::Less)
            {
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
    extrema(candidates, r, true)
}

pub fn min_satisfying(
    candidates: &[String],
    r: &Range,
) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, false)
}