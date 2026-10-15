#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrapError {
    WidthZero,
    WidthTooLarge { width: usize },
    InvalidByte { index: usize, byte: u8 },
}

pub fn wrap(input: &str, width: usize) -> Result<Vec<String>, WrapError> {
    if width == 0 { return Err(WrapError::WidthZero); }
    if width > 256 { return Err(WrapError::WidthTooLarge { width }); }
    for (index, byte) in input.bytes().enumerate() {
        if !(32..=126).contains(&byte) && ![9,10,13].contains(&byte) {
            return Err(WrapError::InvalidByte { index, byte });
        }
    }
    let mut out = Vec::new();
    let mut line = String::new();
    for word in input.split(|c| matches!(c, ' '|'\t'|'\r'|'\n')).filter(|s| !s.is_empty()) {
        let mut suffix = word;
        while !suffix.is_empty() {
            if line.len() == width { out.push(std::mem::take(&mut line)); }
            let sep = usize::from(!line.is_empty());
            let room = width - line.len() - sep;
            if suffix.len() <= width {
                if suffix.len() <= room {
                    if sep == 1 { line.push(' '); }
                    line.push_str(suffix);
                    suffix = "";
                } else { out.push(std::mem::take(&mut line)); }
            } else if line.is_empty() {
                out.push(suffix[..width].to_string());
                suffix = &suffix[width..];
                if !suffix.is_empty() && suffix.len() <= width { out.push(suffix.to_string()); suffix = ""; }
            } else if room >= 2 {
                line.push(' ');
                line.push_str(&suffix[..room]);
                suffix = &suffix[room..];
                out.push(std::mem::take(&mut line));
            } else { out.push(std::mem::take(&mut line)); }
        }
    }
    if !line.is_empty() { out.push(line); }
    Ok(out)
}
