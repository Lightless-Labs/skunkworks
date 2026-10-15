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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryMap {
    pub entries: Vec<(String, Vec<String>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    BadEscape { at: usize },
    InvalidUtf8 { at: usize },
    EmptyKey { at: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalError {
    Parse(ParseError),
    Path(PathError),
    Query(QueryError),
}

fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

fn hex_value(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        _ => b - b'A' + 10,
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
        Err(err) => {
            let decoded_at = err.valid_up_to();
            let original_at = origins.get(decoded_at).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at: original_at })
        }
    }
}

fn component_byte_is_safe(component: Component, b: u8) -> bool {
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
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &b in text.as_bytes() {
        if component_byte_is_safe(component, b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}

fn validate_component_at(
    component: Component,
    value: &str,
    base: usize,
) -> Result<(), ParseError> {
    match decode_component(component, value) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape { at: base + at }),
        Err(CodecError::InvalidUtf8 { at }) => Err(ParseError::InvalidUtf8 { at: base + at }),
    }
}

fn parse_port(port: &str, start: usize) -> Result<u16, ParseError> {
    if port.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }
    if let Some((i, _)) = port.bytes().enumerate().find(|(_, b)| !b.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: start + i });
    }
    let bytes = port.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }

    let mut value: u32 = 0;
    for b in bytes {
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

fn is_host_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }

    for (at, b) in input.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(ParseError::InvalidByte { at });
        }
    }

    let bytes = input.as_bytes();
    let colon = match bytes.iter().position(|&b| b == b':') {
        Some(i) => i,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };
    if colon == 0 || !bytes[0].is_ascii_alphabetic() {
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
        .position(|b| matches!(b, b'/' | b'?' | b'#'))
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
    let hostport = &authority[host_start_rel..];
    let mut ipv6 = false;
    let (host, port) = if hostport.as_bytes().first() == Some(&b'[') {
        let open_at = host_start;
        let close_rel = match hostport.as_bytes().iter().position(|&b| b == b']') {
            Some(i) => i,
            None => return Err(ParseError::InvalidIpv6 { at: open_at }),
        };
        let interior = &hostport[1..close_rel];
        if interior.contains('%') || interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: open_at });
        }
        let suffix = &hostport[close_rel + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.as_bytes()[0] == b':' {
            Some(parse_port(
                &suffix[1..],
                host_start + close_rel + 2,
            )?)
        } else {
            return Err(ParseError::InvalidHost {
                at: host_start + close_rel + 1,
            });
        };
        ipv6 = true;
        (interior.to_ascii_lowercase(), port)
    } else {
        let split = hostport.as_bytes().iter().position(|&b| b == b':');
        let (host_part, port_part) = match split {
            Some(i) => (&hostport[..i], Some((&hostport[i + 1..], host_start + i + 1))),
            None => (hostport, None),
        };
        if host_part.is_empty() {
            return Err(ParseError::InvalidHost { at: host_start });
        }
        if let Some((i, _)) = host_part.bytes().enumerate().find(|(_, b)| !is_host_byte(*b)) {
            return Err(ParseError::InvalidHost { at: host_start + i });
        }
        let port = match port_part {
            Some((p, at)) => Some(parse_port(p, at)?),
            None => None,
        };
        (host_part.to_ascii_lowercase(), port)
    };

    let remainder = &input[authority_end..];
    let fragment_rel = remainder.find('#');
    let before_fragment = fragment_rel.map(|i| &remainder[..i]).unwrap_or(remainder);
    let fragment = fragment_rel.map(|i| remainder[i + 1..].to_owned());
    let query_rel = before_fragment.find('?');
    let path_raw = query_rel.map(|i| &before_fragment[..i]).unwrap_or(before_fragment);
    let query = query_rel.map(|i| before_fragment[i + 1..].to_owned());
    let path = if path_raw.is_empty() { "/".to_owned() } else { path_raw.to_owned() };

    if let Some(ref info) = userinfo {
        validate_component_at(Component::UserInfo, info, authority_start)?;
    }
    if !path_raw.is_empty() {
        validate_component_at(Component::Path, path_raw, authority_end)?;
    }
    if let Some(ref q) = query {
        validate_component_at(
            Component::Query,
            q,
            authority_end + query_rel.unwrap() + 1,
        )?;
    }
    if let Some(ref f) = fragment {
        validate_component_at(
            Component::Fragment,
            f,
            authority_end + fragment_rel.unwrap() + 1,
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
    match decode_component(Component::Path, path) {
        Ok(_) => {}
        Err(CodecError::BadEscape { at }) => return Err(PathError::BadEscape { at }),
        Err(CodecError::InvalidUtf8 { at }) => return Err(PathError::InvalidUtf8 { at }),
    }

    let mut segments: Vec<String> = Vec::new();
    let raw_segments: Vec<&str> = path[1..].split('/').collect();
    let mut terminal_navigation = false;

    for (i, raw) in raw_segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, raw).expect("whole path validated");
        let is_dot = decoded == ".";
        let is_dotdot = decoded == "..";
        if i + 1 == raw_segments.len() {
            terminal_navigation = is_dot || is_dotdot;
        }
        if is_dot {
            continue;
        } else if is_dotdot {
            segments.pop();
        } else {
            segments.push((*raw).to_owned());
        }
    }

    if terminal_navigation {
        segments.push(String::new());
    }
    if segments.is_empty() {
        Ok("/".to_owned())
    } else {
        Ok(format!("/{}", segments.join("/")))
    }
}

