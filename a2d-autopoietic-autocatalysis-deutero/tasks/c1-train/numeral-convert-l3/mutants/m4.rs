#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignedSystem { BalancedTernary, Negabinary }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignedError { Empty, InvalidByte { index: usize }, InvalidChecksum { index: usize }, LeadingZero, Range { index: usize }, ChecksumMismatch { expected: u8, found: u8 }, OutOfRange }
const SIGNED_LIMIT:i128=(1i128<<120)-1;
fn check_digits(b:&[u8]) -> u8 {
    b.iter().enumerate().fold(0u8,|s,(i,c)| {
        let d=if *c==b'T' { 2 } else { (*c-b'0') as usize };
        ((s as usize+((i%7+1)%7)*d)%7) as u8
    })
}
pub fn signed_encode(value:i128, system:SignedSystem, checksum:bool) -> Result<String,SignedError> {
    if value < -SIGNED_LIMIT || value > SIGNED_LIMIT { return Err(SignedError::OutOfRange); }
    let mut n=if system==SignedSystem::Negabinary { value.abs() } else { value };
    let mut b=Vec::new();
    if n==0 { b.push(b'0'); }
    while n!=0 {
        match system {
            SignedSystem::BalancedTernary => {
                let r=n.rem_euclid(3); let d=if r==2 { -1 } else { r };
                b.push(if d == -1 { b'T' } else { b'0'+d as u8 });
                n=(n-d)/3;
            },
            SignedSystem::Negabinary => { let d=n.rem_euclid(2); b.push(b'0'+d as u8); n=(n-d)/-2; }
        }
    }
    b.reverse();
    if checksum { let c=check_digits(&b); b.push(b':'); b.push(b'0'+c); }
    Ok(String::from_utf8(b).unwrap())
}
pub fn signed_decode(input:&str, system:SignedSystem) -> Result<i128,SignedError> {
    let b=input.as_bytes();
    let end=b.iter().position(|c| *c==b':').unwrap_or(b.len());
    if end==0 { return Err(SignedError::Empty); }
    for (index,&c) in b[..end].iter().enumerate() {
        let valid=match system { SignedSystem::BalancedTernary=>b"T01".contains(&c), SignedSystem::Negabinary=>b"01".contains(&c) };
        if !valid { return Err(SignedError::InvalidByte { index }); }
    }
    let suffix=if end<b.len() {
        if b.len()!=end+2 || !(b'0'..=b'6').contains(&b[end+1]) { return Err(SignedError::InvalidChecksum { index:end }); }
        Some(b[end+1]-b'0')
    } else { None };
    if end>1 && b[0]==b'0' { return Err(SignedError::LeadingZero); }
    let base=if system==SignedSystem::BalancedTernary { 3 } else { -2 };
    let mut n=0i128;
    for (index,&c) in b[..end].iter().enumerate() {
        let d=if c==b'T' { -1 } else { (c-b'0') as i128 };
        n=n*base+d;
        if n < -SIGNED_LIMIT || n > SIGNED_LIMIT { return Err(SignedError::Range { index }); }
    }
    if let Some(found)=suffix {
        let expected=check_digits(&b[..end]);
        if found!=expected { return Err(SignedError::ChecksumMismatch { expected,found }); }
    }
    Ok(n)
}
pub fn signed_recode(input:&str, from:SignedSystem, to:SignedSystem, checksum:bool) -> Result<String,SignedError> {
    signed_encode(signed_decode(input,from)?,to,checksum)
}
