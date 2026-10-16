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
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
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

fn parse_port(port: &str, at: usize) -> Result<Option<u16>, ParseError> {
    if port.is_empty() {
        return Err(ParseError::InvalidPort { at });
    }

    if let Some(i) = port.bytes().position(|b| !b.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: at + i });
    }

    let bytes = port.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at });
    }

    let mut value: u32 = 0;
    for &b in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or(ParseError::InvalidPort { at })?;
        if value > 65535 {
            return Err(ParseError::InvalidPort { at });
        }
    }

    if value == 0 {
        return Err(ParseError::InvalidPort { at });
    }
    Ok(Some(value as u16))
}

fn validate_component(raw: &str, base: usize) -> Result<(), ParseError> {
    match decode_component(Component::Path, raw) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape { at: base + at }),
        Err(CodecError::InvalidUtf8 { at }) => Err(ParseError::InvalidUtf8 { at: base + at }),
    }
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

    let auth_start = colon + 3;
    let auth_end = bytes[auth_start..]
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map(|i| auth_start + i)
        .unwrap_or(bytes.len());
    let authority = &input[auth_start..auth_end];
    let auth_bytes = authority.as_bytes();

    let mut ats = auth_bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i));
    let first_at = ats.next();
    if let Some(second_at) = ats.next() {
        return Err(ParseError::InvalidUserInfo {
            at: auth_start + second_at,
        });
    }

    let (userinfo, host_start_rel) = if let Some(at) = first_at {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo { at: auth_start });
        }
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };
    let host_start = auth_start + host_start_rel;
    let hostport = &authority[host_start_rel..];

    let (host, port, ipv6) = if hostport.starts_with('[') {
        let close = match hostport.as_bytes().iter().position(|&b| b == b']') {
            Some(i) => i,
            None => return Err(ParseError::InvalidIpv6 { at: host_start }),
        };
        let interior = &hostport[1..close];
        let parsed = match interior.parse::<std::net::Ipv6Addr>() {
            Ok(addr) => addr,
            Err(_) => return Err(ParseError::InvalidIpv6 { at: host_start }),
        };
        let suffix = &hostport[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            parse_port(&suffix[1..], host_start + close + 2)?
        } else {
            return Err(ParseError::InvalidHost {
                at: host_start + close + 1,
            });
        };
        (interior.to_ascii_lowercase(), port, true)
    } else {
        let host_colon = hostport.as_bytes().iter().position(|&b| b == b':');
        let (raw_host, port, port_at) = if let Some(i) = host_colon {
            (&hostport[..i], Some(&hostport[i + 1..]), host_start + i + 1)
        } else {
            (hostport, None, 0)
        };

        if raw_host.is_empty() {
            return Err(ParseError::InvalidHost { at: host_start });
        }
        for (i, b) in raw_host.bytes().enumerate() {
            if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost { at: host_start + i });
            }
        }

        let port = match port {
            Some(p) => parse_port(p, port_at)?,
            None => None,
        };
        (raw_host.to_ascii_lowercase(), port, false)
    };

    let fragment_mark = bytes[auth_end..].iter().position(|&b| b == b'#')
        .map(|i| auth_end + i);
    let before_fragment_end = fragment_mark.unwrap_or(bytes.len());
    let query_mark = bytes[auth_end..before_fragment_end]
        .iter()
        .position(|&b| b == b'?')
        .map(|i| auth_end + i);

    let path_end = query_mark.unwrap_or(before_fragment_end);
    let raw_path = &input[auth_end..path_end];
    let path = if raw_path.is_empty() { "/".to_owned() } else { raw_path.to_owned() };

    let query = query_mark.map(|q| {
        input[q + 1..before_fragment_end].to_owned()
    });
    let fragment = fragment_mark.map(|f| input[f + 1..].to_owned());

    if let Some(ref user) = userinfo {
        validate_component(user, auth_start)?;
    }
    if !raw_path.is_empty() {
        validate_component(raw_path, auth_end)?;
    }
    if let Some(q) = query_mark {
        validate_component(&input[q + 1..before_fragment_end], q + 1)?;
    }
    if let Some(f) = fragment_mark {
        validate_component(&input[f + 1..], f + 1)?;
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

fn path_codec_error(err: CodecError) -> PathError {
    match err {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    }
}

pub fn normalise_path(path: &str) -> Result<String, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }
    decode_component(Component::Path, path).map_err(path_codec_error)?;

    let mut stack: Vec<&str> = Vec::new();
    let mut segments = path[1..].split('/');
    let mut last_navigation = false;

    for segment in segments.by_ref() {
        let decoded = decode_component(Component::Path, segment).map_err(path_codec_error)?;
        match decoded.as_str() {
            "." => last_navigation = true,
            ".." => {
                stack.pop();
                last_navigation = true;
            }
            _ => {
                stack.push(segment);
                last_navigation = false;
            }
        }
    }

    if last_navigation {
        stack.push("");
    }

    if stack.is_empty() {
        Ok("/".to_owned())
    } else {
        let mut out = String::from("/");
        out.push_str(&stack.join("/"));
        Ok(out)
    }
}

