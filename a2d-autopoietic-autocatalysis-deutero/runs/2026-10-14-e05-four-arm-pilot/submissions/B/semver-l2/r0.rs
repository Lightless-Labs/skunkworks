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

fn is_core_missing_byte(b: u8) -> bool {
    matches!(b, b'.' | b'-' | b'+')
}

fn parse_core(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if *pos == start {
        if start == bytes.len() || is_core_missing_byte(bytes[start]) {
            return Err(VersionError::MissingCore { at: start });
        }
        return Err(VersionError::InvalidChar { at: start });
    }

    let digits = &bytes[start..*pos];
    if digits.len() > 1 && digits[0] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    let mut n = 0u64;
    for &b in digits {
        let d = (b - b'0') as u64;
        n = n
            .checked_mul(10)
            .and_then(|x| x.checked_add(d))
            .ok_or(VersionError::Overflow { at: start })?;
    }
    Ok(n)
}

fn valid_identifier_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    is_pre: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut ident_start = start;

    loop {
        let mut i = ident_start;
        while i < end && bytes[i] != b'.' {
            if !valid_identifier_char(bytes[i]) {
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
            && ident.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: ident_start });
        }
        out.push(ident.to_string());
        if i == end {
            break;
        }
        ident_start = i + 1;
    }

    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let mut pos = 0;
    let major = parse_core(s, &mut pos)?;
    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        let at = pos;
        if pos == s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at });
        }
        return Err(VersionError::InvalidChar { at });
    }
    pos += 1;

    let minor = parse_core(s, &mut pos)?;
    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        let at = pos;
        if pos == s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at });
        }
        return Err(VersionError::InvalidChar { at });
    }
    pos += 1;

    let patch = parse_core(s, &mut pos)?;
    if pos < s.len() && !matches!(s.as_bytes()[pos], b'-' | b'+') {
        return Err(VersionError::InvalidChar { at: pos });
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < s.len() && s.as_bytes()[pos] == b'-' {
        let pre_start = pos + 1;
        let plus = s[pre_start..].find('+').map(|x| pre_start + x);
        let pre_end = plus.unwrap_or(s.len());
        pre = parse_identifiers(s, pre_start, pre_end, true)?;
        pos = pre_end;
    }

    if pos < s.len() && s.as_bytes()[pos] == b'+' {
        let build_start = pos + 1;
        build = parse_identifiers(s, build_start, s.len(), false)?;
    }

    Ok(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn core_tuple(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn all_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn compare_ident(a: &str, b: &str) -> Ordering {
    let an = all_digits(a);
    let bn = all_digits(b);
    match (an, bn) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    let core = core_tuple(a).cmp(&core_tuple(b));
    if core != Ordering::Equal {
        return core;
    }
    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (x, y) in a.pre.iter().zip(&b.pre) {
                let ord = compare_ident(x, y);
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            a.pre.len().cmp(&b.pre.len())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
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

#[derive(Clone, Copy, Debug)]
struct Token<'a> {
    text: &'a str,
    start: usize,
}

fn is_range_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokenize(s: &str) -> Vec<Token<'_>> {
    let bytes = s.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && is_range_ws(bytes[i]) {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !is_range_ws(bytes[i]) {
            i += 1;
        }
        tokens.push(Token {
            text: &s[start..i],
            start,
        });
    }
    tokens
}

fn full_operand(token: &str, start: usize) -> Result<Version, RangeError> {
    if token.is_empty() || !token.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: start });
    }
    parse_version(token).map_err(|error| RangeError::Version { at: start, error })
}

fn padded_version(fields: &[u64], pre: Vec<String>, build: Vec<String>) -> Version {
    Version {
        major: fields.first().copied().unwrap_or(0),
        minor: fields.get(1).copied().unwrap_or(0),
        patch: fields.get(2).copied().unwrap_or(0),
        pre,
        build,
    }
}

#[derive(Clone, Debug)]
struct Partial {
    fields: Vec<Option<u64>>,
    numeric_count: usize,
    full: Option<Version>,
}

