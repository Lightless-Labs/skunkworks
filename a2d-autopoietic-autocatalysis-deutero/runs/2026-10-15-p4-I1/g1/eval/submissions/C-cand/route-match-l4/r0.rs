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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
}

#[derive(Debug, Clone)]
struct Route {
    id: usize,
    name: String,
    method: String,
    segments: Vec<Segment>,
    trailing: bool,
}

#[derive(Debug, Clone)]
struct ParsedPath {
    segments: Vec<String>,
    trailing: bool,
}

#[derive(Debug, Clone)]
pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn is_name(name: &str, allow_hyphen: bool) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_' || (allow_hyphen && *b == b'-'))
}

fn valid_method(method: &str) -> bool {
    !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase())
}

fn decode_segment(s: &str, base: usize) -> Result<String, usize> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let h1 = bytes[i + 1];
            let h2 = bytes[i + 2];
            let digit = |x: u8| -> Option<u8> {
                match x {
                    b'0'..=b'9' => Some(x - b'0'),
                    b'A'..=b'F' => Some(x - b'A' + 10),
                    _ => None,
                }
            };
            let Some(a) = digit(h1) else {
                return Err(base + i);
            };
            let Some(c) = digit(h2) else {
                return Err(base + i);
            };
            let decoded = (a << 4) | c;
            if !(0x20..=0x7e).contains(&decoded)
                || decoded == b'/'
                || is_unreserved(decoded)
            {
                return Err(base + i);
            }
            out.push(decoded);
            i += 3;
        } else {
            if !b.is_ascii() || !is_unreserved(b) {
                return Err(base + i);
            }
            out.push(b);
            i += 1;
        }
    }
    Ok(String::from_utf8(out).unwrap())
}

fn split_path(input: &str, is_pattern: bool) -> Result<ParsedPath, Error> {
    let bad = |at| {
        if is_pattern {
            Error::InvalidPattern { at }
        } else {
            Error::InvalidPath { at }
        }
    };
    if !input.starts_with('/') {
        return Err(bad(0));
    }
    if input == "/" {
        return Ok(ParsedPath {
            segments: Vec::new(),
            trailing: false,
        });
    }

    let trailing = input.ends_with('/');
    let end = if trailing { input.len() - 1 } else { input.len() };
    let body = &input[1..end];
    let mut segments = Vec::new();
    let mut offset = 1;

    for raw in body.split('/') {
        if raw.is_empty() {
            return Err(bad(offset));
        }

        if is_pattern && (raw.starts_with(':') || raw.starts_with('*')) {
            let marker = raw.as_bytes()[0];
            let name = &raw[1..];
            if !is_name(name, false) {
                return Err(bad(offset));
            }
            let kind = if marker == b':' {
                Segment::Param(name.to_owned())
            } else {
                Segment::Wildcard(name.to_owned())
            };
            segments.push(kind);
        } else {
            let decoded = decode_segment(raw, offset).map_err(bad)?;
            segments.push(Segment::Static(decoded));
        }

        offset += raw.len() + 1;
    }

    if is_pattern {
        let mut seen = std::collections::HashSet::new();
        for (i, seg) in segments.iter().enumerate() {
            match seg {
                Segment::Param(name) => {
                    if !seen.insert(name.clone()) {
                        // Recompute the byte offset of this segment.
                        return Err(bad(segment_offset(input, i)));
                    }
                }
                Segment::Wildcard(name) => {
                    if i + 1 != segments.len() {
                        return Err(bad(segment_offset(input, i)));
                    }
                    if !seen.insert(name.clone()) {
                        return Err(bad(segment_offset(input, i)));
                    }
                }
                Segment::Static(_) => {}
            }
        }
    }

    Ok(ParsedPath { segments: match_segments(segments), trailing })
}

fn segment_offset(input: &str, index: usize) -> usize {
    let mut pos = 1;
    let mut current = 0;
    while current < index {
        if let Some(next) = input[pos..].find('/') {
            pos += next + 1;
        } else {
            break;
        }
        current += 1;
    }
    pos
}

fn match_segments(segments: Vec<Segment>) -> Vec<String> {
    // This helper is only used to keep `split_path`'s result type compact.
    // Pattern parsing handles its own segment representation elsewhere.
    segments
        .into_iter()
        .map(|s| match s {
            Segment::Static(v) => v,
            Segment::Param(v) | Segment::Wildcard(v) => v,
        })
        .collect()
}

