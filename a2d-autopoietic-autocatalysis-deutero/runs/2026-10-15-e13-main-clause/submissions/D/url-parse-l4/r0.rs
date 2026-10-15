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
            let hi = hex_value(bytes[i + 1]);
            let lo = hex_value(bytes[i + 2]);
            match (hi, lo) {
                (Some(hi), Some(lo)) => {
                    decoded.push((hi << 4) | lo);
                    offsets.push(i);
                    i += 3;
                }
                _ => return Err(CodecError::BadEscape { at: i }),
            }
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

fn component_safe(component: Component, b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~')
        || match component {
            Component::UserInfo => b == b':',
            Component::Path => b == b'/',
            Component::Query => b == b'/' || b == b'?',
            Component::Fragment => b == b'/' || b == b'?' || b == b'#',
        }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &b in text.as_bytes() {
        if component_safe(component, b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}

fn map_codec_parse(err: CodecError, base: usize) -> ParseError {
    match err {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: base + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: base + at },
    }
}

fn validate_component(input: &str, component: Component, base: usize) -> Result<(), ParseError> {
    decode_component(component, input).map(|_| ()).map_err(|e| map_codec_parse(e, base))
}

fn parse_port(port: &str, port_start: usize) -> Result<Option<u16>, ParseError> {
    if port.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }
    for (i, b) in port.bytes().enumerate() {
        if !b.is_ascii_digit() {
            return Err(ParseError::InvalidPort { at: port_start + i });
        }
    }
    let bytes = port.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for b in bytes {
        value = match value.checked_mul(10).and_then(|v| v.checked_add((b - b'0') as u32)) {
            Some(v) => v,
            None => return Err(ParseError::InvalidPort { at: port_start }),
        };
    }
    if !(1..=65535).contains(&value) {
        return Err(ParseError::InvalidPort { at: port_start });
    }
    Ok(Some(value as u16))
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
    for i in 1..colon {
        let b = bytes[i];
        if !(b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')) {
            return Err(ParseError::InvalidScheme { at: i });
        }
    }

    if bytes.get(colon + 1) != Some(&b'/') || bytes.get(colon + 2) != Some(&b'/') {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let authority_start = colon + 3;
    let mut authority_end = input.len();
    for i in authority_start..input.len() {
        if matches!(bytes[i], b'/' | b'?' | b'#') {
            authority_end = i;
            break;
        }
    }
    let authority = &input[authority_start..authority_end];
    let authority_bytes = authority.as_bytes();

    let mut at_positions = Vec::new();
    for (i, &b) in authority_bytes.iter().enumerate() {
        if b == b'@' {
            at_positions.push(i);
        }
    }
    if at_positions.len() > 1 {
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

    let (host, port, ipv6) = if host_port.as_bytes().first() == Some(&b'[') {
        let close = match host_port.as_bytes().iter().position(|&b| b == b']') {
            Some(i) => i,
            None => return Err(ParseError::InvalidIpv6 { at: host_start }),
        };
        let interior = &host_port[1..close];
        if interior.contains('%') || interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: host_start });
        }
        let suffix = &host_port[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.as_bytes()[0] == b':' {
            parse_port(&suffix[1..], host_start + close + 2)?
        } else {
            return Err(ParseError::InvalidHost {
                at: host_start + close + 1,
            });
        };
        (interior.to_ascii_lowercase(), port, true)
    } else {
        let host_port_bytes = host_port.as_bytes();
        let split = host_port_bytes.iter().position(|&b| b == b':');
        let (host_part, port) = match split {
            Some(i) => {
                let port = parse_port(&host_port[i + 1..], host_start + i + 1)?;
                (&host_port[..i], port)
            }
            None => (host_port, None),
        };
        if host_part.is_empty() {
            return Err(ParseError::InvalidHost { at: host_start });
        }
        for (i, b) in host_part.bytes().enumerate() {
            if !is_host_byte(b) {
                return Err(ParseError::InvalidHost { at: host_start + i });
            }
        }
        (host_part.to_ascii_lowercase(), port, false)
    };

    let mut fragment_start = None;
    for i in authority_end..input.len() {
        if bytes[i] == b'#' {
            fragment_start = Some(i);
            break;
        }
    }
    let before_fragment_end = fragment_start.unwrap_or(input.len());

    let mut query_start = None;
    for i in authority_end..before_fragment_end {
        if bytes[i] == b'?' {
            query_start = Some(i);
            break;
        }
    }

    let path_end = query_start.unwrap_or(before_fragment_end);
    let raw_path = &input[authority_end..path_end];
    let path = if raw_path.is_empty() {
        "/".to_owned()
    } else {
        raw_path.to_owned()
    };

    let query = query_start.map(|start| input[start + 1..before_fragment_end].to_owned());
    let fragment = fragment_start.map(|start| input[start + 1..].to_owned());

    if let Some(ref u) = userinfo {
        validate_component(u, Component::UserInfo, authority_start)?;
    }
    if !raw_path.is_empty() {
        validate_component(raw_path, Component::Path, authority_end)?;
    }
    if let Some(start) = query_start {
        validate_component(
            &input[start + 1..before_fragment_end],
            Component::Query,
            start + 1,
        )?;
    }
    if let Some(start) = fragment_start {
        validate_component(&input[start + 1..], Component::Fragment, start + 1)?;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    NotAbsolute,
    BadEscape { at: usize },
    InvalidUtf8 { at: usize },
}

fn map_codec_path(err: CodecError) -> PathError {
    match err {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    }
}

pub fn normalise_path(path: &str) -> Result<String, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }
    decode_component(Component::Path, path).map_err(map_codec_path)?;

    let segments: Vec<&str> = path[1..].split('/').collect();
    let mut stack: Vec<&str> = Vec::new();
    let mut terminal_navigation = false;

    for (index, segment) in segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, segment).map_err(map_codec_path)?;
        let is_last = index + 1 == segments.len();
        if decoded == "." {
            if is_last {
                terminal_navigation = true;
            }
        } else if decoded == ".." {
            if !stack.is_empty() {
                stack.pop();
            }
            if is_last {
                terminal_navigation = true;
            }
        } else {
            stack.push(segment);
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    Base(ParseError),
    Reference(ParseError),
    Path(PathError),
}

