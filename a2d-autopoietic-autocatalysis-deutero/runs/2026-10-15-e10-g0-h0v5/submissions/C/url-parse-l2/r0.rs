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
            let Some(high) = hex_value(bytes[i + 1]) else {
                return Err(CodecError::BadEscape { at: i });
            };
            let Some(low) = hex_value(bytes[i + 2]) else {
                return Err(CodecError::BadEscape { at: i });
            };
            decoded.push((high << 4) | low);
            offsets.push(i);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            offsets.push(i);
            i += 1;
        }
    }

    match std::str::from_utf8(&decoded) {
        Ok(s) => Ok(s.to_owned()),
        Err(e) => Err(CodecError::InvalidUtf8 {
            at: offsets.get(e.valid_up_to()).copied().unwrap_or(input.len()),
        }),
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

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }

    let bytes = input.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(ParseError::InvalidByte { at: i });
        }
    }

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
        .map(|n| auth_start + n)
        .unwrap_or(bytes.len());
    let authority = &input[auth_start..auth_end];
    let authority_bytes = authority.as_bytes();

    let at_positions: Vec<usize> = authority_bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i))
        .collect();

    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: auth_start + at_positions[1],
        });
    }
    let (userinfo, host_start_rel) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo { at: auth_start });
        }
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let host_start = auth_start + host_start_rel;
    let remaining = &authority[host_start_rel..];
    let remaining_bytes = remaining.as_bytes();

    let (host, port, ipv6) = if remaining_bytes.first() == Some(&b'[') {
        let close = remaining_bytes.iter().position(|&b| b == b']');
        let Some(close) = close else {
            return Err(ParseError::InvalidIpv6 { at: host_start });
        };
        let interior = &remaining[1..close];
        if interior.contains('%') || interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: host_start });
        }

        let after_close = close + 1;
        let suffix = &remaining[after_close..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.as_bytes()[0] == b':' {
            Some(parse_port(&suffix[1..], host_start + after_close + 1)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: host_start + after_close,
            });
        };
        (interior.to_ascii_lowercase(), port, true)
    } else {
        let host_port_colon = remaining_bytes.iter().position(|&b| b == b':');
        let (host_text, port) = if let Some(c) = host_port_colon {
            (
                &remaining[..c],
                Some(parse_port(
                    &remaining[c + 1..],
                    host_start + c + 1,
                )?),
            )
        } else {
            (remaining, None)
        };

        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: host_start });
        }
        for (i, &b) in host_text.as_bytes().iter().enumerate() {
            if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: host_start + i,
                });
            }
        }
        (host_text.to_ascii_lowercase(), port, false)
    };

    let fragment_marker = bytes[auth_end..]
        .iter()
        .position(|&b| b == b'#')
        .map(|n| auth_end + n);
    let before_fragment_end = fragment_marker.unwrap_or(bytes.len());

    let query_marker = bytes[auth_end..before_fragment_end]
        .iter()
        .position(|&b| b == b'?')
        .map(|n| auth_end + n);

    let path_end = query_marker.unwrap_or(before_fragment_end);
    let path = if path_end == auth_end {
        "/".to_owned()
    } else {
        input[auth_end..path_end].to_owned()
    };
    let query = query_marker.map(|q| input[q + 1..before_fragment_end].to_owned());
    let fragment = fragment_marker.map(|f| input[f + 1..].to_owned());

    if let Some(ref userinfo_text) = userinfo {
        let offset = auth_start;
        validate_component(userinfo_text, offset)?;
    }
    if path != "/" || path_end != auth_end {
        validate_component(&path, auth_end)?;
    }
    if let Some(ref query_text) = query {
        validate_component(query_text, query_marker.unwrap() + 1)?;
    }
    if let Some(ref fragment_text) = fragment {
        validate_component(fragment_text, fragment_marker.unwrap() + 1)?;
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

fn parse_port(text: &str, port_start: usize) -> Result<u16, ParseError> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }
    if let Some(i) = bytes.iter().position(|b| !b.is_ascii_digit()) {
        return Err(ParseError::InvalidPort {
            at: port_start + i,
        });
    }
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for &b in bytes {
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

fn validate_component(text: &str, offset: usize) -> Result<(), ParseError> {
    match decode_component(Component::Path, text) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape { at: offset + at }),
        Err(CodecError::InvalidUtf8 { at }) => {
            Err(ParseError::InvalidUtf8 { at: offset + at })
        }
    }
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn main() {}