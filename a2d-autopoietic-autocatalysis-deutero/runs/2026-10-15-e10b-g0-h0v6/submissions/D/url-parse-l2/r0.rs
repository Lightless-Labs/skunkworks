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
        Err(error) => {
            let decoded_offset = error.valid_up_to();
            let input_offset = offsets.get(decoded_offset).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at: input_offset })
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

fn rebase_codec_error(error: CodecError, base: usize) -> ParseError {
    match error {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: base + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: base + at },
    }
}

fn validate_component(component: Component, text: &str, base: usize) -> Result<(), ParseError> {
    decode_component(component, text)
        .map(|_| ())
        .map_err(|error| rebase_codec_error(error, base))
}

fn parse_port(text: &str, start: usize) -> Result<u16, ParseError> {
    if text.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }

    for (offset, byte) in text.bytes().enumerate() {
        if !byte.is_ascii_digit() {
            return Err(ParseError::InvalidPort { at: start + offset });
        }
    }

    if text.len() >= 2 && text.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }

    let mut value: u32 = 0;
    for byte in text.bytes() {
        value = match value
            .checked_mul(10)
            .and_then(|current| current.checked_add((byte - b'0') as u32))
        {
            Some(value) => value,
            None => return Err(ParseError::InvalidPort { at: start }),
        };
        if value > u16::MAX as u32 {
            return Err(ParseError::InvalidPort { at: start });
        }
    }

    if value == 0 {
        return Err(ParseError::InvalidPort { at: start });
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

    let mut at_sign = None;
    for (offset, byte) in authority.bytes().enumerate() {
        if byte == b'@' {
            if at_sign.is_some() {
                return Err(ParseError::InvalidUserInfo {
                    at: authority_start + offset,
                });
            }
            at_sign = Some(offset);
        }
    }

    let (userinfo, hostport_start) = if let Some(offset) = at_sign {
        if offset == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start,
            });
        }
        (
            Some(authority[..offset].to_owned()),
            authority_start + offset + 1,
        )
    } else {
        (None, authority_start)
    };

    let hostport = &input[hostport_start..authority_end];
    let mut ipv6 = false;
    let (host, port) = if hostport.starts_with('[') {
        let close_relative = hostport[1..].find(']');
        let close = match close_relative {
            Some(offset) => 1 + offset,
            None => return Err(ParseError::InvalidIpv6 { at: hostport_start }),
        };
        let interior = &hostport[1..close];
        if interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: hostport_start });
        }

        let suffix_start = hostport_start + close + 1;
        let suffix = &hostport[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(parse_port(&suffix[1..], suffix_start + 1)?)
        } else {
            return Err(ParseError::InvalidHost { at: suffix_start });
        };

        ipv6 = true;
        (interior.to_ascii_lowercase(), port)
    } else {
        let host_colon = hostport.find(':');
        let (host_text, port) = if let Some(offset) = host_colon {
            let port = parse_port(&hostport[offset + 1..], hostport_start + offset + 1)?;
            (&hostport[..offset], Some(port))
        } else {
            (hostport, None)
        };

        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_start });
        }
        for (offset, byte) in host_text.bytes().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: hostport_start + offset,
                });
            }
        }
        (host_text.to_ascii_lowercase(), port)
    };

    let fragment_delimiter = input[authority_end..].find('#')
        .map(|offset| authority_end + offset);
    let before_fragment_end = fragment_delimiter.unwrap_or(input.len());
    let query_delimiter = input[authority_end..before_fragment_end]
        .find('?')
        .map(|offset| authority_end + offset);

    let path_end = query_delimiter.unwrap_or(before_fragment_end);
    let path = if path_end == authority_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };

    let query = query_delimiter.map(|start| {
        let value_start = start + 1;
        input[value_start..before_fragment_end].to_owned()
    });
    let fragment = fragment_delimiter.map(|start| input[start + 1..].to_owned());

    if let Some(userinfo) = userinfo.as_deref() {
        validate_component(
            Component::UserInfo,
            userinfo,
            authority_start,
        )?;
    }
    if path_end != authority_end {
        validate_component(Component::Path, &path, authority_end)?;
    }
    if let (Some(query_start), Some(query_text)) = (query_delimiter, query.as_deref()) {
        validate_component(Component::Query, query_text, query_start + 1)?;
    }
    if let (Some(fragment_start), Some(fragment_text)) =
        (fragment_delimiter, fragment.as_deref())
    {
        validate_component(Component::Fragment, fragment_text, fragment_start + 1)?;
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