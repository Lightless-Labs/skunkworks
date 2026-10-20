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

fn validate_raw_bytes(s: &str) -> Result<(), usize> {
    for (i, b) in s.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(i);
        }
    }
    Ok(())
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
                (Some(h), Some(l)) => {
                    decoded.push((h << 4) | l);
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
            let at = origins.get(e.valid_up_to()).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at })
        }
    }
}

fn component_safe(component: Component, b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(b, b'-' | b'.' | b'_' | b'~')
        || match component {
            Component::UserInfo => b == b':',
            Component::Path => b == b'/',
            Component::Query => matches!(b, b'/' | b'?'),
            Component::Fragment => matches!(b, b'/' | b'?' | b'#'),
        }
}

pub fn encode_component(component: Component, text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len());
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

fn codec_to_parse(e: CodecError, base: usize) -> ParseError {
    match e {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: base + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: base + at },
    }
}

fn validate_port(port: &str, start: usize) -> Result<Option<u16>, ParseError> {
    if port.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }
    for (i, b) in port.bytes().enumerate() {
        if !b.is_ascii_digit() {
            return Err(ParseError::InvalidPort { at: start + i });
        }
    }
    if port.len() >= 2 && port.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }
    let mut value: u32 = 0;
    for b in port.bytes() {
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
    Ok(Some(value as u16))
}

fn parse_url_inner(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    validate_raw_bytes(input).map_err(|at| ParseError::InvalidByte { at })?;

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

    let auth_start = colon + 3;
    let mut auth_end = input.len();
    for i in auth_start..bytes.len() {
        if matches!(bytes[i], b'/' | b'?' | b'#') {
            auth_end = i;
            break;
        }
    }
    let authority = &input[auth_start..auth_end];
    let auth_bytes = authority.as_bytes();

    let ats: Vec<usize> = auth_bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b == b'@' { Some(i) } else { None })
        .collect();
    if ats.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: auth_start + ats[1],
        });
    }
    let (userinfo, host_start_rel) = if let Some(&at) = ats.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo { at: auth_start });
        }
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let hostport = &authority[host_start_rel..];
    let hostport_start = auth_start + host_start_rel;
    let mut ipv6 = false;
    let (host, port) = if hostport.starts_with('[') {
        let close = match hostport.as_bytes().iter().position(|&b| b == b']') {
            Some(i) => i,
            None => return Err(ParseError::InvalidIpv6 { at: hostport_start }),
        };
        let interior = &hostport[1..close];
        if interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: hostport_start });
        }
        let suffix = &hostport[close + 1..];
        let parsed_port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            validate_port(&suffix[1..], hostport_start + close + 2)?
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_start + close + 1,
            });
        };
        ipv6 = true;
        (interior.to_ascii_lowercase(), parsed_port)
    } else {
        let split = hostport.find(':');
        let (host_part, port_part) = match split {
            Some(i) => (&hostport[..i], Some((&hostport[i + 1..], i))),
            None => (hostport, None),
        };
        if host_part.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_start });
        }
        for (i, b) in host_part.bytes().enumerate() {
            if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: hostport_start + i,
                });
            }
        }
        let parsed_port = match port_part {
            Some((p, i)) => validate_port(p, hostport_start + i + 1)?,
            None => None,
        };
        (host_part.to_ascii_lowercase(), parsed_port)
    };

    let tail = &input[auth_end..];
    let hash_rel = tail.find('#');
    let before_fragment = hash_rel.map_or(tail, |i| &tail[..i]);
    let fragment = hash_rel.map(|i| tail[i + 1..].to_owned());
    let query_rel = before_fragment.find('?');
    let before_query = query_rel.map_or(before_fragment, |i| &before_fragment[..i]);
    let query = query_rel.map(|i| before_fragment[i + 1..].to_owned());
    let path = if before_query.is_empty() {
        "/".to_owned()
    } else {
        before_query.to_owned()
    };

    let mut cursor = 0usize;
    if let Some(ref u) = userinfo {
        decode_component(Component::UserInfo, u)
            .map_err(|e| codec_to_parse(e, auth_start + cursor))?;
        cursor = u.len() + 1;
    }
    let path_offset = auth_end;
    if before_query.len() > 0 {
        decode_component(Component::Path, &path)
            .map_err(|e| codec_to_parse(e, path_offset))?;
    }
    if let Some(ref q) = query {
        let q_offset = auth_end + before_fragment.find('?').unwrap() + 1;
        decode_component(Component::Query, q).map_err(|e| codec_to_parse(e, q_offset))?;
    }
    if let Some(ref f) = fragment {
        let f_offset = auth_end + hash_rel.unwrap() + 1;
        decode_component(Component::Fragment, f).map_err(|e| codec_to_parse(e, f_offset))?;
    }
    let _ = cursor;

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

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    parse_url_inner(input)
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
            CodecError::BadEscape { at } => PathError::BadEscape { at: at + 1 + segments[..i].iter().map(|s| s.len() + 1).sum::<usize>() },
            CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at: at + 1 + segments[..i].iter().map(|s| s.len() + 1).sum::<usize>() },
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
    Ok(format!("/{}", stack.join("/")))
}

