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
    for (i, byte) in input.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(ParseError::InvalidByte { at: i });
        }
    }
    Ok(())
}

fn is_scheme_first(byte: u8) -> bool {
    byte.is_ascii_alphabetic()
}

fn is_scheme_rest(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')
}

fn validate_port(port: &str, port_start: usize) -> Result<Option<u16>, ParseError> {
    if port.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    for (i, byte) in port.bytes().enumerate() {
        if !byte.is_ascii_digit() {
            return Err(ParseError::InvalidPort {
                at: port_start + i,
            });
        }
    }

    if port.len() >= 2 && port.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for byte in port.bytes() {
        value = match value
            .checked_mul(10)
            .and_then(|n| n.checked_add((byte - b'0') as u32))
        {
            Some(n) => n,
            None => return Err(ParseError::InvalidPort { at: port_start }),
        };
        if value > u16::MAX as u32 {
            return Err(ParseError::InvalidPort { at: port_start });
        }
    }

    if value == 0 {
        return Err(ParseError::InvalidPort { at: port_start });
    }
    Ok(Some(value as u16))
}

fn codec_to_parse(error: CodecError, offset: usize) -> ParseError {
    match error {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: offset + at },
    }
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    scan_bytes(input)?;

    let bytes = input.as_bytes();
    let colon = match bytes.iter().position(|&b| b == b':') {
        Some(i) => i,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };

    if colon == 0 || !is_scheme_first(bytes[0]) {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for (i, &byte) in bytes[..colon].iter().enumerate().skip(1) {
        if !is_scheme_rest(byte) {
            return Err(ParseError::InvalidScheme { at: i });
        }
    }

    if bytes.get(colon + 1) != Some(&b'/') || bytes.get(colon + 2) != Some(&b'/') {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let authority_start = colon + 3;
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map(|i| authority_start + i)
        .unwrap_or(input.len());
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

    let (userinfo, host_start_local) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start,
            });
        }
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let host_start = authority_start + host_start_local;
    let host_port = &authority[host_start_local..];
    let host_port_bytes = host_port.as_bytes();

    let (host, port, ipv6) = if host_port.starts_with('[') {
        let close = match host_port_bytes.iter().position(|&b| b == b']') {
            Some(i) => i,
            None => return Err(ParseError::InvalidIpv6 { at: host_start }),
        };
        let interior = &host_port[1..close];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: host_start });
        }
        let suffix_start = close + 1;
        let suffix = &host_port[suffix_start..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            validate_port(
                &suffix[1..],
                host_start + suffix_start + 1,
            )?
        } else {
            return Err(ParseError::InvalidHost {
                at: host_start + suffix_start,
            });
        };
        (interior.to_ascii_lowercase(), port, true)
    } else {
        let split = host_port_bytes.iter().position(|&b| b == b':');
        let (host_text, port) = if let Some(i) = split {
            let port = validate_port(&host_port[i + 1..], host_start + i + 1)?;
            (&host_port[..i], port)
        } else {
            (host_port, None)
        };

        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: host_start });
        }
        for (i, byte) in host_text.bytes().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: host_start + i,
                });
            }
        }
        (host_text.to_ascii_lowercase(), port, false)
    };

    let fragment_mark = bytes[authority_end..]
        .iter()
        .position(|&b| b == b'#')
        .map(|i| authority_end + i);

    let before_fragment_end = fragment_mark.unwrap_or(input.len());
    let query_mark = bytes[authority_end..before_fragment_end]
        .iter()
        .position(|&b| b == b'?')
        .map(|i| authority_end + i);

    let path_end = query_mark.unwrap_or(before_fragment_end);
    let raw_path = &input[authority_end..path_end];
    let path = if raw_path.is_empty() {
        "/".to_owned()
    } else {
        raw_path.to_owned()
    };

    let query = query_mark.map(|mark| input[mark + 1..before_fragment_end].to_owned());
    let fragment = fragment_mark.map(|mark| input[mark + 1..].to_owned());

    if let Some(info) = userinfo.as_deref() {
        decode_component(Component::UserInfo, info)
            .map_err(|e| codec_to_parse(e, authority_start))?;
    }
    if !raw_path.is_empty() {
        decode_component(Component::Path, raw_path)
            .map_err(|e| codec_to_parse(e, authority_end))?;
    }
    if let (Some(mark), Some(q)) = (query_mark, query.as_deref()) {
        decode_component(Component::Query, q)
            .map_err(|e| codec_to_parse(e, mark + 1))?;
    }
    if let (Some(mark), Some(f)) = (fragment_mark, fragment.as_deref()) {
        decode_component(Component::Fragment, f)
            .map_err(|e| codec_to_parse(e, mark + 1))?;
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
    let mut offsets = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(CodecError::BadEscape { at: i });
            }
            let high = match hex_value(bytes[i + 1]) {
                Some(v) => v,
                None => return Err(CodecError::BadEscape { at: i }),
            };
            let low = match hex_value(bytes[i + 2]) {
                Some(v) => v,
                None => return Err(CodecError::BadEscape { at: i }),
            };
            decoded.push((high << 4) | low);
            offsets.push(i);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            offsets.push(i);
            i += 1;
        }
    }

    match std::str::from_utf8(&decoded) {
        Ok(text) => Ok(text.to_owned()),
        Err(error) => {
            let decoded_offset = error.valid_up_to();
            let original_offset = offsets.get(decoded_offset).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at: original_offset })
        }
    }
}

