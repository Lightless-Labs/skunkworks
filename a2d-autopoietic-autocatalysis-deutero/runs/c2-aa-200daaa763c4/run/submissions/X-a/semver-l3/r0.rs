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

fn is_ident_byte(b: u8) -> bool {
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
    let mut pos = start;

    loop {
        let ident_start = pos;
        while pos < end && bytes[pos] != b'.' {
            if !is_ident_byte(bytes[pos]) {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
        }

        if pos == ident_start {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }

        let ident = &s[ident_start..pos];
        if prerelease
            && ident.len() > 1
            && ident.bytes().all(|b| b.is_ascii_digit())
            && ident.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: ident_start });
        }
        out.push(ident.to_string());

        if pos == end {
            break;
        }
        pos += 1;
        if pos == end {
            return Err(VersionError::EmptyIdentifier { at: pos });
        }
    }

    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let bytes = s.as_bytes();
    let mut pos = 0usize;
    let mut fields = [0u64; 3];

    for i in 0..3 {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }

        if pos == start {
            if pos == bytes.len() || matches!(bytes[pos], b'.' | b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            return Err(VersionError::InvalidChar { at: pos });
        }

        let digits = &s[start..pos];
        if digits.len() > 1 && digits.as_bytes()[0] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }
        fields[i] = digits
            .parse::<u64>()
            .map_err(|_| VersionError::Overflow { at: start })?;

        if i < 2 {
            if pos == bytes.len() || matches!(bytes[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            if bytes[pos] != b'.' {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
        }
    }

    if pos < bytes.len() && !matches!(bytes[pos], b'-' | b'+') {
        return Err(VersionError::InvalidChar { at: pos });
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] == b'-' {
        let pre_start = pos + 1;
        let plus = s[pre_start..].find('+').map(|i| pre_start + i);
        let pre_end = plus.unwrap_or(bytes.len());
        pre = parse_identifiers(s, pre_start, pre_end, true)?;
        pos = pre_end;
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
        major: fields[0],
        minor: fields[1],
        patch: fields[2],
        pre,
        build,
    })
}

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn is_numeric_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn compare_numeric_id(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    let c = core(a).cmp(&core(b));
    if c != Ordering::Equal {
        return c;
    }

    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (x, y) in a.pre.iter().zip(&b.pre) {
                let xn = is_numeric_id(x);
                let yn = is_numeric_id(y);
                let ord = match (xn, yn) {
                    (true, true) => compare_numeric_id(x, y),
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
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

fn whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn split_tokens(s: &str, base: usize) -> Vec<(String, usize)> {
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
        out.push((s[start..i].to_string(), base + start));
    }
    out
}

fn add_lower(arm: &mut Arm, v: Version, inclusive: bool) {
    let replace = match &arm.lower {
        None => true,
        Some(old) => match compare_versions(&v, &old.version) {
            Ordering::Greater => true,
            Ordering::Less => false,
            Ordering::Equal => !inclusive && old.inclusive,
        },
    };
    if replace {
        arm.lower = Some(Bound { version: v, inclusive });
    }
}

fn add_upper(arm: &mut Arm, v: Version, inclusive: bool) {
    let replace = match &arm.upper {
        None => true,
        Some(old) => match compare_versions(&v, &old.version) {
            Ordering::Less => true,
            Ordering::Greater => false,
            Ordering::Equal => !inclusive && old.inclusive,
        },
    };
    if replace {
        arm.upper = Some(Bound { version: v, inclusive });
    }
}

fn zero_version(major: u64, minor: u64, patch: u64) -> Version {
    Version {
        major,
        minor,
        patch,
        pre: Vec::new(),
        build: Vec::new(),
    }
}

fn prerelease_zero(mut v: Version) -> Version {
    v.pre = vec!["0".to_string()];
    v.build.clear();
    v
}

#[derive(Clone, Debug)]
struct ParsedPartial {
    lower: Version,
    numeric_fields: usize,
    full: bool,
}

fn parse_partial(s: &str, at: usize) -> Result<ParsedPartial, RangeError> {
    if s.contains('-') || s.contains('+') {
        let v = parse_version(s).map_err(|error| RangeError::Version { at, error })?;
        return Ok(ParsedPartial {
            lower: v,
            numeric_fields: 3,
            full: true,
        });
    }

    let parts: Vec<&str> = s.split('.').collect();
    let full = parts.len() >= 3
        && !parts
            .iter()
            .any(|p| matches!(*p, "*" | "x" | "X"));

    if full {
        let v = parse_version(s).map_err(|error| RangeError::Version { at, error })?;
        return Ok(ParsedPartial {
            lower: v,
            numeric_fields: 3,
            full: true,
        });
    }

    if parts.is_empty() || parts.len() > 3 || parts.iter().any(|p| p.is_empty()) {
        return Err(RangeError::Syntax { at });
    }

    let mut values = [0u64; 3];
    let mut numeric_count = 0usize;
    let mut wildcard_seen = false;

    for (i, part) in parts.iter().enumerate() {
        if matches!(*part, "*" | "x" | "X") {
            wildcard_seen = true;
            continue;
        }
        if wildcard_seen || !part.bytes().all(|b| b.is_ascii_digit()) {
            return Err(RangeError::Syntax { at });
        }
        if part.len() > 1 && part.as_bytes()[0] == b'0' {
            return Err(RangeError::Syntax { at });
        }
        let n = part.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
        values[i] = n;
        numeric_count += 1;
    }

    Ok(ParsedPartial {
        lower: zero_version(values[0], values[1], values[2]),
        numeric_fields: numeric_count,
        full: false,
    })
}

fn increment_upper(
    p: &ParsedPartial,
    kind: char,
    at: usize,
) -> Result<Option<Version>, RangeError> {
    let v = &p.lower;
    let k = p.numeric_fields;
    if k == 0 {
        return Ok(None);
    }

    let (major, minor, patch) = (v.major, v.minor, v.patch);
    let mut out = match kind {
        'b' => {
            if major > 0 || k == 1 {
                let n = major.checked_add(1).ok_or(RangeError::Syntax { at })?;
                zero_version(n, 0, 0)
            } else if minor > 0 || k == 2 {
                let n = minor.checked_add(1).ok_or(RangeError::Syntax { at })?;
                zero_version(major, n, 0)
            } else {
                let n = patch.checked_add(1).ok_or(RangeError::Syntax { at })?;
                zero_version(major, minor, n)
            }
        }
        't' => {
            if k == 1 {
                let n = major.checked_add(1).ok_or(RangeError::Syntax { at })?;
                zero_version(n, 0, 0)
            } else {
                let n = minor.checked_add(1).ok_or(RangeError::Syntax { at })?;
                zero_version(major, n, 0)
            }
        }
        'p' => {
            if k == 1 {
                let n = major.checked_add(1).ok_or(RangeError::Syntax { at })?;
                zero_version(n, 0, 0)
            } else if k == 2 {
                let n = minor.checked_add(1).ok_or(RangeError::Syntax { at })?;
                zero_version(major, n, 0)
            } else {
                let n = patch.checked_add(1).ok_or(RangeError::Syntax { at })?;
                zero_version(major, minor, n)
            }
        }
        _ => return Ok(None),
    };
    out = prerelease_zero(out);
    Ok(Some(out))
}

fn add_parsed_partial(arm: &mut Arm, p: ParsedPartial, at: usize) -> Result<(), RangeError> {
    if p.full {
        if !p.lower.pre.is_empty() {
            arm.anchors.push(core(&p.lower));
        }
        add_lower(arm, p.lower.clone(), true);
        add_upper(arm, p.lower, true);
        return Ok(());
    }

    let k = p.numeric_fields;
    if k == 0 {
        return Ok(());
    }

    add_lower(arm, p.lower.clone(), true);
    if k < 3 {
        let upper = increment_upper(&p, 'p', at)?.ok_or(RangeError::Syntax { at })?;
        add_upper(arm, upper, false);
    }
    Ok(())
}

fn parse_full_comparator(
    token: &str,
    at: usize,
    arm: &mut Arm,
) -> Result<(), RangeError> {
    let (op, prefix_len) = if token.starts_with("<=") {
        (Op::Le, 2)
    } else if token.starts_with(">=") {
        (Op::Ge, 2)
    } else if token.starts_with('<') {
        (Op::Lt, 1)
    } else if token.starts_with('>') {
        (Op::Gt, 1)
    } else if token.starts_with('=') {
        (Op::Eq, 1)
    } else {
        (Op::Eq, 0)
    };

    let operand = &token[prefix_len..];
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at });
    }

    let v = parse_version(operand).map_err(|error| RangeError::Version {
        at: at + prefix_len,
        error,
    })?;

    if !v.pre.is_empty() {
        arm.anchors.push(core(&v));
    }

    match op {
        Op::Eq => {
            add_lower(arm, v.clone(), true);
            add_upper(arm, v, true);
        }
        Op::Lt => add_upper(arm, v, false),
        Op::Le => add_upper(arm, v, true),
        Op::Gt => add_lower(arm, v, false),
        Op::Ge => add_lower(arm, v, true),
    }
    Ok(())
}

