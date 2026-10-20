#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alphabet { Standard, UrlSafe }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Padding { Required, Forbidden, Optional }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding { Lf, CrLf }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whitespace { None, IgnoreAscii, Wrapped { width: usize, ending: LineEnding } }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options { pub alphabet: Alphabet, pub padding: Padding, pub whitespace: Whitespace, pub strict: bool, pub max_symbols: Option<usize> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 }, BadPadding { offset: usize }, Truncated { offset: usize },
    AlreadyFinished, InvalidWidth { width: usize }, BadWrap { offset: usize }, NonCanonical { offset: usize },
    MixedAlphabet { offset: usize }, Overlong { offset: usize, limit: usize },
}
fn valid(o: Options) -> Result<(), Error> {
    if let Whitespace::Wrapped { width, .. } = o.whitespace {
        if width < 4 || width > 120 || width % 4 != 0 { return Err(Error::InvalidWidth { width }); }
    }
    Ok(())
}
fn val(b:u8,a:Alphabet)->Option<u8>{match b{b'A'..=b'Z'=>Some(b-65),b'a'..=b'z'=>Some(b-97+26),b'0'..=b'9'=>Some(b-48+52),b'+' if a==Alphabet::Standard=>Some(62),b'/' if a==Alphabet::Standard=>Some(63),b'-' if a==Alphabet::UrlSafe=>Some(62),b'_' if a==Alphabet::UrlSafe=>Some(63),_=>None}}
fn other(b:u8,a:Alphabet)->bool{match a{Alphabet::Standard=>b==b'-'||b==b'_',Alphabet::UrlSafe=>b==b'+'||b==b'/'}}
fn raw(input:&[u8],a:Alphabet,p:Padding)->Vec<u8>{
 let t=match a{Alphabet::Standard=>b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",Alphabet::UrlSafe=>b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"};
 let mut v=Vec::new(); let mut i=0;
 while i+3<=input.len(){let n=((input[i]as u32)<<16)|((input[i+1]as u32)<<8)|input[i+2]as u32; for s in [18,12,6,0]{v.push(t[((n>>s)&63)as usize]);} i+=3;}
 match input.len()-i {1=>{let x=input[i];v.push(t[(x>>2)as usize]);v.push(t[((x&3)<<4)as usize]);if p!=Padding::Forbidden{v.extend_from_slice(b"==");}},2=>{let n=((input[i]as u16)<<8)|input[i+1]as u16;v.push(t[(n>>10)as usize]);v.push(t[((n>>4)&63)as usize]);v.push(t[((n&15)<<2)as usize]);if p!=Padding::Forbidden{v.push(b'=');}},_=>{}}
 v
}
fn framed(v:&[u8],w:Whitespace)->Vec<u8>{let Whitespace::Wrapped{width,ending}=w else{return v.to_vec()};let s:&[u8]=if ending==LineEnding::Lf{b"\n"}else{b"\r\n"};let mut o=Vec::new();for(i,&b)in v.iter().enumerate(){if i>0&&i%width==0{o.extend_from_slice(s);}o.push(b);}o}
pub fn encode(input:&[u8],options:Options)->Result<String,Error>{valid(options)?;Ok(String::from_utf8(framed(&raw(input,options.alphabet,options.padding),options.whitespace)).unwrap())}
pub fn decode(input:&[u8],options:Options)->Result<Vec<u8>,Error>{let mut d=Decoder::new(options)?;let mut v=d.push(input)?;v.extend(d.finish()?);Ok(v)}
pub struct Encoder{opt:Options,tail:Vec<u8>,done:bool,col:usize}
impl Encoder{
 pub fn new(options:Options)->Result<Self,Error>{valid(options)?;Ok(Self{opt:options,tail:Vec::new(),done:false,col:0})}
 fn emit(&mut self,b:&[u8])->String{let r=raw(b,self.opt.alphabet,self.opt.padding);let mut o=Vec::new();if let Whitespace::Wrapped{width,ending}=self.opt.whitespace{for x in r{if self.col==width{if ending==LineEnding::Lf{o.push(10)}else{o.extend_from_slice(b"\r\n")}self.col=0;}o.push(x);self.col+=1;}}else{o=r;}String::from_utf8(o).unwrap()}
 pub fn push(&mut self,b:&[u8])->Result<String,Error>{if self.done{return Err(Error::AlreadyFinished)}if b.is_empty(){return Ok(String::new())}let mut v=core::mem::take(&mut self.tail);v.extend_from_slice(b);let n=v.len()/3*3;self.tail.extend_from_slice(&v[n..]);Ok(self.emit(&v[..n]))}
 pub fn finish(&mut self)->Result<String,Error>{if self.done{return Err(Error::AlreadyFinished)}self.done=true;let t=core::mem::take(&mut self.tail);Ok(self.emit(&t))}
}
pub struct Decoder{opt:Options,pos:usize,seen:usize,g:Vec<(u8,usize)>,pend:Option<usize>,term:bool,row:usize,phase:u8,sep:usize,after:bool,done:bool,err:Option<Error>}
impl Decoder{
 pub fn new(options:Options)->Result<Self,Error>{valid(options)?;Ok(Self{opt:options,pos:0,seen:0,g:Vec::new(),pend:None,term:false,row:0,phase:0,sep:0,after:false,done:false,err:None})}
 fn bad(&mut self,e:Error)->Error{self.err=Some(e.clone());e}
 fn symbol(&mut self,b:u8,p:usize,o:&mut Vec<u8>)->Result<(),Error>{
  if let Some(l)=self.opt.max_symbols{if self.seen>=l{return Err(Error::Overlong{offset:p,limit:l})}}self.seen+=1;
  if self.term{return Err(Error::BadPadding{offset:p})}
  if self.pend.is_some(){if b!=61{return Err(Error::BadPadding{offset:p})}self.g.push((b,p));self.pend=None;self.term=true;let a=val(self.g[0].0,self.opt.alphabet).unwrap();let c=val(self.g[1].0,self.opt.alphabet).unwrap();o.push((a<<2)|(c>>4));if self.opt.strict&&c&15!=0{return Err(Error::NonCanonical{offset:self.g[1].1})}self.g.clear();return Ok(())}
  if b==61{if self.opt.padding==Padding::Forbidden{return Err(Error::BadPadding{offset:p})}match self.g.len(){0|1=>return Err(Error::BadPadding{offset:p}),2=>{self.g.push((b,p));self.pend=Some(p);return Ok(())},3=>{let a=val(self.g[0].0,self.opt.alphabet).unwrap();let c=val(self.g[1].0,self.opt.alphabet).unwrap();let d=val(self.g[2].0,self.opt.alphabet).unwrap();o.extend_from_slice(&[(a<<2)|(c>>4),(c<<4)|(d>>2)]);if self.opt.strict&&d&3!=0{return Err(Error::NonCanonical{offset:self.g[2].1})}self.g.clear();self.term=true;return Ok(())},_=>unreachable!()}}
  let x=match val(b,self.opt.alphabet){Some(x)=>x,None if self.opt.strict&&other(b,self.opt.alphabet)=>return Err(Error::MixedAlphabet{offset:p}),None=>return Err(Error::InvalidByte{offset:p,byte:b})};self.g.push((b,p));if self.g.len()==4{let a=val(self.g[0].0,self.opt.alphabet).unwrap();let c=val(self.g[1].0,self.opt.alphabet).unwrap();let d=val(self.g[2].0,self.opt.alphabet).unwrap();let e=val(self.g[3].0,self.opt.alphabet).unwrap();o.extend_from_slice(&[(a<<2)|(c>>4),(c<<4)|(d>>2),(d<<6)|e]);self.g.clear();}let _=x;Ok(())
 }
 pub fn push(&mut self,b:&[u8])->Result<Vec<u8>,Error>{if let Some(e)=&self.err{return Err(e.clone())}if self.done{return Err(Error::AlreadyFinished)}let mut o=Vec::new();for &x in b{let p=self.pos;self.pos+=1;match self.opt.whitespace{Whitespace::IgnoreAscii if matches!(x,9|10|11|12|13|32)=>continue,Whitespace::Wrapped{width,ending}=>{if self.phase==1{if x!=10{return Err(self.bad(Error::BadWrap{offset:p}))}self.phase=0;self.row=0;self.after=true;continue}if self.row==width{let e=if ending==LineEnding::Lf{10}else{13};if x!=e{return Err(self.bad(Error::BadWrap{offset:p}))}self.sep=p;if ending==LineEnding::CrLf{self.phase=1}else{self.row=0;self.after=true}continue}if matches!(x,9|10|11|12|13|32){return Err(self.bad(Error::BadWrap{offset:p}))}if self.after{self.after=false}self.row+=1},_=>{}}if let Err(e)=self.symbol(x,p,&mut o){return Err(self.bad(e))}}Ok(o)}
 pub fn finish(&mut self)->Result<Vec<u8>,Error>{if let Some(e)=&self.err{return Err(e.clone())}if self.done{return Err(Error::AlreadyFinished)}if self.phase==1{return Err(self.bad(Error::BadWrap{offset:self.sep}))}if self.after{return Err(self.bad(Error::BadWrap{offset:self.sep}))}if let Some(p)=self.pend{return Err(self.bad(Error::BadPadding{offset:p}))}let mut o=Vec::new();if self.g.is_empty(){self.done=true;return Ok(o)}if self.g.len()==1||self.opt.padding==Padding::Required{return Err(self.bad(Error::Truncated{offset:self.g[0].1}))}let v:Vec<u8>=self.g.iter().map(|x|val(x.0,self.opt.alphabet).unwrap()).collect();if v.len()==2{if self.opt.strict&&v[1]&15!=0{return Err(self.bad(Error::NonCanonical{offset:self.g[1].1}))}o.push((v[0]<<2)|(v[1]>>4))}else if v.len()==3{if self.opt.strict&&v[2]&3!=0{return Err(self.bad(Error::NonCanonical{offset:self.g[2].1}))}o.extend_from_slice(&[(v[0]<<2)|(v[1]>>4),(v[1]<<4)|(v[2]>>2)])}self.done=true;Ok(o)}
}
