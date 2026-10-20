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
    match bytes.next() {
        Some(b) if b.is_ascii_lowercase() => {}
        _ => return false,
    }
    bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

fn valid_capture_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    match bytes.next() {
        Some(b) if b.is_ascii_lowercase() => {}
        _ => return false,
    }
    bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn upper_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn decode_segment(input: &str, base: usize, pattern: bool) -> Result<String, usize> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let hi = upper_hex(bytes[i + 1]).ok_or(base + i)?;
            let lo = upper_hex(bytes[i + 2]).ok_or(base + i)?;
            let value = hi * 16 + lo;
            if !(0x20..=0x7e).contains(&value) || value == b'/' || unreserved(value) {
                return Err(base + i);
            }
            decoded.push(value);
            i += 3;
        } else if b.is_ascii() && unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            let _ = pattern;
            return Err(base + i);
        }
    }
    Ok(String::from_utf8(decoded).unwrap())
}

fn split_path<'a>(input: &'a str, pattern: bool) -> Result<(Vec<(&'a str, usize)>, bool), usize> {
    if !input.starts_with('/') {
        return Err(0);
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = input.as_bytes();
    let trailing = bytes.last() == Some(&b'/');
    let end = if trailing { input.len() - 1 } else { input.len() };
    let mut parts = Vec::new();
    let mut start = 1;
    let mut i = 1;
    while i <= end {
        if i == end || bytes[i] == b'/' {
            if i == start {
                return Err(start);
            }
            parts.push((&input[start..i], start));
            start = i + 1;
        }
        i += 1;
    }
    if pattern {
        Ok((parts, trailing))
    } else {
        Ok((parts, trailing))
    }
}

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, bool), Error> {
    let (parts, trailing) = split_path(pattern, true)
        .map_err(|at| Error::InvalidPattern { at })?;
    let mut segments = Vec::new();
    let mut names: Vec<String> = Vec::new();

    for (index, (part, at)) in parts.iter().enumerate() {
        if part.starts_with(':') || part.starts_with('*') {
            let wildcard = part.as_bytes()[0] == b'*';
            let name = &part[1..];
            if !valid_capture_name(name) {
                return Err(Error::InvalidPattern { at: *at });
            }
            if names.iter().any(|n| n == name) {
                return Err(Error::InvalidPattern { at: *at });
            }
            if wildcard && index + 1 != parts.len() {
                return Err(Error::InvalidPattern { at: *at });
            }
            names.push(name.to_string());
            if wildcard {
                segments.push(Segment::Wildcard(name.to_string()));
            } else {
                segments.push(Segment::Param(name.to_string()));
            }
        } else {
            let decoded = decode_segment(part, *at, true)
                .map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
        }
    }
    Ok((segments, trailing))
}

fn parse_request(path: &str) -> Result<(Vec<String>, bool, String), Error> {
    let cut = path
        .char_indices()
        .find(|(_, c)| *c == '?' || *c == '#')
        .map(|(i, _)| i)
        .unwrap_or(path.len());
    let head = &path[..cut];
    let (parts, trailing) =
        split_path(head, false).map_err(|at| Error::InvalidPath { at })?;
    let mut decoded = Vec::new();
    for (part, at) in parts {
        decoded.push(
            decode_segment(part, at, false).map_err(|at| Error::InvalidPath { at })?,
        );
    }
    Ok((decoded, trailing, head.to_string()))
}

fn same_shape(a: &Route, b_segments: &[Segment], b_trailing: bool, policy: SlashPolicy) -> bool {
    if a.segments.len() != b_segments.len() {
        return false;
    }
    if policy == SlashPolicy::Strict && a.trailing != b_trailing {
        return false;
    }
    a.segments.iter().zip(b_segments).all(|(x, y)| match (x, y) {
        (Segment::Static(ax), Segment::Static(by)) => ax == by,
        (Segment::Param(_), Segment::Param(_)) => true,
        (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
        _ => false,
    })
}

fn route_rank(segments: &[Segment]) -> Vec<u8> {
    let mut rank = Vec::new();
    let mut has_wildcard = false;
    for segment in segments {
        match segment {
            Segment::Static(_) => rank.push(2),
            Segment::Param(_) => rank.push(1),
            Segment::Wildcard(_) => {
                rank.push(0);
                has_wildcard = true;
                break;
            }
        }
    }
    if !has_wildcard {
        rank.push(3);
    }
    rank
}

fn match_route(route: &Route, request: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut ri = 0;
    for segment in &route.segments {
        match segment {
            Segment::Static(expected) => {
                if request.get(ri)? != expected {
                    return None;
                }
                ri += 1;
            }
            Segment::Param(name) => {
                let value = request.get(ri)?;
                params.push((name.clone(), value.clone()));
                ri += 1;
            }
            Segment::Wildcard(name) => {
                let value = request[ri..].join("/");
                params.push((name.clone(), value));
                ri = request.len();
                break;
            }
        }
    }
    if ri != request.len() {
        return None;
    }
    Some(params)
}

fn encode_component(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
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
        let (segments, trailing) = parse_pattern(pattern)?;
        if self.routes.iter().any(|route| route.name == name) {
            return Err(Error::DuplicateName);
        }
        for route in &self.routes {
            if route.method == method && same_shape(route, &segments, trailing, self.policy) {
                return Err(Error::Conflict { with: route.id });
            }
        }
        let id = self.routes.len();
        self.routes.push(Route {
            id,
            name: name.to_string(),
            method: method.to_string(),
            segments,
            trailing,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }
        let (request, request_trailing, original_head) = parse_request(path)?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>, Vec<u8>)> = Vec::new();
        for route in &self.routes {
            if self.policy == SlashPolicy::Strict && route.trailing != request_trailing {
                continue;
            }
            if let Some(params) = match_route(route, &request) {
                candidates.push((route, params, route_rank(&route.segments)));
            }
        }
        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let has_head = method == "HEAD"
            && candidates.iter().any(|(r, _, _)| r.method == "HEAD");
        let selected_method = if method == "HEAD" && !has_head {
            "GET"
        } else {
            method
        };

        let mut eligible: Vec<_> = candidates
            .iter()
            .filter(|(route, _, _)| route.method == selected_method)
            .collect();
        if eligible.is_empty() {
            let mut allowed = Vec::new();
            for (route, _, _) in &candidates {
                if !allowed.iter().any(|m: &String| m == &route.method) {
                    allowed.push(route.method.clone());
                }
                if route.method == "GET" && !allowed.iter().any(|m| m == "HEAD") {
                    allowed.push("HEAD".to_string());
                }
            }
            allowed.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        eligible.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.id.cmp(&b.0.id)));
        let (route, params, _) = eligible[0];
        let redirect = if self.policy == SlashPolicy::Redirect
            && !route.segments.is_empty()
            && route.trailing != request_trailing
        {
            let mut target = original_head.clone();
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
        let route = self.routes.iter().find(|route| route.name == name).ok_or(Error::UnknownName)?;

        let mut supplied: Vec<(&str, &str)> = Vec::new();
        for &(key, value) in params {
            if supplied.iter().any(|(existing, _)| *existing == key) {
                return Err(Error::DuplicateParam {
                    name: key.to_string(),
                });
            }
            supplied.push((key, value));
        }

        let mut required: Vec<&str> = Vec::new();
        for segment in &route.segments {
            match segment {
                Segment::Param(key) | Segment::Wildcard(key) => required.push(key),
                Segment::Static(_) => {}
            }
        }
        for key in &required {
            if !supplied.iter().any(|(given, _)| given == key) {
                return Err(Error::MissingParam {
                    name: (*key).to_string(),
                });
            }
        }

        let mut extra: Option<&str> = None;
        for (key, _) in &supplied {
            if !required.iter().any(|known| known == key)
                && extra.map_or(true, |current| key.as_bytes() < current.as_bytes())
            {
                extra = Some(key);
            }
        }
        if let Some(key) = extra {
            return Err(Error::ExtraParam {
                name: key.to_string(),
            });
        }

        for key in &required {
            let value = supplied.iter().find(|(given, _)| given == key).unwrap().1;
            let wildcard = route.segments.iter().any(
                |segment| matches!(segment, Segment::Wildcard(name) if name == key),
            );
            let bytes = value.as_bytes();
            let ascii_printable = bytes.iter().all(|b| (0x20..=0x7e).contains(b));
            let valid = if wildcard {
                ascii_printable
                    && (value.is_empty()
                        || (!value.starts_with('/')
                            && !value.ends_with('/')
                            && !value.contains("//")))
            } else {
                ascii_printable && !value.is_empty() && !value.contains('/')
            };
            if !valid {
                return Err(Error::InvalidValue {
                    name: (*key).to_string(),
                });
            }
        }

        let mut output = String::from("/");
        let mut first = true;
        for segment in &route.segments {
            if let Segment::Wildcard(name) = segment {
                let value = supplied.iter().find(|(key, _)| key == name).unwrap().1;
                if !value.is_empty() {
                    for component in value.split('/') {
                        if !first {
                            output.push('/');
                        }
                        output.push_str(&encode_component(component));
                        first = false;
                    }
                }
            } else {
                if !first {
                    output.push('/');
                }
                match segment {
                    Segment::Static(value) => output.push_str(&encode_component(value)),
                    Segment::Param(name) => {
                        let value = supplied.iter().find(|(key, _)| key == name).unwrap().1;
                        output.push_str(&encode_component(value));
                    }
                    Segment::Wildcard(_) => unreachable!(),
                }
                first = false;
            }
        }

        if route.trailing && !output.ends_with('/') && output != "/" {
            output.push('/');
        }
        Ok(output)
    }
}