fn parse_expanded_token(token: &str, at: usize, arm: &mut Arm) -> Result<(), RangeError> {
    if token.starts_with('^') || token.starts_with('~') {
        let kind = token.as_bytes()[0] as char;
        let operand = &token[1..];
        let p = parse_partial(operand, at + 1)?;
        if p.numeric_fields == 0 {
            return Err(RangeError::Syntax { at });
        }
        if !p.lower.pre.is_empty() {
            arm.anchors.push(core(&p.lower));
        }
        add_lower(arm, p.lower.clone(), true);
        let upper = increment_upper(&p, if kind == '^' { 'b' } else { 't' }, at + 1)?
            .ok_or(RangeError::Syntax { at })?;
        add_upper(arm, upper, false);
        return Ok(());
    }

    let p = parse_partial(token, at)?;
    add_parsed_partial(arm, p, at)
}

fn parse_hyphen_arm(
    tokens: &[(String, usize)],
    first_minus: usize,
    arm: &mut Arm,
) -> Result<(), RangeError> {
    let minus_indices: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| if t.0 == "-" { Some(i) } else { None })
        .collect();

    if minus_indices.len() != 1 || tokens.len() != 3 || minus_indices[0] != 1 {
        return Err(RangeError::Syntax { at: first_minus });
    }

    let (lower_s, lower_at) = (&tokens[0].0, tokens[0].1);
    let (upper_s, upper_at) = (&tokens[2].0, tokens[2].1);
    let lower = parse_partial(lower_s, lower_at)?;
    let upper = parse_partial(upper_s, upper_at)?;

    if lower.numeric_fields == 0 {
        return Err(RangeError::Syntax { at: lower_at });
    }
    if upper.numeric_fields == 0 {
        return Err(RangeError::Syntax { at: upper_at });
    }

    if !lower.lower.pre.is_empty() {
        arm.anchors.push(core(&lower.lower));
    }
    add_lower(arm, lower.lower.clone(), true);

    if upper.full {
        if !upper.lower.pre.is_empty() {
            arm.anchors.push(core(&upper.lower));
        }
        add_upper(arm, upper.lower, true);
    } else {
        let k = upper.numeric_fields;
        if k == 3 {
            add_upper(arm, upper.lower, true);
        } else {
            let bound = increment_upper(&upper, 'p', upper_at)?
                .ok_or(RangeError::Syntax { at: upper_at })?;
            add_upper(arm, bound, false);
        }
    }
    Ok(())
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = Vec::new();
    let mut arm_start = 0usize;
    let mut pos = 0usize;
    let mut had_separator = false;

    while pos < bytes.len() {
        if pos + 1 < bytes.len() && bytes[pos] == b'|' && bytes[pos + 1] == b'|' {
            let part = &s[arm_start..pos];
            let tokens = split_tokens(part, arm_start);
            if tokens.is_empty() {
                return Err(RangeError::Empty { at: pos });
            }

            let mut arm = Arm {
                lower: None,
                upper: None,
                anchors: Vec::new(),
            };
            if let Some((_, minus_at)) = tokens.iter().find(|t| t.0 == "-") {
                parse_hyphen_arm(&tokens, *minus_at, &mut arm)?;
            } else {
                for (token, at) in &tokens {
                    if !(token.as_bytes()[0].is_ascii_digit()
                        || matches!(token.as_str(), "*" | "x" | "X")
                        || matches!(token.as_bytes()[0], b'^' | b'~' | b'<' | b'>' | b'='))
                    {
                        return Err(RangeError::Syntax { at: *at });
                    }

                    if matches!(token.as_bytes()[0], b'<' | b'>' | b'=') {
                        parse_full_comparator(token, *at, &mut arm)?;
                    } else {
                        parse_expanded_token(token, *at, &mut arm)?;
                    }
                }
            }
            arms.push(arm);
            had_separator = true;
            pos += 2;
            while pos < bytes.len() && whitespace(bytes[pos]) {
                pos += 1;
            }
            arm_start = pos;
            continue;
        }
        pos += 1;
    }

    let part = &s[arm_start..];
    let tokens = split_tokens(part, arm_start);
    if tokens.is_empty() {
        if had_separator {
            return Err(RangeError::Empty { at: s.len() });
        }
        return Err(RangeError::Empty { at: 0 });
    }

    let mut arm = Arm {
        lower: None,
        upper: None,
        anchors: Vec::new(),
    };
    if let Some((_, minus_at)) = tokens.iter().find(|t| t.0 == "-") {
        parse_hyphen_arm(&tokens, *minus_at, &mut arm)?;
    } else {
        for (token, at) in &tokens {
            if !(token.as_bytes()[0].is_ascii_digit()
                || matches!(token.as_str(), "*" | "x" | "X")
                || matches!(token.as_bytes()[0], b'^' | b'~' | b'<' | b'>' | b'='))
            {
                return Err(RangeError::Syntax { at: *at });
            }

            if matches!(token.as_bytes()[0], b'<' | b'>' | b'=') {
                parse_full_comparator(token, *at, &mut arm)?;
            } else {
                parse_expanded_token(token, *at, &mut arm)?;
            }
        }
    }
    arms.push(arm);

    Ok(Range { arms })
}

