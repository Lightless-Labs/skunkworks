#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidPattern { at: usize },
    InvalidPath { at: usize },
    NotFound,
    InvalidMethod,
    MethodNotAllowed { allowed: Vec<String> },
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

struct Route {
    method: String,
    segments: Vec<Segment>,
    trailing_slash: bool,
    has_wildcard: bool,
}

pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
}

fn valid_method(method: &str) -> bool {
    !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase())
}

fn name_is_valid(name: &str) -> bool {
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

fn decode_segment(raw: &str, base: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let hi = hex_upper(bytes[i + 1]).ok_or(base + i)?;
            let lo = hex_upper(bytes[i + 2]).ok_or(base + i)?;
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(base + i);
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            return Err(base + i);
        }
    }

    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
}

fn parse_path(input: &str, pattern: bool) -> Result<(Vec<Segment>, bool), usize> {
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(0);
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let trailing_slash = input.ends_with('/');
    let end = if trailing_slash {
        bytes.len() - 1
    } else {
        bytes.len()
    };

    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();
    let mut start = 1;

    while start < end {
        let relative_end = bytes[start..end]
            .iter()
            .position(|&b| b == b'/')
            .map(|i| start + i)
            .unwrap_or(end);

        if relative_end == start {
            return Err(start);
        }

        let raw = &input[start..relative_end];
        if pattern && (raw.starts_with(':') || raw.starts_with('*')) {
            let marker = raw.as_bytes()[0];
            let name = &raw[1..];
            if !name_is_valid(name) || !names.insert(name.to_owned()) {
                return Err(start);
            }

            if marker == b'*' && relative_end != end {
                return Err(start);
            }

            if marker == b':' {
                segments.push(Segment::Param(name.to_owned()));
            } else {
                segments.push(Segment::Wildcard(name.to_owned()));
            }
        } else {
            let decoded = decode_segment(raw, start)?;
            segments.push(Segment::Static(decoded));
        }

        if relative_end == end {
            break;
        }
        start = relative_end + 1;
    }

    // The stripped terminal slash is permitted only when the preceding
    // segment is nonempty. For example, "//" and "/a//" have an empty
    // segment immediately before that slash.
    if trailing_slash && end > 0 && bytes[end - 1] == b'/' {
        return Err(end - 1);
    }

    Ok((segments, trailing_slash))
}

fn path_head(path: &str) -> &str {
    let end = path
        .bytes()
        .position(|b| b == b'?' || b == b'#')
        .unwrap_or(path.len());
    &path[..end]
}

impl Router {
    pub fn new(policy: SlashPolicy) -> Self {
        Self {
            policy,
            routes: Vec::new(),
        }
    }

    pub fn add(&mut self, method: &str, pattern: &str) -> Result<usize, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }

        let (segments, trailing_slash) =
            parse_path(pattern, true).map_err(|at| Error::InvalidPattern { at })?;
        let has_wildcard = matches!(segments.last(), Some(Segment::Wildcard(_)));

        let id = self.routes.len();
        self.routes.push(Route {
            method: method.to_owned(),
            segments,
            trailing_slash,
            has_wildcard,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }

        let head = path_head(path);
        let (parsed_request, request_trailing) =
            parse_path(head, false).map_err(|at| Error::InvalidPath { at })?;
        let request_segments: Vec<String> = parsed_request
            .into_iter()
            .map(|segment| match segment {
                Segment::Static(value) => value,
                _ => unreachable!("request paths do not contain captures"),
            })
            .collect();

        let mut candidates = Vec::new();
        for (index, route) in self.routes.iter().enumerate() {
            if self.policy == SlashPolicy::Strict
                && route.trailing_slash != request_trailing
            {
                continue;
            }
            if let Some(params) = match_route(route, &request_segments) {
                candidates.push((index, params, rank(route)));
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible_method = if method == "HEAD"
            && candidates
                .iter()
                .any(|(i, _, _)| self.routes[*i].method == "HEAD")
        {
            "HEAD"
        } else if method == "HEAD" {
            "GET"
        } else {
            method
        };

        let eligible: Vec<_> = candidates
            .iter()
            .filter(|(i, _, _)| self.routes[*i].method == eligible_method)
            .collect();

        if eligible.is_empty() {
            let mut allowed = std::collections::BTreeSet::new();
            for (i, _, _) in &candidates {
                allowed.insert(self.routes[*i].method.clone());
            }
            if allowed.contains("GET") {
                allowed.insert("HEAD".to_owned());
            }
            return Err(Error::MethodNotAllowed {
                allowed: allowed.into_iter().collect(),
            });
        }

        let winner = eligible
            .into_iter()
            .max_by(|a, b| a.2.cmp(&b.2).then_with(|| b.0.cmp(&a.0)))
            .expect("eligible candidates are nonempty");

        let route = &self.routes[winner.0];
        let redirect = if self.policy == SlashPolicy::Redirect
            && route.trailing_slash != request_trailing
            && head != "/"
        {
            let mut target = head.to_owned();
            if route.trailing_slash {
                target.push('/');
            } else {
                target.pop();
            }
            Some(target)
        } else {
            None
        };

        Ok(Match {
            route: winner.0,
            params: winner.1.clone(),
            redirect,
        })
    }
}

fn rank(route: &Route) -> Vec<u8> {
    let mut result = Vec::new();
    for segment in &route.segments {
        result.push(match segment {
            Segment::Static(_) => 2,
            Segment::Param(_) => 1,
            Segment::Wildcard(_) => 0,
        });
    }
    if !route.has_wildcard {
        result.push(3);
    }
    result
}

fn match_route(route: &Route, request: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut ri = 0;

    for segment in &route.segments {
        match segment {
            Segment::Static(value) => {
                if request.get(ri)? != value {
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

    if ri == request.len() {
        Some(params)
    } else {
        None
    }
}