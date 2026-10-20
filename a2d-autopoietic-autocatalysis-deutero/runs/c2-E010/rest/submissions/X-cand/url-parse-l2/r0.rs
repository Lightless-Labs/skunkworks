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

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub fn decode_component(_component: Component, input: &str) -> Result<String, CodecError> {
    let src = input.as_bytes();
    let mut output = Vec::with_capacity(src.len());
    let mut origins = Vec::with_capacity(src.len());
    let mut cursor = 0;

    while cursor < src.len() {
        let byte = src[cursor];
        if byte != b'%' {
            output.push(byte);
            origins.push(cursor);
            cursor += 1;
            continue;
        }
        if cursor + 2 >= src.len() {
            return Err(CodecError::BadEscape { at: cursor });
        }
        let high = match hex_value(src[cursor + 1]) {
            Some(value) => value,
            None => return Err(CodecError::BadEscape { at: cursor }),
        };
        let low = match hex_value(src[cursor + 2]) {
            Some(value) => value,
            None => return Err(CodecError::BadEscape { at: cursor }),
        };
        output.push((high << 4) | low);
        origins.push(cursor);
        cursor += 3;
    }

    if let Err(error) = std::str::from_utf8(&output) {
        let index = error.valid_up_to();
        return Err(CodecError::InvalidUtf8 {
            at: origins.get(index).copied().unwrap_or(src.len()),
        });
    }
    Ok(String::from_utf8(output).expect("UTF-8 validated"))
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &byte in text.as_bytes() {
        let common_safe = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~');
        let component_safe = match component {
            Component::UserInfo => byte == b':',
            Component::Path => byte == b'/',
            Component::Query => matches!(byte, b'/' | b'?'),
            Component::Fragment => matches!(byte, b'/' | b'?' | b'#'),
        };
        if common_safe || component_safe {
            result.push(byte as char);
        } else {
            result.push('%');
            result.push(HEX[(byte >> 4) as usize] as char);
            result.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    result
}

fn convert_error(err: CodecError, base: usize) -> ParseError {
    match err {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: base + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: base + at },
    }
}

fn validate_component(text: &str, offset: usize) -> Result<(), ParseError> {
    match decode_component(Component::Path, text) {
        Ok(_) => Ok(()),
        Err(err) => Err(convert_error(err, offset)),
    }
}

fn parse_port(text: &str, start: usize) -> Result<u16, ParseError> {
    if text.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }
    for (idx, byte) in text.bytes().enumerate() {
        if !byte.is_ascii_digit() {
            return Err(ParseError::InvalidPort { at: start + idx });
        }
    }
    if text.len() > 1 && text.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }
    let mut number = 0u32;
    for byte in text.bytes() {
        number = number.saturating_mul(10).saturating_add((byte - b'0') as u32);
        if number > 65535 {
            return Err(ParseError::InvalidPort { at: start });
        }
    }
    if number == 0 {
        return Err(ParseError::InvalidPort { at: start });
    }
    Ok(number as u16)
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    let raw = input.as_bytes();
    for (offset, byte) in raw.iter().copied().enumerate() {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(ParseError::InvalidByte { at: offset });
        }
    }

    let colon = match raw.iter().position(|&byte| byte == b':') {
        Some(value) => value,
        None => return Err(ParseError::InvalidScheme { at: raw.len() }),
    };
    if colon == 0 || !raw[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for index in 1..colon {
        let byte = raw[index];
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')) {
            return Err(ParseError::InvalidScheme { at: index });
        }
    }

    let authority_marker = colon + 1;
    if raw.get(authority_marker) != Some(&b'/')
        || raw.get(authority_marker + 1) != Some(&b'/')
    {
        return Err(ParseError::MissingAuthority { at: authority_marker });
    }

    let authority_start = authority_marker + 2;
    let authority_end = raw[authority_start..]
        .iter()
        .position(|&byte| matches!(byte, b'/' | b'?' | b'#'))
        .map(|relative| authority_start + relative)
        .unwrap_or(raw.len());
    let authority = &input[authority_start..authority_end];

    let mut first_at = None;
    for (index, byte) in authority.bytes().enumerate() {
        if byte == b'@' {
            if first_at.is_some() {
                return Err(ParseError::InvalidUserInfo {
                    at: authority_start + index,
                });
            }
            first_at = Some(index);
        }
    }
    let (userinfo, host_offset) = match first_at {
        Some(index) if index == 0 => {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + index,
            })
        }
        Some(index) => (Some(authority[..index].to_owned()), index + 1),
        None => (None, 0),
    };

    let host_port = &authority[host_offset..];
    let host_port_offset = authority_start + host_offset;
    let (host, port, ipv6) = if host_port.starts_with('[') {
        let close = match host_port.find(']') {
            Some(index) => index,
            None => return Err(ParseError::InvalidIpv6 { at: host_port_offset }),
        };
        let inside = &host_port[1..close];
        if inside.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: host_port_offset });
        }
        let suffix = &host_port[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(parse_port(
                &suffix[1..],
                host_port_offset + close + 2,
            )?)
        } else {
            return Err(ParseError::InvalidHost {
                at: host_port_offset + close + 1,
            });
        };
        (inside.to_ascii_lowercase(), port, true)
    } else {
        let separator = host_port.find(':');
        let (host_text, port_text, port_start) = if let Some(index) = separator {
            (
                &host_port[..index],
                Some(&host_port[index + 1..]),
                host_port_offset + index + 1,
            )
        } else {
            (host_port, None, host_port_offset + host_port.len())
        };
        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: host_port_offset });
        }
        for (index, byte) in host_text.bytes().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: host_port_offset + index,
                });
            }
        }
        let port = match port_text {
            Some(text) => Some(parse_port(text, port_start)?),
            None => None,
        };
        (host_text.to_ascii_lowercase(), port, false)
    };

    let tail = &input[authority_end..];
    let fragment_index = tail.find('#');
    let fragment = fragment_index.map(|index| tail[index + 1..].to_owned());
    let before_fragment = &tail[..fragment_index.unwrap_or(tail.len())];
    let query_index = before_fragment.find('?');
    let query = query_index.map(|index| before_fragment[index + 1..].to_owned());
    let path_raw = &before_fragment[..query_index.unwrap_or(before_fragment.len())];
    let path = if path_raw.is_empty() {
        "/".to_owned()
    } else {
        path_raw.to_owned()
    };

    if let Some(ref value) = userinfo {
        validate_component(value, authority_start)?;
    }
    if !path_raw.is_empty() {
        validate_component(path_raw, authority_end)?;
    }
    if let Some(index) = query_index {
        validate_component(
            &before_fragment[index + 1..],
            authority_end + index + 1,
        )?;
    }
    if let Some(index) = fragment_index {
        validate_component(&tail[index + 1..], authority_end + index + 1)?;
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