fn satisfies_bounds(v: &Version, arm: &Arm) -> bool {
    if let Some(b) = &arm.lower {
        match compare_versions(v, &b.version) {
            Ordering::Less => return false,
            Ordering::Equal if !b.inclusive => return false,
            _ => {}
        }
    }
    if let Some(b) = &arm.upper {
        match compare_versions(v, &b.version) {
            Ordering::Greater => return false,
            Ordering::Equal if !b.inclusive => return false,
            _ => {}
        }
    }
    true
}

fn admitted(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    include_prerelease || v.pre.is_empty() || arm.anchors.contains(&core(v))
}

fn matches_arm(v: &Version, arm: &Arm, include_prerelease: bool) -> bool {
    satisfies_bounds(v, arm) && admitted(v, arm, include_prerelease)
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|arm| matches_arm(v, arm, include_prerelease))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

fn stable_candidate_for_arm(arm: &Arm) -> Option<Version> {
    let mut candidates = Vec::new();

    if let Some(lower) = &arm.lower {
        if lower.version.pre.is_empty() {
            let mut v = lower.version.clone();
            if !lower.inclusive {
                let (a, b, c) = core(&v);
                v = if c < u64::MAX {
                    zero_version(a, b, c + 1)
                } else if b < u64::MAX {
                    zero_version(a, b + 1, 0)
                } else if a < u64::MAX {
                    zero_version(a + 1, 0, 0)
                } else {
                    return None;
                };
            }
            candidates.push(v);
        } else {
            candidates.push(zero_version(
                lower.version.major,
                lower.version.minor,
                lower.version.patch,
            ));
        }
    } else {
        candidates.push(zero_version(0, 0, 0));
    }

    for candidate in candidates {
        if satisfies_bounds(&candidate, arm) {
            return Some(candidate);
        }
    }
    None
}

