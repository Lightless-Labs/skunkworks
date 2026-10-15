
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchOptions { pub max_offset: usize, pub fuzz: usize }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    InvalidOptions, BadHeader { line: usize }, BadHunkHeader { line: usize },
    BadBody { line: usize }, CountMismatch { line: usize }, Misordered { line: usize },
    SourceFormat { byte: usize }, SourceMissingLf, TooLarge,
    NoMatch { hunk: usize }, Ambiguous { hunk: usize, first: usize, second: usize },
}
struct Hunk { start: usize, old: usize, new: usize, rows: Vec<(u8,String)> }
fn number(s:&str)->Option<usize> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || (s.len()>1 && s.starts_with('0')) {return None;}
    s.parse::<usize>().ok().filter(|&n| n<=128)
}
fn range(s:&str)->Option<(usize,usize)> {let (a,b)=s.split_once(',')?;Some((number(a)?,number(b)?))}
fn header(s:&str)->Option<(usize,usize,usize,usize)> {
    let s=s.strip_prefix("@@ -")?.strip_suffix(" @@")?;
    let (a,b)=s.split_once(" +")?;let (os,oc)=range(a)?;let (ns,nc)=range(b)?;
    if (oc>0 && os==0)||(nc>0 && ns==0){return None;}
    Some((if oc==0 {os}else{os-1},oc,if nc==0 {ns}else{ns-1},nc))
}
fn parse_patch(patch:&str)->Result<Vec<Hunk>,ApplyError> {
    if patch.is_empty(){return Ok(Vec::new());}
    if !patch.ends_with('\n'){return Err(ApplyError::BadHeader{line:1+patch.bytes().filter(|&b| b==b'\n').count()});}
    let lines:Vec<&str>=patch.split_terminator('\n').collect();
    for (i,wanted) in ["--- before","+++ after"].iter().enumerate() {
        if lines.get(i)!=Some(wanted){return Err(ApplyError::BadHeader{line:i+1});}
    }
    if lines.len()==2 {return Err(ApplyError::BadHunkHeader{line:3});}
    let mut i=2;let mut out=Vec::new();let mut previous_end=0usize;let mut delta=0isize;
    while i<lines.len() {
        let hl=i+1;
        let (start,old,new_start,new)=header(lines[i]).ok_or(ApplyError::BadHunkHeader{line:hl})?;
        i+=1;let mut rows=Vec::new();let mut oc=0;let mut nc=0;let mut changed=false;
        while i<lines.len() && !lines[i].starts_with("@@") {
            let row=lines[i];let p=*row.as_bytes().first().ok_or(ApplyError::BadBody{line:i+1})?;
            if !matches!(p,b' '|b'-'|b'+') || row.contains('\r') {return Err(ApplyError::BadBody{line:i+1});}
            if p!=b'+'{oc+=1;}if p!=b'-'{nc+=1;}if p!=b' '{changed=true;}
            rows.push((p,row[1..].to_owned()));i+=1;
        }
        if oc!=old || nc!=new {return Err(ApplyError::CountMismatch{line:hl});}
        if !changed {return Err(ApplyError::BadBody{line:hl});}
        if start<previous_end || new_start as isize!=start as isize+delta {return Err(ApplyError::Misordered{line:hl});}
        previous_end=start+old;delta+=new as isize-old as isize;
        out.push(Hunk{start,old,new,rows});
        if out.len()>16 {return Err(ApplyError::TooLarge);}
    }
    Ok(out)
}
pub fn apply_unified(source:&str,patch:&str,options:PatchOptions)->Result<String,ApplyError> {
    if options.max_offset>16 || options.fuzz>2 {return Err(ApplyError::InvalidOptions);}
    let hunks=parse_patch(patch)?;
    if let Some(byte)=source.find('\r'){return Err(ApplyError::SourceFormat{byte});}
    if !source.is_empty() && !source.ends_with('\n'){return Err(ApplyError::SourceMissingLf);}
    let mut working:Vec<String>=source.split_terminator('\n').map(str::to_owned).collect();
    if working.len()>128 {return Err(ApplyError::TooLarge);}
    let mut net=0isize;let mut drift=0isize;
    for (index,h) in hunks.iter().enumerate() {
        let lead=h.rows.iter().take_while(|r|r.0==b' ').count().min(options.fuzz);
        let trail=h.rows.iter().rev().take_while(|r|r.0==b' ').count().min(options.fuzz);
        let expected=h.start as isize+net+drift;
        let mut best:Option<(usize,usize)>=None;
        let mut choices:Vec<(usize,usize,usize)>=Vec::new();
        if h.old<=working.len() {
            for l in 0..=lead {for r in 0..=trail {
                let body=&h.rows[l..h.rows.len()-r];
                let pattern:Vec<&str>=body.iter().filter(|x|x.0!=b'+').map(|x|x.1.as_str()).collect();
                for s in 0..=working.len()-h.old {
                    let distance=(s as isize-expected.max(0).min(working.len() as isize)).unsigned_abs();
                    if distance>options.max_offset {continue;}
                    if !working[s+l..s+h.old-r].iter().map(String::as_str).eq(pattern.iter().copied()){continue;}
                    let rank=(l+r,distance);
                    if best.is_none_or(|b|rank<b){best=Some(rank);choices.clear();}
                    if best==Some(rank){choices.push((s,l,r));}
                }
            }}
        }
        if choices.is_empty(){return Err(ApplyError::NoMatch{hunk:index+1});}
        choices.sort_unstable();
        let first=choices[0].0;
        if let Some(second)=choices.iter().find(|c|c.0!=first).map(|c|c.0){return Err(ApplyError::Ambiguous{hunk:index+1,first,second});}
        let (s,l,r)=choices[0];
        let replacement:Vec<String>=h.rows[l..h.rows.len()-r].iter().filter(|x|x.0!=b'-').map(|x|x.1.clone()).collect();
        working.splice(s+l..s+h.old-r,replacement);
        if working.len()>128 {return Err(ApplyError::TooLarge);}
        drift=s as isize-(h.start as isize+net);
        net+=h.new as isize-h.old as isize;
    }
    let mut text=String::new();for line in working {text.push_str(&line);text.push('\n');}Ok(text)
}
