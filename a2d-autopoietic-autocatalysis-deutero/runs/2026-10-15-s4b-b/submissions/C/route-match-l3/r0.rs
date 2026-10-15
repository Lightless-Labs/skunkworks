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
    trailing: bool,
    rank: Vec<u8>,
    wildcard: bool,
}

pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
}

fn valid_method(method: &str) -> bool {
    !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase())
}

fn name_valid(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn hex_upper(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn decode_static(raw: &str, base: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let hi = hex_upper(bytes[i + 1]).ok_or(base + i)?;
            let lo = hex_upper(bytes[i + 2]).ok_or(base + i)?;
            let decoded = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&decoded) || decoded == b'/' || unreserved(decoded) {
                return Err(base + i);
            }
            out.push(decoded);
            i += 3;
        } else {
            if !unreserved(b) {
                return Err(base + i);
            }
            out.push(b);
            i += 1;
        }
    }
    Ok(String::from_utf8(out).unwrap())
}

fn parse_path<'a>(input: &'a str, pattern: bool) -> Result<(Vec<Segment>, bool), usize> {
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(0);
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let trailing = bytes.last() == Some(&b'/');
    let end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();
    let mut start = 1;

    while start < end {
        let mut stop = start;
        while stop < end && bytes[stop] != b'/' {
            stop += 1;
        }
        if stop == start {
            return Err(start);
        }
        let raw = &input[start..stop];

        if pattern && (raw.starts_with(':') || raw.starts_with('*')) {
            let marker = raw.as_bytes()[0];
            let name = &raw[1..];
            if !name_valid(name) || !names.insert(name.to_string()) {
                return Err(start);
            }
            if marker == b'*' && stop != end {
                return Err(start);
            }
            segments.push(if marker == b':' {
                Segment::Param(name.to_string())
            } else {
                Segment::Wildcard(name.to_string())
            });
        } else {
            let decoded = decode_static(raw, start)?;
            segments.push(Segment::Static(decoded));
        }

        if stop == end {
            break;
        }
        start = stop + 1;
        if start == end {
            break;
        }
    }

    Ok((segments, trailing))
}

fn request_head(path: &str) -> &str {
    let cut = path
        .bytes()
        .position(|b| b == b'?' || b == b'#')
        .unwrap_or(path.len());
    &path[..cut]
}

impl Router {
    pub fn new(policy: SlashPolicy) -> Self {
        Self {
            policy,
            routes: Vec::new(),
        }
    }

    pub fn add(&mut self, method: &str, pattern: &str) -> Result<usize, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }
        let (segments, trailing) =
            parse_path(pattern, true).map_err(|at| Error::InvalidPattern { at })?;
        let wildcard = segments.iter().any(|s| matches!(s, Segment::Wildcard(_)));
        let mut rank = Vec::with_capacity(segments.len() + 1);
        for segment in &segments {
            rank.push(match segment {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
                Segment::Wildcard(_) => 0,
            });
            if matches!(segment, Segment::Wildcard(_)) {
                break;
            }
        }
        if !wildcard {
            rank.push(3);
        }
        let id = self.routes.len();
        self.routes.push(Route {
            id,
            method: method.to_string(),
            segments,
            trailing,
            rank,
            wildcard,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }
        let head = request_head(path);
        let (request_segments, request_trailing) =
            parse_path(head, false).map_err(|at| Error::InvalidPath { at })?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();
        for route in &self.routes {
            if self.policy == SlashPolicy::Strict && route.trailing != request_trailing {
                continue;
            }
            let mut params = Vec::new();
            let mut matched = true;
            let mut ri = 0;
            for segment in &route.segments {
                match segment {
                    Segment::Static(value) => {
                        if request_segments.get(ri).map(|s| match s {
                            Segment::Static(v) => v == value,
                            _ => false,
                        }) != Some(true)
                        {
                            matched = false;
                            break;
                        }
                        ri += 1;
                    }
                    Segment::Param(name) => {
                        match request_segments.get(ri) {
                            Some(Segment::Static(value)) => {
                                params.push((name.clone(), value.clone()));
                                ri += 1;
                            }
                            _ => {
                                matched = false;
                                break;
                            }
                        }
                    }
                    Segment::Wildcard(name) => {
                        let rest = request_segments[ri..]
                            .iter()
                            .map(|s| match s {
                                Segment::Static(v) => v.as_str(),
                                _ => "",
                            })
                            .collect::<Vec<_>>()
                            .join("/");
                        params.push((name.clone(), rest));
                        ri = request_segments.len();
                        break;
                    }
                }
            }
            if matched && ri == request_segments.len() {
                candidates.push((route, params));
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible: Vec<_> = if method == "HEAD" {
            if candidates.iter().any(|(r, _)| r.method == "HEAD") {
                candidates
                    .iter()
                    .filter(|(r, _)| r.method == "HEAD")
                    .collect()
            } else {
                candidates.iter().filter(|(r, _)| r.method == "GET").collect()
            }
        } else {
            candidates
                .iter()
                .filter(|(r, _)| r.method == method)
                .collect()
        };

        if eligible.is_empty() {
            let mut allowed: Vec<String> = candidates
                .iter()
                .map(|(r, _)| r.method.clone())
                .collect();
            if allowed.iter().any(|m| m == "GET") {
                allowed.push("HEAD".to_string());
            }
            allowed.sort();
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        let winner = eligible
            .into_iter()
            .max_by(|(a, _), (b, _)| {
                a.rank
                    .cmp(&b.rank)
                    .then_with(|| b.id.cmp(&a.id))
            })
            .unwrap();

        let redirect = if self.policy == SlashPolicy::Redirect
            && !head.is_empty()
            && head != "/"
            && winner.0.trailing != request_trailing
        {
            let mut target = head.to_string();
            if winner.0.trailing {
                target.push('/');
            } else if target.ends_with('/') {
                target.pop();
            }
            Some(target)
        } else {
            None
        };

        Ok(Match {
            route: winner.0.id,
            params: winner.1.clone(),
            redirect,
        })
    }
}

fn main() {}