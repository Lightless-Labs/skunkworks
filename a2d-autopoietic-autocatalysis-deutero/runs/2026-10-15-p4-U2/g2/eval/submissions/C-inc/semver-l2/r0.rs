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

fn ascii_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn parse_core_field(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let start = *pos;
    let bytes = s.as_bytes();

    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }

    if *pos == start {
        return Err(if start == bytes.len() || matches!(bytes[start], b'.' | b'-' | b'+') {
            VersionError::MissingCore { at: start }
        } else {
            VersionError::InvalidChar { at: start }
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
    let mut out = Vec::new();
    let mut i = start;

    loop {
        let ident_start = i;
        while i < end && bytes[i] != b'.' {
            let b = bytes[i];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }

        if i == ident_start {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }

        let ident = &s[ident_start..i];
        if prerelease && ascii_digits(ident) && ident.len() > 1 && ident.starts_with('0') {
            return Err(VersionError::LeadingZero { at: ident_start });
        }
        out.push(ident.to_string());

        if i == end {
            break;
        }
        i += 1;
        if i == end {
            return Err(VersionError::EmptyIdentifier { at: i });
        }
    }

    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let mut pos = 0;
    let major = parse_core_field(s, &mut pos)?;

    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        return Err(if pos == s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            VersionError::MissingCore { at: pos }
        } else {
            VersionError::InvalidChar { at: pos }
        });
    }
    pos += 1;

    let minor = parse_core_field(s, &mut pos)?;

    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        return Err(if pos == s.len() || matches!(s.as_bytes()[pos], b'-' | b'+') {
            VersionError::MissingCore { at: pos }
        } else {
            VersionError::InvalidChar { at: pos }
        });
    }
    pos += 1;

    let patch = parse_core_field(s, &mut pos)?;
    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < s.len() {
        match s.as_bytes()[pos] {
            b'-' => {
                let pre_start = pos + 1;
                let build_pos = s[pre_start..].find('+').map(|n| pre_start + n);
                let pre_end = build_pos.unwrap_or(s.len());
                pre = parse_identifiers(s, pre_start, pre_end, true)?;
                if let Some(bp) = build_pos {
                    build = parse_identifiers(s, bp + 1, s.len(), false)?;
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

fn is_numeric_ident(s: &str) -> bool {
    ascii_digits(s)
}

fn cmp_numeric_strings(a: &str, b: &str) -> Ordering {
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
                    let xn = is_numeric_ident(x);
                    let yn = is_numeric_ident(y);
                    let ord = match (xn, yn) {
                        (true, true) => cmp_numeric_strings(x, y),
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
struct Comparator {
    op: Op,
    version: Version,
    admits: Option<(u64, u64, u64)>,
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
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

fn whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn core(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn full_version(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

fn version_with_core(c: (u64, u64, u64), pre: Vec<String>) -> Version {
    Version {
        major: c.0,
        minor: c.1,
        patch: c.2,
        pre,
        build: Vec::new(),
    }
}

fn bump(
    c: (u64, u64, u64),
    field: usize,
    at: usize,
) -> Result<(u64, u64, u64), RangeError> {
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

#[derive(Clone, Debug)]
struct Partial {
    vals: Vec<u64>,
    count: usize,
    full: Option<Version>,
}

fn parse_partial(token: &str, at: usize) -> Result<Partial, RangeError> {
    let has_suffix = token.contains('-') || token.contains('+');
    let fields: Vec<&str> = token.split('.').collect();
    let no_wildcard = !fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));

    if has_suffix || (fields.len() >= 3 && no_wildcard) {
        let full = full_version(token, at)?;
        return Ok(Partial {
            vals: vec![full.major, full.minor, full.patch],
            count: 3,
            full: Some(full),
        });
    }

    if fields.len() > 3 {
        return Err(RangeError::Syntax { at });
    }

    let mut vals = Vec::new();
    let mut saw_wild = false;

    for f in &fields {
        if f.is_empty() {
            return Err(RangeError::Syntax { at });
        }
        if matches!(*f, "*" | "x" | "X") {
            saw_wild = true;
        } else {
            if saw_wild
                || !f.bytes().all(|b| b.is_ascii_digit())
                || (f.len() > 1 && f.starts_with('0'))
            {
                return Err(RangeError::Syntax { at });
            }
            let n = f.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
            vals.push(n);
        }
    }

    let count = vals.len();
    if count != fields.iter().take_while(|f| !matches!(**f, "*" | "x" | "X")).count() {
        return Err(RangeError::Syntax { at });
    }

    Ok(Partial {
        vals,
        count,
        full: None,
    })
}

fn padded(p: &Partial) -> (u64, u64, u64) {
    (
        p.vals.first().copied().unwrap_or(0),
        p.vals.get(1).copied().unwrap_or(0),
        p.vals.get(2).copied().unwrap_or(0),
    )
}

fn add_cmp(arm: &mut Arm, op: Op, version: Version, admits: bool) {
    let admission = if admits && !version.pre.is_empty() {
        Some(core(&version))
    } else {
        None
    };
    arm.comparators.push(Comparator {
        op,
        version,
        admits: admission,
    });
}

fn parse_comparator_token(
    tok: &str,
    start: usize,
    arm: &mut Arm,
) -> Result<(), RangeError> {
    let (op, n) = if tok.starts_with(">=") {
        (Op::Ge, 2)
    } else if tok.starts_with("<=") {
        (Op::Le, 2)
    } else if tok.starts_with('>') {
        (Op::Gt, 1)
    } else if tok.starts_with('<') {
        (Op::Lt, 1)
    } else if tok.starts_with('=') {
        (Op::Eq, 1)
    } else {
        (Op::Eq, 0)
    };

    let operand = &tok[n..];
    let operand_at = start + n;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax { at: start });
    }

    let v = full_version(operand, operand_at)?;
    add_cmp(arm, op, v, true);
    Ok(())
}

fn parse_expanded_token(
    tok: &str,
    start: usize,
    arm: &mut Arm,
) -> Result<(), RangeError> {
    let prefix = tok.as_bytes()[0];
    let off = if prefix == b'^' || prefix == b'~' {
        1
    } else {
        0
    };
    let operand = &tok[off..];
    let operand_at = start + off;
    let p = parse_partial(operand, operand_at)?;

    if off == 0 {
        if let Some(v) = p.full {
            add_cmp(arm, Op::Eq, v, true);
            return Ok(());
        }
        if p.count == 0 {
            return Ok(());
        }

        let lower = version_with_core(padded(&p), Vec::new());
        add_cmp(arm, Op::Ge, lower, false);

        let upper_core = if p.count == 1 {
            bump(padded(&p), 0, operand_at)?
        } else {
            bump(padded(&p), 1, operand_at)?
        };
        add_cmp(
            arm,
            Op::Lt,
            version_with_core(upper_core, vec!["0".into()]),
            false,
        );
        return Ok(());
    }

    if p.count == 0 {
        return Err(RangeError::Syntax { at: start });
    }

    let lower = p
        .full
        .clone()
        .unwrap_or_else(|| version_with_core(padded(&p), Vec::new()));
    add_cmp(arm, Op::Ge, lower, true);

    let upper_field = if prefix == b'^' {
        let c = padded(&p);
        if c.0 > 0 || p.count == 1 {
            0
        } else if c.1 > 0 || p.count == 2 {
            1
        } else {
            2
        }
    } else if p.count == 1 {
        0
    } else {
        1
    };

    let upper_core = bump(padded(&p), upper_field, operand_at)?;
    add_cmp(
        arm,
        Op::Lt,
        version_with_core(upper_core, vec!["0".into()]),
        false,
    );
    Ok(())
}

fn tokenize(s: &str, base: usize) -> Vec<(String, usize)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();

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

fn parse_hyphen(tokens: &[(String, usize)], arm: &mut Arm) -> Result<(), RangeError> {
    let minus = tokens.iter().find(|(t, _)| t == "-").unwrap().1;
    if tokens.len() != 3 || tokens[1].0 != "-" {
        return Err(RangeError::Syntax { at: minus });
    }

    let (lo, lo_at) = (&tokens[0].0, tokens[0].1);
    let (hi, hi_at) = (&tokens[2].0, tokens[2].1);

    let lp = parse_partial(lo, lo_at)?;
    let hp = parse_partial(hi, hi_at)?;
    if lp.count == 0 {
        return Err(RangeError::Syntax { at: lo_at });
    }
    if hp.count == 0 {
        return Err(RangeError::Syntax { at: hi_at });
    }

    let lower = lp
        .full
        .clone()
        .unwrap_or_else(|| version_with_core(padded(&lp), Vec::new()));
    add_cmp(arm, Op::Ge, lower, true);

    if let Some(v) = hp.full {
        add_cmp(arm, Op::Le, v, true);
    } else {
        let upper_core = if hp.count == 1 {
            bump(padded(&hp), 0, hi_at)?
        } else {
            bump(padded(&hp), 1, hi_at)?
        };
        add_cmp(
            arm,
            Op::Lt,
            version_with_core(upper_core, vec!["0".into()]),
            false,
        );
    }

    Ok(())
}

fn parse_arm(s: &str, base: usize) -> Result<Arm, RangeError> {
    let tokens = tokenize(s, base);

    if tokens.iter().any(|(t, _)| t == "-") {
        let mut arm = Arm {
            comparators: Vec::new(),
        };
        parse_hyphen(&tokens, &mut arm)?;
        return Ok(arm);
    }

    let mut arm = Arm {
        comparators: Vec::new(),
    };

    for (tok, start) in &tokens {
        match tok.as_bytes()[0] {
            b'^' | b'~' => parse_expanded_token(tok, *start, &mut arm)?,
            b'<' | b'>' | b'=' => parse_comparator_token(tok, *start, &mut arm)?,
            b'0'..=b'9' | b'*' | b'x' | b'X' => {
                parse_expanded_token(tok, *start, &mut arm)?
            }
            _ => return Err(RangeError::Syntax { at: *start }),
        }
    }

    Ok(arm)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = Vec::new();
    let mut arm_start = 0usize;
    let mut i = 0usize;
    let mut saw_content = false;

    while i < bytes.len() {
        if bytes[i] == b'|' && i + 1 < bytes.len() && bytes[i + 1] == b'|' {
            let sep = i;
            let left = &s[arm_start..sep];

            if tokenize(left, arm_start).is_empty() {
                return Err(RangeError::Empty { at: sep });
            }
            arms.push(parse_arm(left, arm_start)?);

            i += 2;
            while i < bytes.len() && whitespace(bytes[i]) {
                i += 1;
            }
            arm_start = i;
            saw_content = true;

            if i == bytes.len() {
                return Err(RangeError::Empty { at: s.len() });
            }
        } else {
            if !whitespace(bytes[i]) {
                saw_content = true;
            }
            i += 1;
        }
    }

    let tail = &s[arm_start..];
    if tokenize(tail, arm_start).is_empty() {
        if arms.is_empty() && !saw_content {
            return Err(RangeError::Empty { at: 0 });
        }
        return Err(RangeError::Empty { at: s.len() });
    }

    arms.push(parse_arm(tail, arm_start)?);
    Ok(Range { arms })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        let admits = arm.comparators.iter().any(|c| c.admits == Some(core(v)));
        if !v.pre.is_empty() && !admits {
            return false;
        }

        arm.comparators.iter().all(|c| {
            let ord = compare_versions(v, &c.version);
            match c.op {
                Op::Eq => ord == Ordering::Equal,
                Op::Lt => ord == Ordering::Less,
                Op::Le => ord != Ordering::Greater,
                Op::Gt => ord == Ordering::Greater,
                Op::Ge => ord != Ordering::Less,
            }
        })
    })
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

    for (index, s) in candidates.iter().enumerate() {
        match parse_version(s) {
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
            Some(best_i) => {
                let ord = compare_versions(v, &parsed[best_i]);
                if (greatest && ord == Ordering::Greater)
                    || (!greatest && ord == Ordering::Less)
                {
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
    satisfying(candidates, r, true)
}

pub fn min_satisfying(
    candidates: &[String],
    r: &Range,
) -> Result<Option<String>, CandidateError> {
    satisfying(candidates, r, false)
}

fn main() {}