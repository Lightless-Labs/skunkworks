#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RomanCase { Upper, Lower }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RomanError { Empty, InvalidByte { index: usize }, MixedCase { index: usize }, NonCanonical { index: usize }, Range }
pub fn roman_encode(value: u32, case: RomanCase) -> Result<String, RomanError> {
    if value > 3999 { return Err(RomanError::Range); }
    if value == 0 { return Ok(if case == RomanCase::Upper { "N" } else { "n" }.into()); }
    let mut n = value;
    let mut s = String::new();
    for (v,t) in [(1000,"M"),(900,"CM"),(500,"D"),(400,"CD"),(100,"C"),(90,"XC"),(50,"L"),(40,"XL"),(10,"X"),(9,"IX"),(5,"V"),(4,"IV"),(1,"I")] {
        while n >= v { s.push_str(t); n -= v; }
    }
    if case == RomanCase::Lower { s.make_ascii_lowercase(); }
    Ok(s)
}
fn rv(b: u8) -> u32 {
    match b.to_ascii_uppercase() { b'I'=>1,b'V'=>5,b'X'=>10,b'L'=>50,b'C'=>100,b'D'=>500,b'M'=>1000,_=>0 }
}
pub fn roman_decode(input: &str) -> Result<u32, RomanError> {
    let b = input.as_bytes();
    if b.is_empty() { return Err(RomanError::Empty); }
    for (index,&c) in b.iter().enumerate() {
        if !b"IVXLCDMNivxlcdmn".contains(&c) { return Err(RomanError::InvalidByte { index }); }
    }
    let upper = b[0].is_ascii_uppercase();
    for (index,&c) in b.iter().enumerate() {
        if c.is_ascii_uppercase() != upper { return Err(RomanError::MixedCase { index }); }
    }
    if b.len() == 1 && b[0].eq_ignore_ascii_case(&b'N') { return Ok(0); }
    if b.iter().any(|c| c.eq_ignore_ascii_case(&b'N')) { return Err(RomanError::NonCanonical { index: 0 }); }
    let mut n = 0u32;
    let mut i = 0;
    while i < b.len() {
        let a = rv(b[i]);
        if i+1 < b.len() && a < rv(b[i+1]) { n = n.saturating_add(rv(b[i+1])-a).min(4000); i += 2; }
        else { n = n.saturating_add(a).min(4000); i += 1; }
    }
    if n > 3999 { return Err(RomanError::NonCanonical { index: 0 }); }
    let canonical = roman_encode(n, if upper { RomanCase::Upper } else { RomanCase::Lower })?;
    if canonical != input {
        let index = b.iter().zip(canonical.as_bytes()).position(|(a,c)| a != c).unwrap_or(b.len().min(canonical.len()));
        return Err(RomanError::NonCanonical { index });
    }
    Ok(n)
}