fn scan_reference(input: &str) -> Result<(), ParseError> {
    for (at, b) in input.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(ParseError::InvalidByte { at });
        }
    }
    Ok(())
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
    let before_fragment = fragment_pos.map(|i| &reference[..i]).unwrap_or(reference);
    let fragment = fragment_pos.map(|i| reference[i + 1..].to_owned());
    let query_pos = before_fragment.find('?');
    let ref_path = query_pos.map(|i| &before_fragment[..i]).unwrap_or(before_fragment);
    let ref_query = query_pos.map(|i| before_fragment[i + 1..].to_owned());

    let (selected_path, selected_query) = if ref_path.is_empty() {
        (
            base_path.clone(),
            if query_pos.is_none() {
                base_url.query.clone()
            } else {
                ref_query
            },
        )
    } else {
        let merged = if ref_path.starts_with('/') {
            ref_path.to_owned()
        } else {
            let slash = base_path.rfind('/').map(|i| i + 1).unwrap_or(0);
            format!("{}{}", &base_path[..slash], ref_path)
        };
        (merged, ref_query)
    };

    let userinfo = base_url
        .userinfo
        .as_ref()
        .map(|s| format!("{}@", s))
        .unwrap_or_default();
    let host = if base_url.ipv6 {
        format!("[{}]", base_url.host)
    } else {
        base_url.host.clone()
    };
    let port = base_url
        .port
        .map(|p| format!(":{}", p))
        .unwrap_or_default();
    let query = selected_query
        .as_ref()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();
    let fragment = fragment
        .as_ref()
        .map(|f| format!("#{}", f))
        .unwrap_or_default();
    let constructed = format!(
        "{}://{}{}{}{}{}{}",
        base_url.scheme, userinfo, host, port, selected_path, query, fragment
    );
    let mut url = parse_url(&constructed).map_err(ResolveError::Reference)?;
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

fn form_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &b in text.as_bytes() {
        if b == b' ' {
            out.push('+');
        } else if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}

fn plus_to_space(text: &str) -> String {
    text.replace('+', " ")
}

fn map_query_codec_error(err: CodecError, base: usize) -> QueryError {
    match err {
        CodecError::BadEscape { at } => QueryError::BadEscape { at: base + at },
        CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 { at: base + at },
    }
}

