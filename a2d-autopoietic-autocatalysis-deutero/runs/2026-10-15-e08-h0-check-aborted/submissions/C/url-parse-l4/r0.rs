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

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
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
            let (Some(hi), Some(lo)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) else {
                return Err(CodecError::BadEscape { at: i });
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
        Ok(text) => Ok(text.to_owned()),
        Err(error) => {
            let at = offsets
                .get(error.valid_up_to())
                .copied()
                .unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at })
        }
    }
}

fn unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn encode_with_safe(text: &str, safe: impl Fn(u8) -> bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        if safe(byte) {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    output
}

pub fn encode_component(component: Component, text: &str) -> String {
    encode_with_safe(text, |byte| {
        unreserved(byte)
            || match component {
                Component::UserInfo => byte == b':',
                Component::Path => byte == b'/',
                Component::Query => matches!(byte, b'/' | b'?'),
                Component::Fragment => matches!(byte, b'/' | b'?' | b'#'),
            }
    })
}

fn parse_codec_error(error: CodecError, offset: usize) -> ParseError {
    match error {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: offset + at },
    }
}

fn validate_component(
    component: Component,
    value: &str,
    offset: usize,
) -> Result<(), ParseError> {
    decode_component(component, value)
        .map(|_| ())
        .map_err(|error| parse_codec_error(error, offset))
}

fn parse_port(text: &str, start: usize) -> Result<u16, ParseError> {
    if text.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }
    for (i, byte) in text.bytes().enumerate() {
        if !byte.is_ascii_digit() {
            return Err(ParseError::InvalidPort { at: start + i });
        }
    }
    if text.len() >= 2 && text.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }

    let mut value: u32 = 0;
    for byte in text.bytes() {
        value = match value
            .checked_mul(10)
            .and_then(|number| number.checked_add((byte - b'0') as u32))
        {
            Some(number) => number,
            None => return Err(ParseError::InvalidPort { at: start }),
        };
    }
    if !(1..=65535).contains(&value) {
        return Err(ParseError::InvalidPort { at: start });
    }
    Ok(value as u16)
}

