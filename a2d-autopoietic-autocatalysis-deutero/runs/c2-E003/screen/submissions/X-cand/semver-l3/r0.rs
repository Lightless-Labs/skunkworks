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

fn missing_start(b: Option<u8>) -> bool {
    matches!(b, None | Some(b'.' | b'-' | b'+'))
}

fn parse_digits(bytes: &[u8], start: usize) -> Result<(u64, usize), VersionError> {
    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end == start {
        return if missing_start(bytes.get(start).copied()) {
            Err(VersionError::MissingCore { at: start })
        } else {
            Err(VersionError::InvalidChar { at: start })
        };
    }
    if end - start > 1 && bytes[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    let mut value = 0u64;
    for &d in &bytes[start..end] {
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add((d - b'0') as u64))
            .ok_or(VersionError::Overflow { at: start })?;
    }
    Ok((value, end))
}

fn parse_identifiers(
    bytes: &[u8],
    start: usize,
    end: usize,
    is_pre: bool,
) -> Result<Vec<String>, VersionError> {
    let mut out = Vec::new();
    let mut pos = start;
    loop {
        let ident_start = pos;
        while pos < end && bytes[pos] != b'.' {
            let c = bytes[pos];
            if !(c.is_ascii_alphanumeric() || c == b'-') {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
        }
        if pos == ident_start {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }
        if is_pre {
            let id = &bytes[ident_start..pos];
            if id.iter().all(u8::is_ascii_digit) && id.len() > 1 && id[0] == b'0' {
                return Err(VersionError::LeadingZero { at: ident_start });
            }
        }
        out.push(String::from_utf8(bytes[ident_start..pos].to_vec()).unwrap());
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
    let b = s.as_bytes();
    let (major, mut pos) = parse_digits(b, 0)?;
    if pos >= b.len() || b[pos] != b'.' {
        return if missing_start(b.get(pos).copied()) {
            Err(VersionError::MissingCore { at: pos })
        } else {
            Err(VersionError::InvalidChar { at: pos })
        };
    }
    pos += 1;
    let (minor, next) = parse_digits(b, pos)?;
    pos = next;
    if pos >= b.len() || b[pos] != b'.' {
        return if missing_start(b.get(pos).copied()) {
            Err(VersionError::MissingCore { at: pos })
        } else {
            Err(VersionError::InvalidChar { at: pos })
        };
    }
    pos += 1;
    let (patch, next) = parse_digits(b, pos)?;
    pos = next;

    let mut pre = Vec::new();
    let mut build = Vec::new();
    if pos < b.len() {
        match b[pos] {
            b'-' => {
                let pre_start = pos + 1;
                let plus = b[pre_start..]
                    .iter()
                    .position(|&c| c == b'+')
                    .map(|i| pre_start + i);
                let pre_end = plus.unwrap_or(b.len());
                pre = parse_identifiers(b, pre_start, pre_end, true)?;
                if let Some(plus_pos) = plus {
                    build = parse_identifiers(b, plus_pos + 1, b.len(), false)?;
                }
            }
            b'+' => {
                build = parse_identifiers(b, pos + 1, b.len(), false)?;
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

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
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
        let xn = is_numeric(x);
        let yn = is_numeric(y);
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
    core(a)
        .cmp(&core(b))
        .then_with(|| compare_pre(&a.pre, &b.pre))
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

#[derive(Clone)]
struct Token<'a> {
    text: &'a str,
    start: usize,
}

fn tokenize(s: &str) -> Vec<Token<'_>> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && ws(b[i]) {
            i += 1;
        }
        if i == b.len() {
            break;
        }
        let start = i;
        while i < b.len() && !ws(b[i]) {
            i += 1;
        }
        out.push(Token {
            text: &s[start..i],
            start,
        });
    }
    out
}

#[derive(Clone)]
struct Partial {
    fields: Vec<Option<u64>>,
    full: Option<Version>,
    numeric_count: usize,
}

fn parse_partial(s: &str, start: usize) -> Result<Partial, RangeError> {
    let full_rule = s.contains(['-', '+'])
        || (s.split('.').count() >= 3
            && !s.split('.').any(|x| matches!(x, "*" | "x" | "X")));
    if full_rule {
        let v = parse_version(s).map_err(|error| RangeError::Version { at: start, error })?;
        return Ok(Partial {
            fields: vec![Some(v.major), Some(v.minor), Some(v.patch)],
            full: Some(v),
            numeric_count: 3,
        });
    }

    let pieces: Vec<&str> = s.split('.').collect();
    if pieces.is_empty() || pieces.len() > 3 {
        return Err(RangeError::Syntax { at: start });
    }
    let mut fields = Vec::new();
    let mut numeric_count = 0;
    let mut wildcard_seen = false;
    for piece in pieces {
        if piece.is_empty() {
            return Err(RangeError::Syntax { at: start });
        }
        if matches!(piece, "*" | "x" | "X") {
            wildcard_seen = true;
            fields.push(None);
        } else {
            if wildcard_seen
                || !piece.bytes().all(|b| b.is_ascii_digit())
                || (piece.len() > 1 && piece.starts_with('0'))
            {
                return Err(RangeError::Syntax { at: start });
            }
            let n = piece.parse::<u64>().map_err(|_| RangeError::Syntax { at: start })?;
            numeric_count += 1;
            fields.push(Some(n));
        }
    }
    while fields.len() < 3 {
        fields.push(None);
    }
    Ok(Partial {
        fields,
        full: None,
        numeric_count,
    })
}

fn version_from_core(c: (u64, u64, u64), pre: Vec<String>) -> Version {
    Version {
        major: c.0,
        minor: c.1,
        patch: c.2,
        pre,
        build: Vec::new(),
    }
}

fn add_cmp(arm: &mut Arm, op: Op, version: Version, anchor: bool) {
    if anchor && !version.pre.is_empty() {
        arm.anchors.push(core(&version));
    }
    arm.comparators.push(Comparator { op, version });
}

fn cmp_matches(v: &Version, c: &Comparator) -> bool {
    let ord = compare_versions(v, &c.version);
    match c.op {
        Op::Eq => ord == Ordering::Equal,
        Op::Lt => ord == Ordering::Less,
        Op::Le => ord != Ordering::Greater,
        Op::Gt => ord == Ordering::Greater,
        Op::Ge => ord != Ordering::Less,
    }
}

fn next_core(c: (u64, u64, u64), field: usize, at: usize) -> Result<(u64, u64, u64), RangeError> {
    let mut x = c;
    let target = match field {
        0 => &mut x.0,
        1 => &mut x.1,
        _ => &mut x.2,
    };
    *target = target.checked_add(1).ok_or(RangeError::Syntax { at })?;
    match field {
        0 => {
            x.1 = 0;
            x.2 = 0;
        }
        1 => x.2 = 0,
        _ => {}
    }
    Ok(x)
}

fn expand_bare(arm: &mut Arm, tok: &Token<'_>) -> Result<(), RangeError> {
    let p = parse_partial(tok.text, tok.start)?;
    if let Some(v) = p.full {
        add_cmp(arm, Op::Eq, v, true);
        return Ok(());
    }
    let k = p.numeric_count;
    if k == 0 {
        return Ok(());
    }
    let c = (
        p.fields[0].unwrap_or(0),
        p.fields[1].unwrap_or(0),
        p.fields[2].unwrap_or(0),
    );
    let lower = version_from_core(c, Vec::new());
    add_cmp(arm, Op::Ge, lower, false);
    if k < 3 {
        let upper_core = next_core(c, if k == 1 { 0 } else { 1 }, tok.start)?;
        add_cmp(arm, Op::Lt, version_from_core(upper_core, vec!["0".into()]), false);
    }
    Ok(())
}

fn expand_caret_tilde(
    arm: &mut Arm,
    tok: &Token<'_>,
    caret: bool,
) -> Result<(), RangeError> {
    let operand = &tok.text[1..];
    let start = tok.start + 1;
    let p = parse_partial(operand, start)?;
    if p.numeric_count == 0 {
        return Err(RangeError::Syntax { at: tok.start });
    }
    let c = if let Some(v) = &p.full {
        core(v)
    } else {
        (
            p.fields[0].unwrap_or(0),
            p.fields[1].unwrap_or(0),
            p.fields[2].unwrap_or(0),
        )
    };
    let lower = p.full.clone().unwrap_or_else(|| version_from_core(c, Vec::new()));
    add_cmp(arm, Op::Ge, lower, true);

    let k = p.numeric_count;
    let field = if caret {
        if c.0 > 0 || k == 1 {
            0
        } else if c.1 > 0 || k == 2 {
            1
        } else {
            2
        }
    } else if k == 1 {
        0
    } else {
        1
    };
    let upper = next_core(c, field, start)?;
    add_cmp(arm, Op::Lt, version_from_core(upper, vec!["0".into()]), false);
    Ok(())
}

fn parse_comparator(tok: &Token<'_>, arm: &mut Arm) -> Result<(), RangeError> {
    let (op, len) = if tok.text.starts_with(">=") {
        (Op::Ge, 2)
    } else if tok.text.starts_with("<=") {
        (Op::Le, 2)
    } else {
        match tok.text.as_bytes().first().copied() {
            Some(b'=') => (Op::Eq, 1),
            Some(b'<') => (Op::Lt, 1),
            Some(b'>') => (Op::Gt, 1),
            _ => unreachable!(),
        }
    };
    let operand = &tok.text[len..];
    let at = tok.start + len;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: tok.start });
    }
    let v = parse_version(operand).map_err(|error| RangeError::Version { at, error })?;
    add_cmp(arm, op, v, true);
    Ok(())
}

