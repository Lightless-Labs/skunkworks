#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Side {
    Old,
    New,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Keep {
        old_index: usize,
        new_index: usize,
        line: String,
    },
    Delete {
        old_index: usize,
        line: String,
    },
    Insert {
        new_index: usize,
        line: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffError {
    TooLarge {
        old_len: usize,
        new_len: usize,
    },
    EmbeddedNewline {
        side: Side,
        index: usize,
    },
}

pub fn diff_lines(old: &[String], new: &[String]) -> Result<Vec<Edit>, DiffError> {
    if old.len() > 128 || new.len() > 128 {
        return Err(DiffError::TooLarge {
            old_len: old.len(),
            new_len: new.len(),
        });
    }

    for (index, line) in old.iter().enumerate() {
        if line.contains('\n') || line.contains('\r') {
            return Err(DiffError::EmbeddedNewline {
                side: Side::Old,
                index,
            });
        }
    }

    for (index, line) in new.iter().enumerate() {
        if line.contains('\n') || line.contains('\r') {
            return Err(DiffError::EmbeddedNewline {
                side: Side::New,
                index,
            });
        }
    }

    let old_len = old.len();
    let new_len = new.len();
    let mut lcs = vec![vec![0usize; new_len + 1]; old_len + 1];

    for i in (0..old_len).rev() {
        for j in (0..new_len).rev() {
            lcs[i][j] = if old[i] == new[j] {
                1 + lcs[i + 1][j + 1]
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut edits = Vec::with_capacity(old_len + new_len);
    let (mut i, mut j) = (0, 0);

    while i < old_len || j < new_len {
        if i == old_len {
            edits.push(Edit::Insert {
                new_index: j,
                line: new[j].clone(),
            });
            j += 1;
        } else if j == new_len {
            edits.push(Edit::Delete {
                old_index: i,
                line: old[i].clone(),
            });
            i += 1;
        } else if old[i] == new[j] {
            edits.push(Edit::Keep {
                old_index: i,
                new_index: j,
                line: old[i].clone(),
            });
            i += 1;
            j += 1;
        } else {
            let delete_lcs = lcs[i + 1][j];
            let insert_lcs = lcs[i][j + 1];

            if delete_lcs > insert_lcs
                || (delete_lcs == insert_lcs && (i + j) % 2 == 0)
            {
                edits.push(Edit::Delete {
                    old_index: i,
                    line: old[i].clone(),
                });
                i += 1;
            } else {
                edits.push(Edit::Insert {
                    new_index: j,
                    line: new[j].clone(),
                });
                j += 1;
            }
        }
    }

    Ok(edits)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_lines_are_kept() {
        let old = vec!["cat".to_string(), "dog".to_string()];
        let new = old.clone();
        assert_eq!(
            diff_lines(&old, &new),
            Ok(vec![
                Edit::Keep {
                    old_index: 0,
                    new_index: 0,
                    line: "cat".to_string(),
                },
                Edit::Keep {
                    old_index: 1,
                    new_index: 1,
                    line: "dog".to_string(),
                },
            ])
        );
    }

    #[test]
    fn replacement_deletes_before_inserting_on_even_tie() {
        let old = vec!["one".to_string()];
        let new = vec!["two".to_string()];
        assert_eq!(
            diff_lines(&old, &new),
            Ok(vec![
                Edit::Delete {
                    old_index: 0,
                    line: "one".to_string(),
                },
                Edit::Insert {
                    new_index: 0,
                    line: "two".to_string(),
                },
            ])
        );
    }

    #[test]
    fn empty_old_inserts_each_line() {
        let old = vec![];
        let new = vec!["a".to_string(), "b".to_string()];
        assert_eq!(
            diff_lines(&old, &new),
            Ok(vec![
                Edit::Insert {
                    new_index: 0,
                    line: "a".to_string(),
                },
                Edit::Insert {
                    new_index: 1,
                    line: "b".to_string(),
                },
            ])
        );
    }

    #[test]
    fn too_large_precedes_line_validation() {
        let old = vec!["bad\nline".to_string(); 129];
        let new = vec![];
        assert_eq!(
            diff_lines(&old, &new),
            Err(DiffError::TooLarge {
                old_len: 129,
                new_len: 0,
            })
        );
    }

    #[test]
    fn validates_old_before_new() {
        let old = vec!["bad\rline".to_string()];
        let new = vec!["bad\nline".to_string()];
        assert_eq!(
            diff_lines(&old, &new),
            Err(DiffError::EmbeddedNewline {
                side: Side::Old,
                index: 0,
            })
        );
    }

    #[test]
    fn empty_string_is_a_valid_line() {
        let lines = vec!["".to_string()];
        assert_eq!(
            diff_lines(&lines, &lines),
            Ok(vec![Edit::Keep {
                old_index: 0,
                new_index: 0,
                line: "".to_string(),
            }])
        );
    }
}