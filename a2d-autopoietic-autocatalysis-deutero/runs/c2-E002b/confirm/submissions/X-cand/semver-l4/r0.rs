use std::cmp::Ordering;
use std::collections::HashSet;

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

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

fn missing_byte(b: Option<u8>) -> bool {
    matches!(b, None | Some(b'.') | Some(b'-') | Some(b'+'))
}

fn parse_core_number(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    if start >= bytes.len() || !is_digit(bytes[start]) {
        return match bytes.get(start).copied() {
            Some(b) if missing_byte(Some(b)) => Err(VersionError::MissingCore { at: start }),
            None => Err(VersionError::MissingCore { at: start }),
            _ => Err(VersionError::InvalidChar { at: start }),
        };
    }

    while *pos < bytes.len() && is_digit(bytes[*pos]) {
        *pos += 1;
    }
    let field = &bytes[start..*pos];
    if field.len() > 1 && field[0] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }

    let mut value = 0u64;
    for &b in field {
        let digit = u64::from(b - b'0');
        value = match value.checked_mul(10).and_then(|v| v.checked_add(digit)) {
            Some(v) => v,
            None => return Err(VersionError::Overflow { at: start }),
        };
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
    let mut out = Vec::new();
    let mut id_start = start;

    loop {
        let mut p = id_start;
        while p < end && bytes[p] != b'.' {
            if !is_ident_byte(bytes[p]) {
                return Err(VersionError::InvalidChar { at: p });
            }
            p += 1;
        }

        if p == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }

        if pre {
            let id = &bytes[id_start..p];
            if id.iter().all(|b| is_digit(*b)) && id.len() > 1 && id[0] == b'0' {
                return Err(VersionError::LeadingZero { at: id_start });
            }
        }

        out.push(s[id_start..p].to_string());
        if p >= end {
            break;
        }
        id_start = p + 1;
        if id_start == end {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
    }
    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let mut pos = 0;
    let major = parse_core_number(s, &mut pos)?;
    if s.as_bytes().get(pos) != Some(&b'.') {
        return match s.as_bytes().get(pos).copied() {
            Some(b) if missing_byte(Some(b)) => Err(VersionError::MissingCore { at: pos }),
            None => Err(VersionError::MissingCore { at: pos }),
            _ => Err(VersionError::InvalidChar { at: pos }),
        };
    }
    pos += 1;

    let minor = parse_core_number(s, &mut pos)?;
    if s.as_bytes().get(pos) != Some(&b'.') {
        return match s.as_bytes().get(pos).copied() {
            Some(b) if missing_byte(Some(b)) => Err(VersionError::MissingCore { at: pos }),
            None => Err(VersionError::MissingCore { at: pos }),
            _ => Err(VersionError::InvalidChar { at: pos }),
        };
    }
    pos += 1;

    let patch = parse_core_number(s, &mut pos)?;
    let bytes = s.as_bytes();
    if pos < bytes.len() && bytes[pos] != b'-' && bytes[pos] != b'+' {
        return Err(VersionError::InvalidChar { at: pos });
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();
    if bytes.get(pos) == Some(&b'-') {
        let pre_start = pos + 1;
        let plus = bytes[pre_start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|i| pre_start + i);
        let pre_end = plus.unwrap_or(bytes.len());
        pre = parse_identifiers(s, pre_start, pre_end, true)?;
        pos = pre_end;
    }

    if bytes.get(pos) == Some(&b'+') {
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

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn is_numeric_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn compare_pre_id(a: &str, b: &str) -> Ordering {
    let an = is_numeric_id(a);
    let bn = is_numeric_id(b);
    match (an, bn) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        let ord = compare_pre_id(x, y);
        if ord != Ordering::Equal {
            return ord;
        }
    }
    a.len().cmp(&b.len())
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    core(a)
        .cmp(&core(b))
        .then_with(|| match (a.pre.is_empty(), b.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => compare_pre(&a.pre, &b.pre),
        })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
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

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

fn token_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

#[derive(Clone, Debug)]
struct Token {
    text: String,
    start: usize,
}

fn split_tokens(s: &str, offset: usize) -> Vec<Token> {
    let bytes = s.as_bytes();
    let mut tokens = Vec::new();
    let mut p = 0;
    while p < bytes.len() {
        while p < bytes.len() && token_ws(bytes[p]) {
            p += 1;
        }
        if p == bytes.len() {
            break;
        }
        let start = p;
        while p < bytes.len() && !token_ws(bytes[p]) {
            p += 1;
        }
        tokens.push(Token {
            text: s[start..p].to_string(),
            start: offset + start,
        });
    }
    tokens
}

fn parse_full_operand(token: &Token, operand: &str, operand_start: usize) -> Result<Version, RangeError> {
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: token.start });
    }
    parse_version(operand).map_err(|error| RangeError::Version { at: operand_start, error })
}

