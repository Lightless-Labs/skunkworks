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

fn invalid_raw_byte(input: &str) -> Option<usize> {
    input
        .as_bytes()
        .iter()
        .position(|&b| !(0x21..=0x7e).contains(&b))
}

fn is_scheme_first(b: u8) -> bool {
    b.is_ascii_alphabetic()
}

fn is_scheme_rest(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')
}

fn parse_port(port: &str, offset: usize) -> Result<u16, ParseError> {
    if port.is_empty() {
        return Err(ParseError::InvalidPort { at: offset });
    }

    if let Some(i) = port.bytes().position(|b| !b.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: offset + i });
    }

    if port.len() >= 2 && port.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: offset });
    }

    let mut value: u32 = 0;
    for b in port.bytes() {
        value = match value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
        {
            Some(v) => v,
            None => return Err(ParseError::InvalidPort { at: offset }),
        };
        if value > 65535 {
            return Err(ParseError::InvalidPort { at: offset });
        }
    }

    if value == 0 {
        return Err(ParseError::InvalidPort { at: offset });
    }
    Ok(value as u16)
}

fn codec_to_parse(error: CodecError, offset: usize) -> ParseError {
    match error {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: offset + at },
    }
}

fn validate_component(s: &str, offset: usize) -> Result<(), ParseError> {
    decode_component(Component::Path, s).map(|_| ()).map_err(|e| codec_to_parse(e, offset))
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
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
    if colon == 0 || !is_scheme_first(bytes[0]) {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    if let Some(i) = bytes[1..colon]
        .iter()
        .position(|&b| !is_scheme_rest(b))
    {
        return Err(ParseError::InvalidScheme { at: i + 1 });
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

    let ats: Vec<usize> = authority
        .bytes()
        .enumerate()
        .filter_map(|(i, b)| (b == b'@').then_some(i))
        .collect();
    if ats.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + ats[1],
        });
    }
    let (userinfo, hostport_start) = if let Some(&at) = ats.first() {
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
    let hostport_offset = authority_start + hostport_start;

    let (host, port, ipv6) = if hostport.starts_with('[') {
        let close = match hostport.as_bytes()[1..].iter().position(|&b| b == b']') {
            Some(i) => i + 1,
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
        (interior.to_ascii_lowercase(), port, true)
    } else {
        let split = hostport.find(':');
        let (host_text, port_text) = match split {
            Some(i) => (&hostport[..i], Some((&hostport[i + 1..], i))),
            None => (hostport, None),
        };
        if host_text.is_empty() {
            return Err(ParseError::InvalidHost {
                at: hostport_offset,
            });
        }
        if let Some(i) = host_text.bytes().position(|b| {
            !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
        }) {
            return Err(ParseError::InvalidHost {
                at: hostport_offset + i,
            });
        }
        let port = match port_text {
            Some((p, i)) => Some(parse_port(p, hostport_offset + i + 1)?),
            None => None,
        };
        (host_text.to_ascii_lowercase(), port, false)
    };

    let fragment_delim = input[authority_end..].find('#').map(|i| authority_end + i);
    let before_fragment_end = fragment_delim.unwrap_or(input.len());
    let query_delim = input[authority_end..before_fragment_end]
        .find('?')
        .map(|i| authority_end + i);

    let path_end = query_delim.unwrap_or(before_fragment_end);
    let path = if path_end == authority_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };
    let query = query_delim.map(|q| input[q + 1..before_fragment_end].to_owned());
    let fragment = fragment_delim.map(|f| input[f + 1..].to_owned());

    if let Some(ref info) = userinfo {
        validate_component(info, authority_start)?;
    }
    if path != "/" || path_end != authority_end {
        validate_component(&path, authority_end)?;
    }
    if let Some(q) = query_delim {
        validate_component(query.as_deref().unwrap_or(""), q + 1)?;
    }
    if let Some(f) = fragment_delim {
        validate_component(fragment.as_deref().unwrap_or(""), f + 1)?;
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

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
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
            let hi = match hex_value(bytes[i + 1]) {
                Some(v) => v,
                None => return Err(CodecError::BadEscape { at: i }),
            };
            let lo = match hex_value(bytes[i + 2]) {
                Some(v) => v,
                None => return Err(CodecError::BadEscape { at: i }),
            };
            decoded.push((hi << 4) | lo);
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
        Err(e) => Err(CodecError::InvalidUtf8 {
            at: origins.get(e.valid_up_to()).copied().unwrap_or(input.len()),
        }),
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

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
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
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
    let terminal_navigation = decoded_segments
        .last()
        .map(|s| *s == "." || *s == "..")
        .unwrap_or(false);

    let mut stack: Vec<&str> = Vec::new();
    for (raw, dec) in raw_segments.iter().zip(decoded_segments.iter()) {
        match *dec {
            "." => {}
            ".." => {
                stack.pop();
            }
            _ => stack.push(raw),
        }
    }
    if terminal_navigation {
        stack.push("");
    }
    if stack.is_empty() {
        return Ok("/".to_owned());
    }
    Ok(format!("/{}", stack.join("/")))
}

fn scan_reference(reference: &str) -> Result<(), ParseError> {
    if let Some(at) = invalid_raw_byte(reference) {
        Err(ParseError::InvalidByte { at })
    } else {
        Ok(())
    }
}

fn parse_reference_components(reference: &str) -> (String, Option<String>, Option<String>) {
    let fragment_at = reference.find('#');
    let before_fragment = fragment_at.unwrap_or(reference.len());
    let query_at = reference[..before_fragment].find('?');
    let path_end = query_at.unwrap_or(before_fragment);

    let path = reference[..path_end].to_owned();
    let query = query_at.map(|q| reference[q + 1..before_fragment].to_owned());
    let fragment = fragment_at.map(|f| reference[f + 1..].to_owned());
    (path, query, fragment)
}

fn rebuild_url(
    base: &Url,
    path: &str,
    query: Option<&str>,
    fragment: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str(&base.scheme);
    out.push_str("://");
    if let Some(info) = &base.userinfo {
        out.push_str(info);
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
    if let Some(q) = query {
        out.push('?');
        out.push_str(q);
    }
    if let Some(f) = fragment {
        out.push('#');
        out.push_str(f);
    }
    out
}

fn normalize_resolved(mut url: Url) -> Result<Url, ResolveError> {
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let base_path = normalise_path(&base_url.path).map_err(ResolveError::Path)?;

    scan_reference(reference).map_err(ResolveError::Reference)?;

    let first_delim = reference
        .bytes()
        .position(|b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(reference.len());
    if reference[..first_delim].contains(':') {
        let url = parse_url(reference).map_err(ResolveError::Reference)?;
        return normalize_resolved(url);
    }

    if reference.starts_with("//") {
        let absolute = format!("{}:{}", base_url.scheme, reference);
        let url = parse_url(&absolute).map_err(ResolveError::Reference)?;
        return normalize_resolved(url);
    }

    let (ref_path, ref_query, ref_fragment) = parse_reference_components(reference);
    let (path, query) = if ref_path.is_empty() {
        (
            base_path,
            if ref_query.is_some() {
                ref_query
            } else {
                base_url.query.clone()
            },
        )
    } else {
        let merged = if ref_path.starts_with('/') {
            ref_path
        } else {
            let slash = base_path.rfind('/').map(|i| i + 1).unwrap_or(0);
            format!("{}{}", &base_path[..slash], ref_path)
        };
        (merged, ref_query)
    };

    let constructed = rebuild_url(
        &base_url,
        &path,
        query.as_deref(),
        ref_fragment.as_deref(),
    );
    let url = parse_url(&constructed).map_err(ResolveError::Reference)?;
    normalize_resolved(url)
}