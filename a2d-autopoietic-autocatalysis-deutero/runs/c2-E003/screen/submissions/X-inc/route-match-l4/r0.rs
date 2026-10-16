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

#[derive(Clone, PartialEq, Eq)]
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

struct ParsedPath {
    segments: Vec<String>,
    trailing: bool,
}

fn valid_method(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_uppercase())
}

fn valid_route_name(s: &str) -> bool {
    let mut bytes = s.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-'
        })
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

fn parse_segments(input: &str, pattern: bool) -> Result<(Vec<Segment>, bool), Error> {
    let invalid = |at| {
        if pattern {
            Error::InvalidPattern { at }
        } else {
            Error::InvalidPath { at }
        }
    };
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(invalid(0));
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let trailing = input.ends_with('/');
    let end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut start = 1;

    while start < end {
        let mut stop = start;
        while stop < end && bytes[stop] != b'/' {
            stop += 1;
        }
        if stop == start {
            return Err(invalid(start));
        }

        let raw = &input[start..stop];
        if pattern && (raw.starts_with(':') || raw.starts_with('*')) {
            let marker = raw.as_bytes()[0];
            let name = &raw[1..];
            if !valid_capture_name(name) {
                return Err(invalid(start));
            }
            if !seen.insert(name.to_owned()) {
                return Err(invalid(start));
            }
            if marker == b'*' && stop < end {
                return Err(invalid(start));
            }
            result.push(if marker == b':' {
                Segment::Param(name.to_owned())
            } else {
                Segment::Wildcard(name.to_owned())
            });
        } else {
            let mut decoded = Vec::new();
            let mut i = 0;
            while i < raw.len() {
                let b = raw.as_bytes()[i];
                if b == b'%' {
                    if i + 2 >= raw.len() {
                        return Err(invalid(start + i));
                    }
                    let hi = hex_upper(raw.as_bytes()[i + 1]);
                    let lo = hex_upper(raw.as_bytes()[i + 2]);
                    let Some((hi, lo)) = hi.zip(lo) else {
                        return Err(invalid(start + i));
                    };
                    let value = hi * 16 + lo;
                    if !(0x20..=0x7e).contains(&value)
                        || value == b'/'
                        || unreserved(value)
                    {
                        return Err(invalid(start + i));
                    }
                    decoded.push(value);
                    i += 3;
                } else {
                    if !unreserved(b) {
                        return Err(invalid(start + i));
                    }
                    decoded.push(b);
                    i += 1;
                }
            }
            result.push(Segment::Static(String::from_utf8(decoded).unwrap()));
        }

        if stop == end {
            break;
        }
        start = stop + 1;
        if start == end {
            break;
        }
    }

    if trailing && result.is_empty() {
        return Err(invalid(1));
    }
    Ok((result, trailing))
}

fn parse_request(input: &str) -> Result<ParsedPath, Error> {
    let head_end = input
        .char_indices()
        .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
        .unwrap_or(input.len());
    let head = &input[..head_end];
    let (segments, trailing) = parse_segments(head, false)?;
    Ok(ParsedPath {
        segments: segments
            .into_iter()
            .map(|s| match s {
                Segment::Static(v) => v,
                _ => unreachable!(),
            })
            .collect(),
        trailing,
    })
}

