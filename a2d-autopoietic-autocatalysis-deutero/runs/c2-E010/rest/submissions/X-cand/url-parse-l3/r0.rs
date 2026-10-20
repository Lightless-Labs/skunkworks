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
            if hi.is_none() || lo.is_none() {
                return Err(CodecError::BadEscape { at: i });
            }
            decoded.push((hi.unwrap() << 4) | lo.unwrap());
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
            out.push(HEX[(b & 15) as usize] as char);
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

fn codec_to_parse(err: CodecError, offset: usize) -> ParseError {
    match err {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: offset + at },
    }
}

fn validate_url_component(s: &str, offset: usize, component: Component) -> Result<(), ParseError> {
    decode_component(component, s).map(|_| ()).map_err(|e| codec_to_parse(e, offset))
}

fn parse_port(port_text: &str, port_start: usize) -> Result<u16, ParseError> {
    if port_text.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }
    let bytes = port_text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if !b.is_ascii_digit() {
            return Err(ParseError::InvalidPort { at: port_start + i });
        }
    }
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for &b in bytes {
        value = value.saturating_mul(10).saturating_add((b - b'0') as u32);
        if value > 65535 {
            return Err(ParseError::InvalidPort { at: port_start });
        }
    }
    if value == 0 {
        return Err(ParseError::InvalidPort { at: port_start });
    }
    Ok(value as u16)
}

fn parse_authority(
    authority: &str,
    authority_start: usize,
) -> Result<(Option<String>, String, Option<u16>, bool), ParseError> {
    let bytes = authority.as_bytes();
    let ats: Vec<usize> = bytes.iter().enumerate().filter_map(|(i, &b)| (b == b'@').then_some(i)).collect();
    if ats.len() >= 2 {
        return Err(ParseError::InvalidUserInfo { at: authority_start + ats[1] });
    }

    let (userinfo, hostport, hostport_start) = if let Some(&at) = ats.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo { at: authority_start + at });
        }
        (
            Some(authority[..at].to_owned()),
            &authority[at + 1..],
            authority_start + at + 1,
        )
    } else {
        (None, authority, authority_start)
    };

    if hostport.starts_with('[') {
        let open = hostport_start;
        let close_rel = hostport.as_bytes().iter().position(|&b| b == b']');
        let close_rel = match close_rel {
            Some(v) => v,
            None => return Err(ParseError::InvalidIpv6 { at: open }),
        };
        let interior = &hostport[1..close_rel];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: open });
        }
        let suffix = &hostport[close_rel + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(parse_port(&suffix[1..], hostport_start + close_rel + 2)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_start + close_rel + 1,
            });
        };
        return Ok((userinfo, interior.to_ascii_lowercase(), port, true));
    }

    let colon = hostport.as_bytes().iter().position(|&b| b == b':');
    let (host, port) = match colon {
        Some(i) => (&hostport[..i], Some((&hostport[i + 1..], hostport_start + i + 1))),
        None => (hostport, None),
    };
    if host.is_empty() {
        return Err(ParseError::InvalidHost { at: hostport_start });
    }
    for (i, &b) in host.as_bytes().iter().enumerate() {
        if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
            return Err(ParseError::InvalidHost { at: hostport_start + i });
        }
    }
    let parsed_port = match port {
        Some((s, at)) => Some(parse_port(s, at)?),
        None => None,
    };
    Ok((userinfo, host.to_ascii_lowercase(), parsed_port, false))
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
        if !(b.is_ascii_alphabetic() || b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.')) {
            return Err(ParseError::InvalidScheme { at: i });
        }
    }
    let scheme = input[..colon].to_ascii_lowercase();

    if bytes.get(colon + 1) != Some(&b'/') || bytes.get(colon + 2) != Some(&b'/') {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let authority_start = colon + 3;
    let rest = &input[authority_start..];
    let authority_len = rest
        .as_bytes()
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(rest.len());
    let authority = &rest[..authority_len];
    let after_authority_at = authority_start + authority_len;
    let after_authority = &input[after_authority_at..];

    let (userinfo, host, port, ipv6) = parse_authority(authority, authority_start)?;

    let fragment_rel = after_authority.as_bytes().iter().position(|&b| b == b'#');
    let before_fragment = match fragment_rel {
        Some(i) => &after_authority[..i],
        None => after_authority,
    };
    let fragment = fragment_rel.map(|i| after_authority[i + 1..].to_owned());

    let query_rel = before_fragment.as_bytes().iter().position(|&b| b == b'?');
    let path = match query_rel {
        Some(i) => &before_fragment[..i],
        None => before_fragment,
    };
    let query = query_rel.map(|i| before_fragment[i + 1..].to_owned());
    let path_offset = after_authority_at;
    let query_offset = query_rel.map(|i| after_authority_at + i + 1);
    let fragment_offset = fragment_rel.map(|i| after_authority_at + i + 1);

    if let Some(ref u) = userinfo {
        validate_url_component(u, authority_start, Component::UserInfo)?;
    }
    if !path.is_empty() {
        validate_url_component(path, path_offset, Component::Path)?;
    }
    if let Some(ref q) = query {
        validate_url_component(q, query_offset.unwrap(), Component::Query)?;
    }
    if let Some(ref f) = fragment {
        validate_url_component(f, fragment_offset.unwrap(), Component::Fragment)?;
    }

    Ok(Url {
        scheme,
        userinfo,
        host,
        port,
        path: if path.is_empty() { "/".to_owned() } else { path.to_owned() },
        query,
        fragment,
        ipv6,
    })
}

