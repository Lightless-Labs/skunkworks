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

#[derive(Debug)]
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
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

fn valid_capture_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn parse_segment(raw: &[u8], start: usize) -> Result<String, usize> {
    let mut decoded = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let b = raw[i];
        if b == b'%' {
            if i + 2 >= raw.len() {
                return Err(start + i);
            }
            let hi = hex_value(raw[i + 1]).ok_or(start + i)?;
            let lo = hex_value(raw[i + 2]).ok_or(start + i)?;
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value) || value == b'/' || is_unreserved(value) {
                return Err(start + i);
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            return Err(start + i);
        }
    }
    Ok(String::from_utf8(decoded).expect("decoded segment is ASCII"))
}

fn parse_path(input: &str, pattern: bool) -> Result<(Vec<Segment>, bool), usize> {
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(0);
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let mut parts: Vec<(usize, &[u8])> = Vec::new();
    let mut seg_start = 1;
    let mut i = 1;
    while i <= bytes.len() {
        if i == bytes.len() || bytes[i] == b'/' {
            if i == seg_start {
                if i == bytes.len() && seg_start == bytes.len() {
                    break;
                }
                return Err(seg_start);
            }
            parts.push((seg_start, &bytes[seg_start..i]));
            seg_start = i + 1;
        }
        i += 1;
    }

    let trailing = input.ends_with('/');
    if trailing {
        if parts.is_empty() {
            return Ok((Vec::new(), false));
        }
        if parts.last().is_some_and(|(_, p)| p.is_empty()) {
            parts.pop();
        }
    }
    if parts.is_empty() {
        return Ok((Vec::new(), false));
    }

    let mut segments = Vec::with_capacity(parts.len());
    let mut names = std::collections::HashSet::new();
    for (idx, (start, raw)) in parts.iter().enumerate() {
        if pattern && raw.first() == Some(&b':') {
            let name = std::str::from_utf8(&raw[1..]).unwrap_or("");
            if !valid_capture_name(name) || !names.insert(name.to_string()) {
                return Err(*start);
            }
            segments.push(Segment::Param(name.to_string()));
        } else if pattern && raw.first() == Some(&b'*') {
            let name = std::str::from_utf8(&raw[1..]).unwrap_or("");
            if !valid_capture_name(name) || !names.insert(name.to_string()) {
                return Err(*start);
            }
            if idx + 1 != parts.len() {
                return Err(*start);
            }
            segments.push(Segment::Wildcard(name.to_string()));
        } else {
            let decoded = parse_segment(raw, *start)?;
            segments.push(Segment::Static(decoded));
        }
    }
    Ok((segments, trailing))
}

fn route_rank(segments: &[Segment]) -> Vec<u8> {
    let mut rank = Vec::with_capacity(segments.len() + 1);
    let has_wildcard = segments.iter().any(|s| matches!(s, Segment::Wildcard(_)));
    for segment in segments {
        rank.push(match segment {
            Segment::Static(_) => 2,
            Segment::Param(_) => 1,
            Segment::Wildcard(_) => 0,
        });
    }
    if !has_wildcard {
        rank.push(3);
    }
    rank
}

