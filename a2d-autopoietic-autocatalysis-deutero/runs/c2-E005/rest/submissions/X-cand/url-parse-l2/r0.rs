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
        Ok(s) => Ok(s.to_owned()),
        Err(e) => {
            let at = offsets.get(e.valid_up_to()).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at })
        }
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut output = String::with_capacity(text.len());
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
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
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

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }

    let bytes = input.as_bytes();
    for (at, &byte) in bytes.iter().enumerate() {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(ParseError::InvalidByte { at });
        }
    }

    let colon = match bytes.iter().position(|&b| b == b':') {
        Some(at) => at,
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

    if colon + 2 >= bytes.len()
        || bytes[colon + 1] != b'/'
        || bytes[colon + 2] != b'/'
    {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let authority_start = colon + 3;
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map(|n| authority_start + n)
        .unwrap_or(bytes.len());

    let authority = &bytes[authority_start..authority_end];
    let at_positions: Vec<usize> = authority
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i))
        .collect();

    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }

    let (userinfo, host_start_in_authority) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at,
            });
        }
        (
            Some(input[authority_start..authority_start + at].to_owned()),
            at + 1,
        )
    } else {
        (None, 0)
    };

    let host_start = authority_start + host_start_in_authority;
    let host_bytes = &authority[host_start_in_authority..];
    let mut ipv6 = false;
    let (host, port) = if host_bytes.first() == Some(&b'[') {
        let open_at = host_start;
        let close_rel = match host_bytes.iter().position(|&b| b == b']') {
            Some(pos) => pos,
            None => return Err(ParseError::InvalidIpv6 { at: open_at }),
        };
        let interior = &host_bytes[1..close_rel];
        let interior_str = std::str::from_utf8(interior).unwrap_or("");
        if interior_str.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: open_at });
        }
        let suffix = &host_bytes[close_rel + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix[0] == b':' {
            Some(parse_port(&suffix[1..], host_start + close_rel + 2)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: host_start + close_rel + 1,
            });
        };
        ipv6 = true;
        (interior_str.to_ascii_lowercase(), port)
    } else {
        let split = host_bytes.iter().position(|&b| b == b':');
        let host_part = split.map(|i| &host_bytes[..i]).unwrap_or(host_bytes);
        let host_at = host_start;
        if host_part.is_empty() {
            return Err(ParseError::InvalidHost { at: host_at });
        }
        for (i, &byte) in host_part.iter().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost { at: host_at + i });
            }
        }
        let port = if let Some(split) = split {
            Some(parse_port(
                &host_bytes[split + 1..],
                host_start + split + 1,
            )?)
        } else {
            None
        };
        (
            std::str::from_utf8(host_part)
                .unwrap_or("")
                .to_ascii_lowercase(),
            port,
        )
    };

    let fragment_start = bytes[authority_end..]
        .iter()
        .position(|&b| b == b'#')
        .map(|n| authority_end + n);
    let before_fragment_end = fragment_start.unwrap_or(bytes.len());

    let query_start = bytes[authority_end..before_fragment_end]
        .iter()
        .position(|&b| b == b'?')
        .map(|n| authority_end + n);

    let path_end = query_start.unwrap_or(before_fragment_end);
    let path = if path_end == authority_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };

    let query = query_start.map(|start| {
        let end = before_fragment_end;
        input[start + 1..end].to_owned()
    });
    let fragment = fragment_start.map(|start| input[start + 1..].to_owned());

    if let Some(ref info) = userinfo {
        let base = authority_start;
        decode_component(Component::UserInfo, info)
            .map_err(|e| map_codec_error(e, base))?;
    }
    if path_end != authority_end {
        decode_component(Component::Path, &path)
            .map_err(|e| map_codec_error(e, authority_end))?;
    }
    if let (Some(start), Some(ref value)) = (query_start, query.as_ref()) {
        decode_component(Component::Query, value)
            .map_err(|e| map_codec_error(e, start + 1))?;
    }
    if let (Some(start), Some(ref value)) = (fragment_start, fragment.as_ref()) {
        decode_component(Component::Fragment, value)
            .map_err(|e| map_codec_error(e, start + 1))?;
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

fn parse_port(bytes: &[u8], start: usize) -> Result<u16, ParseError> {
    if bytes.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }

    for (i, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() {
            return Err(ParseError::InvalidPort { at: start + i });
        }
    }

    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }

    let mut value: u32 = 0;
    for &byte in bytes {
        value = match value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
        {
            Some(v) => v,
            None => return Err(ParseError::InvalidPort { at: start }),
        };
        if value > 65535 {
            return Err(ParseError::InvalidPort { at: start });
        }
    }

    if value == 0 {
        return Err(ParseError::InvalidPort { at: start });
    }
    Ok(value as u16)
}