fn partial_operand(token: &str, start: usize) -> Result<Partial, RangeError> {
    let has_suffix = token.contains('-') || token.contains('+');
    let pieces: Vec<&str> = token.split('.').collect();
    let has_three_no_wildcard =
        pieces.len() >= 3 && !pieces.iter().any(|p| matches!(*p, "*" | "x" | "X"));

    if has_suffix || has_three_no_wildcard {
        let v = full_operand(token, start)?;
        return Ok(Partial {
            fields: vec![Some(v.major), Some(v.minor), Some(v.patch)],
            numeric_count: 3,
            full: Some(v),
        });
    }

    if pieces.is_empty() || pieces.len() > 3 {
        return Err(RangeError::Syntax { at: start });
    }

    let mut fields = Vec::new();
    let mut seen_wildcard = false;
    let mut numeric_count = 0;
    for p in pieces {
        if p.is_empty() {
            return Err(RangeError::Syntax { at: start });
        }
        if matches!(p, "*" | "x" | "X") {
            seen_wildcard = true;
            fields.push(None);
            continue;
        }
        if seen_wildcard
            || !p.bytes().all(|b| b.is_ascii_digit())
            || (p.len() > 1 && p.as_bytes()[0] == b'0')
        {
            return Err(RangeError::Syntax { at: start });
        }
        let mut n = 0u64;
        for b in p.bytes() {
            n = n
                .checked_mul(10)
                .and_then(|x| x.checked_add((b - b'0') as u64))
                .ok_or(RangeError::Syntax { at: start })?;
        }
        numeric_count += 1;
        fields.push(Some(n));
    }
    if seen_wildcard {
        // Wildcard fields imply all following fields are wildcard; missing fields are implicit wildcards.
        if fields.iter().skip_while(|x| x.is_some()).any(|x| x.is_some()) {
            return Err(RangeError::Syntax { at: start });
        }
    }
    Ok(Partial {
        fields,
        numeric_count,
        full: None,
    })
}

fn partial_lower(p: &Partial) -> Version {
    if let Some(v) = &p.full {
        return v.clone();
    }
    let vals: Vec<u64> = p.fields.iter().map(|x| x.unwrap_or(0)).collect();
    padded_version(&vals, Vec::new(), Vec::new())
}

fn bump_upper(
    lower: &Version,
    field: usize,
    operand_start: usize,
) -> Result<Version, RangeError> {
    let (major, minor, patch) = (lower.major, lower.minor, lower.patch);
    let mut out = Version {
        major,
        minor,
        patch,
        pre: vec!["0".to_string()],
        build: Vec::new(),
    };
    let target = match field {
        0 => &mut out.major,
        1 => &mut out.minor,
        _ => &mut out.patch,
    };
    *target = target
        .checked_add(1)
        .ok_or(RangeError::Syntax { at: operand_start })?;
    match field {
        0 => {
            out.minor = 0;
            out.patch = 0;
        }
        1 => out.patch = 0,
        _ => {}
    }
    Ok(out)
}

fn add_cmp(arm: &mut Arm, op: Op, version: Version, anchor: bool) {
    arm.comparators.push(Comparator { op, version, anchor });
}

fn add_partial_bare(
    arm: &mut Arm,
    token: Token<'_>,
    p: Partial,
) -> Result<(), RangeError> {
    if let Some(v) = p.full {
        let anchor = !v.pre.is_empty();
        add_cmp(arm, Op::Eq, v, anchor);
        return Ok(());
    }
    match p.numeric_count {
        0 => Ok(()),
        1 | 2 => {
            let lower = partial_lower(&p);
            let upper = bump_upper(&lower, p.numeric_count - 1, token.start)?;
            add_cmp(arm, Op::Ge, lower, false);
            add_cmp(arm, Op::Lt, upper, false);
            Ok(())
        }
        _ => {
            let lower = partial_lower(&p);
            let anchor = !lower.pre.is_empty();
            add_cmp(arm, Op::Eq, lower, anchor);
            Ok(())
        }
    }
}

fn parse_explicit_comparator(
    arm: &mut Arm,
    token: Token<'_>,
) -> Result<(), RangeError> {
    let bytes = token.text.as_bytes();
    let (op, n) = if bytes.starts_with(b">=") {
        (Op::Ge, 2)
    } else if bytes.starts_with(b"<=") {
        (Op::Le, 2)
    } else if bytes.starts_with(b">") {
        (Op::Gt, 1)
    } else if bytes.starts_with(b"<") {
        (Op::Lt, 1)
    } else if bytes.starts_with(b"=") {
        (Op::Eq, 1)
    } else {
        return Err(RangeError::Syntax { at: token.start });
    };
    let operand = &token.text[n..];
    let v = full_operand(operand, token.start + n)?;
    let anchor = !v.pre.is_empty();
    add_cmp(arm, op, v, anchor);
    Ok(())
}

fn parse_caret_tilde(
    arm: &mut Arm,
    token: Token<'_>,
    caret: bool,
) -> Result<(), RangeError> {
    let prefix_len = 1;
    let operand = &token.text[prefix_len..];
    let operand_start = token.start + prefix_len;
    let p = partial_operand(operand, operand_start)?;
    if p.numeric_count == 0 {
        return Err(RangeError::Syntax { at: token.start });
    }
    let lower = partial_lower(&p);
    let upper_field = if caret {
        if lower.major > 0 || p.numeric_count == 1 {
            0
        } else if lower.minor > 0 || p.numeric_count == 2 {
            1
        } else {
            2
        }
    } else if p.numeric_count == 1 {
        0
    } else {
        1
    };
    let upper = bump_upper(&lower, upper_field, operand_start)?;
    let anchor = !lower.pre.is_empty();
    add_cmp(arm, Op::Ge, lower, anchor);
    add_cmp(arm, Op::Lt, upper, false);
    Ok(())
}

