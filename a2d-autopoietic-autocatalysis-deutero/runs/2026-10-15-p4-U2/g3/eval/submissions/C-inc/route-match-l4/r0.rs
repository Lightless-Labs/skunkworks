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

fn valid_method(method: &str) -> bool {
    !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase())
}

fn valid_route_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'))
}

fn valid_capture_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'))
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

fn parse_segment(raw: &[u8], start: usize) -> Result<String, usize> {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let b = raw[i];
        if b == b'%' {
            if i + 2 >= raw.len() {
                return Err(start + i);
            }
            let hi = hex_upper(raw[i + 1]).ok_or(start + i)?;
            let lo = hex_upper(raw[i + 2]).ok_or(start + i)?;
            let decoded = hi * 16 + lo;
            if !(0x20..=0x7e).contains(&decoded)
                || decoded == b'/'
                || is_unreserved(decoded)
            {
                return Err(start + i);
            }
            out.push(decoded);
            i += 3;
        } else {
            if !is_unreserved(b) {
                return Err(start + i);
            }
            out.push(b);
            i += 1;
        }
    }
    Ok(String::from_utf8(out).expect("decoded bytes are ASCII"))
}

fn split_head(input: &str) -> (&str, usize) {
    let cut = input
        .bytes()
        .position(|b| b == b'?' || b == b'#')
        .unwrap_or(input.len());
    (&input[..cut], cut)
}

fn parse_path(input: &str, pattern: bool) -> Result<(Vec<Segment>, bool), Error> {
    let kind_error = |at| {
        if pattern {
            Error::InvalidPattern { at }
        } else {
            Error::InvalidPath { at }
        }
    };

    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(kind_error(0));
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let trailing = bytes.last() == Some(&b'/');
    let end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();

    let mut start = 1;
    let mut pos = 1;
    while pos <= end {
        if pos == end || bytes[pos] == b'/' {
            if pos == start {
                return Err(kind_error(start));
            }
            let raw = &bytes[start..pos];

            if pattern && (raw[0] == b':' || raw[0] == b'*') {
                let name_bytes = &raw[1..];
                let name = std::str::from_utf8(name_bytes).unwrap_or("");
                if !valid_capture_name(name) || !names.insert(name.to_owned()) {
                    return Err(kind_error(start));
                }
                if raw[0] == b'*' && pos != end {
                    return Err(kind_error(start));
                }
                if raw[0] == b'*' {
                    segments.push(Segment::Wildcard(name.to_owned()));
                } else {
                    segments.push(Segment::Param(name.to_owned()));
                }
            } else {
                let decoded = parse_segment(raw, start).map_err(kind_error)?;
                segments.push(Segment::Static(decoded));
            }
            start = pos + 1;
        }
        pos += 1;
    }

    Ok((segments, trailing))
}

fn route_rank(segments: &[Segment]) -> Vec<u8> {
    let mut rank = Vec::new();
    for segment in segments {
        match segment {
            Segment::Static(_) => rank.push(2),
            Segment::Param(_) => rank.push(1),
            Segment::Wildcard(_) => {
                rank.push(0);
                return rank;
            }
        }
    }
    rank.push(3);
    rank
}