fn parse_hyphen_arm(tokens: &[Token<'_>], arm: &mut Arm) -> Result<(), RangeError> {
    let minus = tokens.iter().find(|t| t.text == "-").unwrap();
    if tokens.len() != 3 || tokens[1].text != "-" {
        return Err(RangeError::Syntax { at: minus.start });
    }
    let lower = parse_partial(tokens[0].text, tokens[0].start)?;
    let upper = parse_partial(tokens[2].text, tokens[2].start)?;
    if lower.numeric_count == 0 {
        return Err(RangeError::Syntax { at: tokens[0].start });
    }
    if upper.numeric_count == 0 {
        return Err(RangeError::Syntax { at: tokens[2].start });
    }
    let lc = if let Some(v) = &lower.full {
        core(v)
    } else {
        (
            lower.fields[0].unwrap_or(0),
            lower.fields[1].unwrap_or(0),
            lower.fields[2].unwrap_or(0),
        )
    };
    let lv = lower.full.clone().unwrap_or_else(|| version_from_core(lc, Vec::new()));
    add_cmp(arm, Op::Ge, lv, true);

    if let Some(v) = upper.full {
        add_cmp(arm, Op::Le, v, true);
    } else {
        let uc = (
            upper.fields[0].unwrap_or(0),
            upper.fields[1].unwrap_or(0),
            upper.fields[2].unwrap_or(0),
        );
        if upper.numeric_count == 3 {
            add_cmp(arm, Op::Le, version_from_core(uc, Vec::new()), false);
        } else {
            let next = next_core(
                uc,
                if upper.numeric_count == 1 { 0 } else { 1 },
                tokens[2].start,
            )?;
            add_cmp(arm, Op::Lt, version_from_core(next, vec!["0".into()]), false);
        }
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

    for tok in tokens {
        let first = tok.text.as_bytes().first().copied();
        match first {
            Some(b'<' | b'>' | b'=') => parse_comparator(tok, &mut arm)?,
            Some(b'^') => expand_caret_tilde(&mut arm, tok, true)?,
            Some(b'~') => expand_caret_tilde(&mut arm, tok, false)?,
            Some(b'0'..=b'9' | b'*' | b'x' | b'X') => expand_bare(&mut arm, tok)?,
            _ => return Err(RangeError::Syntax { at: tok.start }),
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
    let mut start = 0;
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].text == "||" {
            if i == start {
                return Err(RangeError::Empty { at: tokens[i].start });
            }
            arms.push(parse_arm(&tokens[start..i])?);
            start = i + 1;
        }
        i += 1;
    }
    if start == tokens.len() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(&tokens[start..])?);
    Ok(Range { arms })
}

fn arm_matches(v: &Version, arm: &Arm, include_pre: bool) -> bool {
    if !include_pre && !v.pre.is_empty() && !arm.anchors.contains(&core(v)) {
        return false;
    }
    arm.comparators.iter().all(|c| cmp_matches(v, c))
}

pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|a| arm_matches(v, a, include_prerelease))
}

