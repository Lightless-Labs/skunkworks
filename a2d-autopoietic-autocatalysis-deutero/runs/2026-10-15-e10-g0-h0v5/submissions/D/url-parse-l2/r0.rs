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

fn is_scheme_start(b: u8) -> bool {
    b.is_ascii_alphabetic()
}

fn is_scheme_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')
}

fn is_host_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn decode_with_offsets(input: &str) -> Result<String, CodecError> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut origins = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(CodecError::BadEscape { at: i });
            }
            let Some(hi) = hex_value(bytes[i + 1]) else {
                return Err(CodecError::BadEscape { at: i });
            };
            let Some(lo) = hex_value(bytes[i + 2]) else {
                return Err(CodecError::BadEscape { at: i });
            };
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
        Ok(text) => Ok(text.to_owned()),
        Err(err) => {
            let at = origins.get(err.valid_up_to()).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at })
        }
    }
}

pub fn decode_component(_component: Component, input: &str) -> Result<String, CodecError> {
    decode_with_offsets(input)
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
            out.push(char::from(b));
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(char::from(HEX[(b >> 4) as usize]));
            out.push(char::from(HEX[(b & 0x0f) as usize]));
        }
    }
    out
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

fn parse_port(port: &str, base: usize) -> Result<u16, ParseError> {
    let bytes = port.as_bytes();
    if bytes.is_empty() {
        return Err(ParseError::InvalidPort { at: base });
    }
    if let Some(i) = bytes.iter().position(|b| !b.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: base + i });
    }
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: base });
    }
    let mut value: u32 = 0;
    for &b in bytes {
        value = value
            .saturating_mul(10)
            .saturating_add((b - b'0') as u32);
        if value > 65535 {
            return Err(ParseError::InvalidPort { at: base });
        }
    }
    if value == 0 {
        return Err(ParseError::InvalidPort { at: base });
    }
    Ok(value as u16)
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }

    let bytes = input.as_bytes();
    if let Some(at) = bytes.iter().position(|b| !(0x21..=0x7e).contains(b)) {
        return Err(ParseError::InvalidByte { at });
    }

    let Some(colon) = bytes.iter().position(|&b| b == b':') else {
        return Err(ParseError::InvalidScheme { at: input.len() });
    };
    if colon == 0 || !is_scheme_start(bytes[0]) {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for i in 1..colon {
        if !is_scheme_char(bytes[i]) {
            return Err(ParseError::InvalidScheme { at: i });
        }
    }

    if bytes.get(colon + 1) != Some(&b'/') || bytes.get(colon + 2) != Some(&b'/') {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let authority_start = colon + 3;
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|b| matches!(b, b'/' | b'?' | b'#'))
        .map(|n| authority_start + n)
        .unwrap_or(bytes.len());
    let authority = &input[authority_start..authority_end];
    let authority_bytes = authority.as_bytes();

    let at_positions: Vec<usize> = authority_bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i))
        .collect();
    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }
    let userinfo_end = at_positions.first().copied();
    if userinfo_end == Some(0) {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start,
        });
    }
    let userinfo = userinfo_end.map(|end| authority[..end].to_owned());
    let hostport_start = userinfo_end.map_or(0, |end| end + 1);
    let hostport = &authority[hostport_start..];
    let hostport_base = authority_start + hostport_start;

    let (host, port, ipv6) = if hostport.as_bytes().first() == Some(&b'[') {
        let Some(close_rel) = hostport.as_bytes()[1..]
            .iter()
            .position(|&b| b == b']')
            .map(|n| n + 1)
        else {
            return Err(ParseError::InvalidIpv6 { at: hostport_base });
        };
        let interior = &hostport[1..close_rel];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: hostport_base });
        }
        let suffix = &hostport[close_rel + 1..];
        let parsed_port = if suffix.is_empty() {
            None
        } else if suffix.as_bytes()[0] == b':' {
            Some(parse_port(&suffix[1..], hostport_base + close_rel + 2)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_base + close_rel + 1,
            });
        };
        (interior.to_ascii_lowercase(), parsed_port, true)
    } else {
        let split = hostport.as_bytes().iter().position(|&b| b == b':');
        let (host_text, port_text, port_base) = match split {
            Some(i) => (
                &hostport[..i],
                Some(&hostport[i + 1..]),
                hostport_base + i + 1,
            ),
            None => (hostport, None, 0),
        };
        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_base });
        }
        if let Some(i) = host_text.as_bytes().iter().position(|&b| !is_host_char(b)) {
            return Err(ParseError::InvalidHost { at: hostport_base + i });
        }
        let parsed_port = match port_text {
            Some(text) => Some(parse_port(text, port_base)?),
            None => None,
        };
        (host_text.to_ascii_lowercase(), parsed_port, false)
    };

    let fragment_rel = input[authority_end..]
        .as_bytes()
        .iter()
        .position(|&b| b == b'#')
        .map(|n| authority_end + n);
    let component_end = fragment_rel.unwrap_or(bytes.len());
    let query_rel = input[authority_end..component_end]
        .as_bytes()
        .iter()
        .position(|&b| b == b'?')
        .map(|n| authority_end + n);

    let path_end = query_rel.unwrap_or(component_end);
    let path = if path_end == authority_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };
    let query = query_rel.map(|start| input[start + 1..component_end].to_owned());
    let fragment = fragment_rel.map(|start| input[start + 1..].to_owned());

    if let (Some(end), Some(value)) = (userinfo_end, userinfo.as_deref()) {
        validate_component(Component::UserInfo, value, authority_start)?;
        let _ = end;
    }
    if path_end != authority_end {
        validate_component(Component::Path, &path, authority_end)?;
    }
    if let Some(start) = query_rel {
        validate_component(
            Component::Query,
            query.as_deref().unwrap_or(""),
            start + 1,
        )?;
    }
    if let Some(start) = fragment_rel {
        validate_component(
            Component::Fragment,
            fragment.as_deref().unwrap_or(""),
            start + 1,
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