#[derive(Clone, Debug)]
struct Partial {
    values: Vec<u64>,
    k: usize,
    full: Option<Version>,
}

fn parse_partial(token: &Token, text: &str, start: usize) -> Result<Partial, RangeError> {
    if text.contains('-') || text.contains('+') {
        let v = parse_full_operand(token, text, start)?;
        return Ok(Partial {
            values: vec![v.major, v.minor, v.patch],
            k: 3,
            full: Some(v),
        });
    }

    let fields: Vec<&str> = text.split('.').collect();
    let has_wild = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));
    if fields.len() >= 3 && !has_wild {
        let v = parse_full_operand(token, text, start)?;
        return Ok(Partial {
            values: vec![v.major, v.minor, v.patch],
            k: 3,
            full: Some(v),
        });
    }

    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|f| f.is_empty()) {
        return Err(RangeError::Syntax { at: token.start });
    }

    let mut values = Vec::new();
    let mut wildcard_seen = false;
    for field in fields {
        if matches!(field, "*" | "x" | "X") {
            wildcard_seen = true;
            continue;
        }
        if wildcard_seen || !field.bytes().all(is_digit) {
            return Err(RangeError::Syntax { at: token.start });
        }
        if field.len() > 1 && field.as_bytes()[0] == b'0' {
            return Err(RangeError::Syntax { at: token.start });
        }
        let value = field.parse::<u64>().map_err(|_| RangeError::Syntax { at: token.start })?;
        values.push(value);
    }

    Ok(Partial {
        k: values.len(),
        values,
        full: None,
    })
}

fn make_version(major: u64, minor: u64, patch: u64, pre: Vec<String>) -> Version {
    Version {
        major,
        minor,
        patch,
        pre,
        build: Vec::new(),
    }
}

fn add_bound(arm: &mut Arm, op: &str, v: Version) {
    match op {
        "=" => {
            add_bound(arm, ">=", v.clone());
            add_bound(arm, "<=", v);
        }
        ">" | ">=" => {
            let candidate = Bound {
                version: v,
                inclusive: op == ">=",
            };
            let replace = match &arm.lower {
                None => true,
                Some(old) => {
                    compare_versions(&candidate.version, &old.version) == Ordering::Greater
                        || (compare_versions(&candidate.version, &old.version) == Ordering::Equal
                            && !candidate.inclusive
                            && old.inclusive)
                }
            };
            if replace {
                arm.lower = Some(candidate);
            }
        }
        "<" | "<=" => {
            let candidate = Bound {
                version: v,
                inclusive: op == "<=",
            };
            let replace = match &arm.upper {
                None => true,
                Some(old) => {
                    compare_versions(&candidate.version, &old.version) == Ordering::Less
                        || (compare_versions(&candidate.version, &old.version) == Ordering::Equal
                            && !candidate.inclusive
                            && old.inclusive)
                }
            };
            if replace {
                arm.upper = Some(candidate);
            }
        }
        _ => {}
    }
}

fn add_anchor(arm: &mut Arm, v: &Version) {
    if !v.pre.is_empty() {
        arm.anchors.push(core(v));
    }
}

fn padded(p: &Partial) -> (u64, u64, u64) {
    (
        p.values.first().copied().unwrap_or(0),
        p.values.get(1).copied().unwrap_or(0),
        p.values.get(2).copied().unwrap_or(0),
    )
}

