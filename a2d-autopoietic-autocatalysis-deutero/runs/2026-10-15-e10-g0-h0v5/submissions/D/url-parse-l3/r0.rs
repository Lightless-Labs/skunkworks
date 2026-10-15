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
        Err(e) => {
            let decoded_at = e.valid_up_to();
            let original_at = origins.get(decoded_at).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at: original_at })
        }
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    for &b in text.as_bytes() {
        let base_safe = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~');
        let component_safe = match component {
            Component::UserInfo => b == b':',
            Component::Path => b == b'/',
            Component::Query => matches!(b, b'/' | b'?'),
            Component::Fragment => matches!(b, b'/' | b'?' | b'#'),
        };
        if base_safe || component_safe {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }

    out
}

fn validate_global_bytes(input: &str) -> Result<(), ParseError> {
    for (i, &b) in input.as_bytes().iter().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(ParseError::InvalidByte { at: i });
        }
    }
    Ok(())
}

fn parse_port(port_text: &str, port_start: usize) -> Result<u16, ParseError> {
    if port_text.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }
    for (i, b) in port_text.bytes().enumerate() {
        if !b.is_ascii_digit() {
            return Err(ParseError::InvalidPort {
                at: port_start + i,
            });
        }
    }
    if port_text.len() >= 2 && port_text.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for b in port_text.bytes() {
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

fn component_parse_error(err: CodecError, offset: usize) -> ParseError {
    match err {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: offset + at },
    }
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
    let scheme = input[..colon].to_ascii_lowercase();

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
        if interior.is_empty()
            || interior.contains('%')
            || interior.parse::<std::net::Ipv6Addr>().is_err()
        {
            return Err(ParseError::InvalidIpv6 {
                at: hostport_start,
            });
        }

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

        ipv6 = true;
        (interior.to_ascii_lowercase(), port)
    } else {
        let split = hostport.as_bytes().iter().position(|&b| b == b':');
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

    let fragment_pos = input[authority_end..]
        .as_bytes()
        .iter()
        .position(|&b| b == b'#')
        .map(|i| authority_end + i);
    let before_fragment_end = fragment_pos.unwrap_or(input.len());

    let query_pos = input[authority_end..before_fragment_end]
        .as_bytes()
        .iter()
        .position(|&b| b == b'?')
        .map(|i| authority_end + i);

    let (path_end, query) = match query_pos {
        Some(qpos) => (
            qpos,
            Some(input[qpos + 1..before_fragment_end].to_owned()),
        ),
        None => (before_fragment_end, None),
    };
    let path = if path_end == authority_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };
    let fragment = fragment_pos.map(|fpos| input[fpos + 1..].to_owned());

    if let Some(ref userinfo_text) = userinfo {
        decode_component(Component::UserInfo, userinfo_text)
            .map_err(|e| component_parse_error(e, authority_start))?;
    }
    if path != "/" || path_end != authority_end {
        decode_component(Component::Path, &path)
            .map_err(|e| component_parse_error(e, authority_end))?;
    }
    if let Some(ref query_text) = query {
        let qpos = query_pos.unwrap();
        decode_component(Component::Query, query_text)
            .map_err(|e| component_parse_error(e, qpos + 1))?;
    }
    if let Some(ref fragment_text) = fragment {
        let fpos = fragment_pos.unwrap();
        decode_component(Component::Fragment, fragment_text)
            .map_err(|e| component_parse_error(e, fpos + 1))?;
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

    decode_component(Component::Path, path).map_err(|e| match e {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    })?;

    let segments: Vec<&str> = path[1..].split('/').collect();
    let mut stack: Vec<&str> = Vec::new();
    let mut terminal_navigation = false;

    for (i, segment) in segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, segment).map_err(|e| match e {
            CodecError::BadEscape { at } => PathError::BadEscape { at: at + 1 },
            CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at: at + 1 },
        })?;

        let is_last = i + 1 == segments.len();
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

fn split_reference(reference: &str) -> (usize, Option<usize>, Option<usize>) {
    let bytes = reference.as_bytes();
    let fragment = bytes.iter().position(|&b| b == b'#');
    let pre_fragment_end = fragment.unwrap_or(bytes.len());
    let query = bytes[..pre_fragment_end].iter().position(|&b| b == b'?');
    let path_end = query.unwrap_or(pre_fragment_end);
    (path_end, query, fragment)
}

fn normalize_resolved_path(mut url: Url) -> Result<Url, ResolveError> {
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let normalized_base_path =
        normalise_path(&base_url.path).map_err(ResolveError::Path)?;

    validate_global_bytes(reference).map_err(ResolveError::Reference)?;

    let bytes = reference.as_bytes();
    let first_delimiter = bytes
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(bytes.len());
    if bytes[..first_delimiter].contains(&b':') {
        let url = parse_url(reference).map_err(ResolveError::Reference)?;
        return normalize_resolved_path(url);
    }

    if reference.starts_with("//") {
        let constructed = format!("{}:{}", base_url.scheme, reference);
        let url = parse_url(&constructed).map_err(ResolveError::Reference)?;
        return normalize_resolved_path(url);
    }

    let (path_end, query_pos, fragment_pos) = split_reference(reference);
    let reference_path = &reference[..path_end];
    let reference_query = query_pos.map(|q| {
        let end = fragment_pos.unwrap_or(reference.len());
        reference[q + 1..end].to_owned()
    });
    let reference_fragment =
        fragment_pos.map(|f| reference[f + 1..].to_owned());

    let (selected_path, selected_query) = if reference_path.is_empty() {
        (
            normalized_base_path.clone(),
            if query_pos.is_none() {
                base_url.query.clone()
            } else {
                reference_query
            },
        )
    } else {
        let merged = if reference_path.starts_with('/') {
            reference_path.to_owned()
        } else {
            let slash = normalized_base_path
                .rfind('/')
                .map(|i| &normalized_base_path[..=i])
                .unwrap_or("/");
            format!("{slash}{reference_path}")
        };
        (merged, reference_query)
    };

    let host_text = if base_url.ipv6 {
        format!("[{}]", base_url.host)
    } else {
        base_url.host.clone()
    };
    let mut constructed = format!("{}://", base_url.scheme);
    if let Some(ref userinfo) = base_url.userinfo {
        constructed.push_str(userinfo);
        constructed.push('@');
    }
    constructed.push_str(&host_text);
    if let Some(port) = base_url.port {
        constructed.push(':');
        constructed.push_str(&port.to_string());
    }
    constructed.push_str(&selected_path);
    if let Some(ref query) = selected_query {
        constructed.push('?');
        constructed.push_str(query);
    }
    if let Some(ref fragment) = reference_fragment {
        constructed.push('#');
        constructed.push_str(fragment);
    }

    let mut url = parse_url(&constructed).map_err(ResolveError::Reference)?;
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}