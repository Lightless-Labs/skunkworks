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
    let mut origins = Vec::with_capacity(bytes.len());
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
        Ok(text) => Ok(text.to_owned()),
        Err(error) => {
            let decoded_offset = error.valid_up_to();
            let original_offset = origins.get(decoded_offset).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at: original_offset })
        }
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
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
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }

    encoded
}

fn validate_component(
    component: Component,
    raw: &str,
    base: usize,
) -> Result<(), ParseError> {
    match decode_component(component, raw) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape { at: base + at }),
        Err(CodecError::InvalidUtf8 { at }) => {
            Err(ParseError::InvalidUtf8 { at: base + at })
        }
    }
}

fn parse_port(raw: &str, at: usize) -> Result<u16, ParseError> {
    if raw.is_empty() {
        return Err(ParseError::InvalidPort { at });
    }

    if let Some(offset) = raw.bytes().position(|byte| !byte.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: at + offset });
    }

    if raw.len() >= 2 && raw.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at });
    }

    let mut value: u32 = 0;
    for byte in raw.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|current| current.checked_add((byte - b'0') as u32))
            .ok_or(ParseError::InvalidPort { at })?;
    }

    if !(1..=65535).contains(&value) {
        return Err(ParseError::InvalidPort { at });
    }

    Ok(value as u16)
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }

    let bytes = input.as_bytes();
    if let Some(at) = bytes
        .iter()
        .position(|&byte| !(0x21..=0x7e).contains(&byte))
    {
        return Err(ParseError::InvalidByte { at });
    }

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
    let authority_bytes = authority.as_bytes();

    let at_positions: Vec<usize> = authority_bytes
        .iter()
        .enumerate()
        .filter_map(|(index, &byte)| (byte == b'@').then_some(index))
        .collect();

    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }
    if at_positions.first().copied() == Some(0) {
        return Err(ParseError::InvalidUserInfo { at: authority_start });
    }

    let (userinfo, hostport, hostport_start) = if let Some(&userinfo_end) = at_positions.first() {
        (
            Some(authority[..userinfo_end].to_owned()),
            &authority[userinfo_end + 1..],
            authority_start + userinfo_end + 1,
        )
    } else {
        (None, authority, authority_start)
    };

    let (host, port, ipv6) = if hostport.starts_with('[') {
        let opening_at = hostport_start;
        let closing = match hostport.as_bytes().iter().position(|&byte| byte == b']') {
            Some(index) => index,
            None => return Err(ParseError::InvalidIpv6 { at: opening_at }),
        };
        let interior = &hostport[1..closing];
        if interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: opening_at });
        }

        let suffix = &hostport[closing + 1..];
        let parsed_port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(parse_port(&suffix[1..], hostport_start + closing + 2)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_start + closing + 1,
            });
        };

        (interior.to_ascii_lowercase(), parsed_port, true)
    } else {
        let host_colon = hostport.as_bytes().iter().position(|&byte| byte == b':');
        let (raw_host, parsed_port) = if let Some(port_colon) = host_colon {
            (
                &hostport[..port_colon],
                Some(parse_port(
                    &hostport[port_colon + 1..],
                    hostport_start + port_colon + 1,
                )?),
            )
        } else {
            (hostport, None)
        };

        if raw_host.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_start });
        }
        for (offset, byte) in raw_host.bytes().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: hostport_start + offset,
                });
            }
        }

        (raw_host.to_ascii_lowercase(), parsed_port, false)
    };

    let remainder = &input[authority_end..];
    let fragment_relative = remainder.find('#');
    let before_fragment = fragment_relative
        .map(|index| &remainder[..index])
        .unwrap_or(remainder);
    let fragment = fragment_relative.map(|index| remainder[index + 1..].to_owned());

    let query_relative = before_fragment.find('?');
    let before_query = query_relative
        .map(|index| &before_fragment[..index])
        .unwrap_or(before_fragment);
    let query = query_relative.map(|index| before_fragment[index + 1..].to_owned());

    let path = if before_query.is_empty() {
        "/".to_owned()
    } else {
        before_query.to_owned()
    };

    if let Some(userinfo_value) = userinfo.as_deref() {
        validate_component(
            Component::UserInfo,
            userinfo_value,
            authority_start,
        )?;
    }
    if !before_query.is_empty() {
        validate_component(Component::Path, &path, authority_end)?;
    }
    if let Some(query_value) = query.as_deref() {
        let query_at = authority_end
            + query_relative.expect("query value implies query delimiter")
            + 1;
        validate_component(Component::Query, query_value, query_at)?;
    }
    if let Some(fragment_value) = fragment.as_deref() {
        let fragment_at = authority_end
            + fragment_relative.expect("fragment value implies fragment delimiter")
            + 1;
        validate_component(Component::Fragment, fragment_value, fragment_at)?;
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