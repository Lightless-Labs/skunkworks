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

fn is_core_terminator(b: Option<u8>) -> bool {
    matches!(b, None | Some(b'.' | b'-' | b'+'))
}

fn parse_core(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if *pos == start {
        return Err(if is_core_terminator(bytes.get(*pos).copied()) {
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

fn valid_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    prerelease: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut ident_start = start;

    loop {
        let mut i = ident_start;
        while i < end && bytes[i] != b'.' {
            if !valid_ident_char(bytes[i]) {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }
        if i == ident_start {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }

        if prerelease {
            let ident = &s[ident_start..i];
            if ident.bytes().all(|b| b.is_ascii_digit())
                && ident.len() > 1
                && ident.as_bytes()[0] == b'0'
            {
                return Err(VersionError::LeadingZero { at: ident_start });
            }
        }
        out.push(s[ident_start..i].to_string());

        if i == end {
            break;
        }
        ident_start = i + 1;
        if ident_start == end {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }
    }
    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let mut pos = 0;
    let major = parse_core(s, &mut pos)?;
    for expected in 0..2 {
        if pos >= s.len() || s.as_bytes()[pos] != b'.' {
            return Err(if is_core_terminator(s.as_bytes().get(pos).copied()) {
                VersionError::MissingCore { at: pos }
            } else {
                VersionError::InvalidChar { at: pos }
            });
        }
        pos += 1;
        let value = parse_core(s, &mut pos)?;
        if expected == 0 {
            let minor = value;
            if pos >= s.len() || s.as_bytes()[pos] != b'.' {
                return Err(if is_core_terminator(s.as_bytes().get(pos).copied()) {
                    VersionError::MissingCore { at: pos }
                } else {
                    VersionError::InvalidChar { at: pos }
                });
            }
            pos += 1;
            let patch = parse_core(s, &mut pos)?;
            let (pre, build) = parse_version_tail(s, pos)?;
            return Ok(Version {
                major,
                minor,
                patch,
                pre,
                build,
            });
        }
    }

    unreachable!()
}

fn parse_version_tail(s: &str, pos: usize) -> Result<(Vec<String>, Vec<String>), VersionError> {
    let bytes = s.as_bytes();
    if pos == bytes.len() {
        return Ok((Vec::new(), Vec::new()));
    }
    if bytes[pos] != b'-' && bytes[pos] != b'+' {
        return Err(VersionError::InvalidChar { at: pos });
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();
    let mut cursor = pos;

    if bytes[cursor] == b'-' {
        cursor += 1;
        let pre_end = bytes[cursor..]
            .iter()
            .position(|&b| b == b'+')
            .map(|n| cursor + n)
            .unwrap_or(bytes.len());
        pre = parse_identifiers(s, cursor, pre_end, true)?;
        cursor = pre_end;
    }

    if cursor < bytes.len() && bytes[cursor] == b'+' {
        cursor += 1;
        build = parse_identifiers(s, cursor, bytes.len(), false)?;
    }

    Ok((pre, build))
}

fn is_numeric_ident(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn compare_numeric_ident(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    let core = (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch));
    if core != Ordering::Equal {
        return core;
    }

    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }

    for (x, y) in a.pre.iter().zip(&b.pre) {
        let xn = is_numeric_ident(x);
        let yn = is_numeric_ident(y);
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
    comparators: Vec<Comparator>,
    anchors: Vec<(u64, u64, u64)>,
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
}

impl Comparator {
    fn test(&self, v: &Version) -> bool {
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

#[derive(Clone, Copy)]
struct Token<'a> {
    text: &'a str,
    start: usize,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokenize(s: &str) -> Vec<Token<'_>> {
    let bytes = s.as_bytes();
    let mut tokens = Vec::new();
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
        tokens.push(Token {
            text: &s[start..i],
            start,
        });
    }
    tokens
}

fn tuple(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn version_at(v: Version, op: Op) -> Comparator {
    Comparator { op, version: v }
}

fn stable_tuple(major: u64, minor: u64, patch: u64) -> Version {
    Version {
        major,
        minor,
        patch,
        pre: Vec::new(),
        build: Vec::new(),
    }
}

fn upper_tuple(major: u64, minor: u64, patch: u64) -> Version {
    Version {
        major,
        minor,
        patch,
        pre: vec!["0".to_string()],
        build: Vec::new(),
    }
}

fn split_fields(s: &str) -> Vec<&str> {
    s.split('.').collect()
}

#[derive(Clone, Debug)]
struct Partial {
    nums: Vec<u64>,
    full: Option<Version>,
    explicit_pre: bool,
}

impl Partial {
    fn numeric_count(&self) -> usize {
        self.nums.len()
    }

    fn padded(&self) -> (u64, u64, u64) {
        (
            self.nums.first().copied().unwrap_or(0),
            self.nums.get(1).copied().unwrap_or(0),
            self.nums.get(2).copied().unwrap_or(0),
        )
    }
}

fn looks_full(s: &str) -> bool {
    let fields = split_fields(s);
    s.contains('-') || s.contains('+') || (fields.len() >= 3 && !fields.iter().any(|f| matches!(*f, "*" | "x" | "X")))
}

fn parse_partial(token: Token<'_>) -> Result<Partial, RangeError> {
    let s = token.text;
    if looks_full(s) {
        let version = parse_version(s).map_err(|error| RangeError::Version {
            at: token.start,
            error,
        })?;
        return Ok(Partial {
            nums: vec![version.major, version.minor, version.patch],
            explicit_pre: !version.pre.is_empty(),
            full: Some(version),
        });
    }

    let fields = split_fields(s);
    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at: token.start });
    }

    let mut nums = Vec::new();
    let mut wildcard_seen = false;
    for field in fields {
        if matches!(field, "*" | "x" | "X") {
            wildcard_seen = true;
            continue;
        }
        if field.is_empty() || wildcard_seen {
            return Err(RangeError::Syntax { at: token.start });
        }
        if !field.bytes().all(|b| b.is_ascii_digit())
            || (field.len() > 1 && field.as_bytes()[0] == b'0')
        {
            return Err(RangeError::Syntax { at: token.start });
        }
        let n = field
            .parse::<u64>()
            .map_err(|_| RangeError::Syntax { at: token.start })?;
        nums.push(n);
    }

    Ok(Partial {
        nums,
        full: None,
        explicit_pre: false,
    })
}

fn add_anchor(arm: &mut Arm, partial: &Partial) {
    if partial.explicit_pre {
        if let Some(v) = &partial.full {
            arm.anchors.push(tuple(v));
        }
    }
}

fn push_comparator(arm: &mut Arm, op: Op, v: Version) {
    let p = Partial {
        explicit_pre: !v.pre.is_empty(),
        nums: vec![v.major, v.minor, v.patch],
        full: Some(v.clone()),
    };
    add_anchor(arm, &p);
    arm.comparators.push(version_at(v, op));
}

fn increment_upper(
    nums: (u64, u64, u64),
    field: usize,
    at: usize,
) -> Result<Version, RangeError> {
    let (mut major, mut minor, mut patch) = nums;
    match field {
        0 => {
            major = major.checked_add(1).ok_or(RangeError::Syntax { at })?;
            Ok(upper_tuple(major, 0, 0))
        }
        1 => {
            minor = minor.checked_add(1).ok_or(RangeError::Syntax { at })?;
            Ok(upper_tuple(major, minor, 0))
        }
        _ => {
            patch = patch.checked_add(1).ok_or(RangeError::Syntax { at })?;
            Ok(upper_tuple(major, minor, patch))
        }
    }
}

fn parse_bare_or_expanded(token: Token<'_>, arm: &mut Arm) -> Result<(), RangeError> {
    let (kind, operand_start, operand) = if token.text.starts_with('^') {
        (Some('^'), token.start + 1, &token.text[1..])
    } else if token.text.starts_with('~') {
        (Some('~'), token.start + 1, &token.text[1..])
    } else {
        (None, token.start, token.text)
    };
    if operand.is_empty() {
        return Err(RangeError::Syntax { at: token.start });
    }
    let partial = parse_partial(Token {
        text: operand,
        start: operand_start,
    })?;

    if kind.is_none() {
        if let Some(full) = partial.full.clone() {
            push_comparator(arm, Op::Eq, full);
            return Ok(());
        }
        let k = partial.numeric_count();
        if k == 0 {
            return Ok(());
        }
        let low = stable_tuple(partial.padded().0, partial.padded().1, partial.padded().2);
        arm.comparators.push(version_at(low, Op::Ge));
        let field = if k == 1 { 0 } else { 1 };
        let high = increment_upper(partial.padded(), field, operand_start)?;
        arm.comparators.push(version_at(high, Op::Lt));
        return Ok(());
    }

    let k = partial.numeric_count();
    if k == 0 {
        return Err(RangeError::Syntax { at: token.start });
    }
    let low = partial.full.clone().unwrap_or_else(|| {
        let (a, b, c) = partial.padded();
        stable_tuple(a, b, c)
    });
    if partial.explicit_pre {
        arm.anchors.push(tuple(&low));
    }
    arm.comparators.push(version_at(low.clone(), Op::Ge));

    let (a, b, c) = partial.padded();
    let field = match kind.unwrap() {
        '^' if a > 0 || k == 1 => 0,
        '^' if b > 0 || k == 2 => 1,
        '^' => 2,
        '~' if k == 1 => 0,
        '~' => 1,
        _ => unreachable!(),
    };
    let high = increment_upper((a, b, c), field, operand_start)?;
    arm.comparators.push(version_at(high, Op::Lt));
    Ok(())
}

fn parse_explicit_comparator(token: Token<'_>, arm: &mut Arm) -> Result<(), RangeError> {
    let s = token.text;
    let (op, n) = if s.starts_with(">=") {
        (Op::Ge, 2)
    } else if s.starts_with("<=") {
        (Op::Le, 2)
    } else if s.starts_with('>') {
        (Op::Gt, 1)
    } else if s.starts_with('<') {
        (Op::Lt, 1)
    } else if s.starts_with('=') {
        (Op::Eq, 1)
    } else {
        unreachable!()
    };
    let operand = &s[n..];
    let operand_start = token.start + n;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: token.start });
    }
    let version = parse_version(operand).map_err(|error| RangeError::Version {
        at: operand_start,
        error,
    })?;
    push_comparator(arm, op, version);
    Ok(())
}