fn same_shape(a: &Route, b: &Route, policy: SlashPolicy) -> bool {
    if a.method != b.method || a.segments.len() != b.segments.len() {
        return false;
    }
    if policy == SlashPolicy::Strict && a.trailing != b.trailing {
        return false;
    }
    a.segments.iter().zip(&b.segments).all(|(x, y)| match (x, y) {
        (Segment::Static(a), Segment::Static(b)) => a == b,
        (Segment::Param(_), Segment::Param(_)) => true,
        (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
        _ => false,
    })
}

fn decode_request(input: &str) -> Result<(Vec<String>, bool), Error> {
    let (head, _) = split_head(input);
    let (segments, trailing) = parse_path(head, false)?;
    let decoded = segments
        .into_iter()
        .map(|s| match s {
            Segment::Static(v) => v,
            _ => unreachable!(),
        })
        .collect();
    Ok((decoded, trailing))
}

fn match_route(route: &Route, path: &[String]) -> Option<Vec<(String, String)>> {
    let wildcard_index = route
        .segments
        .iter()
        .position(|s| matches!(s, Segment::Wildcard(_)));

    if let Some(wi) = wildcard_index {
        if path.len() < wi {
            return None;
        }
        for i in 0..wi {
            match &route.segments[i] {
                Segment::Static(s) if s != &path[i] => return None,
                Segment::Static(_) | Segment::Param(_) => {}
                Segment::Wildcard(_) => unreachable!(),
            }
        }
        let mut params = Vec::new();
        for i in 0..wi {
            if let Segment::Param(name) = &route.segments[i] {
                params.push((name.clone(), path[i].clone()));
            }
        }
        if let Segment::Wildcard(name) = &route.segments[wi] {
            params.push((name.clone(), path[wi..].join("/")));
        }
        Some(params)
    } else {
        if path.len() != route.segments.len() {
            return None;
        }
        let mut params = Vec::new();
        for (segment, value) in route.segments.iter().zip(path) {
            match segment {
                Segment::Static(s) if s != value => return None,
                Segment::Static(_) => {}
                Segment::Param(name) => params.push((name.clone(), value.clone())),
                Segment::Wildcard(_) => unreachable!(),
            }
        }
        Some(params)
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
        let (segments, trailing) = parse_path(pattern, true)?;
        if self.routes.iter().any(|route| route.name == name) {
            return Err(Error::DuplicateName);
        }

        let id = self.routes.len();
        let route = Route {
            id,
            name: name.to_owned(),
            method: method.to_owned(),
            rank: route_rank(&segments),
            segments,
            trailing,
        };
        if let Some(existing) = self.routes.iter().find(|r| same_shape(r, &route, self.policy)) {
            return Err(Error::Conflict { with: existing.id });
        }
        self.routes.push(route);
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }

        let (head, _) = split_head(path);
        let (decoded, trailing) = decode_request(head)?;
        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();

        for route in &self.routes {
            if self.policy == SlashPolicy::Strict && route.trailing != trailing {
                continue;
            }
            if let Some(params) = match_route(route, &decoded) {
                candidates.push((route, params));
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let head_has_head = method == "HEAD"
            && candidates.iter().any(|(route, _)| route.method == "HEAD");
        let effective = if method == "HEAD" && !head_has_head {
            "GET"
        } else {
            method
        };

        let mut eligible: Vec<_> = candidates
            .iter()
            .filter(|(route, _)| route.method == effective)
            .collect();

        if eligible.is_empty() {
            let mut allowed = Vec::new();
            for (route, _) in &candidates {
                if !allowed.contains(&route.method) {
                    allowed.push(route.method.clone());
                }
                if route.method == "GET" && !allowed.iter().any(|m| m == "HEAD") {
                    allowed.push("HEAD".to_owned());
                }
            }
            allowed.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        eligible.sort_by(|(a, _), (b, _)| {
            b.rank
                .cmp(&a.rank)
                .then_with(|| a.id.cmp(&b.id))
        });
        let (route, params) = eligible[0];
        let redirect = if self.policy == SlashPolicy::Redirect
            && !decoded.is_empty()
            && route.trailing != trailing
        {
            let mut target = head.to_owned();
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

        let mut supplied = std::collections::HashMap::new();
        for (key, value) in params {
            if supplied.contains_key(*key) {
                return Err(Error::DuplicateParam {
                    name: (*key).to_owned(),
                });
            }
            supplied.insert(*key, *value);
        }

        let mut capture_names = Vec::new();
        for segment in &route.segments {
            match segment {
                Segment::Param(name) | Segment::Wildcard(name) => capture_names.push(name.as_str()),
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
                name: (*key).to_string(),
            });
        }

        for key in &capture_names {
            let value = supplied[key];
            let valid_ascii = value.bytes().all(|b| (0x20..=0x7e).contains(&b));
            let valid = match route
                .segments
                .iter()
                .find(|s| matches!(s, Segment::Param(n) | Segment::Wildcard(n) if n == key))
                .unwrap()
            {
                Segment::Param(_) => {
                    valid_ascii && !value.is_empty() && !value.as_bytes().contains(&b'/')
                }
                Segment::Wildcard(_) => {
                    valid_ascii
                        && (value.is_empty()
                            || value
                                .split('/')
                                .all(|part| !part.is_empty()))
                }
                _ => unreachable!(),
            };
            if !valid {
                return Err(Error::InvalidValue {
                    name: (*key).to_owned(),
                });
            }
        }

        let mut output = String::new();
        output.push('/');
        let mut first = true;
        for segment in &route.segments {
            if !first {
                output.push('/');
            }
            first = false;
            match segment {
                Segment::Static(value) => output.push_str(&encode_component(value.as_bytes())),
                Segment::Param(key) => {
                    output.push_str(&encode_component(supplied[key.as_str()].as_bytes()))
                }
                Segment::Wildcard(key) => {
                    let value = supplied[key.as_str()];
                    if !value.is_empty() {
                        for (i, component) in value.split('/').enumerate() {
                            if i > 0 {
                                output.push('/');
                            }
                            output.push_str(&encode_component(component.as_bytes()));
                        }
                    }
                }
            }
        }
        if route.trailing && !route.segments.is_empty() {
            output.push('/');
        }
        Ok(output)
    }
}

fn encode_component(bytes: &[u8]) -> String {
    let mut output = String::new();
    for &b in bytes {
        if is_unreserved(b) {
            output.push(b as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            output.push('%');
            output.push(HEX[(b >> 4) as usize] as char);
            output.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    output
}

fn main() {}