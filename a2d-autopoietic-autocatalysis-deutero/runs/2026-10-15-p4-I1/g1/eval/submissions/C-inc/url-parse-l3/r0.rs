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

fn is_ascii_letter(b: u8) -> bool {
    b.is_ascii_alphabetic()
}

fn scan_global(input: &str) -> Result<(), ParseError> {
    for (i, b) in input.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(ParseError::InvalidByte { at: i });
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
            let at = offsets.get(e.valid_up_to()).copied().unwrap_or(input.len());
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

fn map_codec_error(error: CodecError) -> ParseError {
    match error {
        CodecError::BadEscape { at } => ParseError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at },
    }
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    scan_global(input)?;

    let bytes = input.as_bytes();
    let colon = match bytes.iter().position(|&b| b == b':') {
        Some(i) => i,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };
    if colon == 0 || !is_ascii_letter(bytes[0]) {
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
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map(|n| authority_start + n)
        .unwrap_or(bytes.len());
    let authority = &input[authority_start..authority_end];

    let mut at_positions = authority.match_indices('@').map(|(i, _)| i);
    let first_at = at_positions.next();
    if let Some(second_at) = at_positions.next() {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + second_at,
        });
    }
    let (userinfo, hostport, hostport_start) = if let Some(at) = first_at {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start,
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
        let close = match hostport.find(']') {
            Some(i) => i,
            None => {
                return Err(ParseError::InvalidIpv6 {
                    at: hostport_start,
                });
            }
        };
        let interior = &hostport[1..close];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 {
                at: hostport_start,
            });
        }
        let suffix = &hostport[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some((&suffix[1..], hostport_start + close + 2))
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_start + close + 1,
            });
        };
        ipv6 = true;
        (interior.to_ascii_lowercase(), port)
    } else {
        let split = hostport.find(':');
        let (host_text, port) = if let Some(i) = split {
            (
                &hostport[..i],
                Some((&hostport[i + 1..], hostport_start + i + 1)),
            )
        } else {
            (hostport, None)
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

    let port = if let Some((port_text, port_start)) = port {
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
        let pbytes = port_text.as_bytes();
        if pbytes.len() >= 2 && pbytes[0] == b'0' {
            return Err(ParseError::InvalidPort { at: port_start });
        }
        let mut value: u32 = 0;
        for &b in pbytes {
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
        Some(value as u16)
    } else {
        None
    };

    let rest = &input[authority_end..];
    let fragment_pos = rest.find('#');
    let before_fragment = fragment_pos.map(|i| &rest[..i]).unwrap_or(rest);
    let fragment = fragment_pos.map(|i| rest[i + 1..].to_owned());

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

    let mut component_offset = authority_start;
    if let Some(ref ui) = userinfo {
        match decode_component(Component::UserInfo, ui) {
            Ok(_) => {}
            Err(e) => return Err(map_codec_error(rebase_codec(e, component_offset))),
        }
        component_offset += ui.len() + 1;
    }

    if path != "/" || before_query.len() > 0 {
        let path_start = authority_end;
        match decode_component(Component::Path, &path) {
            Ok(_) => {}
            Err(e) => return Err(map_codec_error(rebase_codec(e, path_start))),
        }
    }
    if let Some(ref q) = query {
        let q_start = authority_end + query_pos.unwrap() + 1;
        match decode_component(Component::Query, q) {
            Ok(_) => {}
            Err(e) => return Err(map_codec_error(rebase_codec(e, q_start))),
        }
    }
    if let Some(ref f) = fragment {
        let f_start = authority_end + fragment_pos.unwrap() + 1;
        match decode_component(Component::Fragment, f) {
            Ok(_) => {}
            Err(e) => return Err(map_codec_error(rebase_codec(e, f_start))),
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

fn rebase_codec(error: CodecError, base: usize) -> CodecError {
    match error {
        CodecError::BadEscape { at } => CodecError::BadEscape { at: base + at },
        CodecError::InvalidUtf8 { at } => CodecError::InvalidUtf8 { at: base + at },
    }
}

pub fn normalise_path(path: &str) -> Result<String, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }
    let decoded = decode_component(Component::Path, path).map_err(|e| match e {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    })?;

    let raw_segments: Vec<&str> = path[1..].split('/').collect();
    let decoded_segments: Vec<&str> = decoded[1..].split('/').collect();
    let terminal_navigation = decoded_segments
        .last()
        .map(|s| *s == "." || *s == "..")
        .unwrap_or(false);

    let mut stack: Vec<&str> = Vec::new();
    for (raw, dec) in raw_segments.iter().zip(decoded_segments.iter()) {
        if *dec == "." {
            continue;
        }
        if *dec == ".." {
            stack.pop();
        } else {
            stack.push(raw);
        }
    }
    if terminal_navigation {
        stack.push("");
    }

    let mut result = String::from("/");
    result.push_str(&stack.join("/"));
    Ok(result)
}

fn map_path(error: PathError) -> ResolveError {
    ResolveError::Path(error)
}

fn path_of(url: Url) -> Url {
    url
}

fn split_reference(reference: &str) -> (String, Option<String>, Option<String>) {
    let fragment_at = reference.find('#');
    let before_fragment = fragment_at
        .map(|i| &reference[..i])
        .unwrap_or(reference);
    let fragment = fragment_at.map(|i| reference[i + 1..].to_owned());
    let query_at = before_fragment.find('?');
    let path = query_at
        .map(|i| &before_fragment[..i])
        .unwrap_or(before_fragment)
        .to_owned();
    let query = query_at.map(|i| before_fragment[i + 1..].to_owned());
    (path, query, fragment)
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let normalized_base_path = normalise_path(&base_url.path).map_err(map_path)?;

    scan_global(reference).map_err(ResolveError::Reference)?;

    let first_delim = reference
        .find(|c| matches!(c, '/' | '?' | '#'))
        .unwrap_or(reference.len());
    if reference[..first_delim].contains(':') {
        let mut url = parse_url(reference).map_err(ResolveError::Reference)?;
        url.path = normalise_path(&url.path).map_err(map_path)?;
        return Ok(url);
    }

    if reference.starts_with("//") {
        let constructed = format!("{}:{}", base_url.scheme, reference);
        let mut url = parse_url(&constructed).map_err(ResolveError::Reference)?;
        url.path = normalise_path(&url.path).map_err(map_path)?;
        return Ok(url);
    }

    let (ref_path, ref_query, ref_fragment) = split_reference(reference);
    let (selected_path, selected_query) = if ref_path.is_empty() {
        (
            normalized_base_path.clone(),
            if ref_query.is_none() {
                base_url.query.clone()
            } else {
                ref_query
            },
        )
    } else {
        let merged = if ref_path.starts_with('/') {
            ref_path
        } else {
            let slash = normalized_base_path.rfind('/').unwrap_or(0);
            format!("{}{}", &normalized_base_path[..=slash], ref_path)
        };
        (merged, ref_query)
    };

    let mut constructed = format!("{}://", base_url.scheme);
    if let Some(ui) = &base_url.userinfo {
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
    if let Some(query) = &selected_query {
        constructed.push('?');
        constructed.push_str(query);
    }
    if let Some(fragment) = &ref_fragment {
        constructed.push('#');
        constructed.push_str(fragment);
    }

    let mut url = parse_url(&constructed).map_err(ResolveError::Reference)?;
    url.path = normalise_path(&url.path).map_err(map_path)?;
    Ok(path_of(url))
}

fn main() {}