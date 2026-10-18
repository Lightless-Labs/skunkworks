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

fn valid_method(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_uppercase())
}

fn valid_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_' || *c == b'-')
}

fn valid_capture_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
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

fn decode_segment(raw: &str, base: usize, pattern: bool) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let (Some(h), Some(l)) = (hex_upper(bytes[i + 1]), hex_upper(bytes[i + 2])) else {
                return Err(base + i);
            };
            let decoded = h * 16 + l;
            if !(0x20..=0x7e).contains(&decoded) || decoded == b'/' || unreserved(decoded) {
                return Err(base + i);
            }
            out.push(decoded);
            i += 3;
        } else if unreserved(b) {
            out.push(b);
            i += 1;
        } else {
            let _ = pattern;
            return Err(base + i);
        }
    }
    Ok(String::from_utf8(out).unwrap())
}

fn parse_path(input: &str, pattern: bool) -> Result<(Vec<Segment>, bool), usize> {
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(0);
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let mut trailing = false;
    let mut end = bytes.len();
    if bytes.last() == Some(&b'/') {
        trailing = true;
        end -= 1;
    }

    let mut segments = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut start = 1;
    while start < end {
        let next = bytes[start..end]
            .iter()
            .position(|b| *b == b'/')
            .map(|p| start + p)
            .unwrap_or(end);
        if next == start {
            return Err(start);
        }
        let raw = &input[start..next];
        if pattern && (raw.starts_with(':') || raw.starts_with('*')) {
            let wildcard = raw.starts_with('*');
            let name = &raw[1..];
            if !valid_capture_name(name) || !seen.insert(name.to_string()) {
                return Err(start);
            }
            if wildcard && next != end {
                return Err(start);
            }
            segments.push(if wildcard {
                Segment::Wildcard(name.to_string())
            } else {
                Segment::Param(name.to_string())
            });
        } else {
            let decoded = decode_segment(raw, start, pattern).map_err(|at| at)?;
            segments.push(Segment::Static(decoded));
        }
        if next == end {
            break;
        }
        start = next + 1;
    }
    Ok((segments, trailing))
}

