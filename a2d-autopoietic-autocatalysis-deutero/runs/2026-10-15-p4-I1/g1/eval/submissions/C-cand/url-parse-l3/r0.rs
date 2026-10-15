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

fn validate_global_bytes(input: &str) -> Result<(), ParseError> {
    for (i, b) in input.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(ParseError::InvalidByte { at: i });
        }
    }
    Ok(())
}

pub fn decode_component(_component: Component, input: &str) -> Result<String, CodecError> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut source_offsets = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() || !is_hex(bytes[i + 1]) || !is_hex(bytes[i + 2]) {
                return Err(CodecError::BadEscape { at: i });
            }
            decoded.push((hex_value(bytes[i + 1]) << 4) | hex_value(bytes[i + 2]));
            source_offsets.push(i);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            source_offsets.push(i);
            i += 1;
        }
    }

    match std::str::from_utf8(&decoded) {
        Ok(s) => Ok(s.to_owned()),
        Err(e) => Err(CodecError::InvalidUtf8 {
            at: source_offsets.get(e.valid_up_to()).copied().unwrap_or(input.len()),
        }),
    }
}

fn component_safe(component: Component, b: u8) -> bool {
    if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
        return true;
    }
    match component {
        Component::UserInfo => b == b':',
        Component::Path => b == b'/',
        Component::Query => matches!(b, b'/' | b'?'),
        Component::Fragment => matches!(b, b'/' | b'?' | b'#'),
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &b in text.as_bytes() {
        if component_safe(component, b) {
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

fn validate_component(
    component: Component,
    text: &str,
    original_offset: usize,
) -> Result<(), ParseError> {
    match decode_component(component, text) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => {
            Err(ParseError::BadEscape { at: original_offset + at })
        }
        Err(CodecError::InvalidUtf8 { at }) => {
            Err(ParseError::InvalidUtf8 { at: original_offset + at })
        }
    }
}

fn parse_port(text: &str, start: usize) -> Result<u16, ParseError> {
    if text.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }
    for (i, b) in text.bytes().enumerate() {
        if !b.is_ascii_digit() {
            return Err(ParseError::InvalidPort { at: start + i });
        }
    }
    if text.len() >= 2 && text.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }

    let mut value: u32 = 0;
    for b in text.bytes() {
        value = match value.checked_mul(10).and_then(|v| v.checked_add((b - b'0') as u32)) {
            Some(v) => v,
            None => return Err(ParseError::InvalidPort { at: start }),
        };
        if value > 65535 {
            return Err(ParseError::InvalidPort { at: start });
        }
    }
    if value == 0 {
        return Err(ParseError::InvalidPort { at: start });
    }
    Ok(value as u16)
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    validate_global_bytes(input)?;

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

    let at_positions: Vec<usize> = authority
        .bytes()
        .enumerate()
        .filter_map(|(i, b)| (b == b'@').then_some(i))
        .collect();
    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }

    let (userinfo, hostport, hostport_start) = if let Some(&at) = at_positions.first() {
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
            None => {
                return Err(ParseError::InvalidIpv6 {
                    at: hostport_start,
                })
            }
        };
        let interior = &hostport[1..close];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 {
                at: hostport_start,
            });
        }
        ipv6 = true;
        let suffix = &hostport[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(parse_port(&suffix[1..], hostport_start + close + 2)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_start + close + 1,
            });
        };
        (interior.to_ascii_lowercase(), port)
    } else {
        let split = hostport.find(':');
        let (host_text, port) = match split {
            Some(i) => (
                &hostport[..i],
                Some(parse_port(&hostport[i + 1..], hostport_start + i + 1)?),
            ),
            None => (hostport, None),
        };
        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_start });
        }
        for (i, b) in host_text.bytes().enumerate() {
            if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: hostport_start + i,
                });
            }
        }
        (host_text.to_ascii_lowercase(), port)
    };

    let fragment_start = input[authority_end..]
        .find('#')
        .map(|i| authority_end + i);
    let before_fragment_end = fragment_start.unwrap_or(input.len());
    let query_start = input[authority_end..before_fragment_end]
        .find('?')
        .map(|i| authority_end + i);

    let path_end = query_start.unwrap_or(before_fragment_end);
    let path = if authority_end == path_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };
    let query = query_start.map(|start| input[start + 1..before_fragment_end].to_owned());
    let fragment = fragment_start.map(|start| input[start + 1..].to_owned());

    if let Some(ref info) = userinfo {
        validate_component(Component::UserInfo, info, authority_start)?;
    }
    if authority_end != path_end {
        validate_component(Component::Path, &path, authority_end)?;
    }
    if let Some(start) = query_start {
        validate_component(
            Component::Query,
            query.as_deref().unwrap_or(""),
            start + 1,
        )?;
    }
    if let Some(start) = fragment_start {
        validate_component(
            Component::Fragment,
            fragment.as_deref().unwrap_or(""),
            start + 1,
        )?;
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

    let mut stack: Vec<&str> = Vec::new();
    let segments: Vec<&str> = path[1..].split('/').collect();
    let mut terminal_navigation = false;

    for (i, segment) in segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, segment).map_err(|e| match e {
            CodecError::BadEscape { at } => PathError::BadEscape { at: at + 1 + segment_offset(&segments, i) },
            CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at: at + 1 + segment_offset(&segments, i) },
        })?;
        if decoded == "." {
            terminal_navigation = i + 1 == segments.len();
        } else if decoded == ".." {
            stack.pop();
            terminal_navigation = i + 1 == segments.len();
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

fn segment_offset(segments: &[&str], index: usize) -> usize {
    let mut offset = 0;
    for segment in segments.iter().take(index) {
        offset += segment.len() + 1;
    }
    offset
}

fn scan_reference_bytes(reference: &str) -> Result<(), ResolveError> {
    for (i, b) in reference.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(ResolveError::Reference(ParseError::InvalidByte { at: i }));
        }
    }
    Ok(())
}

