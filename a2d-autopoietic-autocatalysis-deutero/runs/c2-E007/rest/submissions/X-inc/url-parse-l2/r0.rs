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
        Ok(text) => Ok(text.to_owned()),
        Err(error) => Err(CodecError::InvalidUtf8 {
            at: offsets.get(error.valid_up_to()).copied().unwrap_or(input.len()),
        }),
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
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
            encoded.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
}

fn parse_port(port: &str, port_start: usize) -> Result<u16, ParseError> {
    if port.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    for (index, byte) in port.bytes().enumerate() {
        if !byte.is_ascii_digit() {
            return Err(ParseError::InvalidPort {
                at: port_start + index,
            });
        }
    }

    if port.len() >= 2 && port.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for byte in port.bytes() {
        value = match value.checked_mul(10).and_then(|v| v.checked_add((byte - b'0') as u32)) {
            Some(v) if v <= 65535 => v,
            _ => return Err(ParseError::InvalidPort { at: port_start }),
        };
    }

    if value == 0 {
        return Err(ParseError::InvalidPort { at: port_start });
    }
    Ok(value as u16)
}

fn validate_component(
    component: Component,
    text: &str,
    original_start: usize,
) -> Result<(), ParseError> {
    match decode_component(component, text) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape {
            at: original_start + at,
        }),
        Err(CodecError::InvalidUtf8 { at }) => Err(ParseError::InvalidUtf8 {
            at: original_start + at,
        }),
    }
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
        Some(index) => index,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };

    if colon == 0 || !bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for (index, &byte) in bytes[..colon].iter().enumerate().skip(1) {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')) {
            return Err(ParseError::InvalidScheme { at: index });
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
        .unwrap_or(input.len());
    let authority = &input[authority_start..authority_end];

    let at_positions: Vec<usize> = authority
        .bytes()
        .enumerate()
        .filter_map(|(index, byte)| (byte == b'@').then_some(index))
        .collect();

    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }
    if let Some(&userinfo_end) = at_positions.first() {
        if userinfo_end == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + userinfo_end,
            });
        }
    }

    let (userinfo, hostport_start) = if let Some(&userinfo_end) = at_positions.first() {
        (
            Some(authority[..userinfo_end].to_owned()),
            userinfo_end + 1,
        )
    } else {
        (None, 0)
    };

    let hostport = &authority[hostport_start..];
    let hostport_abs = authority_start + hostport_start;

    let (host, port, ipv6) = if hostport.starts_with('[') {
        let close = match hostport.find(']') {
            Some(index) => index,
            None => return Err(ParseError::InvalidIpv6 { at: hostport_abs }),
        };
        let interior = &hostport[1..close];
        if interior.contains('%') || interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: hostport_abs });
        }
        let suffix = &hostport[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if let Some(port_text) = suffix.strip_prefix(':') {
            Some(parse_port(port_text, hostport_abs + close + 2)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_abs + close + 1,
            });
        };
        (interior.to_ascii_lowercase(), port, true)
    } else {
        let split = hostport.find(':');
        let (host_text, port) = match split {
            Some(index) => (
                &hostport[..index],
                Some(parse_port(&hostport[index + 1..], hostport_abs + index + 1)?),
            ),
            None => (hostport, None),
        };

        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_abs });
        }
        for (index, byte) in host_text.bytes().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: hostport_abs + index,
                });
            }
        }
        (host_text.to_ascii_lowercase(), port, false)
    };

    let fragment_delim = input[authority_end..].find('#').map(|i| authority_end + i);
    let before_fragment_end = fragment_delim.unwrap_or(input.len());
    let query_delim = input[authority_end..before_fragment_end]
        .find('?')
        .map(|i| authority_end + i);

    let path_end = query_delim.unwrap_or(before_fragment_end);
    let path = if authority_end == path_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };
    let query = query_delim.map(|index| input[index + 1..before_fragment_end].to_owned());
    let fragment = fragment_delim.map(|index| input[index + 1..].to_owned());

    if let Some(userinfo_text) = userinfo.as_deref() {
        validate_component(
            Component::UserInfo,
            userinfo_text,
            authority_start,
        )?;
    }
    if authority_end != path_end {
        validate_component(Component::Path, &path, authority_end)?;
    }
    if let (Some(index), Some(query_text)) = (query_delim, query.as_deref()) {
        validate_component(Component::Query, query_text, index + 1)?;
    }
    if let (Some(index), Some(fragment_text)) = (fragment_delim, fragment.as_deref()) {
        validate_component(Component::Fragment, fragment_text, index + 1)?;
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