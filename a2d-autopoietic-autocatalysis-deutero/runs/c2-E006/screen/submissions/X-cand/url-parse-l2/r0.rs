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
            let high = hex_value(bytes[i + 1]).ok_or(CodecError::BadEscape { at: i })?;
            let low = hex_value(bytes[i + 2]).ok_or(CodecError::BadEscape { at: i })?;
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
        Ok(value) => Ok(value.to_owned()),
        Err(error) => {
            let decoded_offset = error.valid_up_to();
            let original_offset = offsets.get(decoded_offset).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at: original_offset })
        }
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    for &byte in text.as_bytes() {
        let common_safe = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        let component_safe = match component {
            Component::UserInfo => byte == b':',
            Component::Path => byte == b'/',
            Component::Query => matches!(byte, b'/' | b'?'),
            Component::Fragment => matches!(byte, b'/' | b'?' | b'#'),
        };

        if common_safe || component_safe {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }

    output
}

fn map_codec_error(error: CodecError, base: usize) -> ParseError {
    match error {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: base + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: base + at },
    }
}

fn validate_component(component: Component, text: &str, base: usize) -> Result<(), ParseError> {
    decode_component(component, text)
        .map(|_| ())
        .map_err(|error| map_codec_error(error, base))
}

fn parse_port(port_text: &str, port_start: usize) -> Result<u16, ParseError> {
    if port_text.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    for (index, byte) in port_text.bytes().enumerate() {
        if !byte.is_ascii_digit() {
            return Err(ParseError::InvalidPort {
                at: port_start + index,
            });
        }
    }

    let bytes = port_text.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for byte in bytes {
        value = match value
            .checked_mul(10)
            .and_then(|current| current.checked_add((byte - b'0') as u32))
        {
            Some(next) => next,
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
        Some(index) => index,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };

    if colon == 0 || !bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for (index, byte) in bytes[..colon].iter().enumerate().skip(1) {
        if !(byte.is_ascii_alphanumeric() || matches!(*byte, b'+' | b'-' | b'.')) {
            return Err(ParseError::InvalidScheme { at: index });
        }
    }

    if bytes.get(colon + 1) != Some(&b'/') || bytes.get(colon + 2) != Some(&b'/') {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let authority_start = colon + 3;
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|byte| matches!(*byte, b'/' | b'?' | b'#'))
        .map(|offset| authority_start + offset)
        .unwrap_or(bytes.len());
    let authority = &input[authority_start..authority_end];
    let authority_bytes = authority.as_bytes();

    let mut at_positions = authority_bytes
        .iter()
        .enumerate()
        .filter_map(|(index, byte)| (*byte == b'@').then_some(index));

    let first_at = at_positions.next();
    let second_at = at_positions.next();
    if let Some(index) = second_at {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + index,
        });
    }

    let (userinfo, host_port, host_port_start, userinfo_range) = match first_at {
        Some(index) => {
            if index == 0 {
                return Err(ParseError::InvalidUserInfo {
                    at: authority_start + index,
                });
            }
            (
                Some(authority[..index].to_owned()),
                &authority[index + 1..],
                authority_start + index + 1,
                Some((authority_start, index)),
            )
        }
        None => (None, authority, authority_start, None),
    };

    let (host, port, ipv6) = if host_port.starts_with('[') {
        let opening = host_port_start;
        let closing = match host_port.as_bytes()[1..]
            .iter()
            .position(|&byte| byte == b']')
        {
            Some(offset) => offset + 1,
            None => return Err(ParseError::InvalidIpv6 { at: opening }),
        };
        let interior = &host_port[1..closing];
        if interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: opening });
        }

        let suffix = &host_port[closing + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(parse_port(&suffix[1..], host_port_start + closing + 2)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: host_port_start + closing + 1,
            });
        };

        (interior.to_ascii_lowercase(), port, true)
    } else {
        let host_colon = host_port.as_bytes().iter().position(|&byte| byte == b':');
        let (host_text, port) = match host_colon {
            Some(index) => (
                &host_port[..index],
                Some(parse_port(
                    &host_port[index + 1..],
                    host_port_start + index + 1,
                )?),
            ),
            None => (host_port, None),
        };

        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: host_port_start });
        }
        for (index, byte) in host_text.bytes().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: host_port_start + index,
                });
            }
        }

        (host_text.to_ascii_lowercase(), port, false)
    };

    let fragment_marker = input[authority_end..]
        .as_bytes()
        .iter()
        .position(|&byte| byte == b'#')
        .map(|offset| authority_end + offset);

    let before_fragment_end = fragment_marker.unwrap_or(input.len());
    let query_marker = input[authority_end..before_fragment_end]
        .as_bytes()
        .iter()
        .position(|&byte| byte == b'?')
        .map(|offset| authority_end + offset);

    let path_end = query_marker.unwrap_or(before_fragment_end);
    let path = if path_end == authority_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };

    let query = query_marker.map(|marker| input[marker + 1..before_fragment_end].to_owned());
    let fragment = fragment_marker.map(|marker| input[marker + 1..].to_owned());

    if let Some((base, length)) = userinfo_range {
        validate_component(Component::UserInfo, &input[base..base + length], base)?;
    }
    if path_end != authority_end {
        validate_component(Component::Path, &path, authority_end)?;
    }
    if let Some(marker) = query_marker {
        validate_component(Component::Query, query.as_deref().unwrap_or(""), marker + 1)?;
    }
    if let Some(marker) = fragment_marker {
        validate_component(
            Component::Fragment,
            fragment.as_deref().unwrap_or(""),
            marker + 1,
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