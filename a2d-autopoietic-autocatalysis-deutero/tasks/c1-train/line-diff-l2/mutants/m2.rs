
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Side { Old, New }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Keep { old_index: usize, new_index: usize, line: String },
    Delete { old_index: usize, line: String },
    Insert { new_index: usize, line: String },
}
fn script(old: &[String], new: &[String]) -> Vec<Edit> {
    let (n,m)=(old.len(),new.len());
    let mut dp=vec![vec![0usize;m+1];n+1];
    for i in (0..n).rev() { for j in (0..m).rev() {
        dp[i][j]=if old[i]==new[j] {1+dp[i+1][j+1]} else {dp[i+1][j].max(dp[i][j+1])};
    }}
    let (mut i,mut j)=(0,0); let mut out=Vec::new();
    while i<n || j<m {
        if i<n && j<m && old[i]==new[j] {
            out.push(Edit::Keep{old_index:i,new_index:j,line:old[i].clone()}); i+=1;j+=1;
        } else if i<n && (j==m || dp[i+1][j]>dp[i][j+1] || (dp[i+1][j]==dp[i][j+1] && (i+j)%2==0)) {
            out.push(Edit::Delete{old_index:i,line:old[i].clone()}); i+=1;
        } else { out.push(Edit::Insert{new_index:j,line:new[j].clone()}); j+=1; }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffError {
    ContextTooLarge { context: usize },
    CarriageReturn { side: Side, byte: usize },
    MissingFinalLf { side: Side },
    TooLarge { old_len: usize, new_len: usize },
}
pub fn unified_diff(old: &str, new: &str, context: usize) -> Result<String, DiffError> {
    if context>8 {return Err(DiffError::ContextTooLarge{context});}
    for (side,text) in [(Side::Old,old),(Side::New,new)] {
        if let Some(byte)=text.find('\r') {return Err(DiffError::CarriageReturn{side,byte});}
    }
    for (side,text) in [(Side::Old,old),(Side::New,new)] {
        if !text.is_empty() && !text.ends_with('\n') {return Err(DiffError::MissingFinalLf{side});}
    }
    let a:Vec<String>=old.split_terminator('\n').map(str::to_owned).collect();
    let b:Vec<String>=new.split_terminator('\n').map(str::to_owned).collect();
    if a.len()>128 || b.len()>128 {return Err(DiffError::TooLarge{old_len:a.len(),new_len:b.len()});}
    let rows=script(&a,&b);
    let changed:Vec<usize>=rows.iter().enumerate().filter_map(|(i,e)|if matches!(e,Edit::Keep{..}){None}else{Some(i)}).collect();
    if changed.is_empty(){return Ok(String::new());}
    let mut groups=Vec::new(); let mut first=changed[0];let mut last=first;
    for &next in &changed[1..] {
        if next-last-1<2*context {last=next;} else {groups.push((first,last));first=next;last=next;}
    }
    groups.push((first,last));
    let mut out=String::from("--- before\n+++ after\n");
    for (first,last) in groups {
        let lo=first.saturating_sub(context); let hi=(last+1+context).min(rows.len());
        let mut op=0;let mut np=0;
        for e in &rows[..lo] {match e {Edit::Keep{..}=>{op+=1;np+=1},Edit::Delete{..}=>op+=1,Edit::Insert{..}=>np+=1}}
        let mut oc=0;let mut nc=0;
        for e in &rows[lo..hi] {match e {Edit::Keep{..}=>{oc+=1;nc+=1},Edit::Delete{..}=>oc+=1,Edit::Insert{..}=>nc+=1}}
        let os=if oc==0 {op} else {op+1}; let ns=if nc==0 {np} else {np+1};
        out.push_str(&format!("@@ -{os},{oc} +{ns},{nc} @@\n"));
        for e in &rows[lo..hi] {
            let (p,line)=match e {Edit::Keep{line,..}=>(' ',line),Edit::Delete{line,..}=>('-',line),Edit::Insert{line,..}=>('+',line)};
            out.push(p);out.push_str(line);out.push('\n');
        }
    }
    Ok(out)
}