fn upper_after_partial(
    token_start: usize,
    p: &Partial,
    mode: &str,
) -> Result<Version, RangeError> {
    let (mut major, mut minor, mut patch) = padded(p);
    match mode {
        "bare" => match p.k {
            1 => {
                major = major.checked_add(1).ok_or(RangeError::Syntax { at: token_start })?;
                minor = 0;
                patch = 0;
            }
            2 => {
                minor = minor.checked_add(1).ok_or(RangeError::Syntax { at: token_start })?;
                patch = 0;
            }
            _ => unreachable!(),
        },
        "caret" => {
            if major > 0 || p.k == 1 {
                major = major.checked_add(1).ok_or(RangeError::Syntax { at: token_start })?;
                minor = 0;
                patch = 0;
            } else if minor > 0 || p.k == 2 {
                minor = minor.checked_add(1).ok_or(RangeError::Syntax { at: token_start })?;
                patch = 0;
            } else {
                patch = patch.checked_add(1).ok_or(RangeError::Syntax { at: token_start })?;
            }
        }
        "tilde" => {
            if p.k == 1 {
                major = major.checked_add(1).ok_or(RangeError::Syntax { at: token_start })?;
                minor = 0;
                patch = 0;
            } else {
                minor = minor.checked_add(1).ok_or(RangeError::Syntax { at: token_start })?;
                patch = 0;
            }
        }
        _ => unreachable!(),
    }
    Ok(make_version(major, minor, patch, vec!["0".to_string()]))
}

fn parse_simple_token(arm: &mut Arm, token: &Token) -> Result<(), RangeError> {
    let text = token.text.as_str();
    if text.starts_with('^') || text.starts_with('~') {
        let mode = if text.starts_with('^') { "caret" } else { "tilde" };
        let operand = &text[1..];
        let start = token.start + 1;
        let p = parse_partial(token, operand, start)?;
        if p.k == 0 {
            return Err(RangeError::Syntax { at: token.start });
        }
        let lower = p.full.clone().unwrap_or_else(|| {
            let (a, b, c) = padded(&p);
            make_version(a, b, c, Vec::new())
        });
        add_anchor(arm, &lower);
        add_bound(arm, ">=", lower);
        if p.k < 3 || p.full.is_none() {
            let upper = upper_after_partial(token.start, &p, mode)?;
            add_bound(arm, "<", upper);
        }
        return Ok(());
    }

    if text.starts_with('<') || text.starts_with('>') || text.starts_with('=') {
        let (op, rest) = if text.starts_with("<=") {
            ("<=", &text[2..])
        } else if text.starts_with(">=") {
            (">=", &text[2..])
        } else if text.starts_with('<') {
            ("<", &text[1..])
        } else if text.starts_with('>') {
            (">", &text[1..])
        } else {
            ("=", &text[1..])
        };
        let operand_start = token.start + (text.len() - rest.len());
        let v = parse_full_operand(token, rest, operand_start)?;
        add_anchor(arm, &v);
        add_bound(arm, op, v);
        return Ok(());
    }

    if !text.as_bytes().first().copied().map_or(false, |b| {
        b.is_ascii_digit() || matches!(b, b'*' | b'x' | b'X')
    }) {
        return Err(RangeError::Syntax { at: token.start });
    }

    let p = parse_partial(token, text, token.start)?;
    if p.k == 0 {
        return Ok(());
    }
    if let Some(v) = p.full.clone() {
        add_anchor(arm, &v);
        add_bound(arm, "=", v);
    } else {
        let (a, b, c) = padded(&p);
        let lower = make_version(a, b, c, Vec::new());
        add_bound(arm, ">=", lower);
        if p.k < 3 {
            let upper = upper_after_partial(token.start, &p, "bare")?;
            add_bound(arm, "<", upper);
        }
    }
    Ok(())
}

