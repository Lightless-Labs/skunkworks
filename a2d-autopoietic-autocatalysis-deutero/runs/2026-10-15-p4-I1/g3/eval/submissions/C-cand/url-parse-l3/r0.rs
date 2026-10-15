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

fn first_invalid_byte(input: &str) -> Option<usize> {
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

fn port_value(raw: &str, at: usize) -> Result<u16, ParseError> {
    if raw.is_empty() {
        return Err(ParseError::InvalidPort { at });
    }
    if let Some(i) = raw.bytes().position(|b| !b.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: at + i });
    }
    if raw.len() >= 2 && raw.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at });
    }

    let mut value: u32 = 0;
    for b in raw.bytes() {
        value = match value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
        {
            Some(v) => v,
            None => return Err(ParseError::InvalidPort { at }),
        };
        if value > 65535 {
            return Err(ParseError::InvalidPort { at });
        }
    }
    if value == 0 {
        return Err(ParseError::InvalidPort { at });
    }
    Ok(value as u16)
}

fn validate_component(
    component: Component,
    raw: &str,
    offset: usize,
) -> Result<(), ParseError> {
    match decode_component(component, raw) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape { at: offset + at }),
        Err(CodecError::InvalidUtf8 { at }) => {
            Err(ParseError::InvalidUtf8 { at: offset + at })
        }
    }
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    if let Some(at) = first_invalid_byte(input) {
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
    let auth_bytes = authority.as_bytes();

    let at_positions: Vec<usize> = auth_bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b == b'@' { Some(i) } else { None })
        .collect();

    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }
    let (userinfo, host_start_rel) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at,
            });
        }
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let host_start = authority_start + host_start_rel;
    let host_port = &authority[host_start_rel..];
    let (host, port, ipv6) = if host_port.starts_with('[') {
        let open_at = host_start;
        let close_rel = match host_port.as_bytes()[1..].iter().position(|&b| b == b']') {
            Some(i) => i + 1,
            None => return Err(ParseError::InvalidIpv6 { at: open_at }),
        };
        let interior = &host_port[1..close_rel];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: open_at });
        }

        let suffix = &host_port[close_rel + 1..];
        let parsed_port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(port_value(
                &suffix[1..],
                host_start + close_rel + 2,
            )?)
        } else {
            return Err(ParseError::InvalidHost {
                at: host_start + close_rel + 1,
            });
        };
        (interior.to_ascii_lowercase(), parsed_port, true)
    } else {
        let split = host_port.as_bytes().iter().position(|&b| b == b':');
        let (raw_host, parsed_port) = if let Some(i) = split {
            (&host_port[..i], Some(port_value(&host_port[i + 1..], host_start + i + 1)?))
        } else {
            (host_port, None)
        };

        if raw_host.is_empty() {
            return Err(ParseError::InvalidHost { at: host_start });
        }
        if let Some(i) = raw_host
            .bytes()
            .position(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')))
        {
            return Err(ParseError::InvalidHost { at: host_start + i });
        }
        (raw_host.to_ascii_lowercase(), parsed_port, false)
    };

    let fragment_rel = input[authority_end..]
        .as_bytes()
        .iter()
        .position(|&b| b == b'#')
        .map(|i| authority_end + i);
    let component_end = fragment_rel.unwrap_or(input.len());

    let query_rel = input[authority_end..component_end]
        .as_bytes()
        .iter()
        .position(|&b| b == b'?')
        .map(|i| authority_end + i);

    let path_end = query_rel.unwrap_or(component_end);
    let raw_path = &input[authority_end..path_end];
    let path = if raw_path.is_empty() {
        "/".to_owned()
    } else {
        raw_path.to_owned()
    };

    let query = query_rel.map(|q| input[q + 1..component_end].to_owned());
    let fragment = fragment_rel.map(|f| input[f + 1..].to_owned());

    if let Some(info) = userinfo.as_deref() {
        validate_component(Component::UserInfo, info, authority_start)?;
    }
    if !raw_path.is_empty() {
        validate_component(Component::Path, raw_path, authority_end)?;
    }
    if let (Some(q), Some(raw_query)) = (query_rel, query.as_deref()) {
        validate_component(Component::Query, raw_query, q + 1)?;
    }
    if let (Some(f), Some(raw_fragment)) = (fragment_rel, fragment.as_deref()) {
        validate_component(Component::Fragment, raw_fragment, f + 1)?;
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
    let mut offsets = Vec::with_capacity(bytes.len());
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
            offsets.push(i);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            offsets.push(i);
            i += 1;
        }
    }

    match std::str::from_utf8(&decoded) {
        Ok(s) => Ok(s.to_owned()),
        Err(e) => {
            let decoded_at = e.valid_up_to();
            let original_at = offsets.get(decoded_at).copied().unwrap_or(input.len());
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

pub fn normalise_path(path: &str) -> Result<String, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }
    decode_component(Component::Path, path).map_err(|e| match e {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    })?;

    let mut segments: Vec<&str> = Vec::new();
    let mut final_navigation = false;
    for segment in path[1..].split('/') {
        let decoded = decode_component(Component::Path, segment).map_err(|e| match e {
            CodecError::BadEscape { at } => PathError::BadEscape { at: at + 1 },
            CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at: at + 1 },
        })?;
        if decoded == "." {
            final_navigation = segment.as_ptr() as usize + segment.len()
                == path.as_ptr() as usize + path.len();
        } else if decoded == ".." {
            if !segments.is_empty() {
                segments.pop();
            }
            final_navigation = segment.as_ptr() as usize + segment.len()
                == path.as_ptr() as usize + path.len();
        } else {
            segments.push(segment);
            final_navigation = false;
        }
    }

    if final_navigation {
        segments.push("");
    }
    if segments.is_empty() {
        Ok("/".to_owned())
    } else {
        Ok(format!("/{}", segments.join("/")))
    }
}