fn component_byte_safe(component: Component, byte: u8) -> bool {
    if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
        return true;
    }
    match component {
        Component::UserInfo => byte == b':',
        Component::Path => byte == b'/',
        Component::Query => matches!(byte, b'/' | b'?'),
        Component::Fragment => matches!(byte, b'/' | b'?' | b'#'),
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        if component_byte_safe(component, byte) {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
}

pub fn normalise_path(path: &str) -> Result<String, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }
    decode_component(Component::Path, path).map_err(|error| match error {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    })?;

    let segments: Vec<&str> = path[1..].split('/').collect();
    let mut stack: Vec<&str> = Vec::new();
    let mut terminal_navigation = false;

    for (index, segment) in segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, segment).map_err(|error| match error {
            CodecError::BadEscape { at } => PathError::BadEscape { at: at + 1 + segments[..index].iter().map(|s| s.len() + 1).sum::<usize>() },
            CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at: at + 1 + segments[..index].iter().map(|s| s.len() + 1).sum::<usize>() },
        })?;

        let is_last = index + 1 == segments.len();
        if decoded == "." {
            terminal_navigation = is_last;
        } else if decoded == ".." {
            if !stack.is_empty() {
                stack.pop();
            }
            terminal_navigation = is_last;
        } else {
            stack.push(segment);
            terminal_navigation = false;
        }
    }

    if terminal_navigation {
        stack.push("");
    }

    if stack.is_empty() {
        Ok("/".to_owned())
    } else {
        Ok(format!("/{}", stack.join("/")))
    }
}

fn split_reference(reference: &str) -> (&str, Option<&str>, Option<&str>) {
    let bytes = reference.as_bytes();
    let fragment_mark = bytes.iter().position(|&b| b == b'#');
    let before_fragment_end = fragment_mark.unwrap_or(reference.len());
    let query_mark = bytes[..before_fragment_end].iter().position(|&b| b == b'?');
    let path_end = query_mark.unwrap_or(before_fragment_end);

    let path = &reference[..path_end];
    let query = query_mark.map(|i| &reference[i + 1..before_fragment_end]);
    let fragment = fragment_mark.map(|i| &reference[i + 1..]);
    (path, query, fragment)
}

fn parse_reference_url(text: &str) -> Result<Url, ResolveError> {
    parse_url(text).map_err(ResolveError::Reference)
}

fn with_normalized_path(mut url: Url) -> Result<Url, ResolveError> {
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let base_url = with_normalized_path(base_url).map_err(|e| match e {
        ResolveError::Path(path) => ResolveError::Path(path),
        other => other,
    })?;

    scan_bytes(reference).map_err(ResolveError::Reference)?;

    let first_delimiter = reference
        .bytes()
        .position(|b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(reference.len());
    if reference[..first_delimiter].contains(':') {
        let absolute = parse_reference_url(reference)?;
        return with_normalized_path(absolute);
    }

    if reference.starts_with("//") {
        let constructed = format!("{}:{}", base_url.scheme, reference);
        let network = parse_reference_url(&constructed)?;
        return with_normalized_path(network);
    }

    let (reference_path, reference_query, reference_fragment) = split_reference(reference);

    let selected_path = if reference_path.is_empty() {
        base_url.path.clone()
    } else if reference_path.starts_with('/') {
        reference_path.to_owned()
    } else {
        let slash = base_url.path.rfind('/').map(|i| i + 1).unwrap_or(0);
        format!("{}{}", &base_url.path[..slash], reference_path)
    };

    let selected_query = if reference_path.is_empty() && reference_query.is_none() {
        base_url.query.as_deref()
    } else {
        reference_query
    };

    let mut constructed = format!("{}://", base_url.scheme);
    if let Some(userinfo) = base_url.userinfo.as_deref() {
        constructed.push_str(userinfo);
        constructed.push('@');
    }
    if base_url.ipv6 {
        constructed.push('[');
        constructed.push_str(&base_url.host);
        constructed.push(']');
    } else {
        constructed.push_str(&base_url.host);
    }
    if let Some(port) = base_url.port {
        constructed.push(':');
        constructed.push_str(&port.to_string());
    }
    constructed.push_str(&selected_path);
    if let Some(query) = selected_query {
        constructed.push('?');
        constructed.push_str(query);
    }
    if let Some(fragment) = reference_fragment {
        constructed.push('#');
        constructed.push_str(fragment);
    }

    let resolved = parse_reference_url(&constructed)?;
    with_normalized_path(resolved)
}

fn main() {}