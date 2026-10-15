#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidPattern { at: usize },
    InvalidPath { at: usize },
    NotFound,
    InvalidMethod,
    MethodNotAllowed { allowed: Vec<String> },
    InvalidName,
    DuplicateName,
    Conflict { with: usize },
    UnknownName,
    DuplicateParam { name: String },
    MissingParam { name: String },
    ExtraParam { name: String },
    InvalidValue { name: String },
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
}

#[derive(Clone, Debug)]
struct Route {
    id: usize,
    name: String,
    method: String,
    segments: Vec<Segment>,
    trailing: bool,
    rank: Vec<u8>,
}

pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
}

fn valid_method(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_uppercase())
}

fn valid_route_name(s: &str) -> bool {
    let mut bytes = s.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

fn valid_capture_name(s: &str) -> bool {
    let mut bytes = s.bytes();
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

fn parse_encoded_segment(bytes: &[u8], start: usize) -> Result<String, usize> {
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(start + i);
            }
            let hi = hex_upper(bytes[i + 1]).ok_or(start + i)?;
            let lo = hex_upper(bytes[i + 2]).ok_or(start + i)?;
            let value = hi * 16 + lo;
            if !(0x20..=0x7e).contains(&value) || value == b'/' || unreserved(value) {
                return Err(start + i);
            }
            decoded.push(value);
            i += 3;
        } else {
            if !b.is_ascii() || !unreserved(b) {
                return Err(start + i);
            }
            decoded.push(b);
            i += 1;
        }
    }
    Ok(String::from_utf8(decoded).unwrap())
}

fn parse_path(input: &str, pattern: bool) -> Result<(Vec<Segment>, bool), usize> {
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(0);
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let trailing = bytes.last() == Some(&b'/');
    let body_end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();
    let mut start = 1;

    while start < body_end {
        let end = bytes[start..body_end]
            .iter()
            .position(|&b| b == b'/')
            .map(|p| start + p)
            .unwrap_or(body_end);

        if end == start {
            return Err(start);
        }

        let raw = &bytes[start..end];
        if pattern && (raw[0] == b':' || raw[0] == b'*') {
            let wildcard = raw[0] == b'*';
            let name = std::str::from_utf8(&raw[1..]).unwrap_or("");
            if !valid_capture_name(name) || !names.insert(name.to_owned()) {
                return Err(start);
            }
            if wildcard && end != body_end {
                return Err(start);
            }
            segments.push(if wildcard {
                Segment::Wildcard(name.to_owned())
            } else {
                Segment::Param(name.to_owned())
            });
        } else {
            let decoded = parse_encoded_segment(raw, start)?;
            segments.push(Segment::Static(decoded));
        }

        start = end + 1;
    }

    Ok((segments, trailing))
}

fn encode_segment(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if unreserved(b) {
            out.push(b as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 15) as usize] as char);
        }
    }
    out
}

impl Router {
    pub fn new(policy: SlashPolicy) -> Self {
        Self {
            policy,
            routes: Vec::new(),
        }
    }

