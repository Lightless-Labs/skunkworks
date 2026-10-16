#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidPattern { at: usize },
    InvalidPath { at: usize },
    NotFound,
    InvalidMethod,
    MethodNotAllowed { allowed: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub route: usize,
    pub params: Vec<(String, String)>,
    pub redirect: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashPolicy {
    Strict,
    Ignore,
    Redirect,
}

#[derive(Clone)]
enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
}

struct Route {
    id: usize,
    method: String,
    segments: Vec<Segment>,
    trailing_slash: bool,
    rank: Vec<u8>,
    has_wildcard: bool,
}

pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
}

struct ParsedPath {
    segments: Vec<String>,
    trailing_slash: bool,
}

impl Router {
    pub fn new(policy: SlashPolicy) -> Self {
        Self {
            policy,
            routes: Vec::new(),
        }
    }

    pub fn add(&mut self, method: &str, pattern: &str) -> Result<usize, Error> {
        validate_method(method)?;
        let (segments, trailing_slash, rank, has_wildcard) = parse_pattern(pattern)?;
        let id = self.routes.len();
        self.routes.push(Route {
            id,
            method: method.to_owned(),
            segments,
            trailing_slash,
            rank,
            has_wildcard,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        validate_method(method)?;
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let parsed = parse_path(head, false)?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();
        for route in &self.routes {
            if let Some(params) = match_route(route, &parsed, self.policy) {
                candidates.push((route, params));
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let mut eligible: Vec<(&Route, Vec<(String, String)>)> = if method == "HEAD" {
            let has_head = candidates.iter().any(|(r, _)| r.method == "HEAD");
            let selected = if has_head { "HEAD" } else { "GET" };
            candidates
                .iter()
                .filter(|(r, _)| r.method == selected)
                .map(|(r, p)| (*r, p.clone()))
                .collect()
        } else {
            candidates
                .iter()
                .filter(|(r, _)| r.method == method)
                .map(|(r, p)| (*r, p.clone()))
                .collect()
        };

        if eligible.is_empty() {
            let mut allowed: Vec<String> = candidates
                .iter()
                .map(|(r, _)| r.method.clone())
                .collect();
            if allowed.iter().any(|m| m == "GET") {
                allowed.push("HEAD".to_owned());
            }
            allowed.sort();
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        eligible.sort_by(|(a, _), (b, _)| {
            b.rank
                .cmp(&a.rank)
                .then_with(|| a.id.cmp(&b.id))
        });
        let (route, params) = eligible.remove(0);

        let redirect = if self.policy == SlashPolicy::Redirect
            && route.trailing_slash != parsed.trailing_slash
            && !route.segments.is_empty()
        {
            let mut target = head.to_owned();
            if route.trailing_slash {
                target.push('/');
            } else if target.ends_with('/') {
                target.pop();
            }
            Some(target)
        } else {
            None
        };

        Ok(Match {
            route: route.id,
            params,
            redirect,
        })
    }
}

fn validate_method(method: &str) -> Result<(), Error> {
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(Error::InvalidMethod);
    }
    Ok(())
}

fn parse_pattern(
    pattern: &str,
) -> Result<(Vec<Segment>, bool, Vec<u8>, bool), Error> {
    let parsed = parse_path(pattern, true)?;
    let mut segments = Vec::with_capacity(parsed.segments.len());
    let mut names = std::collections::HashSet::new();
    let mut rank = Vec::with_capacity(parsed.segments.len() + 1);
    let mut has_wildcard = false;

    for (i, raw) in raw_segments(pattern, parsed.trailing_slash).into_iter().enumerate() {
        let start = raw.0;
        let text = raw.1;
        if let Some(name) = text.strip_prefix(':') {
            if !valid_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: start });
            }
            segments.push(Segment::Param(name.to_owned()));
            rank.push(1);
        } else if let Some(name) = text.strip_prefix('*') {
            if !valid_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: start });
            }
            if i + 1 != raw_segments(pattern, parsed.trailing_slash).len() {
                return Err(Error::InvalidPattern { at: start });
            }
            segments.push(Segment::Wildcard(name.to_owned()));
            rank.push(0);
            has_wildcard = true;
        } else {
            let decoded = decode_segment(text, start, true)?;
            segments.push(Segment::Static(decoded));
            rank.push(2);
        }
    }

    if !has_wildcard {
        rank.push(3);
    }
    Ok((segments, parsed.trailing_slash, rank, has_wildcard))
}

