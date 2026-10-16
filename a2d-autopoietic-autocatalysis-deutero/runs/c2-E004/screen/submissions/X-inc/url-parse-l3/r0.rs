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

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn decode_component(_component: Component, input: &str) -> Result<String, CodecError> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut origins = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(CodecError::BadEscape { at: i });
            }
            let hi = hex_value(bytes[i + 1]);
            let lo = hex_value(bytes[i + 2]);
            match (hi, lo) {
                (Some(hi), Some(lo)) => {
                    decoded.push((hi << 4) | lo);
                    origins.push(i);
                    i += 3;
                }
                _ => return Err(CodecError::BadEscape { at: i }),
            }
        } else {
            decoded.push(bytes[i]);
            origins.push(i);
            i += 1;
        }
    }

    match std::str::from_utf8(&decoded) {
        Ok(s) => Ok(s.to_owned()),
        Err(e) => Err(CodecError::InvalidUtf8 {
            at: origins.get(e.valid_up_to()).copied().unwrap_or(input.len()),
        }),
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
            out.push(char::from(b));
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(char::from(HEX[(b >> 4) as usize]));
            out.push(char::from(HEX[(b & 0x0f) as usize]));
        }
    }
    out
}

fn invalid_raw_byte(input: &str) -> Option<usize> {
    input
        .as_bytes()
        .iter()
        .position(|&b| !(0x21..=0x7e).contains(&b))
}

fn codec_to_parse_error(err: CodecError, base: usize) -> ParseError {
    match err {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: base + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: base + at },
    }
}

fn validate_component(
    component: Component,
    text: &str,
    offset: usize,
) -> Result<(), ParseError> {
    decode_component(component, text)
        .map(|_| ())
        .map_err(|e| codec_to_parse_error(e, offset))
}

fn parse_port(port_text: &str, port_start: usize) -> Result<u16, ParseError> {
    if port_text.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    if let Some(i) = port_text.as_bytes().iter().position(|b| !b.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: port_start + i });
    }

    let bytes = port_text.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for &b in bytes {
        value = match value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
        {
            Some(v) => v,
            None => return Err(ParseError::InvalidPort { at: port_start }),
        };
    }
    if !(1..=65535).contains(&value) {
        return Err(ParseError::InvalidPort { at: port_start });
    }
    Ok(value as u16)
}

fn find_authority_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() && !matches!(bytes[i], b'/' | b'?' | b'#') {
        i += 1;
    }
    i
}

fn parse_url_inner(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    if let Some(at) = invalid_raw_byte(input) {
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

    let authority_marker = colon + 1;
    if authority_marker + 1 >= bytes.len()
        || bytes[authority_marker] != b'/'
        || bytes[authority_marker + 1] != b'/'
    {
        return Err(ParseError::MissingAuthority { at: authority_marker });
    }

    let authority_start = authority_marker + 2;
    let authority_end = find_authority_end(bytes, authority_start);
    let authority = &input[authority_start..authority_end];
    let authority_bytes = authority.as_bytes();

    let at_positions: Vec<usize> = authority_bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i))
        .collect();
    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }

    let (userinfo, hostport_start) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at,
            });
        }
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let hostport = &authority[hostport_start..];
    let host_start = authority_start + hostport_start;
    let mut port = None;
    let mut ipv6 = false;
    let host: String;

    if hostport.as_bytes().first() == Some(&b'[') {
        let close = match hostport.as_bytes().iter().position(|&b| b == b']') {
            Some(i) => i,
            None => return Err(ParseError::InvalidIpv6 { at: host_start }),
        };
        let interior = &hostport[1..close];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: host_start });
        }
        host = interior.to_ascii_lowercase();
        ipv6 = true;

        let suffix = &hostport[close + 1..];
        if !suffix.is_empty() {
            if suffix.as_bytes()[0] != b':' {
                return Err(ParseError::InvalidHost {
                    at: host_start + close + 1,
                });
            }
            let port_start = host_start + close + 2;
            port = Some(parse_port(&suffix[1..], port_start)?);
        }
    } else {
        let colon_in_hostport = hostport.as_bytes().iter().position(|&b| b == b':');
        let (host_text, port_text) = match colon_in_hostport {
            Some(i) => {
                let port_start = host_start + i + 1;
                ( &hostport[..i], Some((&hostport[i + 1..], port_start)) )
            }
            None => (hostport, None),
        };

        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: host_start });
        }
        for (i, &b) in host_text.as_bytes().iter().enumerate() {
            if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost { at: host_start + i });
            }
        }
        host = host_text.to_ascii_lowercase();

        if let Some((port_text, port_start)) = port_text {
            port = Some(parse_port(port_text, port_start)?);
        }
    }

    let mut fragment = None;
    let fragment_start = if let Some(hash_rel) = bytes[authority_end..]
        .iter()
        .position(|&b| b == b'#')
    {
        let hash = authority_end + hash_rel;
        fragment = Some(input[hash + 1..].to_owned());
        Some(hash)
    } else {
        None
    };

    let before_fragment_end = fragment_start.unwrap_or(bytes.len());
    let query_rel = bytes[authority_end..before_fragment_end]
        .iter()
        .position(|&b| b == b'?');
    let (path_end, query) = if let Some(rel) = query_rel {
        let q = authority_end + rel;
        (q, Some(input[q + 1..before_fragment_end].to_owned()))
    } else {
        (before_fragment_end, None)
    };

    let path = if path_end == authority_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };

    if let Some(userinfo) = &userinfo {
        validate_component(Component::UserInfo, userinfo, authority_start)?;
    }
    if path_end != authority_end {
        validate_component(Component::Path, &path, authority_end)?;
    }
    if let Some(query) = &query {
        let q_at = authority_end + query_rel.unwrap();
        validate_component(Component::Query, query, q_at + 1)?;
    }
    if let Some(fragment) = &fragment {
        let hash = fragment_start.unwrap();
        validate_component(Component::Fragment, fragment, hash + 1)?;
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

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    parse_url_inner(input)
}

