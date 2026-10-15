use std::net::Ipv6Addr;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub scheme: String,
    pub userinfo: Option<String>,
    pub host: String,
    pub port: Option<u16>,
    pub path: String,
    pub query: Option<String>,
    pub fragment: Option<String>,
    pub ipv6: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    InvalidByte { at: usize },
    InvalidScheme { at: usize },
    MissingAuthority { at: usize },
    InvalidUserInfo { at: usize },
    InvalidHost { at: usize },
    InvalidPort { at: usize },
    InvalidIpv6 { at: usize },
    BadEscape { at: usize },
    InvalidUtf8 { at: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    UserInfo,
    Path,
    Query,
    Fragment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    BadEscape { at: usize },
    InvalidUtf8 { at: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    NotAbsolute,
    BadEscape { at: usize },
    InvalidUtf8 { at: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    Base(ParseError),
    Reference(ParseError),
    Path(PathError),
}

fn invalid_input_byte(input: &str) -> Option<usize> {
    input
        .as_bytes()
        .iter()
        .position(|&b| !(0x21..=0x7e).contains(&b))
}

fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

fn hex_value(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

pub fn decode_component(_component: Component, input: &str) -> Result<String, CodecError> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut origins = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() || !is_hex(bytes[i + 1]) || !is_hex(bytes[i + 2]) {
                return Err(CodecError::BadEscape { at: i });
            }
            decoded.push((hex_value(bytes[i + 1]) << 4) | hex_value(bytes[i + 2]));
            origins.push(i);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            origins.push(i);
            i += 1;
        }
    }

    match std::str::from_utf8(&decoded) {
        Ok(s) => Ok(s.to_owned()),
        Err(e) => {
            let decoded_at = e.valid_up_to();
            let original_at = origins.get(decoded_at).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at: original_at })
        }
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &b in text.as_bytes() {
        let safe = b.is_ascii_alphanumeric()
            || matches!(b, b'-' | b'.' | b'_' | b'~')
            || match component {
                Component::UserInfo => b == b':',
                Component::Path => b == b'/',
                Component::Query => matches!(b, b'/' | b'?'),
                Component::Fragment => matches!(b, b'/' | b'?' | b'#'),
            };
        if safe {
            out.push(b as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}

fn parse_port(port: &str, port_at: usize) -> Result<u16, ParseError> {
    if port.is_empty() {
        return Err(ParseError::InvalidPort { at: port_at });
    }
    if let Some(i) = port.as_bytes().iter().position(|b| !b.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: port_at + i });
    }
    if port.len() >= 2 && port.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_at });
    }

    let mut value: u32 = 0;
    for &b in port.as_bytes() {
        value = match value.checked_mul(10).and_then(|v| v.checked_add((b - b'0') as u32)) {
            Some(v) => v,
            None => return Err(ParseError::InvalidPort { at: port_at }),
        };
        if value > 65535 {
            return Err(ParseError::InvalidPort { at: port_at });
        }
    }
    if value == 0 {
        return Err(ParseError::InvalidPort { at: port_at });
    }
    Ok(value as u16)
}

fn component_parse_error(error: CodecError, offset: usize) -> ParseError {
    match error {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: offset + at },
    }
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    if let Some(at) = invalid_input_byte(input) {
        return Err(ParseError::InvalidByte { at });
    }

    let bytes = input.as_bytes();
    let colon = match bytes.iter().position(|&b| b == b':') {
        Some(i) => i,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };
    if colon == 0 || !bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for i in 1..colon {
        let b = bytes[i];
        if !(b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')) {
            return Err(ParseError::InvalidScheme { at: i });
        }
    }

    if colon + 2 >= bytes.len()
        || bytes[colon + 1] != b'/'
        || bytes[colon + 2] != b'/'
    {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let authority_start = colon + 3;
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map(|i| authority_start + i)
        .unwrap_or(bytes.len());
    let authority = &input[authority_start..authority_end];
    let auth_bytes = authority.as_bytes();

    let at_positions: Vec<usize> = auth_bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i))
        .collect();
    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }

    let (userinfo, hostport, hostport_at) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at,
            });
        }
        (
            Some(authority[..at].to_owned()),
            &authority[at + 1..],
            authority_start + at + 1,
        )
    } else {
        (None, authority, authority_start)
    };

    let mut ipv6 = false;
    let (host, port) = if hostport.starts_with('[') {
        let close = match hostport.as_bytes()[1..].iter().position(|&b| b == b']') {
            Some(i) => i + 1,
            None => return Err(ParseError::InvalidIpv6 { at: hostport_at }),
        };
        let interior = &hostport[1..close];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: hostport_at });
        }
        ipv6 = true;
        let suffix = &hostport[close + 1..];
        let parsed_port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(parse_port(&suffix[1..], hostport_at + close + 2)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_at + close + 1,
            });
        };
        (interior.to_ascii_lowercase(), parsed_port)
    } else {
        let split = hostport.as_bytes().iter().position(|&b| b == b':');
        let (raw_host, raw_port, port_at) = match split {
            Some(i) => (
                &hostport[..i],
                Some(&hostport[i + 1..]),
                hostport_at + i + 1,
            ),
            None => (hostport, None, 0),
        };
        if raw_host.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_at });
        }
        if let Some(i) = raw_host.as_bytes().iter().position(|&b| {
            !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
        }) {
            return Err(ParseError::InvalidHost {
                at: hostport_at + i,
            });
        }
        let parsed_port = match raw_port {
            Some(p) => Some(parse_port(p, port_at)?),
            None => None,
        };
        (raw_host.to_ascii_lowercase(), parsed_port)
    };

    let after_authority = &input[authority_end..];
    let fragment_pos = after_authority.find('#');
    let (before_fragment, fragment) = match fragment_pos {
        Some(i) => (
            &after_authority[..i],
            Some(after_authority[i + 1..].to_owned()),
        ),
        None => (after_authority, None),
    };
    let query_pos = before_fragment.find('?');
    let (raw_path, query) = match query_pos {
        Some(i) => (
            &before_fragment[..i],
            Some(before_fragment[i + 1..].to_owned()),
        ),
        None => (before_fragment, None),
    };
    let path = if raw_path.is_empty() {
        "/".to_owned()
    } else {
        raw_path.to_owned()
    };

    if let Some(ref u) = userinfo {
        decode_component(Component::UserInfo, u)
            .map_err(|e| component_parse_error(e, authority_start))?;
    }
    if !raw_path.is_empty() {
        decode_component(Component::Path, raw_path)
            .map_err(|e| component_parse_error(e, authority_end))?;
    }
    if let Some(ref q) = query {
        let offset = authority_end + query_pos.unwrap() + 1;
        decode_component(Component::Query, q)
            .map_err(|e| component_parse_error(e, offset))?;
    }
    if let Some(ref f) = fragment {
        let offset = authority_end + fragment_pos.unwrap() + 1;
        decode_component(Component::Fragment, f)
            .map_err(|e| component_parse_error(e, offset))?;
    }

    Ok(Url {
        scheme: input[..colon].to_ascii_lowercase(),
        userinfo,
        host,
        port,
        path,
        query,
        fragment,
        ipv6,
    })
}