fn valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'))
}

fn parse_path(input: &str, pattern: bool) -> Result<ParsedPath, Error> {
    let invalid = |at| {
        if pattern {
            Error::InvalidPattern { at }
        } else {
            Error::InvalidPath { at }
        }
    };
    if !input.starts_with('/') {
        return Err(invalid(0));
    }
    if input == "/" {
        return Ok(ParsedPath {
            segments: Vec::new(),
            trailing_slash: false,
        });
    }

    let trailing_slash = input.ends_with('/');
    let end = if trailing_slash { input.len() - 1 } else { input.len() };
    let mut segments = Vec::new();
    let mut start = 1;
    for (i, byte) in input.bytes().enumerate().take(end).skip(1) {
        if byte == b'/' {
            if i == start {
                return Err(invalid(start));
            }
            let raw = &input[start..i];
            if pattern && (raw.starts_with(':') || raw.starts_with('*')) {
                // Capture grammar and wildcard ordering are validated by parse_pattern.
            } else {
                decode_segment(raw, start, pattern)?;
            }
            segments.push(raw.to_owned());
            start = i + 1;
        }
    }
    if start == end {
        return Err(invalid(start));
    }
    let raw = &input[start..end];
    if pattern && (raw.starts_with(':') || raw.starts_with('*')) {
        // Capture grammar and wildcard ordering are validated by parse_pattern.
    } else {
        decode_segment(raw, start, pattern)?;
    }
    segments.push(raw.to_owned());

    Ok(ParsedPath {
        segments: segments
            .into_iter()
            .map(|s| s)
            .collect(),
        trailing_slash,
    })
}

fn raw_segments(input: &str, trailing_slash: bool) -> Vec<(usize, &str)> {
    if input == "/" {
        return Vec::new();
    }
    let end = if trailing_slash { input.len() - 1 } else { input.len() };
    let mut result = Vec::new();
    let mut start = 1;
    for (i, b) in input.bytes().enumerate().take(end).skip(1) {
        if b == b'/' {
            result.push((start, &input[start..i]));
            start = i + 1;
        }
    }
    result.push((start, &input[start..end]));
    result
}

fn decode_segment(raw: &str, base: usize, pattern: bool) -> Result<String, Error> {
    let fail = |at| {
        if pattern {
            Error::InvalidPattern { at }
        } else {
            Error::InvalidPath { at }
        }
    };
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(fail(base + i));
            }
            let hi = hex_upper(bytes[i + 1]).ok_or_else(|| fail(base + i))?;
            let lo = hex_upper(bytes[i + 2]).ok_or_else(|| fail(base + i))?;
            let decoded = hi * 16 + lo;
            if !(0x20..=0x7e).contains(&decoded)
                || decoded == b'/'
                || is_unreserved(decoded)
            {
                return Err(fail(base + i));
            }
            out.push(decoded);
            i += 3;
        } else if is_unreserved(b) {
            out.push(b);
            i += 1;
        } else {
            return Err(fail(base + i));
        }
    }
    Ok(String::from_utf8(out).expect("decoded bytes are ASCII"))
}

fn hex_upper(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn match_route(
    route: &Route,
    path: &ParsedPath,
    policy: SlashPolicy,
) -> Option<Vec<(String, String)>> {
    if policy == SlashPolicy::Strict && route.trailing_slash != path.trailing_slash {
        return None;
    }
    let wildcard_index = route
        .segments
        .iter()
        .position(|s| matches!(s, Segment::Wildcard(_)));
    let fixed_len = wildcard_index.unwrap_or(route.segments.len());
    if path.segments.len() < fixed_len {
        return None;
    }
    if wildcard_index.is_none() && path.segments.len() != fixed_len {
        return None;
    }

    let mut params = Vec::new();
    for i in 0..fixed_len {
        let raw = &path.segments[i];
        let decoded = decode_segment(raw, 0, false).ok()?;
        match &route.segments[i] {
            Segment::Static(s) if *s == decoded => {}
            Segment::Param(name) => params.push((name.clone(), decoded)),
            _ => return None,
        }
    }
    if let Some(index) = wildcard_index {
        if let Segment::Wildcard(name) = &route.segments[index] {
            let mut rest = Vec::new();
            for raw in &path.segments[index..] {
                rest.push(decode_segment(raw, 0, false).ok()?);
            }
            params.push((name.clone(), rest.join("/")));
        }
    }
    Some(params)
}