fn same_shape(a: &Route, b_segments: &[Segment], b_trailing: bool, policy: SlashPolicy) -> bool {
    if a.segments.len() != b_segments.len() {
        return false;
    }
    if policy == SlashPolicy::Strict && a.trailing != b_trailing {
        return false;
    }
    a.segments.iter().zip(b_segments).all(|(left, right)| match (left, right) {
        (Segment::Static(x), Segment::Static(y)) => x == y,
        (Segment::Param(_), Segment::Param(_)) => true,
        (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
        _ => false,
    })
}

fn match_route(route: &Route, path: &[Segment]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut pi = 0;
    for segment in &route.segments {
        match segment {
            Segment::Static(expected) => {
                match path.get(pi) {
                    Some(Segment::Static(actual)) if expected == actual => {}
                    _ => return None,
                }
                pi += 1;
            }
            Segment::Param(name) => {
                match path.get(pi) {
                    Some(Segment::Static(value)) => params.push((name.clone(), value.clone())),
                    _ => return None,
                }
                pi += 1;
            }
            Segment::Wildcard(name) => {
                let tail = path[pi..]
                    .iter()
                    .map(|s| match s {
                        Segment::Static(v) => v.as_str(),
                        _ => "",
                    })
                    .collect::<Vec<_>>()
                    .join("/");
                if path[pi..].iter().any(|s| !matches!(s, Segment::Static(_))) {
                    return None;
                }
                params.push((name.clone(), tail));
                pi = path.len();
            }
        }
    }
    if pi == path.len() {
        Some(params)
    } else {
        None
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
        let (segments, trailing) =
            parse_path(pattern, true).map_err(|at| Error::InvalidPattern { at })?;
        if self.routes.iter().any(|route| route.name == name) {
            return Err(Error::DuplicateName);
        }

        if let Some(route) = self.routes.iter().filter(|r| r.method == method).find(|r| {
            same_shape(r, &segments, trailing, self.policy)
        }) {
            return Err(Error::Conflict { with: route.id });
        }

        let id = self.routes.len();
        let rank = route_rank(&segments);
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
            .char_indices()
            .find(|(_, c)| *c == '?' || *c == '#')
            .map(|(i, _)| i)
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let (parsed, trailing) =
            parse_path(head, false).map_err(|at| Error::InvalidPath { at })?;

        let candidates: Vec<(&Route, Vec<(String, String)>)> = self
            .routes
            .iter()
            .filter(|route| {
                self.policy != SlashPolicy::Strict || route.trailing == trailing
            })
            .filter_map(|route| match_route(route, &parsed).map(|params| (route, params)))
            .collect();

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

        let mut eligible: Vec<_> = candidates
            .iter()
            .filter(|(route, _)| route.method == eligible_method)
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

        eligible.sort_by(|(a, _), (b, _)| {
            b.rank.cmp(&a.rank).then_with(|| a.id.cmp(&b.id))
        });
        let (winner, params) = eligible[0];
        let redirect = if self.policy == SlashPolicy::Redirect
            && !head.is_empty()
            && head != "/"
            && winner.trailing != trailing
        {
            let mut target = head.to_string();
            if winner.trailing {
                target.push('/');
            } else if target.ends_with('/') {
                target.pop();
            }
            Some(target)
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
        let route = self.routes.iter().find(|route| route.name == name).ok_or(Error::UnknownName)?;

        let mut supplied = std::collections::HashMap::new();
        for (key, value) in params {
            if supplied.contains_key(*key) {
                return Err(Error::DuplicateParam {
                    name: (*key).to_string(),
                });
            }
            supplied.insert(*key, *value);
        }

        let mut required = Vec::new();
        for segment in &route.segments {
            match segment {
                Segment::Param(key) | Segment::Wildcard(key) => required.push(key.as_str()),
                Segment::Static(_) => {}
            }
        }
        for key in &required {
            if !supplied.contains_key(key) {
                return Err(Error::MissingParam {
                    name: (*key).to_string(),
                });
            }
        }

        let mut extras: Vec<&str> = supplied
            .keys()
            .copied()
            .filter(|key| !required.contains(key))
            .collect();
        extras.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        if let Some(key) = extras.first() {
            return Err(Error::ExtraParam {
                name: (*key).to_string(),
            });
        }

        for segment in &route.segments {
            let (key, wildcard) = match segment {
                Segment::Param(key) => (key, false),
                Segment::Wildcard(key) => (key, true),
                Segment::Static(_) => continue,
            };
            let value = supplied[key.as_str()];
            let bytes = value.as_bytes();
            let valid_ascii = bytes.iter().all(|b| (0x20..=0x7e).contains(b));
            let valid_value = if wildcard {
                valid_ascii
                    && (value.is_empty()
                        || value
                            .split('/')
                            .all(|part| !part.is_empty()))
            } else {
                valid_ascii && !value.is_empty() && !value.contains('/')
            };
            if !valid_value {
                return Err(Error::InvalidValue { name: key.clone() });
            }
        }

        let mut output = String::from("/");
        let mut emitted = 0usize;
        for segment in &route.segments {
            match segment {
                Segment::Static(value) => {
                    if emitted > 0 {
                        output.push('/');
                    }
                    output.push_str(&encode_segment(value.as_bytes()));
                    emitted += 1;
                }
                Segment::Param(key) => {
                    if emitted > 0 {
                        output.push('/');
                    }
                    output.push_str(&encode_segment(supplied[key.as_str()].as_bytes()));
                    emitted += 1;
                }
                Segment::Wildcard(key) => {
                    let value = supplied[key.as_str()];
                    if !value.is_empty() {
                        for component in value.split('/') {
                            if emitted > 0 {
                                output.push('/');
                            }
                            output.push_str(&encode_segment(component.as_bytes()));
                            emitted += 1;
                        }
                    }
                }
            }
        }

        if emitted == 0 {
            return Ok("/".to_string());
        }
        if route.trailing {
            output.push('/');
        }
        Ok(output)
    }
}

fn encode_segment(bytes: &[u8]) -> String {
    let mut out = String::new();
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &b in bytes {
        if is_unreserved(b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}