fn parse_hyphen_arm(tokens: &[Token], dash_index: usize, arm: &mut Arm) -> Result<(), RangeError> {
    if dash_index != 1 || tokens.len() != 3 {
        return Err(RangeError::Syntax {
            at: tokens[dash_index].start,
        });
    }

    let lower_token = &tokens[0];
    let upper_token = &tokens[2];
    let lower = parse_partial(lower_token, &lower_token.text, lower_token.start)?;
    let upper = parse_partial(upper_token, &upper_token.text, upper_token.start)?;
    if lower.k == 0 {
        return Err(RangeError::Syntax { at: lower_token.start });
    }
    if upper.k == 0 {
        return Err(RangeError::Syntax { at: upper_token.start });
    }

    let lower_v = lower.full.clone().unwrap_or_else(|| {
        let (a, b, c) = padded(&lower);
        make_version(a, b, c, Vec::new())
    });
    add_anchor(arm, &lower_v);
    add_bound(arm, ">=", lower_v);

    if let Some(upper_v) = upper.full.clone() {
        add_anchor(arm, &upper_v);
        add_bound(arm, "<=", upper_v);
    } else {
        let (a, b, c) = padded(&upper);
        let upper_v = make_version(a, b, c, Vec::new());
        if upper.k == 3 {
            add_bound(arm, "<=", upper_v);
        } else {
            let next = upper_after_partial(upper_token.start, &upper, "bare")?;
            add_bound(arm, "<", next);
        }
    }
    Ok(())
}

fn split_arms(s: &str) -> Result<Vec<(usize, usize)>, RangeError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut start = 0;
    let mut p = 0;
    while p < bytes.len() {
        if bytes[p] == b'|' && bytes.get(p + 1) == Some(&b'|') {
            if p > 0 && token_ws(bytes[p - 1]) && p + 2 < bytes.len() && token_ws(bytes[p + 2]) {
                let mut l = start;
                let mut r = p;
                while l < r && token_ws(bytes[l]) {
                    l += 1;
                }
                while r > l && token_ws(bytes[r - 1]) {
                    r -= 1;
                }
                if l == r {
                    return Err(RangeError::Empty { at: p });
                }
                result.push((l, r));
                start = p + 2;
                p += 2;
                continue;
            }
        }
        p += 1;
    }

    let mut l = start;
    let mut r = bytes.len();
    while l < r && token_ws(bytes[l]) {
        l += 1;
    }
    while r > l && token_ws(bytes[r - 1]) {
        r -= 1;
    }
    if l == r {
        return Err(RangeError::Empty { at: bytes.len() });
    }
    result.push((l, r));
    Ok(result)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    if s.as_bytes().iter().all(|b| token_ws(*b)) {
        return Err(RangeError::Empty { at: 0 });
    }

    let spans = split_arms(s)?;
    let mut arms = Vec::new();
    for (start, end) in spans {
        let tokens = split_tokens(&s[start..end], start);
        if tokens.is_empty() {
            return Err(RangeError::Empty { at: start });
        }
        let dash = tokens.iter().position(|t| t.text == "-");
        let mut arm = Arm {
            lower: None,
            upper: None,
            anchors: Vec::new(),
        };
        if let Some(index) = dash {
            parse_hyphen_arm(&tokens, index, &mut arm)?;
        } else {
            for token in &tokens {
                parse_simple_token(&mut arm, token)?;
            }
        }
        arm.anchors.sort();
        arm.anchors.dedup();
        arms.push(arm);
    }
    Ok(Range { arms })
}

fn bound_compare_lower(a: &Bound, b: &Bound) -> Ordering {
    compare_versions(&a.version, &b.version).then_with(|| a.inclusive.cmp(&b.inclusive))
}

fn bound_compare_upper(a: &Bound, b: &Bound) -> Ordering {
    compare_versions(&b.version, &a.version).then_with(|| a.inclusive.cmp(&b.inclusive))
}

fn arm_comparison(arm: &Arm, v: &Version) -> bool {
    if let Some(lower) = &arm.lower {
        let c = compare_versions(v, &lower.version);
        if c == Ordering::Less || (c == Ordering::Equal && !lower.inclusive) {
            return false;
        }
    }
    if let Some(upper) = &arm.upper {
        let c = compare_versions(v, &upper.version);
        if c == Ordering::Greater || (c == Ordering::Equal && !upper.inclusive) {
            return false;
        }
    }
    true
}

fn arm_admits(arm: &Arm, v: &Version) -> bool {
    v.pre.is_empty() || arm.anchors.iter().any(|x| *x == core(v))
}

fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|arm| {
        arm_comparison(arm, v) && (include_prerelease || arm_admits(arm, v))
    })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

