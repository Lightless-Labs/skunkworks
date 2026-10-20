use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Vec<String>,
    pub build: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VersionError {
    Empty,
    MissingCore { at: usize },
    LeadingZero { at: usize },
    Overflow { at: usize },
    EmptyIdentifier { at: usize },
    InvalidChar { at: usize },
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() { return Err(VersionError::Empty); }
    let b = s.as_bytes();
    let mut i = 0;
    let mut core = [0u64; 3];
    for field in 0..3 {
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() { i += 1; }
        if i == start {
            return Err(match b.get(i).copied() {
                None | Some(b'.' | b'-' | b'+') => VersionError::MissingCore { at: i },
                _ => VersionError::InvalidChar { at: i },
            });
        }
        if i - start > 1 && b[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }
        core[field] = s[start..i].parse().map_err(|_| VersionError::Overflow { at: start })?;
        if field < 2 {
            if b.get(i) != Some(&b'.') {
                return Err(match b.get(i).copied() {
                    None | Some(b'-' | b'+') => VersionError::MissingCore { at: i },
                    _ => VersionError::InvalidChar { at: i },
                });
            }
            i += 1;
        }
    }
    let mut pre = Vec::new();
    let mut build = Vec::new();
    if i < b.len() {
        match b[i] {
            b'-' => {
                let p = i + 1;
                let q = s[p..].find('+').map(|n| p + n).unwrap_or(s.len());
                pre = parse_ids(s, p, q, true)?;
                if q < s.len() { build = parse_ids(s, q + 1, s.len(), false)?; }
            }
            b'+' => build = parse_ids(s, i + 1, s.len(), false)?,
            _ => return Err(VersionError::InvalidChar { at: i }),
        }
    }
    Ok(Version { major: core[0], minor: core[1], patch: core[2], pre, build })
}

fn parse_ids(s: &str, start: usize, end: usize, pre: bool) -> Result<Vec<String>, VersionError> {
    let mut ids = Vec::new();
    let mut a = start;
    for i in start..=end {
        if i == end || s.as_bytes()[i] == b'.' {
            if a == i { return Err(VersionError::EmptyIdentifier { at: a }); }
            let id = &s[a..i];
            for (j, c) in id.bytes().enumerate() {
                if !(c.is_ascii_alphanumeric() || c == b'-') {
                    return Err(VersionError::InvalidChar { at: a + j });
                }
            }
            if pre && id.bytes().all(|c| c.is_ascii_digit()) && id.len() > 1 && id.as_bytes()[0] == b'0' {
                return Err(VersionError::LeadingZero { at: a });
            }
            ids.push(id.to_owned());
            a = i + 1;
        }
    }
    Ok(ids)
}

fn num(s: &str) -> bool { !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) }
fn cmp_ids(a: &[String], b: &[String]) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        let xn = num(x); let yn = num(y);
        let o = match (xn,yn) {
            (true,true) => x.len().cmp(&y.len()).then_with(|| x.as_bytes().cmp(y.as_bytes())),
            (true,false) => Ordering::Less,
            (false,true) => Ordering::Greater,
            _ => x.as_bytes().cmp(y.as_bytes()),
        };
        if o != Ordering::Equal { return o; }
    }
    a.len().cmp(&b.len())
}
pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    (a.major,a.minor,a.patch).cmp(&(b.major,b.minor,b.patch))
        .then_with(|| match (a.pre.is_empty(), b.pre.is_empty()) {
            (true,true) => Ordering::Equal, (true,false) => Ordering::Greater,
            (false,true) => Ordering::Less, (false,false) => cmp_ids(&a.pre,&b.pre),
        })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError { Empty { at: usize }, Syntax { at: usize }, Version { at: usize, error: VersionError } }
#[derive(Clone, Debug)]
pub struct Range { arms: Vec<Arm> }
#[derive(Clone, Debug)]
struct Bound { v: Version, inc: bool }
#[derive(Clone, Debug)]
struct Arm { lo: Option<Bound>, hi: Option<Bound>, anchors: Vec<(u64,u64,u64)> }
#[derive(Clone, Copy)]
enum Op { Eq, Lt, Le, Gt, Ge }

