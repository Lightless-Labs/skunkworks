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
            let high = hex_value(bytes[i + 1]);
            let low = hex_value(bytes[i + 2]);
            match (high, low) {
                (Some(high), Some(low)) => {
                    decoded.push((high << 4) | low);
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

pub fn encode_component(component: Component, text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    for &byte in text.as_bytes() {
        let safe = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~')
            || match component {
                Component::UserInfo => byte == b':',
                Component::Path => byte == b'/',
                Component::Query => matches!(byte, b'/' | b'?'),
                Component::Fragment => matches!(byte, b'/' | b'?' | b'#'),
            };

        if safe {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }

    output
}

fn validate_component(
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

fn parse_port(port: &str, port_start: usize) -> Result<u16, ParseError> {
    if port.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    if let Some(offset) = port.bytes().position(|byte| !byte.is_ascii_digit()) {
        return Err(ParseError::InvalidPort {
            at: port_start + offset,
        });
    }

    let bytes = port.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for &byte in bytes {
        value = match value
            .checked_mul(10)
            .and_then(|current| current.checked_add((byte - b'0') as u32))
        {
            Some(value) => value,
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

    for (at, byte) in input.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(ParseError::InvalidByte { at });
        }
    }

    let bytes = input.as_bytes();
    let colon = match bytes.iter().position(|&byte| byte == b':') {
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
        .position(|&byte| matches!(byte, b'/' | b'?' | b'#'))
        .map(|offset| authority_start + offset)
        .unwrap_or(bytes.len());
    let authority = &input[authority_start..authority_end];

    let mut at_positions = authority.match_indices('@').map(|(at, _)| at);
    let first_at = at_positions.next();
    let second_at = at_positions.next();
    if let Some(second_at) = second_at {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + second_at,
        });
    }

    let (userinfo, host_port, host_port_start) = if let Some(user_at) = first_at {
        if user_at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + user_at,
            });
        }
        (
            Some(authority[..user_at].to_owned()),
            &authority[user_at + 1..],
            authority_start + user_at + 1,
        )
    } else {
        (None, authority, authority_start)
    };

    let (host, ipv6, port, port_start) = if host_port.starts_with('[') {
        let open_at = host_port_start;
        let close_rel = match host_port[1..].find(']') {
            Some(offset) => offset + 1,
            None => return Err(ParseError::InvalidIpv6 { at: open_at }),
        };
        let interior = &host_port[1..close_rel];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: open_at });
        }

        let suffix_start = close_rel + 1;
        let suffix = &host_port[suffix_start..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            let port_start = host_port_start + suffix_start + 1;
            Some(parse_port(&suffix[1..], port_start)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: host_port_start + suffix_start,
            });
        };

        let port_start = if suffix.starts_with(':') {
            Some(host_port_start + suffix_start + 1)
        } else {
            None
        };
        (
            interior.to_ascii_lowercase(),
            true,
            port,
            port_start,
        )
    } else {
        let host_colon = host_port.find(':');
        let (host_text, port_text, port_start) = if let Some(offset) = host_colon {
            (
                &host_port[..offset],
                Some(&host_port[offset + 1..]),
                Some(host_port_start + offset + 1),
            )
        } else {
            (host_port, None, None)
        };

        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: host_port_start });
        }
        if let Some(offset) = host_text
            .bytes()
            .position(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')))
        {
            return Err(ParseError::InvalidHost {
                at: host_port_start + offset,
            });
        }

        let port = match (port_text, port_start) {
            (Some(text), Some(start)) => Some(parse_port(text, start)?),
            _ => None,
        };

        (
            host_text.to_ascii_lowercase(),
            false,
            port,
            port_start,
        )
    };

    let fragment_delim = bytes[authority_end..]
        .iter()
        .position(|&byte| byte == b'#')
        .map(|offset| authority_end + offset);
    let component_end = fragment_delim.unwrap_or(bytes.len());

    let query_delim = bytes[authority_end..component_end]
        .iter()
        .position(|&byte| byte == b'?')
        .map(|offset| authority_end + offset);

    let path_end = query_delim.unwrap_or(component_end);
    let path = if authority_end == path_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };

    let query = query_delim.map(|start| input[start + 1..component_end].to_owned());
    let fragment = fragment_delim.map(|start| input[start + 1..].to_owned());

    if let Some(userinfo_value) = userinfo.as_deref() {
        let userinfo_start = authority_start;
        validate_component(Component::UserInfo, userinfo_value, userinfo_start)?;
    }
    if !path.is_empty() && path != "/" || authority_end != path_end {
        validate_component(Component::Path, &path, authority_end)?;
    }
    if let (Some(query_value), Some(query_start)) = (query.as_deref(), query_delim) {
        validate_component(Component::Query, query_value, query_start + 1)?;
    }
    if let (Some(fragment_value), Some(fragment_start)) = (fragment.as_deref(), fragment_delim) {
        validate_component(Component::Fragment, fragment_value, fragment_start + 1)?;
    }

    let _ = port_start;

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