fn parse_pattern(input: &str) -> Result<(Vec<Segment>, bool), Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }
    let trailing = input.ends_with('/');
    let end = if trailing { input.len() - 1 } else { input.len() };
    let body = &input[1..end];
    let mut segments = Vec::new();
    let mut offset = 1;
    let mut seen = std::collections::HashSet::new();

    for (i, raw) in body.split('/').enumerate() {
        if raw.is_empty() {
            return Err(Error::InvalidPattern { at: offset });
        }
        if raw.starts_with(':') || raw.starts_with('*') {
            let name = &raw[1..];
            if !is_name(name, false) {
                return Err(Error::InvalidPattern { at: offset });
            }
            if raw.starts_with('*') && i + 1 != body.split('/').count() {
                return Err(Error::InvalidPattern { at: offset });
            }
            if !seen.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: offset });
            }
            segments.push(if raw.starts_with(':') {
                Segment::Param(name.to_owned())
            } else {
                Segment::Wildcard(name.to_owned())
            });
        } else {
            let decoded = decode_segment(raw, offset)
                .map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
        }
        offset += raw.len() + 1;
    }
    Ok((segments, trailing))
}

fn parse_request(head: &str) -> Result<ParsedPath, Error> {
    if !head.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if head == "/" {
        return Ok(ParsedPath {
            segments: Vec::new(),
            trailing: false,
        });
    }
    let trailing = head.ends_with('/');
    let end = if trailing { head.len() - 1 } else { head.len() };
    let body = &head[1..end];
    let mut segments = Vec::new();
    let mut offset = 1;
    for raw in body.split('/') {
        if raw.is_empty() {
            return Err(Error::InvalidPath { at: offset });
        }
        segments.push(
            decode_segment(raw, offset).map_err(|at| Error::InvalidPath { at })?,
        );
        offset += raw.len() + 1;
    }
    Ok(ParsedPath { segments, trailing })
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
        if !is_name(name, true) {
            return Err(Error::InvalidName);
        }
        let (segments, trailing) = parse_pattern(pattern)?;
        if self.routes.iter().any(|r| r.name == name) {
            return Err(Error::DuplicateName);
        }

        for route in &self.routes {
            if route.method != method || route.segments.len() != segments.len() {
                continue;
            }
            if self.policy == SlashPolicy::Strict && route.trailing != trailing {
                continue;
            }
            let same_shape = route
                .segments
                .iter()
                .zip(&segments)
                .all(|(a, b)| match (a, b) {
                    (Segment::Static(x), Segment::Static(y)) => x == y,
                    (Segment::Param(_), Segment::Param(_)) => true,
                    (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
                    _ => false,
                });
            if same_shape {
                return Err(Error::Conflict { with: route.id });
            }
        }

        let id = self.routes.len();
        self.routes.push(Route {
            id,
            name: name.to_owned(),
            method: method.to_owned(),
            segments,
            trailing,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }
        let cut = path
            .char_indices()
            .find(|(_, c)| *c == '?' || *c == '#')
            .map(|(i, _)| i)
            .unwrap_or(path.len());
        let head = &path[..cut];
        let parsed = parse_request(head)?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>, Vec<u8>)> = Vec::new();
        for route in &self.routes {
            if self.policy == SlashPolicy::Strict && route.trailing != parsed.trailing {
                continue;
            }
            if let Some(params) = match_route(route, &parsed.segments) {
                candidates.push((route, params, rank(&route.segments)));
            }
        }
        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let has_head = method == "HEAD"
            && candidates.iter().any(|(r, _, _)| r.method == "HEAD");
        let chosen_method = if method == "HEAD" && !has_head {
            "GET"
        } else {
            method
        };
        let mut eligible: Vec<_> = candidates
            .iter()
            .filter(|(r, _, _)| r.method == chosen_method)
            .collect();

        if eligible.is_empty() {
            let mut allowed = std::collections::BTreeSet::new();
            for (r, _, _) in &candidates {
                allowed.insert(r.method.clone());
                if r.method == "GET" {
                    allowed.insert("HEAD".to_owned());
                }
            }
            return Err(Error::MethodNotAllowed {
                allowed: allowed.into_iter().collect(),
            });
        }

        eligible.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.id.cmp(&b.0.id)));
        let (route, params, _) = eligible[0];
        let redirect = if self.policy == SlashPolicy::Redirect
            && route.trailing != parsed.trailing
            && !parsed.segments.is_empty()
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
            .find(|r| r.name == name)
            .ok_or(Error::UnknownName)?;

        let mut supplied = std::collections::HashMap::new();
        for (key, value) in params {
            if supplied.insert(*key, *value).is_some() {
                return Err(Error::DuplicateParam {
                    name: (*key).to_owned(),
                });
            }
        }

        let mut required = Vec::new();
        for seg in &route.segments {
            match seg {
                Segment::Param(name) | Segment::Wildcard(name) => required.push(name.as_str()),
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
        let required_set: std::collections::HashSet<&str> =
            required.iter().copied().collect();
        let extra = supplied
            .keys()
            .filter(|k| !required_set.contains(**k))
            .min_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        if let Some(key) = extra {
            return Err(Error::ExtraParam {
                name: (*key).to_owned(),
            });
        }

        for seg in &route.segments {
            match seg {
                Segment::Param(name) => {
                    let value = supplied[name.as_str()];
                    if value.is_empty()
                        || !value.bytes().all(|b| (0x20..=0x7e).contains(&b))
                        || value.contains('/')
                    {
                        return Err(Error::InvalidValue { name: name.clone() });
                    }
                }
                Segment::Wildcard(name) => {
                    let value = supplied[name.as_str()];
                    if !value.bytes().all(|b| (0x20..=0x7e).contains(&b))
                        || (!value.is_empty()
                            && value
                                .split('/')
                                .any(|part| part.is_empty()))
                    {
                        return Err(Error::InvalidValue { name: name.clone() });
                    }
                }
                Segment::Static(_) => {}
            }
        }

        let mut output = String::from("/");
        let mut first = true;
        for seg in &route.segments {
            match seg {
                Segment::Static(value) => {
                    if !first {
                        output.push('/');
                    }
                    output.push_str(&encode(value.as_bytes()));
                    first = false;
                }
                Segment::Param(name) => {
                    if !first {
                        output.push('/');
                    }
                    output.push_str(&encode(supplied[name.as_str()].as_bytes()));
                    first = false;
                }
                Segment::Wildcard(name) => {
                    let value = supplied[name.as_str()];
                    if !value.is_empty() {
                        if !first {
                            output.push('/');
                        }
                        for (i, component) in value.split('/').enumerate() {
                            if i > 0 {
                                output.push('/');
                            }
                            output.push_str(&encode(component.as_bytes()));
                        }
                        first = false;
                    }
                }
            }
        }
        if output != "/" && route.trailing {
            output.push('/');
        }
        Ok(output)
    }
}