fn authority_end(input: &str, start: usize) -> usize {
    input[start..]
        .bytes()
        .position(|byte| matches!(byte, b'/' | b'?' | b'#'))
        .map(|index| start + index)
        .unwrap_or(input.len())
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    for (i, byte) in input.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(ParseError::InvalidByte { at: i });
        }
    }

    let colon = input
        .find(':')
        .ok_or(ParseError::InvalidScheme { at: input.len() })?;
    let scheme_bytes = &input.as_bytes()[..colon];
    if scheme_bytes.is_empty() || !scheme_bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for (i, &byte) in scheme_bytes.iter().enumerate().skip(1) {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')) {
            return Err(ParseError::InvalidScheme { at: i });
        }
    }

    let after_colon = colon + 1;
    if input.as_bytes().get(after_colon) != Some(&b'/')
        || input.as_bytes().get(after_colon + 1) != Some(&b'/')
    {
        return Err(ParseError::MissingAuthority { at: after_colon });
    }

    let auth_start = after_colon + 2;
    let auth_end = authority_end(input, auth_start);
    let authority = &input[auth_start..auth_end];

    let mut at_positions = authority
        .bytes()
        .enumerate()
        .filter_map(|(index, byte)| (byte == b'@').then_some(index));
    let first_at = at_positions.next();
    if let Some(second_at) = at_positions.next() {
        return Err(ParseError::InvalidUserInfo {
            at: auth_start + second_at,
        });
    }

    let (userinfo, hostport_start) = match first_at {
        Some(at) if at == 0 => {
            return Err(ParseError::InvalidUserInfo { at: auth_start });
        }
        Some(at) => (Some(authority[..at].to_owned()), at + 1),
        None => (None, 0),
    };

    let host_start = auth_start + hostport_start;
    let hostport = &authority[hostport_start..];

    let (host, port, ipv6) = if hostport.starts_with('[') {
        let opening = host_start;
        let close = hostport
            .find(']')
            .ok_or(ParseError::InvalidIpv6 { at: opening })?;
        let interior = &hostport[1..close];
        if interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: opening });
        }

        let suffix = &hostport[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(parse_port(
                &suffix[1..],
                host_start + close + 2,
            )?)
        } else {
            return Err(ParseError::InvalidHost {
                at: host_start + close + 1,
            });
        };
        (interior.to_ascii_lowercase(), port, true)
    } else {
        let colon_in_host = hostport.find(':');
        let (host_raw, port) = if let Some(index) = colon_in_host {
            (
                &hostport[..index],
                Some(parse_port(
                    &hostport[index + 1..],
                    host_start + index + 1,
                )?),
            )
        } else {
            (hostport, None)
        };

        if host_raw.is_empty() {
            return Err(ParseError::InvalidHost { at: host_start });
        }
        for (index, byte) in host_raw.bytes().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: host_start + index,
                });
            }
        }
        (host_raw.to_ascii_lowercase(), port, false)
    };

    if let Some(value) = &userinfo {
        validate_component(Component::UserInfo, value, auth_start)?;
    }

    let tail = &input[auth_end..];
    let fragment_pos = tail.find('#');
    let before_fragment = fragment_pos.map(|i| &tail[..i]).unwrap_or(tail);
    let fragment = fragment_pos.map(|i| tail[i + 1..].to_owned());

    let query_pos = before_fragment.find('?');
    let before_query = query_pos
        .map(|i| &before_fragment[..i])
        .unwrap_or(before_fragment);
    let query = query_pos.map(|i| before_fragment[i + 1..].to_owned());

    let path = if before_query.is_empty() {
        "/".to_owned()
    } else {
        before_query.to_owned()
    };

    if !before_query.is_empty() {
        validate_component(Component::Path, &path, auth_end)?;
    }
    if let Some(value) = &query {
        validate_component(
            Component::Query,
            value,
            auth_end + query_pos.unwrap() + 1,
        )?;
    }
    if let Some(value) = &fragment {
        validate_component(
            Component::Fragment,
            value,
            auth_end + fragment_pos.unwrap() + 1,
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

fn path_codec_error(error: CodecError) -> PathError {
    match error {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    }
}

pub fn normalise_path(path: &str) -> Result<String, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }
    decode_component(Component::Path, path).map_err(path_codec_error)?;

    let segments: Vec<&str> = path[1..].split('/').collect();
    let mut stack: Vec<String> = Vec::new();
    let mut terminal_navigation = false;

    for (index, segment) in segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, segment).map_err(path_codec_error)?;
        let is_dot = decoded == ".";
        let is_dotdot = decoded == "..";
        if index + 1 == segments.len() {
            terminal_navigation = is_dot || is_dotdot;
        }

        if is_dot {
            continue;
        } else if is_dotdot {
            stack.pop();
        } else {
            stack.push((*segment).to_owned());
        }
    }

    if terminal_navigation {
        stack.push(String::new());
    }
    if stack.is_empty() {
        Ok("/".to_owned())
    } else {
        Ok(format!("/{}", stack.join("/")))
    }
}

fn check_reference_bytes(reference: &str) -> Result<(), ParseError> {
    for (index, byte) in reference.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(ParseError::InvalidByte { at: index });
        }
    }
    Ok(())
}

