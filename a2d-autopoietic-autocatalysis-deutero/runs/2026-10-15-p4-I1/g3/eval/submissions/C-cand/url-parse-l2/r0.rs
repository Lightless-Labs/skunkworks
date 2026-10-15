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

fn is_ascii_alpha(b: u8) -> bool {
    b.is_ascii_alphabetic()
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
            let decoded_at = e.valid_up_to();
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
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(bytes.len());

    for &b in bytes {
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

fn map_codec_error(error: CodecError, base: usize) -> ParseError {
    match error {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: base + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: base + at },
    }
}

fn validate_component(
    component: Component,
    text: &str,
    base: usize,
) -> Result<(), ParseError> {
    decode_component(component, text)
        .map(|_| ())
        .map_err(|e| map_codec_error(e, base))
}

fn parse_port(port_text: &str, port_start: usize) -> Result<u16, ParseError> {
    if port_text.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    for (i, b) in port_text.bytes().enumerate() {
        if !b.is_ascii_digit() {
            return Err(ParseError::InvalidPort { at: port_start + i });
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

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }

    let bytes = input.as_bytes();
    for (at, &b) in bytes.iter().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(ParseError::InvalidByte { at });
        }
    }

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

    let authority_start = colon + 1;
    if bytes.get(authority_start) != Some(&b'/')
        || bytes.get(authority_start + 1) != Some(&b'/')
    {
        return Err(ParseError::MissingAuthority { at: authority_start });
    }
    let authority_start = authority_start + 2;

    let authority_end = bytes[authority_start..]
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map(|offset| authority_start + offset)
        .unwrap_or(bytes.len());

    let authority = &input[authority_start..authority_end];
    let auth_bytes = authority.as_bytes();

    let at_positions: Vec<usize> = auth_bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i))
        .collect();

    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }

    let (userinfo, hostport_start) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at,
            });
        }
        (Some(&authority[..at]), at + 1)
    } else {
        (None, 0)
    };

    let hostport = &authority[hostport_start..];
    let hostport_abs = authority_start + hostport_start;

    let (host, port, ipv6) = if hostport.as_bytes().first() == Some(&b'[') {
        let closing = hostport.as_bytes()[1..]
            .iter()
            .position(|&b| b == b']')
            .map(|offset| offset + 1)
            .ok_or(ParseError::InvalidIpv6 { at: hostport_abs })?;
        let interior = &hostport[1..closing];

        if interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: hostport_abs });
        }

        let suffix = &hostport[closing + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.as_bytes()[0] == b':' {
            Some(parse_port(
                &suffix[1..],
                hostport_abs + closing + 2,
            )?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_abs + closing + 1,
            });
        };

        (interior.to_ascii_lowercase(), port, true)
    } else {
        let host_colon = hostport.as_bytes().iter().position(|&b| b == b':');
        let (host_text, port) = if let Some(i) = host_colon {
            let port_start = hostport_abs + i + 1;
            (&hostport[..i], Some(parse_port(&hostport[i + 1..], port_start)?))
        } else {
            (hostport, None)
        };

        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_abs });
        }
        for (i, b) in host_text.bytes().enumerate() {
            if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: hostport_abs + i,
                });
            }
        }

        (host_text.to_ascii_lowercase(), port, false)
    };

    let remainder = &input[authority_end..];
    let remainder_bytes = remainder.as_bytes();

    let fragment_rel = remainder_bytes.iter().position(|&b| b == b'#');
    let before_fragment_end = fragment_rel.unwrap_or(remainder.len());
    let before_fragment = &remainder[..before_fragment_end];

    let query_rel = before_fragment.as_bytes().iter().position(|&b| b == b'?');
    let path_end = query_rel.unwrap_or(before_fragment.len());

    let path = if path_end == 0 {
        "/".to_owned()
    } else {
        before_fragment[..path_end].to_owned()
    };

    let query = query_rel.map(|i| before_fragment[i + 1..].to_owned());
    let fragment = fragment_rel.map(|i| remainder[i + 1..].to_owned());

    if let Some(userinfo_text) = userinfo {
        validate_component(
            Component::UserInfo,
            userinfo_text,
            authority_start,
        )?;
    }
    if path_end != 0 {
        validate_component(Component::Path, &path, authority_end)?;
    }
    if let Some(i) = query_rel {
        validate_component(
            Component::Query,
            query.as_deref().unwrap_or(""),
            authority_end + i + 1,
        )?;
    }
    if let Some(i) = fragment_rel {
        validate_component(
            Component::Fragment,
            fragment.as_deref().unwrap_or(""),
            authority_end + i + 1,
        )?;
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

fn main() {}