pub fn matches(v: &Version, r: &Range) -> bool {
    matches_with(v, r, false)
}

#[derive(Clone)]
struct Bound {
    version: Version,
    inclusive: bool,
}

fn arm_bounds(arm: &Arm) -> (Option<Bound>, Option<Bound>, Vec<(u64, u64, u64)>) {
    let mut lower: Option<Bound> = None;
    let mut upper: Option<Bound> = None;
    for c in &arm.comparators {
        match c.op {
            Op::Eq => {
                let b = Bound {
                    version: c.version.clone(),
                    inclusive: true,
                };
                lower = Some(b.clone());
                upper = Some(b);
            }
            Op::Gt | Op::Ge => {
                let b = Bound {
                    version: c.version.clone(),
                    inclusive: matches!(c.op, Op::Ge),
                };
                lower = match lower.take() {
                    None => Some(b),
                    Some(old) => {
                        let o = compare_versions(&b.version, &old.version);
                        if o == Ordering::Greater
                            || (o == Ordering::Equal && !b.inclusive && old.inclusive)
                        {
                            Some(b)
                        } else {
                            Some(old)
                        }
                    }
                };
            }
            Op::Lt | Op::Le => {
                let b = Bound {
                    version: c.version.clone(),
                    inclusive: matches!(c.op, Op::Le),
                };
                upper = match upper.take() {
                    None => Some(b),
                    Some(old) => {
                        let o = compare_versions(&b.version, &old.version);
                        if o == Ordering::Less
                            || (o == Ordering::Equal && !b.inclusive && old.inclusive)
                        {
                            Some(b)
                        } else {
                            Some(old)
                        }
                    }
                };
            }
        }
    }
    if let Some(b) = &mut lower {
        b.version.build.clear();
    }
    if let Some(b) = &mut upper {
        b.version.build.clear();
    }
    (lower, upper, arm.anchors.clone())
}