fn v(a:u64,b:u64,c:u64,p:Vec<String>)->Version { Version{major:a,minor:b,patch:c,pre:p,build:Vec::new()} }
fn cp(x:&Version)->(u64,u64,u64) {(x.major,x.minor,x.patch)}
fn cmp(a:&Version,b:&Version)->Ordering { compare_versions(a,b) }
fn lower(a:&mut Arm,x:Version,inc:bool) {
    let n=Bound{v:x,inc};
    if a.lo.as_ref().map_or(true,|o| cmp(&n.v,&o.v)==Ordering::Greater || (cmp(&n.v,&o.v)==Ordering::Equal && !inc && o.inc)) { a.lo=Some(n); }
}
fn upper(a:&mut Arm,x:Version,inc:bool) {
    let n=Bound{v:x,inc};
    if a.hi.as_ref().map_or(true,|o| cmp(&n.v,&o.v)==Ordering::Less || (cmp(&n.v,&o.v)==Ordering::Equal && !inc && o.inc)) { a.hi=Some(n); }
}
fn constraint(a:&mut Arm,o:Op,x:Version) {
    if !x.pre.is_empty() { a.anchors.push(cp(&x)); }
    match o {
        Op::Eq => { lower(a,x.clone(),true); upper(a,x,true); }
        Op::Lt => upper(a,x,false), Op::Le => upper(a,x,true),
        Op::Gt => lower(a,x,false), Op::Ge => lower(a,x,true),
    }
}
fn ws(b:u8)->bool { matches!(b,b' '|b'\t'|b'\r'|b'\n') }
fn toks(s:&str,a:usize,z:usize)->Vec<(usize,usize)> {
    let mut r=Vec::new();let mut i=a;
    while i<z { while i<z&&ws(s.as_bytes()[i]){i+=1} if i==z{break}
        let p=i;while i<z&&!ws(s.as_bytes()[i]){i+=1}r.push((p,i));}
    r
}
fn partial(raw:&str, at:usize)->Result<(Vec<Option<u64>>,Option<Version>,usize),RangeError> {
    if raw.contains(['-','+']) || (raw.split('.').count()>=3 && !raw.split('.').any(|x|matches!(x,"*"|"x"|"X"))) {
        let x=parse_version(raw).map_err(|error|RangeError::Version{at,error})?;
        return Ok((vec![Some(x.major),Some(x.minor),Some(x.patch)],Some(x),3));
    }
    let p:Vec<_>=raw.split('.').collect();
    if raw.is_empty()||p.len()>3{return Err(RangeError::Syntax{at})}
    let mut f=Vec::new();let mut wild=false;let mut k=0;
    for x in p {
        if x.is_empty(){return Err(RangeError::Syntax{at})}
        if matches!(x,"*"|"x"|"X"){wild=true;f.push(None)}
        else {
            if wild||!x.bytes().all(|c|c.is_ascii_digit())||(x.len()>1&&x.starts_with('0')) {return Err(RangeError::Syntax{at})}
            f.push(Some(x.parse().map_err(|_|RangeError::Syntax{at})?));k+=1;
        }
    }
    Ok((f,None,k))
}
fn tuple(f:&[Option<u64>])->(u64,u64,u64) {
    let mut a=[0;3];for(i,x)in f.iter().enumerate(){if let Some(n)=x{a[i]=*n}}(a[0],a[1],a[2])
}
fn bump(t:(u64,u64,u64),n:usize,at:usize)->Result<(u64,u64,u64),RangeError>{
    let mut a=[t.0,t.1,t.2];a[n]=a[n].checked_add(1).ok_or(RangeError::Syntax{at})?;
    for x in a.iter_mut().skip(n+1){*x=0}(a[0],a[1],a[2]).pipe(Ok)
}
trait Pipe:Sized{fn pipe<T>(self,f:impl FnOnce(Self)->T)->T{f(self)}} impl<T> Pipe for T {}
fn zero(t:(u64,u64,u64))->Version{v(t.0,t.1,t.2,vec!["0".into()])}
fn partial_up(f:&[Option<u64>],k:usize,at:usize)->Result<Option<Version>,RangeError>{
    if k==0{return Ok(None)}let t=tuple(f);let n=if k==1{0}else{1};Ok(Some(zero(bump(t,n,at)?)))
}
fn operand(raw:&str,at:usize,kind:u8,a:&mut Arm)->Result<(),RangeError>{
    let cut=if kind==b'^'||kind==b'~'{1}else{0};
    let (f,full,k)=partial(&raw[cut..],at+cut)?;
    if cut!=0 {
        if k==0{return Err(RangeError::Syntax{at})}
        let t=tuple(&f);let lo=full.clone().unwrap_or_else(||v(t.0,t.1,t.2,Vec::new()));
        constraint(a,Op::Ge,lo);
        let n=if kind==b'^'{if t.0>0||k==1{0}else if t.1>0||k==2{1}else{2}}else if k==1{0}else{1};
        upper(a,zero(bump(t,n,at)?),false);return Ok(())
    }
    if let Some(x)=full{constraint(a,Op::Eq,x);return Ok(())}
    if k==0{return Ok(())}
    let t=tuple(&f);constraint(a,Op::Ge,v(t.0,t.1,t.2,Vec::new()));
    if let Some(x)=partial_up(&f,k,at)?{upper(a,x,false)}Ok(())
}
fn comparator(raw:&str,at:usize,a:&mut Arm)->Result<(),RangeError>{
    let b=raw.as_bytes();let(o,n)=if b.starts_with(b">="){(Op::Ge,2)}else if b.starts_with(b"<="){(Op::Le,2)}else if b.starts_with(b">"){(Op::Gt,1)}else if b.starts_with(b"<"){(Op::Lt,1)}else{(Op::Eq,1)};
    if n==b.len()||!b[n].is_ascii_digit(){return Err(RangeError::Syntax{at})}
    let x=parse_version(&raw[n..]).map_err(|error|RangeError::Version{at:at+n,error})?;constraint(a,o,x);Ok(())
}
fn arm(s:&str,start:usize,end:usize)->Result<Arm,RangeError>{
    let t=toks(s,start,end);if t.is_empty(){return Err(RangeError::Empty{at:start})}
    let minus=t.iter().find(|&&(a,b)|&s[a..b]=="-").map(|x|x.0);
    if let Some(m)=minus {
        if t.len()!=3||t[1]!=(m,m+1){return Err(RangeError::Syntax{at:m})}
        let (lf,lv,lk)=partial(&s[t[0].0..t[0].1],t[0].0)?;
        let (hf,hv,hk)=partial(&s[t[2].0..t[2].1],t[2].0)?;
        if lk==0{return Err(RangeError::Syntax{at:t[0].0})}if hk==0{return Err(RangeError::Syntax{at:t[2].0})}
        let mut a=Arm{lo:None,hi:None,anchors:Vec::new()};
        let l=lv.unwrap_or_else(||{let x=tuple(&lf);v(x.0,x.1,x.2,Vec::new())});constraint(&mut a,Op::Ge,l);
        if let Some(h)=hv{constraint(&mut a,Op::Le,h)}else if hk==3{let x=tuple(&hf);constraint(&mut a,Op::Le,v(x.0,x.1,x.2,Vec::new()))}
        else if let Some(h)=partial_up(&hf,hk,t[2].0)?{upper(&mut a,h,false)}return Ok(a)
    }
    let mut a=Arm{lo:None,hi:None,anchors:Vec::new()};
    for &(x,y)in&t {let q=&s[x..y];let c=q.as_bytes()[0];if matches!(c,b'<'|b'>'|b'='){comparator(q,x,&mut a)?}
        else if matches!(c,b'^'|b'~'){operand(q,x,c,&mut a)?}
        else if c.is_ascii_digit()||matches!(c,b'*'|b'x'|b'X'){operand(q,x,0,&mut a)?}
        else{return Err(RangeError::Syntax{at:x})}}
    Ok(a)
}
pub fn parse_range(s:&str)->Result<Range,RangeError>{
    let b=s.as_bytes();let mut sep=Vec::new();let mut i=0;
    while i+1<b.len(){if b[i]==b'|'&&b[i+1]==b'|'&&(i==0||ws(b[i-1]))&&(i+2==b.len()||ws(b[i+2])){sep.push(i);i+=2}else{i+=1}}
    if s.bytes().all(ws){return Err(RangeError::Empty{at:0})}
    let mut arms=Vec::new();let mut st=0;
    for p in sep {let a=(st..p).find(|&j|!ws(b[j])).unwrap_or(p);let z=(st..p).rev().find(|&j|!ws(b[j])).map(|j|j+1).unwrap_or(st);
        if a>=z{return Err(RangeError::Empty{at:p})}arms.push(arm(s,a,z)?);st=p+2;}
    let a=(st..b.len()).find(|&j|!ws(b[j])).unwrap_or(b.len());let z=(st..b.len()).rev().find(|&j|!ws(b[j])).map(|j|j+1).unwrap_or(st);
    if a>=z{return Err(RangeError::Empty{at:b.len()})}arms.push(arm(s,a,z)?);Ok(Range{arms})
}
fn bounds(x:&Version,a:&Arm)->bool{
    if let Some(l)=&a.lo{let c=cmp(x,&l.v);if c==Ordering::Less||(c==Ordering::Equal&&!l.inc){return false}}
    if let Some(h)=&a.hi{let c=cmp(x,&h.v);if c==Ordering::Greater||(c==Ordering::Equal&&!h.inc){return false}}true
}
fn amatch(x:&Version,a:&Arm,all:bool)->bool{bounds(x,a)&&(x.pre.is_empty()||all||a.anchors.contains(&cp(x)))}
pub fn matches_with(v:&Version,r:&Range,include_prerelease:bool)->bool{r.arms.iter().any(|a|amatch(v,a,include_prerelease))}
pub fn matches(v:&Version,r:&Range)->bool{matches_with(v,r,false)}
#[derive(Clone,Debug,PartialEq,Eq)]pub struct CandidateError{pub index:usize,pub error:VersionError}
fn extreme(c:&[String],r:&Range,max:bool)->Result<Option<String>,CandidateError>{
    let mut vs=Vec::new();for(i,s)in c.iter().enumerate(){vs.push(parse_version(s).map_err(|error|CandidateError{index:i,error})?)}
    let mut best=None;for i in 0..vs.len(){if !matches(&vs[i],r){continue}if let Some(j)=best{let o=compare_versions(&vs[i],&vs[j]);if(max&&o==Ordering::Greater)||(!max&&o==Ordering::Less){best=Some(i)}}else{best=Some(i)}}Ok(best.map(|i|c[i].clone()))
}
pub fn max_satisfying(c:&[String],r:&Range)->Result<Option<String>,CandidateError>{extreme(c,r,true)}
pub fn min_satisfying(c:&[String],r:&Range)->Result<Option<String>,CandidateError>{extreme(c,r,false)}
fn next(t:(u64,u64,u64))->Option<(u64,u64,u64)>{if t.2<u64::MAX{Some((t.0,t.1,t.2+1))}else if t.1<u64::MAX{Some((t.0,t.1+1,0))}else if t.0<u64::MAX{Some((t.0+1,0,0))}else{None}}
fn pair(a:&Arm,b:&Arm,all:bool)->bool{
    let mut x=Arm{lo:None,hi:None,anchors:Vec::new()};
    for q in [a,b]{if let Some(l)=&q.lo{lower(&mut x,l.v.clone(),l.inc)}if let Some(h)=&q.hi{upper(&mut x,h.v.clone(),h.inc)}}
    if let(Some(l),Some(h))=(&x.lo,&x.hi){let c=cmp(&l.v,&h.v);if c==Ordering::Greater||(c==Ordering::Equal&&(!l.inc||!h.inc)){return false}}
    if all{return true}
    for t in a.anchors.iter().filter(|t|b.anchors.contains(t)){let z=zero(*t);if bounds(&z,a)&&bounds(&z,b){return true}}
    let candidate=match &x.lo{None=>v(0,0,0,Vec::new()),Some(l)if l.v.pre.is_empty()&&l.inc=>l.v.clone(),Some(l)if !l.v.pre.is_empty()=>v(l.v.major,l.v.minor,l.v.patch,Vec::new()),Some(l)=>match next(cp(&l.v)){Some(t)=>v(t.0,t.1,t.2,Vec::new()),None=>return false}};
    candidate.pre.is_empty()&&bounds(&candidate,a)&&bounds(&candidate,b)
}
pub fn intersects(a:&Range,b:&Range,include_prerelease:bool)->bool{a.arms.iter().any(|x|b.arms.iter().any(|y|pair(x,y,include_prerelease)))}
fn disp(v:&Version)->String{let mut s=format!("{}.{}.{}",v.major,v.minor,v.patch);if !v.pre.is_empty(){s.push('-');s.push_str(&v.pre.join("."))}s}
fn c_arm(a:&Arm)->Option<String>{
    let any=pair(a,&Arm{lo:None,hi:None,anchors:Vec::new()},false)||a.anchors.iter().any(|t|{let z=zero(*t);bounds(&z,a)});
    if !any{return None}
    let l=a.lo.as_ref().map(|x|format!("{}{}",if x.inc{">="}else{">"},disp(&x.v))).unwrap_or("*".into());
    let h=a.hi.as_ref().map(|x|format!("{}{}",if x.inc{"<="}else{"<"},disp(&x.v))).unwrap_or("*".into());
    let mut an=a.anchors.clone();an.sort();an.dedup();let p=if an.is_empty(){"-".into()}else{an.iter().map(|t|format!("{}.{}.{}",t.0,t.1,t.2)).collect::<Vec<_>>().join(",")};
    Some(format!("[{l};{h};pre={p}]"))
}
pub fn canonical(r:&Range)->String{let mut x:Vec<_>=r.arms.iter().filter_map(c_arm).collect();x.sort();x.dedup();if x.is_empty(){"EMPTY".into()}else{x.join(" || ")}}
