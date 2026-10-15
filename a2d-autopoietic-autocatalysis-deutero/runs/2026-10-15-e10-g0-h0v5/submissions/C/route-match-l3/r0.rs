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
    method: String,
    segments: Vec<Segment>,
    trailing: bool,
    rank: Vec<u8>,
}

pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
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
        let (segments, trailing, rank) = parse_pattern(pattern)?;
        let id = self.routes.len();
        self.routes.push(Route {
            method: method.to_owned(),
            segments,
            trailing,
            rank,
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
        let (request_segments, request_trailing) = parse_path(head)?;

        let mut candidates = Vec::new();
        for (i, route) in self.routes.iter().enumerate() {
            if self.policy == SlashPolicy::Strict && route.trailing != request_trailing {
                continue;
            }
            if let Some(params) = match_segments(&route.segments, &request_segments) {
                candidates.push((i, params));
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible: Vec<(usize, Vec<(String, String)>)> = if method == "HEAD" {
            if candidates
                .iter()
                .any(|(i, _)| self.routes[*i].method == "HEAD")
            {
                candidates
                    .iter()
                    .filter(|(i, _)| self.routes[*i].method == "HEAD")
                    .cloned()
                    .collect()
            } else {
                candidates
                    .iter()
                    .filter(|(i, _)| self.routes[*i].method == "GET")
                    .cloned()
                    .collect()
            }
        } else {
            candidates
                .iter()
                .filter(|(i, _)| self.routes[*i].method == method)
                .cloned()
                .collect()
        };

        if eligible.is_empty() {
            let mut allowed = candidates
                .iter()
                .map(|(i, _)| self.routes[*i].method.clone())
                .collect::<Vec<_>>();
            if allowed.iter().any(|m| m == "GET") {
                allowed.push("HEAD".to_owned());
            }
            allowed.sort();
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        let mut best = eligible[0].clone();
        for candidate in eligible.into_iter().skip(1) {
            if self.routes[candidate.0].rank > self.routes[best.0].rank {
                best = candidate;
            }
        }

        let route = &self.routes[best.0];
        let redirect = if self.policy == SlashPolicy::Redirect
            && route.trailing != request_trailing
            && !head.is_empty()
            && head != "/"
        {
            let mut target = head.to_owned();
            if route.trailing {
                target.push('/');
            } else {
                target.pop();
            }
            Some(target)
        } else {
            None
        };

        Ok(Match {
            route: best.0,
            params: best.1,
            redirect,
        })
    }
}

fn validate_method(method: &str) -> Result<(), Error> {
    if !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase()) {
        Ok(())
    } else {
        Err(Error::InvalidMethod)
    }
}

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, bool, Vec<u8>), Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    let (raw_segments, trailing) = split_head(pattern, true, true)?;
    let mut segments = Vec::new();
    let mut names = Vec::<String>::new();
    let mut rank = Vec::new();

    for (index, (raw, start)) in raw_segments.iter().enumerate() {
        if raw.starts_with(':') || raw.starts_with('*') {
            let wildcard = raw.as_bytes()[0] == b'*';
            let name = &raw[1..];
            if !valid_name(name) || names.iter().any(|n| n == name) {
                return Err(Error::InvalidPattern { at: *start });
            }
            if wildcard && index + 1 != raw_segments.len() {
                return Err(Error::InvalidPattern { at: *start });
            }
            names.push(name.to_owned());
            if wildcard {
                segments.push(Segment::Wildcard(name.to_owned()));
                rank.push(0);
            } else {
                segments.push(Segment::Param(name.to_owned()));
                rank.push(1);
            }
        } else {
            let decoded = decode_static(raw, *start, true)?;
            segments.push(Segment::Static(decoded));
            rank.push(2);
        }
    }

    if !segments.iter().any(|s| matches!(s, Segment::Wildcard(_))) {
        rank.push(3);
    }
    Ok((segments, trailing, rank))
}

fn parse_path(path: &str) -> Result<(Vec<String>, bool), Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    let (raw_segments, trailing) = split_head(path, false, false)?;
    let mut segments = Vec::new();
    for (raw, start) in raw_segments {
        segments.push(decode_static(raw, start, false)?);
    }
    Ok((segments, trailing))
}

fn split_head<'a>(
    input: &'a str,
    pattern: bool,
    _unused: bool,
) -> Result<(Vec<(&'a str, usize)>, bool), Error> {
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let trailing = input.ends_with('/');
    let end = if trailing { input.len() - 1 } else { input.len() };
    let mut out = Vec::new();
    let mut start = 1;

    while start < end {
        let next = input[start..end]
            .find('/')
            .map(|i| start + i)
            .unwrap_or(end);
        if next == start {
            return Err(if pattern {
                Error::InvalidPattern { at: start }
            } else {
                Error::InvalidPath { at: start }
            });
        }
        out.push((&input[start..next], start));
        start = next + 1;
    }

    if out.is_empty() && end > 1 {
        return Err(if pattern {
            Error::InvalidPattern { at: 1 }
        } else {
            Error::InvalidPath { at: 1 }
        });
    }

    Ok((out, trailing))
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn decode_static(raw: &str, offset: usize, pattern: bool) -> Result<String, Error> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len()
                || !is_upper_hex(bytes[i + 1])
                || !is_upper_hex(bytes[i + 2])
            {
                return Err(invalid_at(pattern, offset + i));
            }
            let value = (hex(bytes[i + 1]) << 4) | hex(bytes[i + 2]);
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(invalid_at(pattern, offset + i));
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            return Err(invalid_at(pattern, offset + i));
        }
    }

    Ok(String::from_utf8(decoded).unwrap())
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn is_upper_hex(b: u8) -> bool {
    b.is_ascii_digit() || (b'A'..=b'F').contains(&b)
}

fn hex(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

fn invalid_at(pattern: bool, at: usize) -> Error {
    if pattern {
        Error::InvalidPattern { at }
    } else {
        Error::InvalidPath { at }
    }
}

fn match_segments(
    pattern: &[Segment],
    request: &[String],
) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut pi = 0;
    let mut ri = 0;

    while pi < pattern.len() {
        match &pattern[pi] {
            Segment::Wildcard(name) => {
                let value = request[ri..].join("/");
                params.push((name.clone(), value));
                return Some(params);
            }
            Segment::Static(value) => {
                if request.get(ri)? != value {
                    return None;
                }
            }
            Segment::Param(name) => {
                params.push((name.clone(), request.get(ri)?.clone()));
            }
        }
        pi += 1;
        ri += 1;
    }

    if ri == request.len() {
        Some(params)
    } else {
        None
    }
}

fn main() {}