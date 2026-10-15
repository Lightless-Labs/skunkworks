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
    pattern: String,
    segments: Vec<Segment>,
    trailing: bool,
    rank: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedPath {
    segments: Vec<String>,
    trailing: bool,
}

pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn valid_method(method: &str) -> bool {
    !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase())
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-'))
}

fn valid_capture_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
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
            let hex = |c: u8| -> Option<u8> {
                match c {
                    b'0'..=b'9' => Some(c - b'0'),
                    b'A'..=b'F' => Some(c - b'A' + 10),
                    _ => None,
                }
            };
            let (Some(a), Some(b)) = (hex(h1), hex(h2)) else {
                return Err(base + i);
            };
            let decoded = (a << 4) | b;
            if !(0x20..=0x7e).contains(&decoded)
                || decoded == b'/'
                || is_unreserved(decoded)
            {
                return Err(base + i);
            }
            out.push(decoded);
            i += 3;
        } else {
            if !is_unreserved(b) {
                return Err(base + i);
            }
            out.push(b);
            i += 1;
        }
    }
    Ok(String::from_utf8(out).expect("decoded segments are ASCII"))
}

fn parse_path(input: &str, pattern: bool) -> Result<(Vec<Segment>, bool), usize> {
    if !input.starts_with('/') {
        return Err(0);
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = input.as_bytes();
    let mut raw_segments: Vec<(usize, &str)> = Vec::new();
    let mut start = 1;
    for i in 1..bytes.len() {
        if bytes[i] == b'/' {
            raw_segments.push((start, &input[start..i]));
            start = i + 1;
        }
    }

    let trailing = input.ends_with('/');
    if trailing {
        raw_segments.push((start, &input[start..]));
        raw_segments.pop();
    } else {
        raw_segments.push((start, &input[start..]));
    }

    let mut result = Vec::with_capacity(raw_segments.len());
    let mut names = std::collections::HashSet::new();

    for (index, (offset, raw)) in raw_segments.iter().enumerate() {
        if raw.is_empty() {
            return Err(*offset);
        }

        if pattern && (raw.starts_with(':') || raw.starts_with('*')) {
            let wildcard = raw.as_bytes()[0] == b'*';
            let name = &raw[1..];
            if !valid_capture_name(name) {
                return Err(*offset);
            }
            if !names.insert(name.to_owned()) {
                return Err(*offset);
            }
            if wildcard && index + 1 != raw_segments.len() {
                return Err(*offset);
            }
            result.push(if wildcard {
                Segment::Wildcard(name.to_owned())
            } else {
                Segment::Param(name.to_owned())
            });
        } else {
            let decoded = decode_segment(raw, *offset)?;
            result.push(Segment::Static(decoded));
        }
    }
    Ok((result, trailing))
}

fn parse_request(input: &str) -> Result<(ParsedPath, String), usize> {
    let head_end = input
        .char_indices()
        .find_map(|(i, c)| (c == '?' || c == '#').then_some(i))
        .unwrap_or(input.len());
    let head = &input[..head_end];
    let (segments, trailing) = parse_path(head, false)?;
    let decoded = segments
        .into_iter()
        .map(|s| match s {
            Segment::Static(v) => v,
            _ => unreachable!(),
        })
        .collect();
    Ok((ParsedPath { segments: decoded, trailing }, head.to_owned()))
}

fn shape_conflicts(a: &Route, b_segments: &[Segment], b_trailing: bool, policy: SlashPolicy) -> bool {
    if a.segments.len() != b_segments.len() {
        return false;
    }
    if policy == SlashPolicy::Strict && a.trailing != b_trailing {
        return false;
    }
    a.segments.iter().zip(b_segments).all(|(x, y)| match (x, y) {
        (Segment::Static(a), Segment::Static(b)) => a == b,
        (Segment::Param(_), Segment::Param(_)) => true,
        (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
        _ => false,
    })
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
        if !valid_name(name) {
            return Err(Error::InvalidName);
        }
        let (segments, trailing) =
            parse_path(pattern, true).map_err(|at| Error::InvalidPattern { at })?;

        if self.routes.iter().any(|r| r.name == name) {
            return Err(Error::DuplicateName);
        }

        if let Some(existing) = self.routes.iter().filter(|r| r.method == method).find(|r| {
            shape_conflicts(r, &segments, trailing, self.policy)
        }) {
            return Err(Error::Conflict { with: existing.id });
        }

        let id = self.routes.len();
        let mut rank: Vec<u8> = segments
            .iter()
            .map(|segment| match segment {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
                Segment::Wildcard(_) => 0,
            })
            .collect();
        if !segments.iter().any(|s| matches!(s, Segment::Wildcard(_))) {
            rank.push(3);
        }
        self.routes.push(Route {
            id,
            name: name.to_owned(),
            method: method.to_owned(),
            pattern: pattern.to_owned(),
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
        let (parsed, original_head) =
            parse_request(path).map_err(|at| Error::InvalidPath { at })?;

        let mut path_candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();
        for route in &self.routes {
            if self.policy == SlashPolicy::Strict && route.trailing != parsed.trailing {
                continue;
            }
            let mut params = Vec::new();
            let mut pi = 0;
            let mut ri = 0;
            let mut matched = true;
            while ri < route.segments.len() {
                match &route.segments[ri] {
                    Segment::Static(expected) => {
                        if pi >= parsed.segments.len() || parsed.segments[pi] != *expected {
                            matched = false;
                            break;
                        }
                        pi += 1;
                    }
                    Segment::Param(name) => {
                        if pi >= parsed.segments.len() {
                            matched = false;
                            break;
                        }
                        params.push((name.clone(), parsed.segments[pi].clone()));
                        pi += 1;
                    }
                    Segment::Wildcard(name) => {
                        let value = parsed.segments[pi..].join("/");
                        params.push((name.clone(), value));
                        pi = parsed.segments.len();
                    }
                }
                ri += 1;
            }
            if matched && pi == parsed.segments.len() {
                path_candidates.push((route, params));
            }
        }

        if path_candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let has_head = method == "HEAD"
            && path_candidates.iter().any(|(route, _)| route.method == "HEAD");
        let effective_method = if method == "HEAD" && !has_head {
            "GET"
        } else {
            method
        };

        let mut eligible: Vec<_> = path_candidates
            .iter()
            .filter(|(route, _)| route.method == effective_method)
            .collect();

        if eligible.is_empty() {
            let mut allowed: Vec<String> = path_candidates
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

        eligible.sort_by(|(a, _), (b, _)| b.rank.cmp(&a.rank).then_with(|| a.id.cmp(&b.id)));
        let (winner, params) = eligible[0];
        let redirect = if self.policy == SlashPolicy::Redirect
            && !original_head.is_empty()
            && original_head != "/"
            && winner.trailing != parsed.trailing
        {
            if winner.trailing {
                Some(format!("{original_head}/"))
            } else {
                Some(original_head.trim_end_matches('/').to_owned())
            }
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
        extras.sort();
        if let Some(key) = extras.first() {
            return Err(Error::ExtraParam {
                name: (*key).to_string(),
            });
        }

        for segment in &route.segments {
            let name = match segment {
                Segment::Param(name) | Segment::Wildcard(name) => name,
                Segment::Static(_) => continue,
            };
            let value = supplied[name.as_str()];
            let bytes = value.as_bytes();
            let valid_ascii = bytes.iter().all(|b| (0x20..=0x7e).contains(b));
            let valid = match segment {
                Segment::Param(_) => {
                    valid_ascii && !value.is_empty() && !value.contains('/')
                }
                Segment::Wildcard(_) => {
                    valid_ascii
                        && (value.is_empty()
                            || value
                                .split('/')
                                .all(|part| !part.is_empty()))
                }
                Segment::Static(_) => true,
            };
            if !valid {
                return Err(Error::InvalidValue { name: name.clone() });
            }
        }

        let mut output = String::from("/");
        let mut emitted_any = false;
        for segment in &route.segments {
            match segment {
                Segment::Static(value) => {
                    if emitted_any {
                        output.push('/');
                    }
                    encode_component(value, &mut output);
                    emitted_any = true;
                }
                Segment::Param(name) => {
                    if emitted_any {
                        output.push('/');
                    }
                    encode_component(supplied[name.as_str()], &mut output);
                    emitted_any = true;
                }
                Segment::Wildcard(name) => {
                    let value = supplied[name.as_str()];
                    if !value.is_empty() {
                        if emitted_any {
                            output.push('/');
                        }
                        for (i, part) in value.split('/').enumerate() {
                            if i > 0 {
                                output.push('/');
                            }
                            encode_component(part, &mut output);
                        }
                        emitted_any = true;
                    }
                }
            }
        }
        if !emitted_any {
            output = "/".to_owned();
        } else if route.trailing {
            output.push('/');
        }
        Ok(output)
    }
}

fn encode_component(value: &str, out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for b in value.bytes() {
        if is_unreserved(b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
}