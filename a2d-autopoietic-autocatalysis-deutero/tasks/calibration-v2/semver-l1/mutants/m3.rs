// Deliberate mutant: equality includes build metadata
use std::cmp::Ordering;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u64, pub minor: u64, pub patch: u64,
    pub pre: Vec<String>, pub build: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VersionError {
    Empty, MissingCore { at: usize }, LeadingZero { at: usize },
    Overflow { at: usize }, EmptyIdentifier { at: usize }, InvalidChar { at: usize },
}
pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() { return Err(VersionError::Empty); }
    let b=s.as_bytes(); let mut p=0; let mut nums=[0;3];
    for i in 0..3 {
        let start=p;
        while p<b.len() && b[p].is_ascii_digit() { p+=1; }
        if start==p {
            return Err(if p==b.len() || matches!(b[p], b'.'|b'-'|b'+') {
                VersionError::MissingCore{at:p}
            } else { VersionError::InvalidChar{at:p} });
        }
        if p-start>1 && b[start]==b'0' { return Err(VersionError::LeadingZero{at:start}); }
        nums[i]=s[start..p].parse().map_err(|_| VersionError::Overflow{at:start})?;
        if i<2 {
            if p==b.len() || matches!(b[p], b'-'|b'+') { return Err(VersionError::MissingCore{at:p}); }
            if b[p]!=b'.' { return Err(VersionError::InvalidChar{at:p}); }
            p+=1;
        }
    }
    let mut pre=Vec::new(); let mut build=Vec::new();
    if p<b.len() && b[p]==b'-' {
        p+=1; pre=identifiers(s,&mut p,true)?;
    }
    if p<b.len() && b[p]==b'+' {
        p+=1; build=identifiers(s,&mut p,false)?;
    }
    if p<b.len() { return Err(VersionError::InvalidChar{at:p}); }
    Ok(Version{major:nums[0],minor:nums[1],patch:nums[2],pre,build})
}
fn identifiers(s:&str,p:&mut usize,pre:bool)->Result<Vec<String>,VersionError> {
    let b=s.as_bytes(); let mut out=Vec::new();
    loop {
        let start=*p;
        while *p<b.len() && b[*p]!=b'.' && !(pre && b[*p]==b'+') {
            if !(b[*p].is_ascii_alphanumeric() || b[*p]==b'-') { return Err(VersionError::InvalidChar{at:*p}); }
            *p+=1;
        }
        if *p==start { return Err(VersionError::EmptyIdentifier{at:start}); }
        let id=&s[start..*p];
        if pre && id.len()>1 && id.as_bytes()[0]==b'0' && id.bytes().all(|c|c.is_ascii_digit()) {
            return Err(VersionError::LeadingZero{at:start});
        }
        out.push(id.to_string());
        if *p==b.len() || (pre && b[*p]==b'+') { break; }
        *p+=1;
    }
    Ok(out)
}
pub fn compare_versions(a:&Version,b:&Version)->Ordering {
    let core=(a.major,a.minor,a.patch).cmp(&(b.major,b.minor,b.patch));
    if core!=Ordering::Equal { return core; }
    if a.pre.is_empty() || b.pre.is_empty() {
        return match (a.pre.is_empty(),b.pre.is_empty()) {
            (true,false)=>Ordering::Greater,(false,true)=>Ordering::Less,_=>Ordering::Equal,
        };
    }
    for (x,y) in a.pre.iter().zip(&b.pre) {
        let xn=x.bytes().all(|c|c.is_ascii_digit()); let yn=y.bytes().all(|c|c.is_ascii_digit());
        let c=match (xn,yn) {
            (true,true)=>x.len().cmp(&y.len()).then_with(||x.cmp(y)),
            (true,false)=>Ordering::Less,(false,true)=>Ordering::Greater,
            (false,false)=>x.cmp(y),
        };
        if c!=Ordering::Equal {return c;}
    }
    a.pre.len().cmp(&b.pre.len())
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError { Empty { at: usize }, Syntax { at: usize }, Version { at: usize, error: VersionError } }
#[derive(Clone, Debug)]
pub struct Range { arms: Vec<Arm> }
#[derive(Clone, Debug)]
struct Arm { atoms: Vec<Atom>, anchors: BTreeSet<(u64,u64,u64)> }
#[derive(Clone, Debug)]
struct Atom { op: u8, v: Version }
fn tuple(v:&Version)->(u64,u64,u64) { (v.major,v.minor,v.patch) }
fn plain(t:(u64,u64,u64))->Version { Version{major:t.0,minor:t.1,patch:t.2,pre:vec![],build:vec![]} }
fn floor(t:(u64,u64,u64))->Version { let mut v=plain(t); v.pre.push("0".into()); v }
fn version_at(s:&str,at:usize)->Result<Version,RangeError> { parse_version(s).map_err(|error|RangeError::Version{at,error}) }
fn add(a:&mut Arm,op:u8,v:Version,anchor:bool) {
    if anchor && !v.pre.is_empty() {a.anchors.insert(tuple(&v));}
    a.atoms.push(Atom{op,v});
}
fn tokens(s:&str)->Vec<(&str,usize)> {
    let b=s.as_bytes();let mut p=0;let mut out=vec![];
    while p<b.len() {
        if matches!(b[p], b' '|b'\t'|b'\r'|b'\n') {p+=1;continue;}
        let st=p;
        while p<b.len() && !matches!(b[p], b' '|b'\t'|b'\r'|b'\n') {p+=1;}
        out.push((&s[st..p],st));
    }
    out
}
fn atom_matches(v:&Version,a:&Atom)->bool {
    let c=compare_versions(v,&a.v);
    match a.op {0=>c==Ordering::Equal && v.build==a.v.build,1=>c==Ordering::Less,2=>c!=Ordering::Greater,3=>c==Ordering::Greater,_=>c!=Ordering::Less}
}
fn arm_matches(v:&Version,a:&Arm,include:bool)->bool {
    (include || v.pre.is_empty() || a.anchors.contains(&tuple(v))) && a.atoms.iter().all(|c|atom_matches(v,c))
}
pub fn parse_range(s:&str)->Result<Range,RangeError> {
    let ts=tokens(s);if ts.is_empty(){return Err(RangeError::Empty{at:0});}
    let mut a=Arm{atoms:vec![],anchors:BTreeSet::new()};
    for (s,at) in ts { full_token(&mut a,s,at)?; }
    Ok(Range{arms:vec![a]})
}
fn full_token(a:&mut Arm,s:&str,at:usize)->Result<(),RangeError> {
    let (op,n)=if s.starts_with(">="){(4,2)}else if s.starts_with("<="){(2,2)}
        else if s.starts_with('>'){(3,1)}else if s.starts_with('<'){(1,1)}else if s.starts_with('='){(0,1)}else{(0,0)};
    let part=&s[n..];
    if part.is_empty() || !part.as_bytes()[0].is_ascii_digit(){return Err(RangeError::Syntax{at});}
    add(a,op,version_at(part,at+n)?,true);Ok(())
}
pub fn matches(v:&Version,r:&Range)->bool {r.arms.iter().any(|a|arm_matches(v,a,false))}
