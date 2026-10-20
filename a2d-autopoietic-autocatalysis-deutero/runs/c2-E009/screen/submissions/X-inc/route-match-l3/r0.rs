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

#[derive(Debug, Clone)]
enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
}

#[derive(Debug, Clone)]
struct Route {
    id: usize,
    method: String,
    segments: Vec<Segment>,
    trailing_slash: bool,
    rank: Vec<u8>,
}

#[derive(Debug, Clone)]
struct ParsedPath {
    segments: Vec<String>,
    trailing_slash: bool,
    head: String,
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
        let (segments, trailing_slash) =
            parse_pattern(pattern).map_err(|at| Error::InvalidPattern { at })?;

        let mut rank = Vec::with_capacity(segments.len() + 1);
        let mut has_wildcard = false;
        for segment in &segments {
            match segment {
                Segment::Static(_) => rank.push(2),
                Segment::Param(_) => rank.push(1),
                Segment::Wildcard(_) => {
                    rank.push(0);
                    has_wildcard = true;
                }
            }
        }
        if !has_wildcard {
            rank.push(3);
        }

        let id = self.routes.len();
        self.routes.push(Route {
            id,
            method: method.to_owned(),
            segments,
            trailing_slash,
            rank,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        validate_method(method)?;
        let parsed = parse_request_path(path).map_err(|at| Error::InvalidPath { at })?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();
        for route in &self.routes {
            if let Some(params) = match_route(route, &parsed.segments) {
                if self.policy != SlashPolicy::Strict
                    || route.trailing_slash == parsed.trailing_slash
                {
                    candidates.push((route, params));
                }
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible_method = if method == "HEAD"
            && candidates.iter().any(|(route, _)| route.method == "HEAD")
        {
            "HEAD"
        } else if method == "HEAD" {
            "GET"
        } else {
            method
        };

        let mut eligible: Vec<(&Route, Vec<(String, String)>)> = candidates
            .iter()
            .filter(|(route, _)| route.method == eligible_method)
            .map(|(route, params)| (*route, params.clone()))
            .collect();

        if eligible.is_empty() {
            let mut allowed: Vec<String> = candidates
                .iter()
                .map(|(route, _)| route.method.clone())
                .collect();
            if allowed.iter().any(|m| m == "GET") {
                allowed.push("HEAD".to_owned());
            }
            allowed.sort();
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        eligible.sort_by(|(a, _), (b, _)| {
            b.rank.cmp(&a.rank).then_with(|| a.id.cmp(&b.id))
        });
        let (winner, params) = eligible.remove(0);

        let redirect = if self.policy == SlashPolicy::Redirect
            && winner.trailing_slash != parsed.trailing_slash
            && !parsed.segments.is_empty()
        {
            let mut head = parsed.head.clone();
            if winner.trailing_slash {
                head.push('/');
            } else if head.ends_with('/') {
                head.pop();
            }
            Some(head)
        } else {
            None
        };

        Ok(Match {
            route: winner.id,
            params,
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

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, bool), usize> {
    if !pattern.starts_with('/') {
        return Err(0);
    }
    if pattern == "/" {
        return Ok((Vec::new(), false));
    }

    let trailing_slash = pattern.ends_with('/');
    let end = if trailing_slash {
        pattern.len() - 1
    } else {
        pattern.len()
    };

    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();
    let bytes = pattern.as_bytes();
    let mut start = 1;

    while start < end {
        let mut stop = start;
        while stop < end && bytes[stop] != b'/' {
            stop += 1;
        }
        if stop == start {
            return Err(start);
        }

        let raw = &pattern[start..stop];
        if raw.as_bytes()[0] == b':' || raw.as_bytes()[0] == b'*' {
            let marker = raw.as_bytes()[0];
            let name = &raw[1..];
            if !valid_capture_name(name) || !names.insert(name.to_owned()) {
                return Err(start);
            }
            if marker == b'*' && stop < end {
                return Err(start);
            }
            if marker == b':' {
                segments.push(Segment::Param(name.to_owned()));
            } else {
                segments.push(Segment::Wildcard(name.to_owned()));
            }
        } else {
            segments.push(Segment::Static(decode_segment(raw, start)?));
        }

        start = stop + 1;
    }

    Ok((segments, trailing_slash))
}

fn parse_request_path(path: &str) -> Result<ParsedPath, usize> {
    let head_end = path
        .char_indices()
        .find(|(_, c)| *c == '?' || *c == '#')
        .map(|(i, _)| i)
        .unwrap_or(path.len());
    let head = &path[..head_end];

    if !head.starts_with('/') {
        return Err(0);
    }
    if head == "/" {
        return Ok(ParsedPath {
            segments: Vec::new(),
            trailing_slash: false,
            head: head.to_owned(),
        });
    }

    let trailing_slash = head.ends_with('/');
    let end = if trailing_slash {
        head.len() - 1
    } else {
        head.len()
    };
    let bytes = head.as_bytes();
    let mut segments = Vec::new();
    let mut start = 1;

    while start < end {
        let mut stop = start;
        while stop < end && bytes[stop] != b'/' {
            stop += 1;
        }
        if stop == start {
            return Err(start);
        }
        segments.push(decode_segment(&head[start..stop], start)?);
        start = stop + 1;
    }

    Ok(ParsedPath {
        segments,
        trailing_slash,
        head: head.to_owned(),
    })
}

fn valid_capture_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn decode_segment(raw: &str, offset: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(offset + i);
            }
            let hi = upper_hex(bytes[i + 1]).ok_or(offset + i)?;
            let lo = upper_hex(bytes[i + 2]).ok_or(offset + i)?;
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(offset + i);
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            return Err(offset + i);
        }
    }

    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
}

fn upper_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn match_route(route: &Route, path_segments: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut i = 0;

    for segment in &route.segments {
        match segment {
            Segment::Static(expected) => {
                let actual = path_segments.get(i)?;
                if actual != expected {
                    return None;
                }
                i += 1;
            }
            Segment::Param(name) => {
                let actual = path_segments.get(i)?;
                params.push((name.clone(), actual.clone()));
                i += 1;
            }
            Segment::Wildcard(name) => {
                let value = path_segments[i..].join("/");
                params.push((name.clone(), value));
                i = path_segments.len();
                break;
            }
        }
    }

    if i == path_segments.len() {
        Some(params)
    } else {
        None
    }
}