fn scan_reference(reference: &str) -> Result<(), ParseError> {
    validate_raw_bytes(reference).map_err(|at| ParseError::InvalidByte { at })
}

fn url_to_raw(url: &Url) -> String {
    let mut s = format!("{}://", url.scheme);
    if let Some(ref ui) = url.userinfo {
        s.push_str(ui);
        s.push('@');
    }
    if url.ipv6 {
        s.push('[');
        s.push_str(&url.host);
        s.push(']');
    } else {
        s.push_str(&url.host);
    }
    if let Some(port) = url.port {
        s.push(':');
        s.push_str(&port.to_string());
    }
    s
}

fn attach_components(mut raw: String, path: &str, query: Option<&str>, fragment: Option<&str>) -> String {
    raw.push_str(path);
    if let Some(q) = query {
        raw.push('?');
        raw.push_str(q);
    }
    if let Some(f) = fragment {
        raw.push('#');
        raw.push_str(f);
    }
    raw
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let normalized_base_path = normalise_path(&base_url.path).map_err(ResolveError::Path)?;
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
        let constructed = format!("{}:{}", base_url.scheme, reference);
        let mut url = parse_url(&constructed).map_err(ResolveError::Reference)?;
        url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
        return Ok(url);
    }

    let hash = reference.find('#');
    let before_fragment = hash.map_or(reference, |i| &reference[..i]);
    let fragment = hash.map(|i| &reference[i + 1..]);
    let query_pos = before_fragment.find('?');
    let ref_path = query_pos.map_or(before_fragment, |i| &before_fragment[..i]);
    let ref_query = query_pos.map(|i| &before_fragment[i + 1..]);

    let (selected_path, selected_query) = if ref_path.is_empty() {
        (
            normalized_base_path.as_str(),
            if query_pos.is_none() {
                base_url.query.as_deref()
            } else {
                ref_query
            },
        )
    } else {
        let merged = if ref_path.starts_with('/') {
            ref_path.to_owned()
        } else {
            let slash = normalized_base_path.rfind('/').unwrap_or(0);
            format!("{}{}", &normalized_base_path[..=slash], ref_path)
        };
        (Box::leak(merged.into_boxed_str()) as &str, ref_query)
    };

    let raw = attach_components(
        url_to_raw(&base_url),
        selected_path,
        selected_query,
        fragment,
    );
    let mut url = parse_url(&raw).map_err(ResolveError::Reference)?;
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