fn normalize_url_path(mut url: Url) -> Result<Url, ResolveError> {
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

fn append_optional(out: &mut String, marker: char, value: Option<&str>) {
    if let Some(value) = value {
        out.push(marker);
        out.push_str(value);
    }
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let base_url = normalize_url_path(base_url).map_err(|e| match e {
        ResolveError::Path(p) => ResolveError::Path(p),
        _ => unreachable!(),
    })?;
    scan_reference_bytes(reference)?;

    let first_delimiter = reference
        .bytes()
        .position(|b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(reference.len());
    if reference[..first_delimiter].contains(':') {
        let url = parse_url(reference).map_err(ResolveError::Reference)?;
        return normalize_url_path(url);
    }

    if reference.starts_with("//") {
        let absolute = format!("{}:{}", base_url.scheme, reference);
        let url = parse_url(&absolute).map_err(ResolveError::Reference)?;
        return normalize_url_path(url);
    }

    let fragment_start = reference.find('#');
    let before_fragment = fragment_start.unwrap_or(reference.len());
    let query_start = reference[..before_fragment].find('?');
    let path_end = query_start.unwrap_or(before_fragment);
    let ref_path = &reference[..path_end];
    let ref_query = query_start.map(|i| reference[i + 1..before_fragment].to_owned());
    let ref_fragment = fragment_start.map(|i| reference[i + 1..].to_owned());

    let (selected_path, selected_query) = if ref_path.is_empty() {
        (
            base_url.path.clone(),
            if query_start.is_some() {
                ref_query
            } else {
                base_url.query.clone()
            },
        )
    } else {
        let merged = if ref_path.starts_with('/') {
            ref_path.to_owned()
        } else {
            let slash = base_url.path.rfind('/').map(|i| i + 1).unwrap_or(0);
            format!("{}{}", &base_url.path[..slash], ref_path)
        };
        (merged, ref_query)
    };

    let mut absolute = format!("{}://", base_url.scheme);
    if let Some(userinfo) = &base_url.userinfo {
        absolute.push_str(userinfo);
        absolute.push('@');
    }
    if base_url.ipv6 {
        absolute.push('[');
        absolute.push_str(&base_url.host);
        absolute.push(']');
    } else {
        absolute.push_str(&base_url.host);
    }
    if let Some(port) = base_url.port {
        absolute.push(':');
        absolute.push_str(&port.to_string());
    }
    absolute.push_str(&selected_path);
    append_optional(&mut absolute, '?', selected_query.as_deref());
    append_optional(&mut absolute, '#', ref_fragment.as_deref());

    let url = parse_url(&absolute).map_err(ResolveError::Reference)?;
    normalize_url_path(url)
}

fn main() {}