fn map_path_resolve(err: PathError) -> ResolveError {
    ResolveError::Path(err)
}

fn global_scan_reference(reference: &str) -> Result<(), ParseError> {
    for (at, b) in reference.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(ParseError::InvalidByte { at });
        }
    }
    Ok(())
}

fn normalize_url_path(mut url: Url) -> Result<Url, ResolveError> {
    url.path = normalise_path(&url.path).map_err(map_path_resolve)?;
    Ok(url)
}

fn split_reference(reference: &str) -> (&str, Option<&str>, Option<&str>) {
    let bytes = reference.as_bytes();
    let hash = bytes.iter().position(|&b| b == b'#');
    let before_fragment = hash.unwrap_or(reference.len());
    let query = bytes[..before_fragment].iter().position(|&b| b == b'?');
    let path_end = query.unwrap_or(before_fragment);
    let path = &reference[..path_end];
    let query_value = query.map(|i| &reference[i + 1..before_fragment]);
    let fragment = hash.map(|i| &reference[i + 1..]);
    (path, query_value, fragment)
}

fn build_url_raw(
    scheme: &str,
    userinfo: Option<&str>,
    host: &str,
    ipv6: bool,
    port: Option<u16>,
    path: &str,
    query: Option<&str>,
    fragment: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str(scheme);
    out.push_str("://");
    if let Some(userinfo) = userinfo {
        out.push_str(userinfo);
        out.push('@');
    }
    if ipv6 {
        out.push('[');
        out.push_str(host);
        out.push(']');
    } else {
        out.push_str(host);
    }
    if let Some(port) = port {
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

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let base_path = normalise_path(&base_url.path).map_err(ResolveError::Path)?;
    global_scan_reference(reference).map_err(ResolveError::Reference)?;

    if reference.contains(':') {
        let colon = reference.find(':').unwrap();
        let first_delimiter = reference
            .bytes()
            .position(|b| matches!(b, b'/' | b'?' | b'#'))
            .unwrap_or(reference.len());
        if colon < first_delimiter {
            let url = parse_url(reference).map_err(ResolveError::Reference)?;
            return normalize_url_path(url);
        }
    }

    if reference.starts_with("//") {
        let constructed = format!("{}:{}", base_url.scheme, reference);
        let url = parse_url(&constructed).map_err(ResolveError::Reference)?;
        return normalize_url_path(url);
    }

    let (reference_path, reference_query, reference_fragment) = split_reference(reference);

    let (selected_path, selected_query) = if reference_path.is_empty() {
        (
            base_path.as_str().to_owned(),
            if reference_query.is_some() {
                reference_query.map(str::to_owned)
            } else {
                base_url.query.clone()
            },
        )
    } else {
        let merged = if reference_path.starts_with('/') {
            reference_path.to_owned()
        } else {
            let slash = base_path.rfind('/').map(|i| i + 1).unwrap_or(0);
            format!("{}{}", &base_path[..slash], reference_path)
        };
        (merged, reference_query.map(str::to_owned))
    };

    let constructed = build_url_raw(
        &base_url.scheme,
        base_url.userinfo.as_deref(),
        &base_url.host,
        base_url.ipv6,
        base_url.port,
        &selected_path,
        selected_query.as_deref(),
        reference_fragment,
    );
    let url = parse_url(&constructed).map_err(ResolveError::Reference)?;
    normalize_url_path(url)
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

fn replace_plus(text: &str) -> String {
    text.replace('+', " ")
}

fn map_codec_query(err: CodecError, base: usize) -> QueryError {
    match err {
        CodecError::BadEscape { at } => QueryError::BadEscape { at: base + at },
        CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 { at: base + at },
    }
}

pub fn parse_query(input: &str) -> Result<QueryMap, QueryError> {
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    let mut field_start = 0;

    for field in input.split('&') {
        let this_start = field_start;
        field_start += field.len() + 1;
        if field.is_empty() {
            continue;
        }

        let eq = field.find('=');
        let raw_key = eq.map(|i| &field[..i]).unwrap_or(field);
        let raw_value = eq.map(|i| &field[i + 1..]).unwrap_or("");
        let key_plus = replace_plus(raw_key);
        let key = decode_component(Component::Query, &key_plus)
            .map_err(|e| map_codec_query(e, this_start))?;
        if key.is_empty() {
            return Err(QueryError::EmptyKey { at: this_start });
        }

        let value_offset = this_start + eq.map(|i| i + 1).unwrap_or(field.len());
        let value_plus = replace_plus(raw_value);
        let value = decode_component(Component::Query, &value_plus)
            .map_err(|e| map_codec_query(e, value_offset))?;

        if let Some((_, values)) = entries.iter_mut().find(|(existing, _)| existing == &key) {
            values.push(value);
        } else {
            entries.push((key, vec![value]));
        }
    }

    Ok(QueryMap { entries })
}

fn form_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &b in text.as_bytes() {
        if matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else if b == b' ' {
            out.push('+');
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}

pub fn serialise_query(map: &QueryMap) -> String {
    let mut merged: Vec<(&str, Vec<&str>)> = Vec::new();

    for (key, values) in &map.entries {
        if key.is_empty() || values.is_empty() {
            continue;
        }
        if let Some((_, accumulated)) = merged.iter_mut().find(|(existing, _)| *existing == key) {
            accumulated.extend(values.iter().map(String::as_str));
        } else {
            merged.push((key.as_str(), values.iter().map(String::as_str).collect()));
        }
    }

    let mut fields = Vec::new();
    for (key, values) in merged {
        for value in values {
            fields.push(format!("{}={}", form_encode(key), form_encode(value)));
        }
    }
    fields.join("&")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalError {
    Parse(ParseError),
    Path(PathError),
    Query(QueryError),
}

pub fn canonical_url(input: &str) -> Result<String, CanonicalError> {
    let url = parse_url(input).map_err(CanonicalError::Parse)?;
    let path = normalise_path(&url.path).map_err(CanonicalError::Path)?;
    let query_map = match url.query.as_deref() {
        Some(q) => Some(parse_query(q).map_err(CanonicalError::Query)?),
        None => None,
    };

    let mut out = String::new();
    out.push_str(&url.scheme);
    out.push_str("://");

    if let Some(userinfo) = url.userinfo.as_deref() {
        let decoded = decode_component(Component::UserInfo, userinfo)
            .map_err(|e| CanonicalError::Parse(map_codec_parse(e, 0)))?;
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

    let keep_port = match (url.scheme.as_str(), url.port) {
        ("http", Some(80)) | ("https", Some(443)) => false,
        (_, Some(_)) => true,
        _ => false,
    };
    if keep_port {
        out.push(':');
        out.push_str(&url.port.unwrap().to_string());
    }

    let mut canonical_segments = Vec::new();
    for segment in path.split('/') {
        let decoded = decode_component(Component::Path, segment)
            .map_err(|e| CanonicalError::Path(map_codec_path(e)))?;
        canonical_segments.push(encode_component_unreserved(&decoded));
    }
    out.push_str(&canonical_segments.join("/"));

    if let Some(query_map) = query_map.as_ref() {
        out.push('?');
        out.push_str(&serialise_query(query_map));
    }
    if let Some(fragment) = url.fragment.as_deref() {
        let decoded = decode_component(Component::Fragment, fragment)
            .map_err(|e| CanonicalError::Parse(map_codec_parse(e, 0)))?;
        out.push('#');
        out.push_str(&encode_component(Component::Fragment, &decoded));
    }

    Ok(out)
}

fn encode_component_unreserved(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &b in text.as_bytes() {
        if matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}