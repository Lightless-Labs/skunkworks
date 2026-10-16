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
            let Some(high) = hex_value(bytes[i + 1]) else {
                return Err(CodecError::BadEscape { at: i });
            };
            let Some(low) = hex_value(bytes[i + 2]) else {
                return Err(CodecError::BadEscape { at: i });
            };
            decoded.push((high << 4) | low);
            origins.push(i);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            origins.push(i);
            i += 1;
        }
    }

    match std::str::from_utf8(&decoded) {
        Ok(text) => Ok(text.to_owned()),
        Err(error) => {
            let at = origins
                .get(error.valid_up_to())
                .copied()
                .unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at })
        }
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        if is_unreserved(byte) || component_safe_extra(component, byte) {
            out.push(byte as char);
        } else {
            push_percent(&mut out, byte);
        }
    }
    out
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }

    let bytes = input.as_bytes();
    for (at, &byte) in bytes.iter().enumerate() {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(ParseError::InvalidByte { at });
        }
    }

    let colon = match bytes.iter().position(|&b| b == b':') {
        Some(colon) => colon,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };

    if colon == 0 || !bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for (at, &byte) in bytes[..colon].iter().enumerate().skip(1) {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')) {
            return Err(ParseError::InvalidScheme { at });
        }
    }

    if bytes.get(colon + 1) != Some(&b'/') || bytes.get(colon + 2) != Some(&b'/') {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let authority_start = colon + 3;
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map(|n| authority_start + n)
        .unwrap_or(bytes.len());

    let authority = &bytes[authority_start..authority_end];
    let at_positions: Vec<usize> = authority
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i))
        .collect();

    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }
    if at_positions.first() == Some(&0) {
        return Err(ParseError::InvalidUserInfo { at: authority_start });
    }

    let (userinfo, host_start_relative) = if let Some(&at) = at_positions.first() {
        (
            Some(input[authority_start..authority_start + at].to_owned()),
            at + 1,
        )
    } else {
        (None, 0)
    };
    let host_start = authority_start + host_start_relative;
    let host_authority = &authority[host_start_relative..];

    let (host, port, ipv6) = if host_authority.first() == Some(&b'[') {
        let close = host_authority.iter().position(|&b| b == b']');
        let Some(close) = close else {
            return Err(ParseError::InvalidIpv6 { at: host_start });
        };
        let interior = &host_authority[1..close];
        let interior_str = std::str::from_utf8(interior).unwrap_or("");
        if interior_str.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: host_start });
        }

        let suffix_start = close + 1;
        let suffix = &host_authority[suffix_start..];
        let port = if suffix.is_empty() {
            None
        } else if suffix[0] == b':' {
            Some(parse_port(
                &suffix[1..],
                host_start + suffix_start + 1,
            )?)
        } else {
            return Err(ParseError::InvalidHost {
                at: host_start + suffix_start,
            });
        };

        (
            interior_str.to_ascii_lowercase(),
            port,
            true,
        )
    } else {
        let host_port_colon = host_authority.iter().position(|&b| b == b':');
        let host_end = host_port_colon.unwrap_or(host_authority.len());
        let host_bytes = &host_authority[..host_end];

        if host_bytes.is_empty() {
            return Err(ParseError::InvalidHost { at: host_start });
        }
        for (offset, &byte) in host_bytes.iter().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: host_start + offset,
                });
            }
        }

        let port = if let Some(port_colon) = host_port_colon {
            Some(parse_port(
                &host_authority[port_colon + 1..],
                host_start + port_colon + 1,
            )?)
        } else {
            None
        };

        (
            std::str::from_utf8(host_bytes)
                .unwrap_or("")
                .to_ascii_lowercase(),
            port,
            false,
        )
    };

    let after_authority = &input[authority_end..];
    let fragment_relative = after_authority.find('#');
    let before_fragment = fragment_relative
        .map(|i| &after_authority[..i])
        .unwrap_or(after_authority);
    let fragment = fragment_relative.map(|i| after_authority[i + 1..].to_owned());

    let query_relative = before_fragment.find('?');
    let before_query = query_relative
        .map(|i| &before_fragment[..i])
        .unwrap_or(before_fragment);
    let query = query_relative.map(|i| before_fragment[i + 1..].to_owned());

    let path = if before_query.is_empty() {
        "/".to_owned()
    } else {
        before_query.to_owned()
    };

    let mut validation_components: Vec<(&str, usize)> = Vec::new();
    if let (Some(userinfo), Some(at)) = (userinfo.as_deref(), at_positions.first()) {
        validation_components.push((userinfo, authority_start));
        let _ = at;
    }
    if before_query.len() > 0 {
        validation_components.push((before_query, authority_end));
    }
    if let Some(query) = query.as_deref() {
        let query_at = authority_end + query_relative.unwrap() + 1;
        validation_components.push((query, query_at));
    }
    if let Some(fragment) = fragment.as_deref() {
        let fragment_at = authority_end + fragment_relative.unwrap() + 1;
        validation_components.push((fragment, fragment_at));
    }

    for (component, offset) in validation_components {
        let kind = if userinfo.as_deref() == Some(component)
            && offset == authority_start
            && at_positions.first().is_some()
        {
            Component::UserInfo
        } else if query.as_deref() == Some(component) {
            Component::Query
        } else if fragment.as_deref() == Some(component) {
            Component::Fragment
        } else {
            Component::Path
        };
        if let Err(error) = decode_component(kind, component) {
            return Err(rebase_parse_error(error, offset));
        }
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
    decode_component(Component::Path, path).map_err(|error| match error {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    })?;

    let mut stack: Vec<&str> = Vec::new();
    let mut last_navigation = false;
    for segment in path[1..].split('/') {
        let decoded = decode_component(Component::Path, segment).map_err(|error| match error {
            CodecError::BadEscape { at } => PathError::BadEscape { at },
            CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
        })?;
        if decoded == "." {
            last_navigation = true;
        } else if decoded == ".." {
            if !stack.is_empty() {
                stack.pop();
            }
            last_navigation = true;
        } else {
            stack.push(segment);
            last_navigation = false;
        }
    }

    if last_navigation {
        stack.push("");
    }

    if stack.is_empty() {
        Ok("/".to_owned())
    } else {
        Ok(format!("/{}", stack.join("/")))
    }
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let mut base_url = parse_url(base).map_err(ResolveError::Base)?;
    base_url.path = normalise_path(&base_url.path).map_err(ResolveError::Path)?;

    for (at, &byte) in reference.as_bytes().iter().enumerate() {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(ResolveError::Reference(ParseError::InvalidByte { at }));
        }
    }

    let delimiter = reference
        .as_bytes()
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(reference.len());
    if reference.as_bytes()[..delimiter].contains(&b':') {
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

    let (ref_before_fragment, fragment) = match reference.find('#') {
        Some(i) => (&reference[..i], Some(reference[i + 1..].to_owned())),
        None => (reference, None),
    };
    let (reference_path, reference_query) = match ref_before_fragment.find('?') {
        Some(i) => (
            &ref_before_fragment[..i],
            Some(ref_before_fragment[i + 1..].to_owned()),
        ),
        None => (ref_before_fragment, None),
    };

    let (selected_path, selected_query) = if reference_path.is_empty() {
        (
            base_url.path.clone(),
            if reference_query.is_some() {
                reference_query
            } else {
                base_url.query.clone()
            },
        )
    } else {
        let merged = if reference_path.starts_with('/') {
            reference_path.to_owned()
        } else {
            let slash = base_url.path.rfind('/').map(|i| &base_url.path[..=i]).unwrap_or("/");
            format!("{slash}{reference_path}")
        };
        (merged, reference_query)
    };

    let constructed = build_raw_url(
        &base_url,
        &selected_path,
        selected_query.as_deref(),
        fragment.as_deref(),
    );
    let mut url = parse_url(&constructed).map_err(ResolveError::Reference)?;
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

pub fn parse_query(input: &str) -> Result<QueryMap, QueryError> {
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();

    for (field_start, field) in split_with_offsets(input, b'&') {
        if field.is_empty() {
            continue;
        }

        let (raw_key, raw_value, has_equals) = match field.find('=') {
            Some(i) => (&field[..i], &field[i + 1..], true),
            None => (field, "", false),
        };
        let _ = has_equals;

        let key_input = raw_key.replace('+', " ");
        let key = decode_component(Component::Query, &key_input).map_err(|error| {
            rebase_query_error(error, field_start)
        })?;
        if key.is_empty() {
            return Err(QueryError::EmptyKey { at: field_start });
        }

        let value_start = field_start + raw_key.len() + 1;
        let value_input = raw_value.replace('+', " ");
        let value = decode_component(Component::Query, &value_input)
            .map_err(|error| rebase_query_error(error, value_start))?;

        if let Some((_, values)) = entries.iter_mut().find(|(existing, _)| *existing == key) {
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
        if let Some((_, existing_values)) = merged.iter_mut().find(|(existing, _)| existing == key) {
            existing_values.extend(values.iter().cloned());
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

pub fn canonical_url(input: &str) -> Result<String, CanonicalError> {
    let url = parse_url(input).map_err(CanonicalError::Parse)?;
    let normalized_path =
        normalise_path(&url.path).map_err(CanonicalError::Path)?;
    let query_map = match url.query.as_deref() {
        Some(query) => Some(parse_query(query).map_err(CanonicalError::Query)?),
        None => None,
    };

    let mut out = format!("{}://", url.scheme);
    if let Some(userinfo) = url.userinfo.as_deref() {
        let decoded =
            decode_component(Component::UserInfo, userinfo).map_err(|error| {
                CanonicalError::Parse(match error {
                    CodecError::BadEscape { at } => ParseError::BadEscape { at },
                    CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at },
                })
            })?;
        out.push_str(&encode_component(Component::UserInfo, &decoded));
        out.push('@');
    }

    if url.ipv6 {
        let address = url
            .host
            .parse::<std::net::Ipv6Addr>()
            .map_err(|_| CanonicalError::Parse(ParseError::InvalidIpv6 { at: 0 }))?;
        out.push('[');
        out.push_str(&address.to_string());
        out.push(']');
    } else {
        out.push_str(&url.host);
    }

    let omit_port = matches!(
        (url.scheme.as_str(), url.port),
        ("http", Some(80)) | ("https", Some(443))
    );
    if let Some(port) = url.port.filter(|_| !omit_port) {
        out.push(':');
        out.push_str(&port.to_string());
    }

    let mut canonical_segments = Vec::new();
    for segment in normalized_path.split('/') {
        let decoded = decode_component(Component::Path, segment).map_err(|error| {
            CanonicalError::Path(match error {
                CodecError::BadEscape { at } => PathError::BadEscape { at },
                CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
            })
        })?;
        canonical_segments.push(encode_unreserved_only(&decoded));
    }
    out.push_str(&canonical_segments.join("/"));

    if let Some(map) = query_map {
        out.push('?');
        out.push_str(&serialise_query(&map));
    }
    if let Some(fragment) = url.fragment.as_deref() {
        let decoded = decode_component(Component::Fragment, fragment).map_err(|error| {
            CanonicalError::Parse(match error {
                CodecError::BadEscape { at } => ParseError::BadEscape { at },
                CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at },
            })
        })?;
        out.push('#');
        out.push_str(&encode_component(Component::Fragment, &decoded));
    }

    Ok(out)
}

fn parse_port(bytes: &[u8], start: usize) -> Result<u16, ParseError> {
    if bytes.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }

    if let Some((offset, _)) = bytes.iter().enumerate().find(|(_, byte)| !byte.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: start + offset });
    }
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }

    let mut value: u32 = 0;
    for &byte in bytes {
        value = match value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
        {
            Some(value) => value,
            None => return Err(ParseError::InvalidPort { at: start }),
        };
    }
    if !(1..=65535).contains(&value) {
        return Err(ParseError::InvalidPort { at: start });
    }
    Ok(value as u16)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn component_safe_extra(component: Component, byte: u8) -> bool {
    match component {
        Component::UserInfo => byte == b':',
        Component::Path => byte == b'/',
        Component::Query => matches!(byte, b'/' | b'?'),
        Component::Fragment => matches!(byte, b'/' | b'?' | b'#'),
    }
}