fn parse_hyphen_arm(tokens: &[Token<'_>], minus_index: usize) -> Result<Arm, RangeError> {
    let minus = tokens[minus_index];
    if tokens.len() != 3 || minus_index != 1 {
        return Err(RangeError::Syntax { at: minus.start });
    }

    let lower_p = partial_operand(tokens[0].text, tokens[0].start)?;
    let upper_p = partial_operand(tokens[2].text, tokens[2].start)?;
    if lower_p.numeric_count == 0 {
        return Err(RangeError::Syntax { at: tokens[0].start });
    }
    if upper_p.numeric_count == 0 {
        return Err(RangeError::Syntax { at: tokens[2].start });
    }

    let mut arm = Arm {
        comparators: Vec::new(),
    };
    let lower = partial_lower(&lower_p);
    let lower_anchor = !lower.pre.is_empty();
    add_cmp(&mut arm, Op::Ge, lower, lower_anchor);

    if let Some(upper) = upper_p.full {
        let anchor = !upper.pre.is_empty();
        add_cmp(&mut arm, Op::Le, upper, anchor);
    } else if upper_p.numeric_count == 3 {
        add_cmp(&mut arm, Op::Le, partial_lower(&upper_p), false);
    } else {
        let upper_base = partial_lower(&upper_p);
        let upper = bump_upper(
            &upper_base,
            upper_p.numeric_count - 1,
            tokens[2].start,
        )?;
        add_cmp(&mut arm, Op::Lt, upper, false);
    }
    Ok(arm)
}

fn parse_arm(tokens: &[Token<'_>]) -> Result<Arm, RangeError> {
    if let Some(index) = tokens.iter().position(|t| t.text == "-") {
        return parse_hyphen_arm(tokens, index);
    }

    let mut arm = Arm {
        comparators: Vec::new(),
    };
    for token in tokens {
        let text = token.text;
        let first = text.as_bytes().first().copied().unwrap_or(0);
        if matches!(text, "^" | "~") {
            return Err(RangeError::Syntax { at: token.start });
        }
        if matches!(first, b'<' | b'>' | b'=') {
            parse_explicit_comparator(&mut arm, *token)?;
        } else if first == b'^' {
            parse_caret_tilde(&mut arm, *token, true)?;
        } else if first == b'~' {
            parse_caret_tilde(&mut arm, *token, false)?;
        } else if first.is_ascii_digit() || matches!(first, b'*' | b'x' | b'X') {
            let p = partial_operand(text, token.start)?;
            add_partial_bare(&mut arm, *token, p)?;
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
            if i + 1 == tokens.len() {
                return Err(RangeError::Empty { at: s.len() });
            }
        } else {
            current.push(tokens[i]);
        }
        i += 1;
    }
    if !current.is_empty() {
        arms.push(parse_arm(&current)?);
    } else {
        return Err(RangeError::Empty { at: s.len() });
    }

    Ok(Range { arms })
}

fn cmp_matches(v: &Version, cmp: &Comparator) -> bool {
    let ord = compare_versions(v, &cmp.version);
    match cmp.op {
        Op::Eq => ord == Ordering::Equal,
        Op::Lt => ord == Ordering::Less,
        Op::Le => ord != Ordering::Greater,
        Op::Gt => ord == Ordering::Greater,
        Op::Ge => ord != Ordering::Less,
    }
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        let comparisons_pass = arm.comparators.iter().all(|c| cmp_matches(v, c));
        if !comparisons_pass {
            return false;
        }
        if v.pre.is_empty() {
            return true;
        }
        arm.comparators.iter().any(|c| {
            c.anchor && core_tuple(&c.version) == core_tuple(v)
        })
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn extrema(
    candidates: &[String],
    r: &Range,
    want_max: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, text) in candidates.iter().enumerate() {
        match parse_version(text) {
            Ok(v) => parsed.push(v),
            Err(error) => return Err(CandidateError { index, error }),
        }
    }

    let mut best: Option<usize> = None;
    for (i, v) in parsed.iter().enumerate() {
        if !matches(v, r) {
            continue;
        }
        match best {
            None => best = Some(i),
            Some(j) => {
                let ord = compare_versions(v, &parsed[j]);
                if (want_max && ord == Ordering::Greater) || (!want_max && ord == Ordering::Less) {
                    best = Some(i);
                }
            }
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