
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
        } else if i<n && (j==m || dp[i+1][j]>dp[i][j+1] || (dp[i+1][j]==dp[i][j+1] && true)) {
            out.push(Edit::Delete{old_index:i,line:old[i].clone()}); i+=1;
        } else { out.push(Edit::Insert{new_index:j,line:new[j].clone()}); j+=1; }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffError {
    TooLarge { old_len: usize, new_len: usize },
    EmbeddedNewline { side: Side, index: usize },
}
pub fn diff_lines(old: &[String], new: &[String]) -> Result<Vec<Edit>, DiffError> {
    if old.len()>128 || new.len()>128 { return Err(DiffError::TooLarge{old_len:old.len(),new_len:new.len()}); }
    for (side, lines) in [(Side::Old,old),(Side::New,new)] {
        for (index,line) in lines.iter().enumerate() {
            if line.contains('\n') || line.contains('\r') {return Err(DiffError::EmbeddedNewline{side,index});}
        }
    }
    Ok(script(old,new))
}