fn push_percent(out: &mut String, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push('%');
    out.push(HEX[(byte >> 4) as usize] as char);
    out.push(HEX[(byte & 0x0f) as usize] as char);
}

fn rebase_parse_error(error: CodecError, offset: usize) -> ParseError {
    match error {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: offset + at },
    }
}

fn rebase_query_error(error: CodecError, offset: usize) -> QueryError {
    match error {
        CodecError::BadEscape { at } => QueryError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 { at: offset + at },
    }
}

fn split_with_offsets(input: &str, delimiter: u8) -> Vec<(usize, &str)> {
    let bytes = input.as_bytes();
    let mut result = Vec::new();
    let mut start = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        if byte == delimiter {
            result.push((start, &input[start..i]));
            start = i + 1;
        }
    }
    result.push((start, &input[start..]));
    result
}

fn form_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        if is_unreserved(byte) {
            out.push(byte as char);
        } else if byte == b' ' {
            out.push('+');
        } else {
            push_percent(&mut out, byte);
        }
    }
    out
}

fn encode_unreserved_only(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        if is_unreserved(byte) {
            out.push(byte as char);
        } else {
            push_percent(&mut out, byte);
        }
    }
    out
}

fn build_raw_url(
    base: &Url,
    path: &str,
    query: Option<&str>,
    fragment: Option<&str>,
) -> String {
    let mut out = format!("{}://", base.scheme);
    if let Some(userinfo) = base.userinfo.as_deref() {
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