fn form_encode(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::new();
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

fn form_decode_part(s: &str) -> Result<String, CodecError> {
    let replaced = s.replace('+', " ");
    decode_component(Component::Query, &replaced)
}

pub fn parse_query(input: &str) -> Result<QueryMap, QueryError> {
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();

    for (field_start, field) in input.split('&').scan(0usize, |offset, f| {
        let start = *offset;
        *offset += f.len() + 1;
        Some((start, f))
    }) {
        if field.is_empty() {
            continue;
        }
        let (raw_key, raw_value) = match field.find('=') {
            Some(i) => (&field[..i], &field[i + 1..]),
            None => (field, ""),
        };

        let key = form_decode_part(raw_key).map_err(|e| match e {
            CodecError::BadEscape { at } => QueryError::BadEscape { at: field_start + at },
            CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 { at: field_start + at },
        })?;
        if key.is_empty() {
            return Err(QueryError::EmptyKey { at: field_start });
        }
        let value = form_decode_part(raw_value).map_err(|e| {
            let value_offset = field_start + field.find('=').map_or(field.len(), |i| i + 1);
            match e {
                CodecError::BadEscape { at } => QueryError::BadEscape { at: value_offset + at },
                CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 { at: value_offset + at },
            }
        })?;

        if let Some((_, values)) = entries.iter_mut().find(|(k, _)| *k == key) {
            values.push(value);
        } else {
            entries.push((key, vec![value]));
        }
    }

    Ok(QueryMap { entries })
}

pub fn serialise_query(map: &QueryMap) -> String {
    let mut merged: Vec<(&str, Vec<&str>)> = Vec::new();
    for (key, values) in &map.entries {
        if key.is_empty() || values.is_empty() {
            continue;
        }
        if let Some((_, existing)) = merged.iter_mut().find(|(k, _)| *k == key) {
            existing.extend(values.iter().map(String::as_str));
        } else {
            merged.push((key.as_str(), values.iter().map(String::as_str).collect()));
        }
    }

    let mut fields = Vec::new();
    for (key, values) in merged {
        let encoded_key = form_encode(key);
        for value in values {
            fields.push(format!("{}={}", encoded_key, form_encode(value)));
        }
    }
    fields.join("&")
}

fn encode_unreserved(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::new();
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
    let query_map = match url.query.as_deref() {
        Some(q) => Some(parse_query(q).map_err(CanonicalError::Query)?),
        None => None,
    };

    let mut out = format!("{}://", url.scheme);
    if let Some(userinfo) = &url.userinfo {
        let decoded = decode_component(Component::UserInfo, userinfo).map_err(|e| {
            CanonicalError::Parse(codec_to_parse(e, 0))
        })?;
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

    let omit_port = match (url.scheme.as_str(), url.port) {
        ("http", Some(80)) | ("https", Some(443)) => true,
        _ => false,
    };
    if let Some(port) = url.port {
        if !omit_port {
            out.push(':');
            out.push_str(&port.to_string());
        }
    }

    let mut encoded_segments = Vec::new();
    for segment in path.split('/') {
        let decoded = decode_component(Component::Path, segment).map_err(|e| {
            let offset = match e {
                CodecError::BadEscape { at } | CodecError::InvalidUtf8 { at } => at,
            };
            match e {
                CodecError::BadEscape { .. } => CanonicalError::Path(PathError::BadEscape { at: offset }),
                CodecError::InvalidUtf8 { .. } => CanonicalError::Path(PathError::InvalidUtf8 { at: offset }),
            }
        })?;
        encoded_segments.push(encode_unreserved(&decoded));
    }
    out.push_str(&encoded_segments.join("/"));

    if let Some(map) = query_map {
        out.push('?');
        out.push_str(&serialise_query(&map));
    }
    if let Some(fragment) = &url.fragment {
        let decoded = decode_component(Component::Fragment, fragment).map_err(|e| {
            CanonicalError::Parse(codec_to_parse(e, 0))
        })?;
        out.push('#');
        out.push_str(&encode_component(Component::Fragment, &decoded));
    }
    Ok(out)
}