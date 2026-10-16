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

fn is_name(name: &str) -> bool {
    let b = name.as_bytes();
    !b.is_empty()
        && b[0].is_ascii_lowercase()
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_' || *c == b'-')
}

fn valid_method(method: &str) -> bool {
    !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase())
}

fn capture_name_ok(name: &str) -> bool {
    let b = name.as_bytes();
    !b.is_empty()
        && b[0].is_ascii_lowercase()
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

fn decode_segment(segment: &str, base: usize) -> Result<String, usize> {
    let bytes = segment.as_bytes();
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
        } else if is_unreserved(b) {
            out.push(b);
            i += 1;
        } else {
            return Err(base + i);
        }
    }
    Ok(String::from_utf8(out).expect("decoded bytes are ASCII"))
}

fn parse_path(input: &str, pattern: bool) -> Result<(Vec<Segment>, bool), usize> {
    let invalid = |at| at;
    if !input.starts_with('/') {
        return Err(invalid(0));
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = input.as_bytes();
    let trailing = input.ends_with('/');
    let end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut seen = std::collections::HashSet::new();
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
        let is_last = stop == end;

        if pattern && (raw.starts_with(':') || raw.starts_with('*')) {
            let marker = raw.as_bytes()[0];
            let name = &raw[1..];
            if !capture_name_ok(name) {
                return Err(start);
            }
            if !seen.insert(name.to_owned()) {
                return Err(start);
            }
            if marker == b'*' && !is_last {
                return Err(start);
            }
            segments.push(if marker == b':' {
                Segment::Param(name.to_owned())
            } else {
                Segment::Wildcard(name.to_owned())
            });
        } else {
            let decoded = decode_segment(raw, start)?;
            segments.push(Segment::Static(decoded));
        }

        if stop == end {
            break;
        }
        start = stop + 1;
    }

    Ok((segments, trailing))
}

fn encode_segment(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        if is_unreserved(b) {
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
        if !is_name(name) {
            return Err(Error::InvalidName);
        }
        let (segments, trailing) =
            parse_path(pattern, true).map_err(|at| Error::InvalidPattern { at })?;

        if self.routes.iter().any(|route| route.name == name) {
            return Err(Error::DuplicateName);
        }

        let mut rank = Vec::with_capacity(segments.len() + 1);
        for segment in &segments {
            rank.push(match segment {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
                Segment::Wildcard(_) => 0,
            });
        }
        let has_wildcard = segments
            .iter()
            .any(|segment| matches!(segment, Segment::Wildcard(_)));
        if !has_wildcard {
            rank.push(3);
        }

        for route in &self.routes {
            if route.method != method {
                continue;
            }
            if self.policy == SlashPolicy::Strict && route.trailing != trailing {
                continue;
            }
            if route.segments.len() != segments.len() {
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
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let (request_segments, request_trailing) =
            parse_path(head, false).map_err(|at| Error::InvalidPath { at })?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();
        for route in &self.routes {
            if self.policy == SlashPolicy::Strict && route.trailing != request_trailing {
                continue;
            }
            let mut params = Vec::new();
            let mut ri = 0;
            let mut pi = 0;
            let mut matched = true;
            while ri < route.segments.len() {
                match &route.segments[ri] {
                    Segment::Static(expected) => {
                        if pi >= request_segments.len()
                            || request_segments[pi] != Segment::Static(expected.clone())
                        {
                            matched = false;
                            break;
                        }
                        pi += 1;
                    }
                    Segment::Param(name) => {
                        if pi >= request_segments.len() {
                            matched = false;
                            break;
                        }
                        if let Segment::Static(value) = &request_segments[pi] {
                            params.push((name.clone(), value.clone()));
                        } else {
                            matched = false;
                            break;
                        }
                        pi += 1;
                    }
                    Segment::Wildcard(name) => {
                        let value = request_segments[pi..]
                            .iter()
                            .map(|segment| match segment {
                                Segment::Static(value) => value.as_str(),
                                _ => "",
                            })
                            .collect::<Vec<_>>()
                            .join("/");
                        params.push((name.clone(), value));
                        pi = request_segments.len();
                        break;
                    }
                }
                ri += 1;
            }
            if matched && pi == request_segments.len() {
                candidates.push((route, params));
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let has_head = method == "HEAD"
            && candidates.iter().any(|(route, _)| route.method == "HEAD");
        let effective_method = if method == "HEAD" && !has_head {
            "GET"
        } else {
            method
        };

        let eligible: Vec<_> = candidates
            .iter()
            .filter(|(route, _)| route.method == effective_method)
            .collect();

        if eligible.is_empty() {
            let mut allowed = std::collections::BTreeSet::new();
            for (route, _) in &candidates {
                allowed.insert(route.method.clone());
                if route.method == "GET" {
                    allowed.insert("HEAD".to_owned());
                }
            }
            return Err(Error::MethodNotAllowed {
                allowed: allowed.into_iter().collect(),
            });
        }

        let (winner, params) = eligible
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
            && winner.trailing != request_trailing
        {
            let mut target = head.to_owned();
            if winner.trailing {
                target.push('/');
            } else {
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

        let extra = supplied
            .keys()
            .filter(|key| !capture_names.contains(key))
            .min()
            .copied();
        if let Some(key) = extra {
            return Err(Error::ExtraParam {
                name: key.to_owned(),
            });
        }

        for segment in &route.segments {
            let (name, wildcard) = match segment {
                Segment::Param(name) => (name, false),
                Segment::Wildcard(name) => (name, true),
                Segment::Static(_) => continue,
            };
            let value = supplied[name.as_str()];
            let bytes = value.as_bytes();
            let valid_ascii = bytes.iter().all(|b| (0x20..=0x7e).contains(b));
            let valid_shape = if wildcard {
                value.is_empty()
                    || ( !value.starts_with('/')
                        && !value.ends_with('/')
                        && !value.contains("//"))
            } else {
                !value.is_empty() && !value.contains('/')
            };
            if !valid_ascii || !valid_shape {
                return Err(Error::InvalidValue { name: name.clone() });
            }
        }

        if route.segments.is_empty() {
            return Ok("/".to_owned());
        }

        let mut output = String::new();
        for segment in &route.segments {
            output.push('/');
            match segment {
                Segment::Static(value) => output.push_str(&encode_segment(value)),
                Segment::Param(name) => {
                    output.push_str(&encode_segment(supplied[name.as_str()]));
                }
                Segment::Wildcard(name) => {
                    let value = supplied[name.as_str()];
                    if !value.is_empty() {
                        output.push_str(
                            &value
                                .split('/')
                                .map(encode_segment)
                                .collect::<Vec<_>>()
                                .join("/"),
                        );
                    }
                }
            }
        }
        if route.trailing {
            output.push('/');
        }
        Ok(output)
    }
}