fn make_absolute(base: &Url, path: &str, query: Option<&str>, fragment: Option<&str>) -> String {
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
    let base_url = normalize_url_path(base_url)?;

    if let Some(at) = first_invalid_byte(reference) {
        return Err(ResolveError::Reference(ParseError::InvalidByte { at }));
    }

    let boundary = reference
        .as_bytes()
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(reference.len());
    if reference.as_bytes()[..boundary].contains(&b':') {
        let url = parse_url(reference).map_err(ResolveError::Reference)?;
        return normalize_url_path(url);
    }

    if reference.starts_with("//") {
        let constructed = format!("{}:{}", base_url.scheme, reference);
        let url = parse_url(&constructed).map_err(ResolveError::Reference)?;
        return normalize_url_path(url);
    }

    let fragment_rel = reference.find('#');
    let before_fragment_end = fragment_rel.unwrap_or(reference.len());
    let query_rel = reference[..before_fragment_end].find('?');
    let path_end = query_rel.unwrap_or(before_fragment_end);
    let reference_path = &reference[..path_end];
    let reference_query = query_rel.map(|q| reference[q + 1..before_fragment_end].to_owned());
    let reference_fragment = fragment_rel.map(|f| reference[f + 1..].to_owned());

    if reference_path.is_empty() {
        let query = if query_rel.is_some() {
            reference_query
        } else {
            base_url.query.clone()
        };
        let constructed = make_absolute(
            &base_url,
            &base_url.path,
            query.as_deref(),
            reference_fragment.as_deref(),
        );
        let url = parse_url(&constructed).map_err(ResolveError::Reference)?;
        return normalize_url_path(url);
    }

    let merged_path = if reference_path.starts_with('/') {
        reference_path.to_owned()
    } else {
        let slash = base_url.path.rfind('/').map(|i| i + 1).unwrap_or(0);
        format!("{}{}", &base_url.path[..slash], reference_path)
    };

    let constructed = make_absolute(
        &base_url,
        &merged_path,
        reference_query.as_deref(),
        reference_fragment.as_deref(),
    );
    let url = parse_url(&constructed).map_err(ResolveError::Reference)?;
    normalize_url_path(url)
}

fn main() {}