fn parse_hyphen_arm(tokens: &[Token<'_>], arm: &mut Arm) -> Result<(), RangeError> {
    let minuses: Vec<usize> = tokens
        .iter()
        .filter(|t| t.text == "-")
        .map(|t| t.start)
        .collect();
    if minuses.len() != 1 || tokens.len() != 3 || tokens[1].text != "-" {
        return Err(RangeError::Syntax {
            at: minuses[0],
        });
    }

    let lower = parse_partial(tokens[0])?;
    let upper = parse_partial(tokens[2])?;
    if lower.numeric_count() == 0 {
        return Err(RangeError::Syntax { at: tokens[0].start });
    }
    if upper.numeric_count() == 0 {
        return Err(RangeError::Syntax { at: tokens[2].start });
    }

    let low_v = lower
        .full
        .clone()
        .unwrap_or_else(|| {
            let (a, b, c) = lower.padded();
            stable_tuple(a, b, c)
        });
    if lower.explicit_pre {
        arm.anchors.push(tuple(&low_v));
    }
    arm.comparators.push(version_at(low_v, Op::Ge));

    if let Some(full) = upper.full.clone() {
        if upper.explicit_pre {
            arm.anchors.push(tuple(&full));
        }
        arm.comparators.push(version_at(full, Op::Le));
    } else {
        let k = upper.numeric_count();
        let field = if k == 1 { 0 } else { 1 };
        let high = increment_upper(upper.padded(), field, tokens[2].start)?;
        arm.comparators.push(version_at(high, Op::Lt));
    }
    Ok(())
}

fn parse_arm(tokens: &[Token<'_>]) -> Result<Arm, RangeError> {
    let mut arm = Arm {
        comparators: Vec::new(),
        anchors: Vec::new(),
    };
    if tokens.iter().any(|t| t.text == "-") {
        parse_hyphen_arm(tokens, &mut arm)?;
        return Ok(arm);
    }

    for token in tokens {
        let first = token.text.as_bytes()[0];
        if matches!(first, b'<' | b'>' | b'=') {
            parse_explicit_comparator(*token, &mut arm)?;
        } else if matches!(first, b'^' | b'~' | b'*' | b'x' | b'X')
            || first.is_ascii_digit()
        {
            parse_bare_or_expanded(*token, &mut arm)?;
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
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].text == "||" {
            if current.is_empty() {
                return Err(RangeError::Empty { at: tokens[i].start });
            }
            arms.push(parse_arm(&current)?);
            current.clear();
            i += 1;
            if i == tokens.len() {
                return Err(RangeError::Empty { at: s.len() });
            }
        } else {
            current.push(tokens[i]);
            i += 1;
        }
    }
    if current.is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(&current)?);
    Ok(Range { arms })
}

fn arm_matches(v: &Version, arm: &Arm) -> bool {
    if !v.pre.is_empty() && !arm.anchors.contains(&tuple(v)) {
        return false;
    }
    arm.comparators.iter().all(|c| c.test(v))
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
        match best {
            None => best = Some(index),
            Some(old) => {
                let ord = compare_versions(version, &parsed[old]);
                if (want_max && ord == Ordering::Greater) || (!want_max && ord == Ordering::Less) {
                    best = Some(index);
                }
            }
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