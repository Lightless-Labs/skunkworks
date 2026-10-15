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

#[derive(Clone)]
enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
}

#[derive(Clone)]
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

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn hex_upper(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn decode_segment(s: &str, base: usize, pattern: bool) -> Result<String, Error> {
    let bad = |at| {
        if pattern {
            Error::InvalidPattern { at }
        } else {
            Error::InvalidPath { at }
        }
    };
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(bad(base + i));
            }
            let value = match (hex_upper(bytes[i + 1]), hex_upper(bytes[i + 2])) {
                (Some(h), Some(l)) => (h << 4) | l,
                _ => return Err(bad(base + i)),
            };
            if !(0x20..=0x7e).contains(&value) || value == b'/' || is_unreserved(value) {
                return Err(bad(base + i));
            }
            out.push(value);
            i += 3;
        } else if is_unreserved(b) {
            out.push(b);
            i += 1;
        } else {
            return Err(bad(base + i));
        }
    }
    Ok(String::from_utf8(out).unwrap())
}

fn valid_capture_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn parse_pattern(input: &str) -> Result<(Vec<Segment>, bool), Error> {
    let bad = |at| Error::InvalidPattern { at };
    if !input.starts_with('/') {
        return Err(bad(0));
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = input.as_bytes();
    let trailing = bytes.last() == Some(&b'/');
    let end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut names = Vec::<String>::new();
    let mut start = 1;

    while start < end {
        let mut stop = start;
        while stop < end && bytes[stop] != b'/' {
            stop += 1;
        }
        if stop == start {
            return Err(bad(start));
        }
        let part = &input[start..stop];

        if part.starts_with(':') || part.starts_with('*') {
            let marker = part.as_bytes()[0];
            let name = &part[1..];
            if !valid_capture_name(name) || names.iter().any(|n| n == name) {
                return Err(bad(start));
            }
            if marker == b'*' && stop < end {
                return Err(bad(start));
            }
            names.push(name.to_string());
            if marker == b':' {
                segments.push(Segment::Param(name.to_string()));
            } else {
                segments.push(Segment::Wildcard(name.to_string()));
            }
        } else {
            segments.push(Segment::Static(decode_segment(part, start, true)?));
        }

        if stop < end {
            start = stop + 1;
            if start == end {
                return Err(bad(start));
            }
        } else {
            break;
        }
    }
    Ok((segments, trailing))
}

fn parse_path(input: &str) -> Result<(Vec<String>, bool), Error> {
    let bad = |at| Error::InvalidPath { at };
    if !input.starts_with('/') {
        return Err(bad(0));
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = input.as_bytes();
    let trailing = bytes.last() == Some(&b'/');
    let end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut start = 1;

    while start < end {
        let mut stop = start;
        while stop < end && bytes[stop] != b'/' {
            stop += 1;
        }
        if stop == start {
            return Err(bad(start));
        }
        segments.push(decode_segment(&input[start..stop], start, false)?);

        if stop < end {
            start = stop + 1;
            if start == end {
                return Err(bad(start));
            }
        } else {
            break;
        }
    }
    Ok((segments, trailing))
}

fn valid_method(method: &str) -> bool {
    !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase())
}

fn valid_route_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..].iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-')
        })
}

