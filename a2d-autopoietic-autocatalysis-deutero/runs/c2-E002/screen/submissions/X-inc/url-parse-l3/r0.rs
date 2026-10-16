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

fn scan_bytes(input: &str) -> Result<(), ParseError> {
    for (at, byte) in input.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(ParseError::InvalidByte { at });
        }
    }
    Ok(())
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
        Ok(text) => Ok(text.to_owned()),
        Err(error) => {
            let decoded_at = error.valid_up_to();
            let original_at = origins.get(decoded_at).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at: original_at })
        }
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    for byte in text.bytes() {
        let safe = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~')
            || match component {
                Component::UserInfo => byte == b':',
                Component::Path => byte == b'/',
                Component::Query => matches!(byte, b'/' | b'?'),
                Component::Fragment => matches!(byte, b'/' | b'?' | b'#'),
            };

        if safe {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }

    encoded
}

fn map_component_error(error: CodecError, offset: usize) -> ParseError {
    match error {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: offset + at },
    }
}

fn validate_component(
    component: Component,
    text: &str,
    offset: usize,
) -> Result<(), ParseError> {
    decode_component(component, text).map(|_| ()).map_err(|e| map_component_error(e, offset))
}

fn parse_port(port_text: &str, port_start: usize) -> Result<u16, ParseError> {
    if port_text.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    for (i, byte) in port_text.bytes().enumerate() {
        if !byte.is_ascii_digit() {
            return Err(ParseError::InvalidPort {
                at: port_start + i,
            });
        }
    }

    if port_text.len() >= 2 && port_text.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for byte in port_text.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::InvalidPort { at: port_start })?;
    }

    if !(1..=65535).contains(&value) {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    Ok(value as u16)
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    scan_bytes(input)?;

    let bytes = input.as_bytes();
    let colon = match bytes.iter().position(|&b| b == b':') {
        Some(colon) => colon,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };

    if colon == 0 || !bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }

    for (i, &byte) in bytes[..colon].iter().enumerate().skip(1) {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')) {
            return Err(ParseError::InvalidScheme { at: i });
        }
    }

    if bytes.get(colon + 1) != Some(&b'/') || bytes.get(colon + 2) != Some(&b'/') {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let scheme = input[..colon].to_ascii_lowercase();
    let authority_start = colon + 3;
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map(|relative| authority_start + relative)
        .unwrap_or(input.len());
    let authority = &input[authority_start..authority_end];

    let at_positions: Vec<usize> = authority
        .bytes()
        .enumerate()
        .filter_map(|(i, byte)| (byte == b'@').then_some(i))
        .collect();

    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }

    let (userinfo, userinfo_offset, hostport_offset, hostport) =
        if let Some(&at) = at_positions.first() {
            if at == 0 {
                return Err(ParseError::InvalidUserInfo {
                    at: authority_start,
                });
            }
            (
                Some(authority[..at].to_owned()),
                Some(authority_start),
                authority_start + at + 1,
                &authority[at + 1..],
            )
        } else {
            (None, None, authority_start, authority)
        };

    let mut ipv6 = false;
    let (host, port) = if hostport.starts_with('[') {
        let close = match hostport.find(']') {
            Some(index) => index,
            None => {
                return Err(ParseError::InvalidIpv6 {
                    at: hostport_offset,
                })
            }
        };
        let interior = &hostport[1..close];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 {
                at: hostport_offset,
            });
        }

        let suffix = &hostport[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(parse_port(&suffix[1..], hostport_offset + close + 2)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_offset + close + 1,
            });
        };

        ipv6 = true;
        (interior.to_ascii_lowercase(), port)
    } else {
        let host_colon = hostport.find(':');
        let (host_text, port) = match host_colon {
            Some(index) => (
                &hostport[..index],
                Some(parse_port(
                    &hostport[index + 1..],
                    hostport_offset + index + 1,
                )?),
            ),
            None => (hostport, None),
        };

        if host_text.is_empty() {
            return Err(ParseError::InvalidHost {
                at: hostport_offset,
            });
        }

        for (i, byte) in host_text.bytes().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: hostport_offset + i,
                });
            }
        }

        (host_text.to_ascii_lowercase(), port)
    };

    let fragment_pos = input[authority_end..]
        .find('#')
        .map(|relative| authority_end + relative);
    let before_fragment_end = fragment_pos.unwrap_or(input.len());

    let query_pos = input[authority_end..before_fragment_end]
        .find('?')
        .map(|relative| authority_end + relative);

    let path_end = query_pos.unwrap_or(before_fragment_end);
    let raw_path = &input[authority_end..path_end];
    let path = if raw_path.is_empty() {
        "/".to_owned()
    } else {
        raw_path.to_owned()
    };

    let query = query_pos.map(|start| {
        let end = before_fragment_end;
        input[start + 1..end].to_owned()
    });
    let fragment = fragment_pos.map(|start| input[start + 1..].to_owned());

    if let (Some(userinfo), Some(offset)) = (&userinfo, userinfo_offset) {
        validate_component(Component::UserInfo, userinfo, offset)?;
    }
    if !raw_path.is_empty() {
        validate_component(Component::Path, raw_path, authority_end)?;
    }
    if let Some(start) = query_pos {
        let end = before_fragment_end;
        validate_component(Component::Query, &input[start + 1..end], start + 1)?;
    }
    if let Some(start) = fragment_pos {
        validate_component(Component::Fragment, &input[start + 1..], start + 1)?;
    }

    Ok(Url {
        scheme,
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

    decode_component(Component::Path, path).map_err(|error| match error {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    })?;

    let mut stack: Vec<&str> = Vec::new();
    let segments: Vec<&str> = path[1..].split('/').collect();
    let mut final_navigation = false;

    for (index, segment) in segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, segment).map_err(|error| match error {
            CodecError::BadEscape { at } => PathError::BadEscape { at: at + 1 },
            CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at: at + 1 },
        })?;

        let is_dot = decoded == ".";
        let is_dotdot = decoded == "..";
        final_navigation = index + 1 == segments.len() && (is_dot || is_dotdot);

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
        let mut result = String::from("/");
        result.push_str(&stack.join("/"));
        Ok(result)
    }
}