fn match_route(route: &Route, path: &[String]) -> Option<Vec<(String, String)>> {
    let wildcard_index = route
        .segments
        .iter()
        .position(|s| matches!(s, Segment::Wildcard(_)));
    let fixed_len = wildcard_index.unwrap_or(route.segments.len());
    if path.len() < fixed_len || (wildcard_index.is_none() && path.len() != fixed_len) {
        return None;
    }

    let mut params = Vec::new();
    for (i, segment) in route.segments[..fixed_len].iter().enumerate() {
        match segment {
            Segment::Static(value) if value != &path[i] => return None,
            Segment::Static(_) => {}
            Segment::Param(name) => params.push((name.clone(), path[i].clone())),
            Segment::Wildcard(_) => unreachable!(),
        }
    }
    if let Some(i) = wildcard_index {
        if let Segment::Wildcard(name) = &route.segments[i] {
            params.push((name.clone(), path[i..].join("/")));
        }
    }
    Some(params)
}

fn rank(segments: &[Segment]) -> Vec<u8> {
    let mut result = Vec::new();
    for segment in segments {
        match segment {
            Segment::Static(_) => result.push(2),
            Segment::Param(_) => result.push(1),
            Segment::Wildcard(_) => {
                result.push(0);
                return result;
            }
        }
    }
    result.push(3);
    result
}

fn encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::new();
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

fn main() {}