fn scan_reference_bytes(reference: &str) -> Result<(), ParseError> {
    for (at, b) in reference.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(ParseError::InvalidByte { at });
        }
    }
    Ok(())
}

fn split_reference(reference: &str) -> (&str, Option<&str>, Option<&str>) {
    let fragment_at = reference.find('#');
    let before_fragment = fragment_at.map(|i| &reference[..i]).unwrap_or(reference);
    let fragment = fragment_at.map(|i| &reference[i + 1..]);

    let query_at = before_fragment.find('?');
    let path = query_at.map(|i| &before_fragment[..i]).unwrap_or(before_fragment);
    let query = query_at.map(|i| &before_fragment[i + 1..]);
    (path, query, fragment)
}

fn build_absolute(
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
    if let Some(user) = userinfo {
        out.push_str(user);
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

    scan_reference_bytes(reference).map_err(ResolveError::Reference)?;

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

    let (reference_path, reference_query, reference_fragment) = split_reference(reference);
    let (path, query) = if reference_path.is_empty() {
        (
            base_path.clone(),
            if reference_query.is_none() {
                base_url.query.as_deref()
            } else {
                reference_query
            },
        )
    } else {
        let merged = if reference_path.starts_with('/') {
            reference_path.to_owned()
        } else {
            let slash = base_path.rfind('/').map(|i| i + 1).unwrap_or(0);
            let mut m = base_path[..slash].to_owned();
            m.push_str(reference_path);
            m
        };
        (merged, reference_query)
    };

    let constructed = build_absolute(
        &base_url.scheme,
        base_url.userinfo.as_deref(),
        &base_url.host,
        base_url.ipv6,
        base_url.port,
        &path,
        query,
        reference_fragment,
    );
    let mut url = parse_url(&constructed).map_err(ResolveError::Reference)?;
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

fn form_decode_plus(text: &str) -> String {
    text.replace('+', " ")
}

fn query_codec_error(error: CodecError, base: usize) -> QueryError {
    match error {
        CodecError::BadEscape { at } => QueryError::BadEscape { at: base + at },
        CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 { at: base + at },
    }
}

pub fn parse_query(input: &str) -> Result<QueryMap, QueryError> {
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    let mut offset = 0;

    for field in input.split('&') {
        let field_start = offset;
        offset += field.len() + 1;

        if field.is_empty() {
            continue;
        }

        let equals = field.find('=');
        let (raw_key, raw_value, value_offset) = match equals {
            Some(i) => (&field[..i], &field[i + 1..], field_start + i + 1),
            None => (field, "", field_start + field.len()),
        };

        let key_input = form_decode_plus(raw_key);
        let key = decode_component(Component::Query, &key_input)
            .map_err(|e| query_codec_error(e, field_start))?;
        if key.is_empty() {
            return Err(QueryError::EmptyKey { at: field_start });
        }

        let value_input = form_decode_plus(raw_value);
        let value = decode_component(Component::Query, &value_input)
            .map_err(|e| query_codec_error(e, value_offset))?;

        if let Some((_, values)) = entries.iter_mut().find(|(k, _)| *k == key) {
            values.push(value);
        } else {
            entries.push((key, vec![value]));
        }
    }

    Ok(QueryMap { entries })
}

fn form_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &b in text.as_bytes() {
        if b == b' ' {
            out.push('+');
        } else if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
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

pub fn serialise_query(map: &QueryMap) -> String {
    let mut merged: Vec<(&str, Vec<&str>)> = Vec::new();

    for (key, values) in &map.entries {
        if key.is_empty() || values.is_empty() {
            continue;
        }
        if let Some((_, accumulated)) = merged.iter_mut().find(|(k, _)| *k == key) {
            accumulated.extend(values.iter().map(String::as_str));
        } else {
            merged.push((key.as_str(), values.iter().map(String::as_str).collect()));
        }
    }

    let mut fields = Vec::new();
    for (key, values) in merged {
        let key = form_encode(key);
        for value in values {
            fields.push(format!("{}={}", key, form_encode(value)));
        }
    }
    fields.join("&")
}

pub fn canonical_url(input: &str) -> Result<String, CanonicalError> {
    let url = parse_url(input).map_err(CanonicalError::Parse)?;
    let path = normalise_path(&url.path).map_err(CanonicalError::Path)?;
    let query_map = match url.query.as_deref() {
        Some(query) => Some(parse_query(query).map_err(CanonicalError::Query)?),
        None => None,
    };

    let mut out = String::new();
    out.push_str(&url.scheme);
    out.push_str("://");

    if let Some(userinfo) = url.userinfo.as_deref() {
        let decoded = decode_component(Component::UserInfo, userinfo)
            .map_err(|e| match e {
                CodecError::BadEscape { at } => CanonicalError::Parse(ParseError::BadEscape { at }),
                CodecError::InvalidUtf8 { at } => CanonicalError::Parse(ParseError::InvalidUtf8 { at }),
            })?;
        out.push_str(&encode_component(Component::UserInfo, &decoded));
        out.push('@');
    }

    if url.ipv6 {
        let addr = url.host.parse::<std::net::Ipv6Addr>()
            .expect("validated IPv6 host");
        out.push('[');
        out.push_str(&addr.to_string());
        out.push(']');
    } else {
        out.push_str(&url.host);
    }

    let omit_port = matches!((url.scheme.as_str(), url.port), ("http", Some(80)) | ("https", Some(443)));
    if let Some(port) = url.port.filter(|_| !omit_port) {
        out.push(':');
        out.push_str(&port.to_string());
    }

    let mut encoded_segments = Vec::new();
    for segment in path.split('/') {
        let decoded = decode_component(Component::Path, segment)
            .map_err(|e| match e {
                CodecError::BadEscape { at } => CanonicalError::Path(PathError::BadEscape { at }),
                CodecError::InvalidUtf8 { at } => CanonicalError::Path(PathError::InvalidUtf8 { at }),
            })?;
        let mut encoded = String::new();
        for &b in decoded.as_bytes() {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
                encoded.push(b as char);
            } else {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                encoded.push('%');
                encoded.push(HEX[(b >> 4) as usize] as char);
                encoded.push(HEX[(b & 0x0f) as usize] as char);
            }
        }
        encoded_segments.push(encoded);
    }
    out.push_str(&encoded_segments.join("/"));

    if let Some(query) = query_map.as_ref() {
        out.push('?');
        out.push_str(&serialise_query(query));
    }

    if let Some(fragment) = url.fragment.as_deref() {
        let decoded = decode_component(Component::Fragment, fragment)
            .map_err(|e| match e {
                CodecError::BadEscape { at } => CanonicalError::Parse(ParseError::BadEscape { at }),
                CodecError::InvalidUtf8 { at } => CanonicalError::Parse(ParseError::InvalidUtf8 { at }),
            })?;
        out.push('#');
        out.push_str(&encode_component(Component::Fragment, &decoded));
    }

    Ok(out)
}