fn construct_absolute_url(
    scheme: &str,
    userinfo: Option<&str>,
    host: &str,
    ipv6: bool,
    port: Option<u16>,
    path: &str,
    query: Option<&str>,
    fragment: Option<&str>,
) -> String {
    let mut result = String::new();
    result.push_str(scheme);
    result.push_str("://");

    if let Some(userinfo) = userinfo {
        result.push_str(userinfo);
        result.push('@');
    }

    if ipv6 {
        result.push('[');
        result.push_str(host);
        result.push(']');
    } else {
        result.push_str(host);
    }

    if let Some(port) = port {
        result.push(':');
        result.push_str(&port.to_string());
    }

    result.push_str(path);
    if let Some(query) = query {
        result.push('?');
        result.push_str(query);
    }
    if let Some(fragment) = fragment {
        result.push('#');
        result.push_str(fragment);
    }

    result
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let base_path = normalise_path(&base_url.path).map_err(ResolveError::Path)?;
    scan_bytes(reference).map_err(ResolveError::Reference)?;

    let delimiter_end = reference
        .bytes()
        .position(|byte| matches!(byte, b'/' | b'?' | b'#'))
        .unwrap_or(reference.len());
    let has_scheme = reference[..delimiter_end].contains(':');

    if has_scheme {
        let mut url = parse_url(reference).map_err(ResolveError::Reference)?;
        url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
        return Ok(url);
    }

    if reference.starts_with("//") {
        let absolute = format!("{}:{}", base_url.scheme, reference);
        let mut url = parse_url(&absolute).map_err(ResolveError::Reference)?;
        url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
        return Ok(url);
    }

    let fragment_pos = reference.find('#');
    let before_fragment_end = fragment_pos.unwrap_or(reference.len());
    let query_pos = reference[..before_fragment_end].find('?');

    let path_end = query_pos.unwrap_or(before_fragment_end);
    let reference_path = &reference[..path_end];
    let query = query_pos.map(|start| {
        reference[start + 1..before_fragment_end].to_owned()
    });
    let fragment = fragment_pos.map(|start| reference[start + 1..].to_owned());

    let (selected_path, selected_query) = if reference_path.is_empty() {
        (
            base_path.clone(),
            if query_pos.is_some() {
                query
            } else {
                base_url.query.clone()
            },
        )
    } else {
        let merged = if reference_path.starts_with('/') {
            reference_path.to_owned()
        } else {
            let slash = base_path.rfind('/').unwrap_or(0);
            let mut merged = base_path[..=slash].to_owned();
            merged.push_str(reference_path);
            merged
        };
        (merged, query)
    };

    let absolute = construct_absolute_url(
        &base_url.scheme,
        base_url.userinfo.as_deref(),
        &base_url.host,
        base_url.ipv6,
        base_url.port,
        &selected_path,
        selected_query.as_deref(),
        fragment.as_deref(),
    );

    let mut url = parse_url(&absolute).map_err(ResolveError::Reference)?;
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}