fn anchor_witness(arm: &Arm, anchor: (u64, u64, u64)) -> Option<Version> {
    let v = Version {
        major: anchor.0,
        minor: anchor.1,
        patch: anchor.2,
        pre: vec!["0".to_string()],
        build: Vec::new(),
    };
    if satisfies_bounds(&v, arm) {
        Some(v)
    } else {
        None
    }
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    for aa in &a.arms {
        for bb in &b.arms {
            let mut combined = Arm {
                lower: None,
                upper: None,
                anchors: Vec::new(),
            };
            if let Some(x) = &aa.lower {
                add_lower(&mut combined, x.version.clone(), x.inclusive);
            }
            if let Some(x) = &bb.lower {
                add_lower(&mut combined, x.version.clone(), x.inclusive);
            }
            if let Some(x) = &aa.upper {
                add_upper(&mut combined, x.version.clone(), x.inclusive);
            }
            if let Some(x) = &bb.upper {
                add_upper(&mut combined, x.version.clone(), x.inclusive);
            }

            if stable_candidate_for_arm(&combined).is_some() {
                return true;
            }
            if include_prerelease {
                if let Some(lower) = &combined.lower {
                    if !lower.version.pre.is_empty() && satisfies_bounds(&lower.version, &combined) {
                        return true;
                    }
                    let v = Version {
                        major: lower.version.major,
                        minor: lower.version.minor,
                        patch: lower.version.patch,
                        pre: vec!["0".to_string()],
                        build: Vec::new(),
                    };
                    if satisfies_bounds(&v, &combined) {
                        return true;
                    }
                } else if satisfies_bounds(
                    &Version {
                        major: 0,
                        minor: 0,
                        patch: 0,
                        pre: vec!["0".to_string()],
                        build: Vec::new(),
                    },
                    &combined,
                ) {
                    return true;
                }
            } else {
                for anchor in aa.anchors.iter().filter(|x| bb.anchors.contains(x)) {
                    if anchor_witness(&combined, *anchor).is_some() {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn version_display(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

fn arm_string(arm: &Arm) -> String {
    let lower = match &arm.lower {
        None => "*".to_string(),
        Some(b) => format!(
            "{}{}",
            if b.inclusive { ">=" } else { ">" },
            version_display(&b.version)
        ),
    };
    let upper = match &arm.upper {
        None => "*".to_string(),
        Some(b) => format!(
            "{}{}",
            if b.inclusive { "<=" } else { "<" },
            version_display(&b.version)
        ),
    };
    let mut anchors = arm.anchors.clone();
    anchors.sort();
    anchors.dedup();
    let anchor_string = if anchors.is_empty() {
        "-".to_string()
    } else {
        anchors
            .iter()
            .map(|(a, b, c)| format!("{a}.{b}.{c}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    format!("[{lower};{upper};pre={anchor_string}]")
}

pub fn canonical(r: &Range) -> String {
    let mut items = Vec::new();
    for arm in &r.arms {
        let mut viable = stable_candidate_for_arm(arm).is_some();
        if !viable {
            for anchor in &arm.anchors {
                if anchor_witness(arm, *anchor).is_some() {
                    viable = true;
                    break;
                }
            }
        }
        if viable {
            items.push(arm_string(arm));
        }
    }
    items.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    items.dedup();
    if items.is_empty() {
        "EMPTY".to_string()
    } else {
        items.join(" || ")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn satisfying_extreme(
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
    for i in 0..parsed.len() {
        if !matches(&parsed[i], r) {
            continue;
        }
        best = match best {
            None => Some(i),
            Some(j) => {
                let ord = compare_versions(&parsed[i], &parsed[j]);
                let better = if greatest {
                    ord == Ordering::Greater
                } else {
                    ord == Ordering::Less
                };
                if better { Some(i) } else { Some(j) }
            }
        };
    }

    Ok(best.map(|i| candidates[i].clone()))
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