fn segment_kind(segment: &Segment) -> u8 {
    match segment {
        Segment::Static(_) => 2,
        Segment::Param(_) => 1,
        Segment::Wildcard(_) => 0,
    }
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

        let (segments, trailing) = parse_pattern(pattern)?;
        if self.routes.iter().any(|route| route.name == name) {
            return Err(Error::DuplicateName);
        }

        for route in &self.routes {
            if route.method != method || route.segments.len() != segments.len() {
                continue;
            }
            if self.policy == SlashPolicy::Strict && route.trailing != trailing {
                continue;
            }
            let same_shape = route.segments.iter().zip(&segments).all(|(a, b)| {
                match (a, b) {
                    (Segment::Static(x), Segment::Static(y)) => x == y,
                    (Segment::Param(_), Segment::Param(_))
                    | (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
                    _ => false,
                }
            });
            if same_shape {
                return Err(Error::Conflict { with: route.id });
            }
        }

        let mut rank: Vec<u8> = segments.iter().map(segment_kind).collect();
        if !matches!(segments.last(), Some(Segment::Wildcard(_))) {
            rank.push(3);
        }
        let id = self.routes.len();
        self.routes.push(Route {
            id,
            name: name.to_string(),
            method: method.to_string(),
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
        let (decoded, trailing) = parse_path(head)?;

        let mut candidates = Vec::<(&Route, Vec<(String, String)>)>::new();
        for route in &self.routes {
            if let Some(params) = match_route(route, &decoded) {
                if self.policy == SlashPolicy::Strict && route.trailing != trailing {
                    continue;
                }
                candidates.push((route, params));
            }
        }
        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let effective_method = if method == "HEAD" {
            if candidates.iter().any(|(route, _)| route.method == "HEAD") {
                "HEAD"
            } else {
                "GET"
            }
        } else {
            method
        };

        let eligible: Vec<_> = candidates
            .iter()
            .filter(|(route, _)| route.method == effective_method)
            .collect();

        if eligible.is_empty() {
            let mut allowed: Vec<String> = candidates
                .iter()
                .map(|(route, _)| route.method.clone())
                .collect();
            if allowed.iter().any(|m| m == "GET") {
                allowed.push("HEAD".to_string());
            }
            allowed.sort();
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        let (route, params) = eligible
            .into_iter()
            .max_by(|(a, _), (b, _)| a.rank.cmp(&b.rank).then_with(|| b.id.cmp(&a.id)))
            .unwrap();

        let redirect = if self.policy == SlashPolicy::Redirect
            && head != "/"
            && route.trailing != trailing
        {
            let mut target = head.to_string();
            if route.trailing {
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
            params: params.clone(),
            redirect,
        })
    }

    pub fn build(&self, name: &str, params: &[(&str, &str)]) -> Result<String, Error> {
        let route = self
            .routes
            .iter()
            .find(|route| route.name == name)
            .ok_or(Error::UnknownName)?;

        let mut supplied = Vec::<(&str, &str)>::with_capacity(params.len());
        for &(key, value) in params {
            if supplied.iter().any(|(existing, _)| *existing == key) {
                return Err(Error::DuplicateParam {
                    name: key.to_string(),
                });
            }
            supplied.push((key, value));
        }

        let mut captures = Vec::new();
        for segment in &route.segments {
            match segment {
                Segment::Param(capture) | Segment::Wildcard(capture) => {
                    captures.push(capture.as_str())
                }
                Segment::Static(_) => {}
            }
        }

        for capture in &captures {
            if !supplied.iter().any(|(key, _)| key == capture) {
                return Err(Error::MissingParam {
                    name: (*capture).to_string(),
                });
            }
        }

        let mut extras: Vec<&str> = supplied
            .iter()
            .map(|(key, _)| *key)
            .filter(|key| !captures.contains(key))
            .collect();
        extras.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        if let Some(key) = extras.first() {
            return Err(Error::ExtraParam {
                name: (*key).to_string(),
            });
        }

        for segment in &route.segments {
            let (capture, wildcard) = match segment {
                Segment::Param(capture) => (capture.as_str(), false),
                Segment::Wildcard(capture) => (capture.as_str(), true),
                Segment::Static(_) => continue,
            };
            let value = supplied.iter().find(|(key, _)| *key == capture).unwrap().1;
            let bytes = value.as_bytes();
            let valid_ascii = bytes.iter().all(|b| (0x20..=0x7e).contains(b));
            let valid_shape = if wildcard {
                value.is_empty()
                    || (!value.starts_with('/')
                        && !value.ends_with('/')
                        && !value.split('/').any(str::is_empty))
            } else {
                !value.is_empty() && !value.contains('/')
            };
            if !valid_ascii || !valid_shape {
                return Err(Error::InvalidValue {
                    name: capture.to_string(),
                });
            }
        }

        let mut output = String::from("/");
        let mut emitted = false;
        for segment in &route.segments {
            match segment {
                Segment::Static(text) => {
                    if emitted {
                        output.push('/');
                    }
                    encode_component(text.as_bytes(), &mut output);
                    emitted = true;
                }
                Segment::Param(capture) => {
                    if emitted {
                        output.push('/');
                    }
                    let value = supplied.iter().find(|(key, _)| *key == capture).unwrap().1;
                    encode_component(value.as_bytes(), &mut output);
                    emitted = true;
                }
                Segment::Wildcard(capture) => {
                    let value = supplied.iter().find(|(key, _)| *key == capture).unwrap().1;
                    if !value.is_empty() {
                        if emitted {
                            output.push('/');
                        }
                        for (index, part) in value.split('/').enumerate() {
                            if index > 0 {
                                output.push('/');
                            }
                            encode_component(part.as_bytes(), &mut output);
                        }
                        emitted = true;
                    }
                }
            }
        }

        if !emitted {
            return Ok("/".to_string());
        }
        if route.trailing {
            output.push('/');
        }
        Ok(output)
    }
}

fn match_route(route: &Route, request: &[String]) -> Option<Vec<(String, String)>> {
    let wildcard = route.segments.last().and_then(|segment| match segment {
        Segment::Wildcard(name) => Some(name),
        _ => None,
    });
    let fixed_len = route.segments.len() - usize::from(wildcard.is_some());
    if request.len() < fixed_len || (wildcard.is_none() && request.len() != fixed_len) {
        return None;
    }

    let mut params = Vec::new();
    for (index, segment) in route.segments.iter().take(fixed_len).enumerate() {
        match segment {
            Segment::Static(text) if text != &request[index] => return None,
            Segment::Static(_) => {}
            Segment::Param(name) => params.push((name.clone(), request[index].clone())),
            Segment::Wildcard(_) => return None,
        }
    }
    if let Some(name) = wildcard {
        params.push((name.clone(), request[fixed_len..].join("/")));
    }
    Some(params)
}

fn encode_component(bytes: &[u8], out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &byte in bytes {
        if is_unreserved(byte) {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
}

fn main() {}