    pub fn add(&mut self, name: &str, method: &str, pattern: &str) -> Result<usize, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }
        if !valid_route_name(name) {
            return Err(Error::InvalidName);
        }

        let (segments, trailing) =
            parse_path(pattern, true).map_err(|at| Error::InvalidPattern { at })?;

        if self.routes.iter().any(|r| r.name == name) {
            return Err(Error::DuplicateName);
        }

        for route in &self.routes {
            if route.method != method
                || route.segments.len() != segments.len()
                || (self.policy == SlashPolicy::Strict && route.trailing != trailing)
            {
                continue;
            }
            let same = route
                .segments
                .iter()
                .zip(&segments)
                .all(|(a, b)| match (a, b) {
                    (Segment::Static(x), Segment::Static(y)) => x == y,
                    (Segment::Param(_), Segment::Param(_)) => true,
                    (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
                    _ => false,
                });
            if same {
                return Err(Error::Conflict { with: route.id });
            }
        }

        let mut rank = Vec::new();
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
        if !segments.iter().any(|s| matches!(s, Segment::Wildcard(_))) {
            rank.push(3);
        }

        let id = self.routes.len();
        self.routes.push(Route {
            id,
            name: name.to_owned(),
            method: method.to_owned(),
            segments,
            trailing,
            rank,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }

        let head_end = path
            .bytes()
            .position(|b| b == b'?' || b == b'#')
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let (request_segments, request_trailing) =
            parse_path(head, false).map_err(|at| Error::InvalidPath { at })?;

        let decoded: Vec<String> = request_segments
            .into_iter()
            .map(|s| match s {
                Segment::Static(v) => v,
                _ => unreachable!(),
            })
            .collect();

        let mut path_candidates = Vec::new();
        for route in &self.routes {
            let mut captures = Vec::new();
            let mut ri = 0;
            let mut pi = 0;
            let mut matched = true;

            while ri < route.segments.len() {
                match &route.segments[ri] {
                    Segment::Wildcard(name) => {
                        captures.push((name.clone(), decoded[pi..].join("/")));
                        pi = decoded.len();
                        break;
                    }
                    Segment::Static(value) => {
                        if decoded.get(pi) != Some(value) {
                            matched = false;
                            break;
                        }
                    }
                    Segment::Param(name) => {
                        if let Some(value) = decoded.get(pi) {
                            captures.push((name.clone(), value.clone()));
                        } else {
                            matched = false;
                            break;
                        }
                    }
                }
                ri += 1;
                pi += 1;
            }

            if !matched || pi != decoded.len() {
                continue;
            }
            if self.policy == SlashPolicy::Strict && route.trailing != request_trailing {
                continue;
            }

            path_candidates.push((route, captures));
        }

        if path_candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible: Vec<_> = if method == "HEAD" {
            if path_candidates.iter().any(|(r, _)| r.method == "HEAD") {
                path_candidates
                    .iter()
                    .filter(|(r, _)| r.method == "HEAD")
                    .collect()
            } else {
                path_candidates
                    .iter()
                    .filter(|(r, _)| r.method == "GET")
                    .collect()
            }
        } else {
            path_candidates
                .iter()
                .filter(|(r, _)| r.method == method)
                .collect()
        };

        if eligible.is_empty() {
            let mut allowed = std::collections::BTreeSet::new();
            for (route, _) in &path_candidates {
                allowed.insert(route.method.clone());
            }
            if allowed.contains("GET") {
                allowed.insert("HEAD".to_owned());
            }
            return Err(Error::MethodNotAllowed {
                allowed: allowed.into_iter().collect(),
            });
        }

        let (winner, params) = eligible
            .into_iter()
            .max_by(|(a, _), (b, _)| a.rank.cmp(&b.rank).then_with(|| b.id.cmp(&a.id)))
            .unwrap();

        let redirect = if self.policy == SlashPolicy::Redirect
            && !decoded.is_empty()
            && winner.trailing != request_trailing
        {
            let mut value = head.to_owned();
            if winner.trailing {
                value.push('/');
            } else if value.ends_with('/') {
                value.pop();
            }
            Some(value)
        } else {
            None
        };

        Ok(Match {
            route: winner.id,
            params: params.clone(),
            redirect,
        })
    }

    pub fn build(&self, name: &str, params: &[(&str, &str)]) -> Result<String, Error> {
        let route = self
            .routes
            .iter()
            .find(|r| r.name == name)
            .ok_or(Error::UnknownName)?;

        let mut supplied = std::collections::HashMap::new();
        for &(key, value) in params {
            if supplied.contains_key(key) {
                return Err(Error::DuplicateParam {
                    name: key.to_owned(),
                });
            }
            supplied.insert(key, value);
        }

        let mut capture_names = Vec::new();
        for segment in &route.segments {
            match segment {
                Segment::Param(n) | Segment::Wildcard(n) => capture_names.push(n.as_str()),
                Segment::Static(_) => {}
            }
        }

        for key in &capture_names {
            if !supplied.contains_key(key) {
                return Err(Error::MissingParam {
                    name: (*key).to_owned(),
                });
            }
        }

        let mut extras: Vec<&str> = supplied
            .keys()
            .copied()
            .filter(|key| !capture_names.contains(key))
            .collect();
        extras.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        if let Some(key) = extras.first() {
            return Err(Error::ExtraParam {
                name: (*key).to_owned(),
            });
        }

        for segment in &route.segments {
            let (key, wildcard) = match segment {
                Segment::Param(n) => (n.as_str(), false),
                Segment::Wildcard(n) => (n.as_str(), true),
                Segment::Static(_) => continue,
            };
            let value = supplied.get(key).copied().unwrap();
            let bytes = value.as_bytes();
            let valid_ascii = bytes.iter().all(|b| (0x20..=0x7e).contains(b));
            let valid_value = if wildcard {
                value.is_empty()
                    || (valid_ascii
                        && !value.starts_with('/')
                        && !value.ends_with('/')
                        && !value.contains("//"))
            } else {
                !value.is_empty() && valid_ascii && !value.contains('/')
            };
            if !valid_value {
                return Err(Error::InvalidValue {
                    name: key.to_owned(),
                });
            }
        }

        let mut out = String::from("/");
        let mut emitted = Vec::new();
        for segment in &route.segments {
            match segment {
                Segment::Static(value) => emitted.push(encode_segment(value)),
                Segment::Param(name) => {
                    emitted.push(encode_segment(supplied.get(name.as_str()).copied().unwrap()))
                }
                Segment::Wildcard(name) => {
                    let value = supplied.get(name.as_str()).copied().unwrap();
                    if !value.is_empty() {
                        emitted.extend(value.split('/').map(encode_segment));
                    }
                }
            }
        }

        out.push_str(&emitted.join("/"));
        if route.trailing && !emitted.is_empty() {
            out.push('/');
        }
        Ok(out)
    }
}