fn path_head(path: &str) -> &str {
    let cut = path
        .char_indices()
        .find(|(_, c)| *c == '?' || *c == '#')
        .map(|(i, _)| i)
        .unwrap_or(path.len());
    &path[..cut]
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
        if !valid_name(name) {
            return Err(Error::InvalidName);
        }
        let (segments, trailing) =
            parse_path(pattern, true).map_err(|at| Error::InvalidPattern { at })?;
        if self.routes.iter().any(|r| r.name == name) {
            return Err(Error::DuplicateName);
        }

        let mut rank = Vec::new();
        let has_wildcard = segments.iter().any(|s| matches!(s, Segment::Wildcard(_)));
        for segment in &segments {
            rank.push(match segment {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
                Segment::Wildcard(_) => 0,
            });
        }
        if !has_wildcard {
            rank.push(3);
        }

        for route in &self.routes {
            if route.method != method || route.segments.len() != segments.len() {
                continue;
            }
            if self.policy == SlashPolicy::Strict && route.trailing != trailing {
                continue;
            }
            let same_shape = route.segments.iter().zip(&segments).all(|(a, b)| match (a, b) {
                (Segment::Static(x), Segment::Static(y)) => x == y,
                (Segment::Param(_), Segment::Param(_))
                | (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
                _ => false,
            });
            if same_shape {
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
            rank,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }
        let head = path_head(path);
        let (request, trailing) =
            parse_path(head, false).map_err(|at| Error::InvalidPath { at })?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();
        for route in &self.routes {
            if self.policy == SlashPolicy::Strict && route.trailing != trailing {
                continue;
            }
            let mut params = Vec::new();
            let mut matched = true;
            let mut i = 0;
            while i < route.segments.len() {
                match &route.segments[i] {
                    Segment::Static(expected) => {
                        if request.get(i).map(|s| match s {
                            Segment::Static(v) => v == expected,
                            _ => false,
                        }) != Some(true)
                        {
                            matched = false;
                            break;
                        }
                    }
                    Segment::Param(name) => {
                        if let Some(Segment::Static(value)) = request.get(i) {
                            params.push((name.clone(), value.clone()));
                        } else {
                            matched = false;
                            break;
                        }
                    }
                    Segment::Wildcard(name) => {
                        let tail = request[i..]
                            .iter()
                            .map(|s| match s {
                                Segment::Static(v) => v.as_str(),
                                _ => "",
                            })
                            .collect::<Vec<_>>()
                            .join("/");
                        params.push((name.clone(), tail));
                        i = route.segments.len();
                        break;
                    }
                }
                i += 1;
            }
            if matched
                && (route
                    .segments
                    .last()
                    .is_some_and(|s| matches!(s, Segment::Wildcard(_)))
                    || request.len() == route.segments.len())
            {
                candidates.push((route, params));
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible_method = if method == "HEAD" {
            if candidates.iter().any(|(r, _)| r.method == "HEAD") {
                "HEAD"
            } else {
                "GET"
            }
        } else {
            method
        };

        let mut eligible: Vec<_> = candidates
            .iter()
            .filter(|(r, _)| r.method == eligible_method)
            .collect();

        if eligible.is_empty() {
            let mut allowed = std::collections::BTreeSet::new();
            for (route, _) in &candidates {
                allowed.insert(route.method.clone());
            }
            if allowed.contains("GET") {
                allowed.insert("HEAD".to_string());
            }
            return Err(Error::MethodNotAllowed {
                allowed: allowed.into_iter().collect(),
            });
        }

        eligible.sort_by(|(a, _), (b, _)| b.rank.cmp(&a.rank).then_with(|| a.id.cmp(&b.id)));
        let (winner, params) = eligible[0];
        let redirect = if self.policy == SlashPolicy::Redirect
            && !head.eq("/")
            && winner.trailing != trailing
        {
            let mut value = head.to_string();
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
        let Some(route) = self.routes.iter().find(|r| r.name == name) else {
            return Err(Error::UnknownName);
        };

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
                Segment::Param(n) | Segment::Wildcard(n) => required.push(n.as_str()),
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

        let known: std::collections::HashSet<&str> = required.iter().copied().collect();
        if let Some(extra) = supplied.keys().filter(|k| !known.contains(**k)).min() {
            return Err(Error::ExtraParam {
                name: (*extra).to_string(),
            });
        }

        for segment in &route.segments {
            let (n, wildcard) = match segment {
                Segment::Param(n) => (n, false),
                Segment::Wildcard(n) => (n, true),
                Segment::Static(_) => continue,
            };
            let value = supplied[n.as_str()];
            let bytes = value.as_bytes();
            let valid = bytes.iter().all(|b| (0x20..=0x7e).contains(b))
                && if wildcard {
                    value.is_empty()
                        || value.split('/').all(|part| {
                            !part.is_empty()
                                && part.bytes().all(|b| (0x20..=0x7e).contains(&b))
                        })
                } else {
                    !value.is_empty() && !value.contains('/')
                };
            if !valid {
                return Err(Error::InvalidValue { name: n.clone() });
            }
        }

        let mut out = String::from("/");
        let mut emitted = 0usize;
        for segment in &route.segments {
            if let Segment::Wildcard(n) = segment {
                let value = supplied[n.as_str()];
                if !value.is_empty() {
                    for component in value.split('/') {
                        if emitted > 0 {
                            out.push('/');
                        }
                        out.push_str(&encode_segment(component));
                        emitted += 1;
                    }
                }
            } else {
                if emitted > 0 {
                    out.push('/');
                }
                match segment {
                    Segment::Static(s) => out.push_str(&encode_segment(s)),
                    Segment::Param(n) => out.push_str(&encode_segment(supplied[n.as_str()])),
                    Segment::Wildcard(_) => unreachable!(),
                }
                emitted += 1;
            }
        }
        if route.trailing && emitted > 0 {
            out.push('/');
        }
        Ok(out)
    }
}