pub fn parse_query(input: &str) -> Result<QueryMap, QueryError> {
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    let mut field_start = 0;

    for field in input.split('&') {
        let start = field_start;
        field_start += field.len() + 1;
        if field.is_empty() {
            continue;
        }
        let eq = field.find('=');
        let (raw_key, raw_value, value_offset) = match eq {
            Some(i) => (&field[..i], &field[i + 1..], start + i + 1),
            None => (field, "", start + field.len()),
        };

        let key_prepared = plus_to_space(raw_key);
        let key = decode_component(Component::Query, &key_prepared)
            .map_err(|e| map_query_codec_error(e, start))?;
        if key.is_empty() {
            return Err(QueryError::EmptyKey { at: start });
        }

        let value_prepared = plus_to_space(raw_value);
        let value = decode_component(Component::Query, &value_prepared)
            .map_err(|e| map_query_codec_error(e, value_offset))?;

        if let Some((_, values)) = entries.iter_mut().find(|(k, _)| *k == key) {
            values.push(value);
        } else {
            entries.push((key, vec![value]));
        }
    }
    Ok(QueryMap { entries })
}

pub fn serialise_query(map: &QueryMap) -> String {
    let mut merged: Vec<(String, Vec<String>)> = Vec::new();
    for (key, values) in &map.entries {
        if key.is_empty() || values.is_empty() {
            continue;
        }
        if let Some((_, existing)) = merged.iter_mut().find(|(k, _)| k == key) {
            existing.extend(values.iter().cloned());
        } else {
            merged.push((key.clone(), values.clone()));
        }
    }

    let mut fields = Vec::new();
    for (key, values) in merged {
        let encoded_key = form_encode(&key);
        for value in values {
            fields.push(format!("{}={}", encoded_key, form_encode(&value)));
        }
    }
    fields.join("&")
}

fn canonical_path(path: &str) -> Result<String, PathError> {
    let mut out = Vec::new();
    for segment in path.split('/') {
        let decoded = decode_component(Component::Path, segment).map_err(|e| match e {
            CodecError::BadEscape { at } => PathError::BadEscape { at },
            CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
        })?;
        let encoded = encode_unreserved(&decoded);
        out.push(encoded);
    }
    Ok(out.join("/"))
}

fn encode_unreserved(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &b in text.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}

pub fn canonical_url(input: &str) -> Result<String, CanonicalError> {
    let url = parse_url(input).map_err(CanonicalError::Parse)?;
    let path = normalise_path(&url.path).map_err(CanonicalError::Path)?;
    let query_map = match &url.query {
        Some(q) => Some(parse_query(q).map_err(CanonicalError::Query)?),
        None => None,
    };

    let mut out = format!("{}://", url.scheme);
    if let Some(userinfo) = &url.userinfo {
        let decoded = decode_component(Component::UserInfo, userinfo)
            .map_err(|e| CanonicalError::Parse(match e {
                CodecError::BadEscape { at } => ParseError::BadEscape { at },
                CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at },
            }))?;
        out.push_str(&encode_component(Component::UserInfo, &decoded));
        out.push('@');
    }
    if url.ipv6 {
        let addr = url
            .host
            .parse::<std::net::Ipv6Addr>()
            .map_err(|_| CanonicalError::Parse(ParseError::InvalidIpv6 { at: 0 }))?;
        out.push('[');
        out.push_str(&addr.to_string());
        out.push(']');
    } else {
        out.push_str(&url.host);
    }

    if let Some(port) = url.port {
        let omit = (url.scheme == "http" && port == 80) || (url.scheme == "https" && port == 443);
        if !omit {
            out.push(':');
            out.push_str(&port.to_string());
        }
    }

    out.push_str(&canonical_path(&path).map_err(CanonicalError::Path)?);

    if let Some(map) = query_map {
        out.push('?');
        out.push_str(&serialise_query(&map));
    }
    if let Some(fragment) = &url.fragment {
        let decoded = decode_component(Component::Fragment, fragment)
            .map_err(|e| CanonicalError::Parse(match e {
                CodecError::BadEscape { at } => ParseError::BadEscape { at },
                CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at },
            }))?;
        out.push('#');
        out.push_str(&encode_component(Component::Fragment, &decoded));
    }
    Ok(out)
}

fn main() {}