fn rank_for(segments: &[Segment]) -> Vec<u8> {
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
        let (segments, trailing) = parse_segments(pattern, true)?;
        if self.routes.iter().any(|r| r.name == name) {
            return Err(Error::DuplicateName);
        }
        for route in &self.routes {
            if route.method != method {
                continue;
            }
            if self.policy == SlashPolicy::Strict && route.trailing != trailing {
                continue;
            }
            if route.segments.len() == segments.len()
                && route
                    .segments
                    .iter()
                    .zip(&segments)
                    .all(|(a, b)| match (a, b) {
                        (Segment::Static(x), Segment::Static(y)) => x == y,
                        (Segment::Param(_), Segment::Param(_)) => true,
                        (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
                        _ => false,
                    })
            {
                return Err(Error::Conflict { with: route.id });
            }
        }

        let id = self.routes.len();
        self.routes.push(Route {
            id,
            name: name.to_owned(),
            method: method.to_owned(),
            rank: rank_for(&segments),
            segments,
            trailing,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }
        let parsed = parse_request(path)?;
        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();

        for route in &self.routes {
            if self.policy == SlashPolicy::Strict && route.trailing != parsed.trailing {
                continue;
            }
            let mut params = Vec::new();
            let mut i = 0;
            let mut matched = true;

            for segment in &route.segments {
                match segment {
                    Segment::Static(value) => {
                        if parsed.segments.get(i) != Some(value) {
                            matched = false;
                            break;
                        }
                        i += 1;
                    }
                    Segment::Param(name) => {
                        if let Some(value) = parsed.segments.get(i) {
                            params.push((name.clone(), value.clone()));
                            i += 1;
                        } else {
                            matched = false;
                            break;
                        }
                    }
                    Segment::Wildcard(name) => {
                        params.push((name.clone(), parsed.segments[i..].join("/")));
                        i = parsed.segments.len();
                        break;
                    }
                }
            }

            if matched && i == parsed.segments.len() {
                candidates.push((route, params));
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let has_head = method == "HEAD"
            && candidates
                .iter()
                .any(|(route, _)| route.method == "HEAD");
        let selected_method = if method == "HEAD" && !has_head {
            "GET"
        } else {
            method
        };
        let eligible: Vec<_> = candidates
            .iter()
            .filter(|(route, _)| route.method == selected_method)
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

        let mut winner = eligible[0];
        for candidate in eligible.iter().skip(1) {
            if candidate.0.rank > winner.0.rank {
                winner = candidate;
            }
        }

        let redirect = if self.policy == SlashPolicy::Redirect
            && !parsed.segments.is_empty()
            && winner.0.trailing != parsed.trailing
        {
            let head_end = path
                .char_indices()
                .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
                .unwrap_or(path.len());
            let mut head = path[..head_end].to_owned();
            if winner.0.trailing {
                head.push('/');
            } else if head.ends_with('/') {
                head.pop();
            }
            Some(head)
        } else {
            None
        };

        Ok(Match {
            route: winner.0.id,
            params: winner.1.clone(),
            redirect,
        })
    }

    pub fn build(&self, name: &str, params: &[(&str, &str)]) -> Result<String, Error> {
        let Some(route) = self.routes.iter().find(|route| route.name == name) else {
            return Err(Error::UnknownName);
        };

        let mut supplied = std::collections::HashMap::new();
        for (key, value) in params {
            if supplied.insert(*key, *value).is_some() {
                return Err(Error::DuplicateParam {
                    name: (*key).to_owned(),
                });
            }
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
                    name: (*key).to_owned(),
                });
            }
        }

        let mut extras: Vec<&str> = supplied
            .keys()
            .copied()
            .filter(|key| !required.contains(key))
            .collect();
        extras.sort();
        if let Some(key) = extras.first() {
            return Err(Error::ExtraParam {
                name: (*key).to_string(),
            });
        }

        for segment in &route.segments {
            match segment {
                Segment::Param(key) => {
                    let value = supplied.get(key.as_str()).unwrap();
                    if value.is_empty()
                        || !value.bytes().all(|b| (0x20..=0x7e).contains(&b))
                        || value.contains('/')
                    {
                        return Err(Error::InvalidValue { name: key.clone() });
                    }
                }
                Segment::Wildcard(key) => {
                    let value = supplied.get(key.as_str()).unwrap();
                    if !value.bytes().all(|b| (0x20..=0x7e).contains(&b))
                        || (!value.is_empty() && value.split('/').any(|part| part.is_empty()))
                    {
                        return Err(Error::InvalidValue { name: key.clone() });
                    }
                }
                Segment::Static(_) => {}
            }
        }

        let mut output = String::new();
        for segment in &route.segments {
            output.push('/');
            match segment {
                Segment::Static(value) => output.push_str(&encode_component(value)),
                Segment::Param(key) => {
                    output.push_str(&encode_component(supplied.get(key.as_str()).unwrap()))
                }
                Segment::Wildcard(key) => {
                    let value = supplied.get(key.as_str()).unwrap();
                    for (i, part) in value.split('/').enumerate() {
                        if i > 0 {
                            output.push('/');
                        }
                        output.push_str(&encode_component(part));
                    }
                }
            }
        }
        if output.is_empty() {
            output.push('/');
        }
        if route.trailing && output != "/" {
            output.push('/');
        }
        Ok(output)
    }
}