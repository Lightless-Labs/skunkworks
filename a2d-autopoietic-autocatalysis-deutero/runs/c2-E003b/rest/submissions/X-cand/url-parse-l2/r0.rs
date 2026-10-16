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

fn validate_component(input: &str, base: usize) -> Result<(), ParseError> {
    match decode_component(Component::Path, input) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape { at: base + at }),
        Err(CodecError::InvalidUtf8 { at }) => Err(ParseError::InvalidUtf8 { at: base + at }),
    }
}

fn parse_port(port_text: &str, port_start: usize) -> Result<u16, ParseError> {
    if port_text.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    for (offset, byte) in port_text.bytes().enumerate() {
        if !byte.is_ascii_digit() {
            return Err(ParseError::InvalidPort {
                at: port_start + offset,
            });
        }
    }

    if port_text.len() >= 2 && port_text.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for byte in port_text.bytes() {
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
        Some(position) => position,
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

    let at_positions: Vec<usize> = authority
        .bytes()
        .enumerate()
        .filter_map(|(offset, byte)| (byte == b'@').then_some(offset))
        .collect();

    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }

    let (userinfo, hostport_start) = if let Some(&userinfo_end) = at_positions.first() {
        if userinfo_end == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + userinfo_end,
            });
        }
        (
            Some(authority[..userinfo_end].to_owned()),
            userinfo_end + 1,
        )
    } else {
        (None, 0)
    };

    let hostport = &authority[hostport_start..];
    let hostport_absolute_start = authority_start + hostport_start;
    let (host, port, ipv6) = if hostport.as_bytes().first() == Some(&b'[') {
        let opening_at = hostport_absolute_start;
        let close = match hostport.as_bytes()[1..]
            .iter()
            .position(|&byte| byte == b']')
        {
            Some(offset) => offset + 1,
            None => return Err(ParseError::InvalidIpv6 { at: opening_at }),
        };
        let interior = &hostport[1..close];
        if interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: opening_at });
        }

        let suffix = &hostport[close + 1..];
        let parsed_port = if suffix.is_empty() {
            None
        } else if suffix.as_bytes()[0] == b':' {
            Some(parse_port(
                &suffix[1..],
                hostport_absolute_start + close + 2,
            )?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_absolute_start + close + 1,
            });
        };
        (interior.to_ascii_lowercase(), parsed_port, true)
    } else {
        let hostport_bytes = hostport.as_bytes();
        let separator = hostport_bytes.iter().position(|&byte| byte == b':');
        let (host_text, port_text, port_start) = match separator {
            Some(offset) => (
                &hostport[..offset],
                Some(&hostport[offset + 1..]),
                hostport_absolute_start + offset + 1,
            ),
            None => (hostport, None, 0),
        };

        if host_text.is_empty() {
            return Err(ParseError::InvalidHost {
                at: hostport_absolute_start,
            });
        }
        for (offset, byte) in host_text.bytes().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: hostport_absolute_start + offset,
                });
            }
        }

        let parsed_port = match port_text {
            Some(text) => Some(parse_port(text, port_start)?),
            None => None,
        };
        (host_text.to_ascii_lowercase(), parsed_port, false)
    };

    let remainder = &input[authority_end..];
    let fragment_relative = remainder.find('#');
    let before_fragment = fragment_relative
        .map(|offset| &remainder[..offset])
        .unwrap_or(remainder);
    let fragment = fragment_relative.map(|offset| remainder[offset + 1..].to_owned());

    let query_relative = before_fragment.find('?');
    let before_query = query_relative
        .map(|offset| &before_fragment[..offset])
        .unwrap_or(before_fragment);
    let query = query_relative.map(|offset| before_fragment[offset + 1..].to_owned());

    let path = if before_query.is_empty() {
        "/".to_owned()
    } else {
        before_query.to_owned()
    };

    if let Some(value) = &userinfo {
        validate_component(value, authority_start)?;
    }
    if before_query.len() > 0 {
        validate_component(before_query, authority_end)?;
    }
    if let Some(query_offset) = query_relative {
        validate_component(&before_fragment[query_offset + 1..], authority_end + query_offset + 1)?;
    }
    if let Some(fragment_offset) = fragment_relative {
        validate_component(&remainder[fragment_offset + 1..], authority_end + fragment_offset + 1)?;
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