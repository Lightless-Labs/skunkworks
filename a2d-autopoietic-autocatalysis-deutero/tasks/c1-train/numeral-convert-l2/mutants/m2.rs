#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseStyle { pub upper: bool, pub group: usize, pub min_width: usize }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseError { BadBase { which: usize, value: u8 }, BadStyle, Empty { operand: usize }, InvalidTag { operand: usize, index: usize }, InvalidDigit { operand: usize, index: usize }, Separator { operand: usize, index: usize }, LeadingZero { operand: usize, index: usize } }
fn radix(v: u8, which: usize) -> Result<(),BaseError> {
    if !(2..=36).contains(&v) { Err(BaseError::BadBase { which, value:v }) } else { Ok(()) }
}
fn styling(s: &BaseStyle) -> Result<(),BaseError> {
    if s.group > 8 || s.min_width > 256 { Err(BaseError::BadStyle) } else { Ok(()) }
}
fn digit(b: u8) -> Option<u8> {
    match b { b'0'..=b'9'=>Some(b-b'0'), b'A'..=b'Z'=>Some(b-b'A'+10), b'a'..=b'z'=>Some(b-b'a'+10), _=>None }
}
fn number(input: &str, base:u8, operand:usize) -> Result<Vec<u8>,BaseError> {
    if input.is_empty() { return Err(BaseError::Empty { operand }); }
    let b=input.as_bytes();
    let start=if let Some(i)=b.iter().position(|c| *c==b'#') {
        if &input[..i]!=base.to_string() { return Err(BaseError::InvalidTag { operand, index:0 }); }
        i+1
    } else { 0 };
    if start==b.len() { return Err(BaseError::Empty { operand }); }
    for i in start..b.len() {
        if b[i]!=b'_' && digit(b[i]).is_none_or(|v| v>=base) { return Err(BaseError::InvalidDigit { operand,index:i-start }); }
    }
    for i in start..b.len() {
        if b[i]==b'_' && (i==start || i+1==b.len() || b[i-1]==b'_' || b[i+1]==b'_') { return Err(BaseError::Separator { operand,index:i }); }
    }
    let count=b[start..].iter().filter(|c| **c!=b'_').count();
    if count>1 && b[start]==b'0' { return Err(BaseError::LeadingZero { operand,index:start }); }
    let mut a=vec![0u8];
    for &c in &b[start..] {
        if c==b'_' { continue; }
        let mut carry=digit(c).unwrap() as u32;
        for v in &mut a { let t=*v as u32*base as u32+carry; *v=(t%256) as u8; carry=t/256; }
        while carry>0 { a.push((carry%256) as u8); carry/=256; }
    }
    Ok(a)
}
fn render(mut a:Vec<u8>, base:u8, s:&BaseStyle) -> String {
    let alphabet=if s.upper { b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ" } else { b"0123456789abcdefghijklmnopqrstuvwxyz" };
    let mut rev=Vec::new();
    loop {
        let mut rem=0u32;
        for v in a.iter_mut().rev() { let t=rem*256+*v as u32; *v=(t/base as u32) as u8; rem=t%base as u32; }
        rev.push(alphabet[rem as usize]);
        while a.len()>1 && a.last()==Some(&0) { a.pop(); }
        if a==[0] { break; }
    }
    while rev.len()<s.min_width { rev.push(b'0'); }
    rev.reverse();
    let len=rev.len();
    let mut out=String::new();
    for (i,c) in rev.into_iter().enumerate() {
        if s.group>0 && i>0 && (len-i)%s.group==0 { out.push('_'); }
        out.push(c as char);
    }
    out
}
pub fn convert_base(input:&str, from:u8, to:u8, style:BaseStyle) -> Result<String,BaseError> {
    radix(from,0)?; radix(to,1)?; styling(&style)?;
    Ok(render(number(input,from,0)?,to,&style))
}
pub fn add_base(left:&str, right:&str, base:u8, style:BaseStyle) -> Result<String,BaseError> {
    radix(base,0)?; styling(&style)?;
    let mut a=number(left,base,0)?;
    let b=number(right,base,1)?;
    a.resize(a.len().max(b.len()),0);
    let mut carry=0u16;
    for i in 0..a.len() { let t=a[i] as u16+b.get(i).copied().unwrap_or(0) as u16+carry; a[i]=(t%256) as u8; carry=t/256; }
    if carry>0 { a.push(carry as u8); }
    Ok(render(a,base,&style))
}