fn normalize_url_path(mut url: Url) -> Result<Url, ResolveError> {
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let base_path = normalise_path(&base_url.path).map_err(ResolveError::Path)?;
    check_reference_bytes(reference).map_err(ResolveError::Reference)?;

    if reference.starts_with("//") {
        let constructed = format!("{}:{}", base_url.scheme, reference);
        let url = parse_url(&constructed).map_err(ResolveError::Reference)?;
        return normalize_url_path(url);
    }

    let delimiter = reference
        .bytes()
        .position(|byte| matches!(byte, b'/' | b'?' | b'#'))
        .unwrap_or(reference.len());
    if reference[..delimiter].contains(':') {
        let url = parse_url(reference).map_err(ResolveError::Reference)?;
        return normalize_url_path(url);
    }

    let fragment_pos = reference.find('#');
    let before_fragment = fragment_pos
        .map(|index| &reference[..index])
        .unwrap_or(reference);
    let fragment = fragment_pos.map(|index| reference[index + 1..].to_owned());

    let query_pos = before_fragment.find('?');
    let reference_path = query_pos
        .map(|index| &before_fragment[..index])
        .unwrap_or(before_fragment);
    let reference_query = query_pos.map(|index| before_fragment[index + 1..].to_owned());

    let (selected_path, selected_query) = if reference_path.is_empty() {
        (
            base_path.clone(),
            if query_pos.is_some() {
                reference_query
            } else {
                base_url.query.clone()
            },
        )
    } else {
        let path = if reference_path.starts_with('/') {
            reference_path.to_owned()
        } else {
            let slash = base_path.rfind('/').map(|index| index + 1).unwrap_or(0);
            format!("{}{}", &base_path[..slash], reference_path)
        };
        (path, reference_query)
    };

    let mut raw = format!("{}://", base_url.scheme);
    if let Some(userinfo) = &base_url.userinfo {
        raw.push_str(userinfo);
        raw.push('@');
    }
    if base_url.ipv6 {
        raw.push('[');
        raw.push_str(&base_url.host);
        raw.push(']');
    } else {
        raw.push_str(&base_url.host);
    }
    if let Some(port) = base_url.port {
        raw.push(':');
        raw.push_str(&port.to_string());
    }
    raw.push_str(&selected_path);
    if let Some(query) = &selected_query {
        raw.push('?');
        raw.push_str(query);
    }
    if let Some(fragment) = &fragment {
        raw.push('#');
        raw.push_str(fragment);
    }

    let mut url = parse_url(&raw).map_err(ResolveError::Reference)?;
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
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

fn query_codec_error(error: CodecError, offset: usize) -> QueryError {
    match error {
        CodecError::BadEscape { at } => QueryError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 { at: offset + at },
    }
}

fn decode_query_part(text: &str, offset: usize) -> Result<String, QueryError> {
    let replaced = text.replace('+', " ");
    decode_component(Component::Query, &replaced)
        .map_err(|error| query_codec_error(error, offset))
}

pub fn parse_query(input: &str) -> Result<QueryMap, QueryError> {
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    let mut offset = 0;

    for field in input.split('&') {
        let field_len = field.len();
        if !field.is_empty() {
            let equals = field.find('=');
            let (key_raw, value_raw, value_offset) = match equals {
                Some(index) => (
                    &field[..index],
                    &field[index + 1..],
                    offset + index + 1,
                ),
                None => (field, "", offset + field.len()),
            };

            let key = decode_query_part(key_raw, offset)?;
            if key.is_empty() {
                return Err(QueryError::EmptyKey { at: offset });
            }
            let value = decode_query_part(value_raw, value_offset)?;

            if let Some((_, values)) = entries
                .iter_mut()
                .find(|(existing, _)| *existing == key)
            {
                values.push(value);
            } else {
                entries.push((key, vec![value]));
            }
        }
        offset += field_len + 1;
    }

    Ok(QueryMap { entries })
}

fn form_encode(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::new();
    for &byte in text.as_bytes() {
        if byte == b' ' {
            output.push('+');
        } else if unreserved(byte) {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    output
}

pub fn serialise_query(map: &QueryMap) -> String {
    let mut merged: Vec<(String, Vec<String>)> = Vec::new();
    for (key, values) in &map.entries {
        if key.is_empty() || values.is_empty() {
            continue;
        }
        if let Some((_, existing_values)) = merged
            .iter_mut()
            .find(|(existing, _)| existing == key)
        {
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
    let path = normalise_path(&url.path).map_err(CanonicalError::Path)?;
    let query = match &url.query {
        Some(value) => Some(parse_query(value).map_err(CanonicalError::Query)?),
        None => None,
    };

    let mut output = format!("{}://", url.scheme);
    if let Some(userinfo) = &url.userinfo {
        let decoded = decode_component(Component::UserInfo, userinfo)
            .expect("userinfo is validated by parse_url");
        output.push_str(&encode_component(Component::UserInfo, &decoded));
        output.push('@');
    }

    if url.ipv6 {
        let address = url
            .host
            .parse::<std::net::Ipv6Addr>()
            .expect("IPv6 host is validated by parse_url");
        output.push('[');
        output.push_str(&address.to_string());
        output.push(']');
    } else {
        output.push_str(&url.host);
    }

    if let Some(port) = url.port {
        let is_default =
            (url.scheme == "http" && port == 80) || (url.scheme == "https" && port == 443);
        if !is_default {
            output.push(':');
            output.push_str(&port.to_string());
        }
    }

    let mut encoded_segments = Vec::new();
    for segment in path.split('/') {
        let decoded = decode_component(Component::Path, segment)
            .expect("normalized path is validated");
        encoded_segments.push(encode_with_safe(&decoded, unreserved));
    }
    output.push_str(&encoded_segments.join("/"));

    if let Some(query_map) = query {
        output.push('?');
        output.push_str(&serialise_query(&query_map));
    }
    if let Some(fragment) = &url.fragment {
        let decoded = decode_component(Component::Fragment, fragment)
            .expect("fragment is validated by parse_url");
        output.push('#');
        output.push_str(&encode_component(Component::Fragment, &decoded));
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_checks_authority_and_components() {
        assert_eq!(
            parse_url("HTTP://Example.COM").unwrap(),
            Url {
                scheme: "http".into(),
                userinfo: None,
                host: "example.com".into(),
                port: None,
                path: "/".into(),
                query: None,
                fragment: None,
                ipv6: false,
            }
        );
        assert_eq!(
            parse_url("http://[::1]:8443/a%20b").unwrap().host,
            "::1"
        );
        assert_eq!(
            parse_url("http://h/%ZZ"),
            Err(ParseError::BadEscape { at: 9 })
        );
    }

    #[test]
    fn codec_preserves_offsets_and_components() {
        assert_eq!(
            decode_component(Component::Path, "%FF%").unwrap_err(),
            CodecError::BadEscape { at: 3 }
        );
        assert_eq!(
            decode_component(Component::Path, "%FF").unwrap_err(),
            CodecError::InvalidUtf8 { at: 0 }
        );
        assert_eq!(
            encode_component(Component::Path, "a b/c"),
            "a%20b/c"
        );
    }

    #[test]
    fn paths_resolution_and_queries() {
        assert_eq!(normalise_path("/a/./b/../c").unwrap(), "/a/c");
        assert_eq!(normalise_path("/a//../b").unwrap(), "/a/b");
        let resolved = resolve_url("http://example/a/b", "../c?q#f").unwrap();
        assert_eq!(resolved.path, "/c");
        assert_eq!(resolved.query.as_deref(), Some("q"));

        let query = parse_query("x=one+two&x=three&z=").unwrap();
        assert_eq!(
            query.entries,
            vec![
                ("x".to_owned(), vec!["one two".to_owned(), "three".to_owned()]),
                ("z".to_owned(), vec!["".to_owned()])
            ]
        );
        assert_eq!(serialise_query(&query), "x=one+two&x=three&z=");
    }

    #[test]
    fn canonical_form_is_stable() {
        let input = "HTTP://Example:80/a/./b?x=one+two#top";
        let canonical = canonical_url(input).unwrap();
        assert_eq!(canonical, "http://example/a/b?x=one+two#top");
        assert_eq!(canonical_url(&canonical).unwrap(), canonical);
    }
}

fn main() {}