fn stable_witness(lower: &Option<Bound>, upper: &Option<Bound>) -> bool {
    let mut candidate = if let Some(l) = lower {
        let mut v = l.version.clone();
        if !l.inclusive {
            if !v.pre.is_empty() {
                v.pre.clear();
            } else if let Some(c) = successor_core(core(&v)) {
                v = version_from_core(c, Vec::new());
            } else {
                return false;
            }
        }
        if !v.pre.is_empty() {
            v.pre.clear();
        }
        v
    } else {
        version_from_core((0, 0, 0), Vec::new())
    };
    if compare_versions(&candidate, &version_from_core((0, 0, 0), Vec::new())) == Ordering::Less {
        candidate = version_from_core((0, 0, 0), Vec::new());
    }
    bound_accepts(&candidate, lower, upper)
}

fn successor_core(c: (u64, u64, u64)) -> Option<(u64, u64, u64)> {
    if c.2 < u64::MAX {
        Some((c.0, c.1, c.2 + 1))
    } else if c.1 < u64::MAX {
        Some((c.0, c.1 + 1, 0))
    } else if c.0 < u64::MAX {
        Some((c.0 + 1, 0, 0))
    } else {
        None
    }
}

fn bound_accepts(v: &Version, lower: &Option<Bound>, upper: &Option<Bound>) -> bool {
    if let Some(l) = lower {
        let o = compare_versions(v, &l.version);
        if o == Ordering::Less || (o == Ordering::Equal && !l.inclusive) {
            return false;
        }
    }
    if let Some(u) = upper {
        let o = compare_versions(v, &u.version);
        if o == Ordering::Greater || (o == Ordering::Equal && !u.inclusive) {
            return false;
        }
    }
    true
}