pub fn normalise_path(path: &str) -> Result<String, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }
    let decoded_path = decode_component(Component::Path, path).map_err(|e| match e {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    })?;

    let mut raw_segments: Vec<&str> = path[1..].split('/').collect();
    let decoded_segments: Vec<&str> = decoded_path[1..].split('/').collect();
    let terminal_navigation = decoded_segments
        .last()
        .map(|s| *s == "." || *s == "..")
        .unwrap_or(false);

    let mut stack: Vec<&str> = Vec::new();
    for (raw, decoded) in raw_segments.drain(..).zip(decoded_segments.into_iter()) {
        if decoded == "." {
            continue;
        } else if decoded == ".." {
            stack.pop();
        } else {
            stack.push(raw);
        }
    }
    if terminal_navigation {
        stack.push("");
    }
    Ok(format!("/{}", stack.join("/")))
}

fn raw_url_for_parts(url: &Url, path: &str, query: Option<&str>, fragment: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str(&url.scheme);
    out.push_str("://");
    if let Some(u) = &url.userinfo {
        out.push_str(u);
        out.push('@');
    }
    if url.ipv6 {
        out.push('[');
        out.push_str(&url.host);
        out.push(']');
    } else {
        out.push_str(&url.host);
    }
    if let Some(port) = url.port {
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

fn split_reference(reference: &str) -> (&str, Option<&str>, Option<&str>) {
    let fragment_i = reference.as_bytes().iter().position(|&b| b == b'#');
    let before_fragment = match fragment_i {
        Some(i) => &reference[..i],
        None => reference,
    };
    let query_i = before_fragment.as_bytes().iter().position(|&b| b == b'?');
    let path = match query_i {
        Some(i) => &before_fragment[..i],
        None => before_fragment,
    };
    let query = query_i.map(|i| &before_fragment[i + 1..]);
    let fragment = fragment_i.map(|i| &reference[i + 1..]);
    (path, query, fragment)
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let base_path = normalise_path(&base_url.path).map_err(ResolveError::Path)?;

    for (i, &b) in reference.as_bytes().iter().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(ResolveError::Reference(ParseError::InvalidByte { at: i }));
        }
    }

    let first_structural = reference
        .as_bytes()
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(reference.len());
    if reference.as_bytes()[..first_structural].contains(&b':') {
        let mut u = parse_url(reference).map_err(ResolveError::Reference)?;
        u.path = normalise_path(&u.path).map_err(ResolveError::Path)?;
        return Ok(u);
    }

    if reference.starts_with("//") {
        let constructed = format!("{}:{}", base_url.scheme, reference);
        let mut u = parse_url(&constructed).map_err(ResolveError::Reference)?;
        u.path = normalise_path(&u.path).map_err(ResolveError::Path)?;
        return Ok(u);
    }

    let (ref_path, ref_query, ref_fragment) = split_reference(reference);
    let (selected_path, selected_query) = if ref_path.is_empty() {
        (
            base_path.as_str().to_owned(),
            if ref_query.is_some() {
                ref_query
            } else {
                base_url.query.as_deref()
            },
        )
    } else {
        let merged = if ref_path.starts_with('/') {
            ref_path.to_owned()
        } else {
            let slash = base_path.rfind('/').map(|i| &base_path[..=i]).unwrap_or("/");
            format!("{slash}{ref_path}")
        };
        (merged, ref_query)
    };

    let constructed = raw_url_for_parts(
        &base_url,
        &selected_path,
        selected_query,
        ref_fragment,
    );
    let mut u = parse_url(&constructed).map_err(ResolveError::Reference)?;
    u.path = normalise_path(&u.path).map_err(ResolveError::Path)?;
    Ok(u)
}