pub fn normalise_path(path: &str) -> Result<String, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }

    let decoded = decode_component(Component::Path, path).map_err(|e| match e {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    })?;

    let raw_segments: Vec<&str> = path[1..].split('/').collect();
    let decoded_segments: Vec<&str> = decoded[1..].split('/').collect();
    let mut stack: Vec<&str> = Vec::new();

    for (i, raw) in raw_segments.iter().enumerate() {
        match decoded_segments.get(i).copied().unwrap_or("") {
            "." => {}
            ".." => {
                stack.pop();
            }
            _ => stack.push(*raw),
        }
    }

    if let Some(last) = decoded_segments.last() {
        if *last == "." || *last == ".." {
            stack.push("");
        }
    }

    let mut out = String::from("/");
    out.push_str(&stack.join("/"));
    Ok(out)
}

fn scan_reference_bytes(reference: &str) -> Result<(), ParseError> {
    if let Some(at) = invalid_raw_byte(reference) {
        Err(ParseError::InvalidByte { at })
    } else {
        Ok(())
    }
}

fn construct_absolute(base: &Url, path: &str, query: Option<&str>, fragment: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str(&base.scheme);
    out.push_str("://");
    if let Some(userinfo) = &base.userinfo {
        out.push_str(userinfo);
        out.push('@');
    }
    if base.ipv6 {
        out.push('[');
        out.push_str(&base.host);
        out.push(']');
    } else {
        out.push_str(&base.host);
    }
    if let Some(port) = base.port {
        out.push(':');
        out.push_str(&port.to_string());
    }
    out.push_str(path);
    if let Some(query) = query {
        out.push('?');
        out.push_str(query);
    }
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

fn normalize_url_path(mut url: Url) -> Result<Url, ResolveError> {
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let normalized_base_path =
        normalise_path(&base_url.path).map_err(ResolveError::Path)?;

    scan_reference_bytes(reference).map_err(ResolveError::Reference)?;

    let bytes = reference.as_bytes();
    let first_delim = bytes
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(bytes.len());

    if bytes[..first_delim].contains(&b':') {
        let absolute = parse_url(reference).map_err(ResolveError::Reference)?;
        return normalize_url_path(absolute);
    }

    if reference.starts_with("//") {
        let constructed = format!("{}:{}", base_url.scheme, reference);
        let parsed = parse_url(&constructed).map_err(ResolveError::Reference)?;
        return normalize_url_path(parsed);
    }

    let hash = bytes.iter().position(|&b| b == b'#');
    let before_fragment_end = hash.unwrap_or(bytes.len());
    let query_rel = bytes[..before_fragment_end]
        .iter()
        .position(|&b| b == b'?');
    let (path_end, reference_query) = if let Some(q) = query_rel {
        (q, Some(&reference[q + 1..before_fragment_end]))
    } else {
        (before_fragment_end, None)
    };
    let reference_path = &reference[..path_end];
    let reference_fragment = hash.map(|h| &reference[h + 1..]);

    let (selected_path, selected_query) = if reference_path.is_empty() {
        (
            normalized_base_path.clone(),
            if query_rel.is_some() {
                reference_query
            } else {
                base_url.query.as_deref()
            },
        )
    } else {
        let merged = if reference_path.starts_with('/') {
            reference_path.to_owned()
        } else {
            let slash = normalized_base_path.rfind('/').unwrap_or(0);
            let mut value = normalized_base_path[..=slash].to_owned();
            value.push_str(reference_path);
            value
        };
        (merged, reference_query)
    };

    let constructed = construct_absolute(
        &base_url,
        &selected_path,
        selected_query,
        reference_fragment,
    );
    let parsed = parse_url(&constructed).map_err(ResolveError::Reference)?;
    normalize_url_path(parsed)
}