fn next_core(mut major: u64, mut minor: u64, mut patch: u64) -> Option<(u64, u64, u64)> {
    if patch < u64::MAX {
        patch += 1;
        Some((major, minor, patch))
    } else if minor < u64::MAX {
        minor += 1;
        patch = 0;
        Some((major, minor, patch))
    } else if major < u64::MAX {
        major += 1;
        minor = 0;
        patch = 0;
        Some((major, minor, patch))
    } else {
        None
    }
}

fn minimum_pre() -> Vec<String> {
    vec!["0".to_string()]
}

fn interval_has_release(arm: &Arm) -> bool {
    let mut candidate = make_version(0, 0, 0, Vec::new());

    if let Some(lower) = &arm.lower {
        if lower.version.pre.is_empty() {
            if lower.inclusive {
                candidate = make_version(
                    lower.version.major,
                    lower.version.minor,
                    lower.version.patch,
                    Vec::new(),
                );
            } else if let Some(tuple) = next_core(
                lower.version.major,
                lower.version.minor,
                lower.version.patch,
            ) {
                candidate = make_version(tuple.0, tuple.1, tuple.2, Vec::new());
            } else {
                return false;
            }
        } else {
            candidate = make_version(
                lower.version.major,
                lower.version.minor,
                lower.version.patch,
                Vec::new(),
            );
        }
    }

    arm_comparison(arm, &candidate)
}

fn interval_has_admitted_pre(arm: &Arm) -> bool {
    for &(major, minor, patch) in &arm.anchors {
        let pre = minimum_pre();
        let candidate = Version {
            major,
            minor,
            patch,
            pre: pre.clone(),
            build: Vec::new(),
        };
        if arm_comparison(arm, &candidate) {
            return true;
        }
    }
    false
}

fn arm_has_default_witness(arm: &Arm) -> bool {
    interval_has_release(arm) || interval_has_admitted_pre(arm)
}

fn version_no_build(v: &Version) -> String {
    let mut out = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        out.push('-');
        out.push_str(&v.pre.join("."));
    }
    out
}