pub fn normalise_path(path: &str) -> Result<String, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }
    decode_component(Component::Path, path).map_err(|e| match e {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    })?;

    let segments: Vec<&str> = path[1..].split('/').collect();
    let mut stack: Vec<&str> = Vec::new();
    let mut final_navigation = false;

    for (index, segment) in segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, segment).expect("path validated");
        let is_dot = decoded == ".";
        let is_dotdot = decoded == "..";
        if index + 1 == segments.len() {
            final_navigation = is_dot || is_dotdot;
        }
        if is_dot {
            continue;
        } else if is_dotdot {
            stack.pop();
        } else {
            stack.push(segment);
        }
    }

    if final_navigation {
        stack.push("");
    }
    if stack.is_empty() {
        Ok("/".to_owned())
    } else {
        let mut out = String::from("/");
        out.push_str(&stack.join("/"));
        Ok(out)
    }
}

fn validate_reference_bytes(reference: &str) -> Result<(), ResolveError> {
    if let Some(at) = invalid_input_byte(reference) {
        Err(ResolveError::Reference(ParseError::InvalidByte { at }))
    } else {
        Ok(())
    }
}

fn normalize_url_path(mut url: Url) -> Result<Url, ResolveError> {
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

fn append_optional(out: &mut String, marker: char, value: &Option<String>) {
    if let Some(value) = value {
        out.push(marker);
        out.push_str(value);
    }
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let normalized_base_path =
        normalise_path(&base_url.path).map_err(ResolveError::Path)?;
    validate_reference_bytes(reference)?;

    if let Some(colon) = reference.find(':') {
        let first_delimiter = reference
            .char_indices()
            .find(|(_, c)| matches!(c, '/' | '?' | '#'))
            .map(|(i, _)| i)
            .unwrap_or(reference.len());
        if colon < first_delimiter {
            let url = parse_url(reference).map_err(ResolveError::Reference)?;
            return normalize_url_path(url);
        }
    }

    if reference.starts_with("//") {
        let constructed = format!("{}:{}", base_url.scheme, reference);
        let url = parse_url(&constructed).map_err(ResolveError::Reference)?;
        return normalize_url_path(url);
    }

    let fragment_pos = reference.find('#');
    let (before_fragment, fragment) = match fragment_pos {
        Some(i) => (&reference[..i], Some(reference[i + 1..].to_owned())),
        None => (reference, None),
    };
    let query_pos = before_fragment.find('?');
    let (reference_path, reference_query) = match query_pos {
        Some(i) => (
            &before_fragment[..i],
            Some(before_fragment[i + 1..].to_owned()),
        ),
        None => (before_fragment, None),
    };

    let (selected_path, selected_query) = if reference_path.is_empty() {
        (
            normalized_base_path,
            if reference_query.is_none() {
                base_url.query.clone()
            } else {
                reference_query
            },
        )
    } else {
        let merged = if reference_path.starts_with('/') {
            reference_path.to_owned()
        } else {
            let slash = base_url.path.rfind('/').unwrap_or(0);
            let mut value = base_url.path[..=slash].to_owned();
            value.push_str(reference_path);
            value
        };
        (merged, reference_query)
    };

    let mut raw = String::new();
    raw.push_str(&base_url.scheme);
    raw.push_str("://");
    if let Some(userinfo) = &base_url.userinfo {
        raw.push_str(userinfo);
        raw.push('@');
    }
    if base_url.ipv6 {
        raw.push('[');
        raw.push_str(&base_url.host);
        raw.push(']');
    } else {
        raw.push_str(&base_url.host);
    }
    if let Some(port) = base_url.port {
        raw.push(':');
        raw.push_str(&port.to_string());
    }
    raw.push_str(&selected_path);
    append_optional(&mut raw, '?', &selected_query);
    append_optional(&mut raw, '#', &fragment);

    let mut url = parse_url(&raw).map_err(ResolveError::Reference)?;
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

fn main() {}