fn arm_has_witness(arm: &Arm) -> bool {
    let (lo, hi, anchors) = arm_bounds(arm);
    if stable_witness(&lo, &hi) {
        return true;
    }
    anchors.into_iter().any(|c| {
        let v = version_from_core(c, vec!["0".into()]);
        bound_accepts(&v, &lo, &hi)
    })
}

fn arms_intersect(a: &Arm, b: &Arm, include_pre: bool) -> bool {
    let (al, au, aa) = arm_bounds(a);
    let (bl, bu, ba) = arm_bounds(b);
    let lower = stronger_lower(al, bl);
    let upper = stronger_upper(au, bu);
    if include_pre {
        return arm_has_witness(&Arm {
            comparators: a.comparators.iter().chain(&b.comparators).cloned().collect(),
            anchors: Vec::new(),
        });
    }
    if stable_witness(&lower, &upper) {
        return true;
    }
    aa.into_iter()
        .filter(|x| ba.contains(x))
        .any(|c| bound_accepts(&version_from_core(c, vec!["0".into()]), &lower, &upper))
}

fn stronger_lower(a: Option<Bound>, b: Option<Bound>) -> Option<Bound> {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(x), Some(y)) => {
            let o = compare_versions(&x.version, &y.version);
            if o == Ordering::Greater || (o == Ordering::Equal && !x.inclusive && y.inclusive) {
                Some(x)
            } else {
                Some(y)
            }
        }
    }
}

fn stronger_upper(a: Option<Bound>, b: Option<Bound>) -> Option<Bound> {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(x), Some(y)) => {
            let o = compare_versions(&x.version, &y.version);
            if o == Ordering::Less || (o == Ordering::Equal && !x.inclusive && y.inclusive) {
                Some(x)
            } else {
                Some(y)
            }
        }
    }
}

pub fn intersects(a: &Range, b: &Range, include_prerelease: bool) -> bool {
    a.arms
        .iter()
        .any(|x| b.arms.iter().any(|y| arms_intersect(x, y, include_prerelease)))
}

fn fmt_version(v: &Version) -> String {
    let mut s = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        s.push('-');
        s.push_str(&v.pre.join("."));
    }
    s
}

pub fn canonical(r: &Range) -> String {
    let mut strings = Vec::new();
    for arm in &r.arms {
        if !arm_has_witness(arm) {
            continue;
        }
        let (lo, hi, mut anchors) = arm_bounds(arm);
        anchors.sort_unstable();
        anchors.dedup();
        let lower = lo.map_or_else(
            || "*".to_string(),
            |b| format!("{}{}", if b.inclusive { ">=" } else { ">" }, fmt_version(&b.version)),
        );
        let upper = hi.map_or_else(
            || "*".to_string(),
            |b| format!("{}{}", if b.inclusive { "<=" } else { "<" }, fmt_version(&b.version)),
        );
        let anchor_text = if anchors.is_empty() {
            "-".to_string()
        } else {
            anchors
                .iter()
                .map(|c| format!("{}.{}.{}", c.0, c.1, c.2))
                .collect::<Vec<_>>()
                .join(",")
        };
        strings.push(format!("[{lower};{upper};pre={anchor_text}]"));
    }
    strings.sort();
    strings.dedup();
    if strings.is_empty() {
        "EMPTY".into()
    } else {
        strings.join(" || ")
    }
}

fn extrema(candidates: &[String], r: &Range, want_max: bool) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, s) in candidates.iter().enumerate() {
        parsed.push(parse_version(s).map_err(|error| CandidateError { index, error })?);
    }
    let mut best: Option<usize> = None;
    for i in 0..parsed.len() {
        if !matches(&parsed[i], r) {
            continue;
        }
        if let Some(old) = best {
            let ord = compare_versions(&parsed[i], &parsed[old]);
            if (want_max && ord == Ordering::Greater) || (!want_max && ord == Ordering::Less) {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }
    Ok(best.map(|i| candidates[i].clone()))
}

pub fn max_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, true)
}

pub fn min_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, false)
}