fn canonical_arm(arm: &Arm) -> String {
    let lower = arm.lower.as_ref().map_or("*".to_string(), |b| {
        format!(
            "{}{}",
            if b.inclusive { ">=" } else { ">" },
            version_no_build(&b.version)
        )
    });
    let upper = arm.upper.as_ref().map_or("*".to_string(), |b| {
        format!(
            "{}{}",
            if b.inclusive { "<=" } else { "<" },
            version_no_build(&b.version)
        )
    });
    let anchors = if arm.anchors.is_empty() {
        "-".to_string()
    } else {
        arm.anchors
            .iter()
            .map(|(a, b, c)| format!("{a}.{b}.{c}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    format!("[{lower};{upper};pre={anchors}]")
}

pub fn canonical(r: &Range) -> String {
    let mut arms: Vec<String> = r
        .arms
        .iter()
        .filter(|arm| arm_has_default_witness(arm))
        .map(canonical_arm)
        .collect();
    arms.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    arms.dedup();
    if arms.is_empty() {
        "EMPTY".to_string()
    } else {
        arms.join(" || ")
    }
}

fn witness_between(lower: Option<&Bound>, upper: Option<&Bound>) -> bool {
    let Some(lower) = lower else {
        return true;
    };
    let Some(upper) = upper else {
        return true;
    };

    let cmp = compare_versions(&lower.version, &upper.version);
    if cmp == Ordering::Less {
        return true;
    }
    if cmp == Ordering::Greater {
        return false;
    }
    lower.inclusive && upper.inclusive
}

fn arm_bounds_overlap(a: &Arm, b: &Arm) -> bool {
    let lower = match (&a.lower, &b.lower) {
        (Some(x), Some(y)) => Some(if bound_compare_lower(x, y) == Ordering::Less {
            y
        } else {
            x
        }),
        (Some(x), None) => Some(x),
        (None, Some(y)) => Some(y),
        _ => None,
    };
    let upper = match (&a.upper, &b.upper) {
        (Some(x), Some(y)) => Some(if bound_compare_upper(x, y) == Ordering::Less {
            y
        } else {
            x
        }),
        (Some(x), None) => Some(x),
        (None, Some(y)) => Some(y),
        _ => None,
    };
    witness_between(lower, upper)
}

fn arm_pair_witness(a: &Arm, b: &Arm, include_prerelease: bool) -> bool {
    if !arm_bounds_overlap(a, b) {
        return false;
    }
    if include_prerelease {
        return true;
    }
    let mut combined = Arm {
        lower: a.lower.clone(),
        upper: a.upper.clone(),
        anchors: Vec::new(),
    };
    if let Some(bl) = &b.lower {
        add_bound(&mut combined, ">=", bl.version.clone());
        if !bl.inclusive {
            add_bound(&mut combined, ">", bl.version.clone());
        }
    }
    if let Some(bu) = &b.upper {
        add_bound(&mut combined, "<=", bu.version.clone());
        if !bu.inclusive {
            add_bound(&mut combined, "<", bu.version.clone());
        }
    }
    if interval_has_release(&combined) {
        return true;
    }
    for anchor in &a.anchors {
        if b.anchors.contains(anchor) {
            let candidate = Version {
                major: anchor.0,
                minor: anchor.1,
                patch: anchor.2,
                pre: minimum_pre(),
                build: Vec::new(),
            };
            if arm_comparison(a, &candidate) && arm_comparison(b, &candidate) {
                return true;
            }
        }
    }
    false
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    a.arms.iter().any(|x| {
        b.arms
            .iter()
            .any(|y| arm_pair_witness(x, y, include_prerelease))
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
    maximum: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::new();
    for (index, text) in candidates.iter().enumerate() {
        match parse_version(text) {
            Ok(v) => parsed.push((index, v)),
            Err(error) => return Err(CandidateError { index, error }),
        }
    }

    let mut best: Option<(usize, Version)> = None;
    for (index, v) in parsed {
        if !matches(&v, r) {
            continue;
        }
        let replace = best.as_ref().map_or(true, |(_, old)| {
            let ord = compare_versions(&v, old);
            if maximum {
                ord == Ordering::Greater
            } else {
                ord == Ordering::Less
            }
        });
        if replace {
            best = Some((index, v));
        }
    }
    Ok(best.map(|(index, _)| candidates[index].clone()))
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
    InvalidCandidate {
        package: String,
        index: usize,
        error: VersionError,
    },
    InvalidRange {
        package: String,
        label: String,
        path: Vec<usize>,
        error: RangeError,
    },
    NoSolution {
        package: String,
        labels: Vec<String>,
    },
}

#[derive(Clone, Debug)]
enum ValidExpr {
    Range(Range),
    Union(Vec<ValidExpr>),
    Intersection(Vec<ValidExpr>),
}

fn validate_expr(
    expr: &SetExpr,
    package: &str,
    label: &str,
    path: &mut Vec<usize>,
) -> Result<ValidExpr, SelectError> {
    match expr {
        SetExpr::Range(s) => parse_range(s)
            .map(ValidExpr::Range)
            .map_err(|error| SelectError::InvalidRange {
                package: package.to_string(),
                label: label.to_string(),
                path: path.clone(),
                error,
            }),
        SetExpr::Union(children) => {
            let mut out = Vec::new();
            for (i, child) in children.iter().enumerate() {
                path.push(i);
                let value = validate_expr(child, package, label, path);
                path.pop();
                out.push(value?);
            }
            Ok(ValidExpr::Union(out))
        }
        SetExpr::Intersection(children) => {
            let mut out = Vec::new();
            for (i, child) in children.iter().enumerate() {
                path.push(i);
                let value = validate_expr(child, package, label, path);
                path.pop();
                out.push(value?);
            }
            Ok(ValidExpr::Intersection(out))
        }
    }
}

fn eval_expr(expr: &ValidExpr, v: &Version, include: bool) -> bool {
    match expr {
        ValidExpr::Range(r) => matches_with(v, r, include),
        ValidExpr::Union(children) => children.iter().any(|x| eval_expr(x, v, include)),
        ValidExpr::Intersection(children) => children.iter().all(|x| eval_expr(x, v, include)),
    }
}

fn satisfies_all(exprs: &[ValidExpr], v: &Version, include: bool) -> bool {
    exprs.iter().all(|expr| eval_expr(expr, v, include))
}

fn minimum_conflict(
    constraints: &[Constraint],
    exprs: &[ValidExpr],
    candidates: &[(String, Version)],
    include: bool,
) -> Vec<String> {
    if candidates.is_empty() {
        return Vec::new();
    }

    let n = constraints.len();
    for size in 1..=n {
        let mut indices: Vec<usize> = (0..size).collect();
        loop {
            let any = candidates.iter().any(|(_, v)| {
                indices
                    .iter()
                    .all(|&i| eval_expr(&exprs[i], v, include))
            });
            if !any {
                let mut labels: Vec<String> = indices
                    .iter()
                    .map(|&i| constraints[i].label.clone())
                    .collect();
                labels.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
                return labels;
            }

            let mut i = size;
            while i > 0 {
                i -= 1;
                if indices[i] != i + n - size {
                    break;
                }
            }
            if i == 0 && indices[0] == n - size {
                break;
            }
            indices[i] += 1;
            for j in i + 1..size {
                indices[j] = indices[j - 1] + 1;
            }
        }
    }
    Vec::new()
}

pub fn select_packages(
    packages: &[Package],
    preference: Preference,
    include_prerelease: bool,
) -> Result<Vec<Selection>, SelectError> {
    let mut names = HashSet::new();
    for package in packages {
        if !names.insert(package.name.clone()) {
            return Err(SelectError::DuplicatePackage {
                name: package.name.clone(),
            });
        }
    }

    for package in packages {
        let mut labels = HashSet::new();
        for constraint in &package.constraints {
            if !labels.insert(constraint.label.clone()) {
                return Err(SelectError::DuplicateLabel {
                    package: package.name.clone(),
                    label: constraint.label.clone(),
                });
            }
        }
    }

    let mut parsed_candidates: Vec<Vec<(String, Version)>> = Vec::new();
    for package in packages {
        let mut parsed = Vec::new();
        for (index, text) in package.candidates.iter().enumerate() {
            match parse_version(text) {
                Ok(v) => parsed.push((text.clone(), v)),
                Err(error) => {
                    return Err(SelectError::InvalidCandidate {
                        package: package.name.clone(),
                        index,
                        error,
                    })
                }
            }
        }
        parsed_candidates.push(parsed);
    }

    let mut valid_exprs: Vec<Vec<ValidExpr>> = Vec::new();
    for package in packages {
        let mut exprs = Vec::new();
        for constraint in &package.constraints {
            exprs.push(validate_expr(
                &constraint.expr,
                &package.name,
                &constraint.label,
                &mut Vec::new(),
            )?);
        }
        valid_exprs.push(exprs);
    }

    let mut selections = Vec::new();
    for (i, package) in packages.iter().enumerate() {
        let candidates = &parsed_candidates[i];
        let exprs = &valid_exprs[i];
        let mut eligible: Vec<usize> = candidates
            .iter()
            .enumerate()
            .filter_map(|(index, (_, v))| {
                satisfies_all(exprs, v, include_prerelease).then_some(index)
            })
            .collect();

        if eligible.is_empty() {
            return Err(SelectError::NoSolution {
                package: package.name.clone(),
                labels: minimum_conflict(
                    &package.constraints,
                    exprs,
                    candidates,
                    include_prerelease,
                ),
            });
        }

        if matches!(preference, Preference::StableFirst) {
            let stable: Vec<usize> = eligible
                .iter()
                .copied()
                .filter(|&index| candidates[index].1.pre.is_empty())
                .collect();
            if !stable.is_empty() {
                eligible = stable;
            }
        }

        let choose_highest = !matches!(preference, Preference::Lowest);
        let mut best = eligible[0];
        for &index in eligible.iter().skip(1) {
            let ordering = compare_versions(&candidates[index].1, &candidates[best].1);
            if (choose_highest && ordering == Ordering::Greater)
                || (!choose_highest && ordering == Ordering::Less)
            {
                best = index;
            }
        }

        selections.push(Selection {
            package: package.name.clone(),
            version: candidates[best].0.clone(),
        });
    }

    Ok(selections)
}