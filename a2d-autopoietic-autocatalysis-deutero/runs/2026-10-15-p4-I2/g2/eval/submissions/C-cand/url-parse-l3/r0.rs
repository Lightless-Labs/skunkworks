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

fn is_ascii_alpha(b: u8) -> bool {
    b.is_ascii_alphabetic()
}

fn global_byte_error(input: &str) -> Option<usize> {
    input
        .as_bytes()
        .iter()
        .position(|&b| !(0x21..=0x7e).contains(&b))
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
            let hi = hex_value(bytes[i + 1]).ok_or(CodecError::BadEscape { at: i })?;
            let lo = hex_value(bytes[i + 2]).ok_or(CodecError::BadEscape { at: i })?;
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
        Err(e) => {
            let at = origins.get(e.valid_up_to()).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at })
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

fn validate_component(
    component: Component,
    text: &str,
    original_offset: usize,
) -> Result<(), ParseError> {
    match decode_component(component, text) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape {
            at: original_offset + at,
        }),
        Err(CodecError::InvalidUtf8 { at }) => Err(ParseError::InvalidUtf8 {
            at: original_offset + at,
        }),
    }
}

fn parse_port(port_text: &str, port_start: usize) -> Result<u16, ParseError> {
    if port_text.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    if let Some(i) = port_text.bytes().position(|b| !b.is_ascii_digit()) {
        return Err(ParseError::InvalidPort {
            at: port_start + i,
        });
    }

    let bytes = port_text.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for &b in bytes {
        value = match value.checked_mul(10).and_then(|v| v.checked_add((b - b'0') as u32)) {
            Some(v) => v,
            None => return Err(ParseError::InvalidPort { at: port_start }),
        };
        if value > 65535 {
            return Err(ParseError::InvalidPort { at: port_start });
        }
    }

    if value == 0 {
        return Err(ParseError::InvalidPort { at: port_start });
    }
    Ok(value as u16)
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    if let Some(at) = global_byte_error(input) {
        return Err(ParseError::InvalidByte { at });
    }

    let bytes = input.as_bytes();
    let colon = match bytes.iter().position(|&b| b == b':') {
        Some(i) => i,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };

    if colon == 0 || !is_ascii_alpha(bytes[0]) {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for (i, &b) in bytes[..colon].iter().enumerate().skip(1) {
        if !(b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')) {
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
    let (userinfo, hostport, hostport_start) = if let Some(&at) = ats.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at,
            });
        }
        (
            Some(&authority[..at]),
            &authority[at + 1..],
            authority_start + at + 1,
        )
    } else {
        (None, authority, authority_start)
    };

    let mut ipv6 = false;
    let (host_raw, port) = if hostport.starts_with('[') {
        let close = match hostport.as_bytes()[1..].iter().position(|&b| b == b']') {
            Some(i) => i + 1,
            None => return Err(ParseError::InvalidIpv6 { at: hostport_start }),
        };
        let interior = &hostport[1..close];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: hostport_start });
        }

        let suffix = &hostport[close + 1..];
        let parsed_port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(parse_port(
                &suffix[1..],
                hostport_start + close + 2,
            )?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_start + close + 1,
            });
        };
        ipv6 = true;
        (interior, parsed_port)
    } else {
        let split = hostport.find(':');
        let (host, parsed_port) = if let Some(i) = split {
            (
                &hostport[..i],
                Some(parse_port(&hostport[i + 1..], hostport_start + i + 1)?),
            )
        } else {
            (hostport, None)
        };

        if host.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_start });
        }
        for (i, b) in host.bytes().enumerate() {
            if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: hostport_start + i,
                });
            }
        }
        (host, parsed_port)
    };

    let host = host_raw.to_ascii_lowercase();
    let mut fragment = None;
    let mut before_fragment_end = input.len();
    if let Some(hash) = input[authority_end..].find('#') {
        let hash_at = authority_end + hash;
        fragment = Some(input[hash_at + 1..].to_owned());
        before_fragment_end = hash_at;
    }

    let mut query = None;
    let mut before_query_end = before_fragment_end;
    if let Some(q) = input[authority_end..before_fragment_end].find('?') {
        let query_at = authority_end + q;
        query = Some(input[query_at + 1..before_fragment_end].to_owned());
        before_query_end = query_at;
    }

    let path = if before_query_end == authority_end {
        "/".to_owned()
    } else {
        input[authority_end..before_query_end].to_owned()
    };

    if let Some(ui) = userinfo {
        validate_component(Component::UserInfo, ui, authority_start)?;
    }
    if path != "/" || before_query_end != authority_end {
        validate_component(Component::Path, &path, authority_end)?;
    }
    if let Some(ref q) = query {
        let q_start = authority_end + input[authority_end..before_fragment_end]
            .find('?')
            .unwrap()
            + 1;
        validate_component(Component::Query, q, q_start)?;
    }
    if let Some(ref f) = fragment {
        let f_start = before_fragment_end + 1;
        validate_component(Component::Fragment, f, f_start)?;
    }

    Ok(Url {
        scheme: input[..colon].to_ascii_lowercase(),
        userinfo: userinfo.map(str::to_owned),
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

    let raw_segments: Vec<&str> = path[1..].split('/').collect();
    let mut decoded_segments = Vec::with_capacity(raw_segments.len());
    for seg in &raw_segments {
        decoded_segments.push(
            decode_component(Component::Path, seg).expect("whole path already validated"),
        );
    }

    let terminal_navigation = decoded_segments
        .last()
        .map(|s| s == "." || s == "..")
        .unwrap_or(false);

    let mut stack: Vec<&str> = Vec::new();
    for (raw, decoded) in raw_segments.iter().zip(decoded_segments.iter()) {
        if decoded == "." {
            continue;
        }
        if decoded == ".." {
            stack.pop();
            continue;
        }
        stack.push(raw);
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

fn path_error_to_resolve(e: PathError) -> ResolveError {
    ResolveError::Path(e)
}

fn normalize_url_path(mut url: Url) -> Result<Url, ResolveError> {
    url.path = normalise_path(&url.path).map_err(path_error_to_resolve)?;
    Ok(url)
}

fn split_reference(reference: &str) -> (&str, Option<&str>, Option<&str>) {
    let hash = reference.find('#');
    let (before_fragment, fragment) = match hash {
        Some(i) => (&reference[..i], Some(&reference[i + 1..])),
        None => (reference, None),
    };
    let q = before_fragment.find('?');
    match q {
        Some(i) => (
            &before_fragment[..i],
            Some(&before_fragment[i + 1..]),
            fragment,
        ),
        None => (before_fragment, None, fragment),
    }
}

fn absolute_reference_colon(reference: &str) -> bool {
    let end = reference
        .bytes()
        .position(|b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(reference.len());
    reference[..end].contains(':')
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let base_url = normalize_url_path(base_url)?;

    if let Some(at) = global_byte_error(reference) {
        return Err(ResolveError::Reference(ParseError::InvalidByte { at }));
    }

    if absolute_reference_colon(reference) {
        let url = parse_url(reference).map_err(ResolveError::Reference)?;
        return normalize_url_path(url);
    }

    if reference.starts_with("//") {
        let constructed = format!("{}:{}", base_url.scheme, reference);
        let url = parse_url(&constructed).map_err(ResolveError::Reference)?;
        return normalize_url_path(url);
    }

    let (ref_path, ref_query, ref_fragment) = split_reference(reference);
    let selected_path = if ref_path.is_empty() {
        base_url.path.clone()
    } else if ref_path.starts_with('/') {
        ref_path.to_owned()
    } else {
        let slash = base_url.path.rfind('/').unwrap_or(0);
        format!("{}{}", &base_url.path[..=slash], ref_path)
    };

    let selected_query = if ref_path.is_empty() && ref_query.is_none() {
        base_url.query.as_deref()
    } else {
        ref_query
    };

    let mut constructed = String::new();
    constructed.push_str(&base_url.scheme);
    constructed.push_str("://");
    if let Some(ref ui) = base_url.userinfo {
        constructed.push_str(ui);
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
    if let Some(q) = selected_query {
        constructed.push('?');
        constructed.push_str(q);
    }
    if let Some(f) = ref_fragment {
        constructed.push('#');
        constructed.push_str(f);
    }

    let url = parse_url(&constructed).map_err(ResolveError::Reference)?;
    normalize_url_path(url)
}

fn main() {}