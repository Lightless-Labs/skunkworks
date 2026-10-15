// R1,R2,R4,R17
#[test]
fn empty_and_ascii_separators() {
    assert_eq!(split("", &[]), Ok(vec![]));assert_eq!(split(" \t\r\n  ", &[]), Ok(vec![]));
}

// R4,R16
#[test]
fn unicode_whitespace_is_text() {
    assert_eq!(split("x y z", &[]), Ok(vec![Word { text: "x y z".to_string(), glob: vec![] }]));
}

// R5,R6,R7,R17
#[test]
fn adjacent_fragments_merge() {
    assert_eq!(split("ab'cd'\"ef\"gh", &[]), Ok(vec![Word { text: "abcdefgh".to_string(), glob: vec![] }]));
}

// R8
#[test]
fn empty_quote_activation() {
    assert_eq!(split("'' \"\" a''b ''\"\"", &[]), Ok(vec![Word { text: "".to_string(), glob: vec![] }, Word { text: "".to_string(), glob: vec![] }, Word { text: "ab".to_string(), glob: vec![] }, Word { text: "".to_string(), glob: vec![] }]));
}

// R6,R16
#[test]
fn single_quote_verbatim() {
    assert_eq!(split("'x\\q\n\"$Z`'", &[]), Ok(vec![Word { text: "x\\q\n\"$Z`".to_string(), glob: vec![] }]));
}

// R7
#[test]
fn double_quote_separators() {
    assert_eq!(split("\"a\t b\rc\nd'e\"", &[]), Ok(vec![Word { text: "a\t b\rc\nd'e".to_string(), glob: vec![] }]));
}

// R9
#[test]
fn outside_escaped_scalars() {
    assert_eq!(split("x\\ y \\' \\\" \\\\ \\$Z", &[]), Ok(vec![Word { text: "x y".to_string(), glob: vec![] }, Word { text: "'".to_string(), glob: vec![] }, Word { text: "\"".to_string(), glob: vec![] }, Word { text: "\\".to_string(), glob: vec![] }, Word { text: "$Z".to_string(), glob: vec![] }]));
}

// R8,R10
#[test]
fn continuation_does_not_create_words() {
    assert_eq!(split("\\\n \\\n", &[]), Ok(vec![]));assert_eq!(split("aa\\\nbb", &[]), Ok(vec![Word { text: "aabb".to_string(), glob: vec![] }]));assert_eq!(split("''\\\n", &[]), Ok(vec![Word { text: "".to_string(), glob: vec![] }]));assert_eq!(split("\"\\\n\"", &[]), Ok(vec![Word { text: "".to_string(), glob: vec![] }]));
}

// R9,R10
#[test]
fn cr_is_not_continuation() {
    assert_eq!(split("a\\\r\nb", &[]), Ok(vec![Word { text: "a\r".to_string(), glob: vec![] }, Word { text: "b".to_string(), glob: vec![] }]));
}

// R11
#[test]
fn double_unknown_escape_retained() {
    assert_eq!(split("\"a\\qb\\ c\\'d\"", &[]), Ok(vec![Word { text: "a\\qb\\ c\\'d".to_string(), glob: vec![] }]));
}

// R11
#[test]
fn double_known_escapes() {
    assert_eq!(split("\"a\\\"b\\\\c\\$Z\\\nd\"", &[]), Ok(vec![Word { text: "a\"b\\c$Zd".to_string(), glob: vec![] }]));
}

// R2,R12
#[test]
fn trailing_escape_after_multibyte() {
    assert_eq!(split("λ x\\", &[]), Err(SplitError::TrailingEscape { pos: 4 }));
}

// R12,R14
#[test]
fn double_trailing_escape_precedence() {
    assert_eq!(split("xx \"a\\", &[]), Err(SplitError::TrailingEscape { pos: 5 }));
}

// R6,R12,R13
#[test]
fn single_backslash_is_not_escape() {
    assert_eq!(split("zz 'a\\", &[]), Err(SplitError::UnterminatedSingle { pos: 3 }));
}

// R2,R13
#[test]
fn quote_opening_byte_offset() {
    assert_eq!(split("é \"done\" z'bad", &[]), Err(SplitError::UnterminatedSingle { pos: 11 }));
}

// R2,R13
#[test]
fn double_opening_byte_offset() {
    assert_eq!(split("λ 'ok' x\"tail", &[]), Err(SplitError::UnterminatedDouble { pos: 9 }));
}

// R3,R14
#[test]
fn nul_preempts_earlier_quote() {
    assert_eq!(split("'bad\0", &[]), Err(SplitError::Nul { pos: 4 }));
}

// R2,R3
#[test]
fn nul_first_byte_offset() {
    assert_eq!(split("é\0x\0", &[]), Err(SplitError::Nul { pos: 2 }));
}

// R3
#[test]
fn nul_in_quoted_or_escaped_input() {
    assert_eq!(split("'a\0b'", &[]), Err(SplitError::Nul { pos: 2 }));assert_eq!(split("\\\0", &[]), Err(SplitError::Nul { pos: 1 }));
}

// R15
#[test]
fn ordinary_shell_punctuation() {
    assert_eq!(split("#a;|&=~(z)", &[]), Ok(vec![Word { text: "#a;|&=~(z)".to_string(), glob: vec![] }]));
}

// R5,R13
#[test]
fn closed_fragments_before_unclosed() {
    assert_eq!(split("x''y\"z\"'t", &[]), Err(SplitError::UnterminatedSingle { pos: 7 }));
}

// R4,R5,R17
#[test]
fn large_deterministic_sequence() {
    assert_eq!(split("'v0' 'v1' 'v2' 'v3' 'v4' 'v5' 'v6' 'v7' 'v8' 'v9' 'v10' 'v11' 'v12' 'v13' 'v14' 'v15' 'v16' 'v17' 'v18' 'v19' 'v20' 'v21' 'v22' 'v23' 'v24' 'v25' 'v26' 'v27' 'v28' 'v29' 'v30' 'v31' 'v32' 'v33' 'v34' 'v35' 'v36' 'v37' 'v38' 'v39' 'v40' 'v41' 'v42' 'v43' 'v44' 'v45' 'v46' 'v47' 'v48' 'v49' 'v50' 'v51' 'v52' 'v53' 'v54' 'v55' 'v56' 'v57' 'v58' 'v59' 'v60' 'v61' 'v62' 'v63' 'v64' 'v65' 'v66' 'v67' 'v68' 'v69' 'v70' 'v71' 'v72' 'v73' 'v74' 'v75' 'v76' 'v77' 'v78' 'v79' 'v80' 'v81' 'v82' 'v83' 'v84' 'v85' 'v86' 'v87' 'v88' 'v89' 'v90' 'v91' 'v92' 'v93' 'v94' 'v95' 'v96' 'v97' 'v98' 'v99' 'v100' 'v101' 'v102' 'v103' 'v104' 'v105' 'v106' 'v107' 'v108' 'v109' 'v110' 'v111' 'v112' 'v113' 'v114' 'v115' 'v116' 'v117' 'v118' 'v119' 'v120' 'v121' 'v122' 'v123' 'v124' 'v125' 'v126' 'v127' 'v128' 'v129' 'v130' 'v131' 'v132' 'v133' 'v134' 'v135' 'v136' 'v137' 'v138' 'v139' 'v140' 'v141' 'v142' 'v143' 'v144' 'v145' 'v146' 'v147' 'v148' 'v149'", &[]), Ok(vec![Word { text: "v0".to_string(), glob: vec![] }, Word { text: "v1".to_string(), glob: vec![] }, Word { text: "v2".to_string(), glob: vec![] }, Word { text: "v3".to_string(), glob: vec![] }, Word { text: "v4".to_string(), glob: vec![] }, Word { text: "v5".to_string(), glob: vec![] }, Word { text: "v6".to_string(), glob: vec![] }, Word { text: "v7".to_string(), glob: vec![] }, Word { text: "v8".to_string(), glob: vec![] }, Word { text: "v9".to_string(), glob: vec![] }, Word { text: "v10".to_string(), glob: vec![] }, Word { text: "v11".to_string(), glob: vec![] }, Word { text: "v12".to_string(), glob: vec![] }, Word { text: "v13".to_string(), glob: vec![] }, Word { text: "v14".to_string(), glob: vec![] }, Word { text: "v15".to_string(), glob: vec![] }, Word { text: "v16".to_string(), glob: vec![] }, Word { text: "v17".to_string(), glob: vec![] }, Word { text: "v18".to_string(), glob: vec![] }, Word { text: "v19".to_string(), glob: vec![] }, Word { text: "v20".to_string(), glob: vec![] }, Word { text: "v21".to_string(), glob: vec![] }, Word { text: "v22".to_string(), glob: vec![] }, Word { text: "v23".to_string(), glob: vec![] }, Word { text: "v24".to_string(), glob: vec![] }, Word { text: "v25".to_string(), glob: vec![] }, Word { text: "v26".to_string(), glob: vec![] }, Word { text: "v27".to_string(), glob: vec![] }, Word { text: "v28".to_string(), glob: vec![] }, Word { text: "v29".to_string(), glob: vec![] }, Word { text: "v30".to_string(), glob: vec![] }, Word { text: "v31".to_string(), glob: vec![] }, Word { text: "v32".to_string(), glob: vec![] }, Word { text: "v33".to_string(), glob: vec![] }, Word { text: "v34".to_string(), glob: vec![] }, Word { text: "v35".to_string(), glob: vec![] }, Word { text: "v36".to_string(), glob: vec![] }, Word { text: "v37".to_string(), glob: vec![] }, Word { text: "v38".to_string(), glob: vec![] }, Word { text: "v39".to_string(), glob: vec![] }, Word { text: "v40".to_string(), glob: vec![] }, Word { text: "v41".to_string(), glob: vec![] }, Word { text: "v42".to_string(), glob: vec![] }, Word { text: "v43".to_string(), glob: vec![] }, Word { text: "v44".to_string(), glob: vec![] }, Word { text: "v45".to_string(), glob: vec![] }, Word { text: "v46".to_string(), glob: vec![] }, Word { text: "v47".to_string(), glob: vec![] }, Word { text: "v48".to_string(), glob: vec![] }, Word { text: "v49".to_string(), glob: vec![] }, Word { text: "v50".to_string(), glob: vec![] }, Word { text: "v51".to_string(), glob: vec![] }, Word { text: "v52".to_string(), glob: vec![] }, Word { text: "v53".to_string(), glob: vec![] }, Word { text: "v54".to_string(), glob: vec![] }, Word { text: "v55".to_string(), glob: vec![] }, Word { text: "v56".to_string(), glob: vec![] }, Word { text: "v57".to_string(), glob: vec![] }, Word { text: "v58".to_string(), glob: vec![] }, Word { text: "v59".to_string(), glob: vec![] }, Word { text: "v60".to_string(), glob: vec![] }, Word { text: "v61".to_string(), glob: vec![] }, Word { text: "v62".to_string(), glob: vec![] }, Word { text: "v63".to_string(), glob: vec![] }, Word { text: "v64".to_string(), glob: vec![] }, Word { text: "v65".to_string(), glob: vec![] }, Word { text: "v66".to_string(), glob: vec![] }, Word { text: "v67".to_string(), glob: vec![] }, Word { text: "v68".to_string(), glob: vec![] }, Word { text: "v69".to_string(), glob: vec![] }, Word { text: "v70".to_string(), glob: vec![] }, Word { text: "v71".to_string(), glob: vec![] }, Word { text: "v72".to_string(), glob: vec![] }, Word { text: "v73".to_string(), glob: vec![] }, Word { text: "v74".to_string(), glob: vec![] }, Word { text: "v75".to_string(), glob: vec![] }, Word { text: "v76".to_string(), glob: vec![] }, Word { text: "v77".to_string(), glob: vec![] }, Word { text: "v78".to_string(), glob: vec![] }, Word { text: "v79".to_string(), glob: vec![] }, Word { text: "v80".to_string(), glob: vec![] }, Word { text: "v81".to_string(), glob: vec![] }, Word { text: "v82".to_string(), glob: vec![] }, Word { text: "v83".to_string(), glob: vec![] }, Word { text: "v84".to_string(), glob: vec![] }, Word { text: "v85".to_string(), glob: vec![] }, Word { text: "v86".to_string(), glob: vec![] }, Word { text: "v87".to_string(), glob: vec![] }, Word { text: "v88".to_string(), glob: vec![] }, Word { text: "v89".to_string(), glob: vec![] }, Word { text: "v90".to_string(), glob: vec![] }, Word { text: "v91".to_string(), glob: vec![] }, Word { text: "v92".to_string(), glob: vec![] }, Word { text: "v93".to_string(), glob: vec![] }, Word { text: "v94".to_string(), glob: vec![] }, Word { text: "v95".to_string(), glob: vec![] }, Word { text: "v96".to_string(), glob: vec![] }, Word { text: "v97".to_string(), glob: vec![] }, Word { text: "v98".to_string(), glob: vec![] }, Word { text: "v99".to_string(), glob: vec![] }, Word { text: "v100".to_string(), glob: vec![] }, Word { text: "v101".to_string(), glob: vec![] }, Word { text: "v102".to_string(), glob: vec![] }, Word { text: "v103".to_string(), glob: vec![] }, Word { text: "v104".to_string(), glob: vec![] }, Word { text: "v105".to_string(), glob: vec![] }, Word { text: "v106".to_string(), glob: vec![] }, Word { text: "v107".to_string(), glob: vec![] }, Word { text: "v108".to_string(), glob: vec![] }, Word { text: "v109".to_string(), glob: vec![] }, Word { text: "v110".to_string(), glob: vec![] }, Word { text: "v111".to_string(), glob: vec![] }, Word { text: "v112".to_string(), glob: vec![] }, Word { text: "v113".to_string(), glob: vec![] }, Word { text: "v114".to_string(), glob: vec![] }, Word { text: "v115".to_string(), glob: vec![] }, Word { text: "v116".to_string(), glob: vec![] }, Word { text: "v117".to_string(), glob: vec![] }, Word { text: "v118".to_string(), glob: vec![] }, Word { text: "v119".to_string(), glob: vec![] }, Word { text: "v120".to_string(), glob: vec![] }, Word { text: "v121".to_string(), glob: vec![] }, Word { text: "v122".to_string(), glob: vec![] }, Word { text: "v123".to_string(), glob: vec![] }, Word { text: "v124".to_string(), glob: vec![] }, Word { text: "v125".to_string(), glob: vec![] }, Word { text: "v126".to_string(), glob: vec![] }, Word { text: "v127".to_string(), glob: vec![] }, Word { text: "v128".to_string(), glob: vec![] }, Word { text: "v129".to_string(), glob: vec![] }, Word { text: "v130".to_string(), glob: vec![] }, Word { text: "v131".to_string(), glob: vec![] }, Word { text: "v132".to_string(), glob: vec![] }, Word { text: "v133".to_string(), glob: vec![] }, Word { text: "v134".to_string(), glob: vec![] }, Word { text: "v135".to_string(), glob: vec![] }, Word { text: "v136".to_string(), glob: vec![] }, Word { text: "v137".to_string(), glob: vec![] }, Word { text: "v138".to_string(), glob: vec![] }, Word { text: "v139".to_string(), glob: vec![] }, Word { text: "v140".to_string(), glob: vec![] }, Word { text: "v141".to_string(), glob: vec![] }, Word { text: "v142".to_string(), glob: vec![] }, Word { text: "v143".to_string(), glob: vec![] }, Word { text: "v144".to_string(), glob: vec![] }, Word { text: "v145".to_string(), glob: vec![] }, Word { text: "v146".to_string(), glob: vec![] }, Word { text: "v147".to_string(), glob: vec![] }, Word { text: "v148".to_string(), glob: vec![] }, Word { text: "v149".to_string(), glob: vec![] }]));
}

// R18,R19,R20,R21,R22
#[test]
fn longest_ascii_names_and_literal_dollar() {
    assert_eq!(split("$_A9/$Ab-$$Ab-$9-$é", &[("_A9", "one"), ("Ab", "two")]), Ok(vec![Word { text: "one/two-$two-$9-$é".to_string(), glob: vec![] }]));assert_eq!(split("${Ab}:$_:$$", &[("Ab", "two"), ("_", "under"), ("", "unused")]), Ok(vec![Word { text: "two:under:$$".to_string(), glob: vec![] }]));
}

// R22,R25
#[test]
fn environment_first_duplicate_and_case() {
    assert_eq!(split("$A:$a:${A:-fallback}", &[("A", ""), ("A", "later"), ("a", "low")]), Ok(vec![Word { text: ":low:fallback".to_string(), glob: vec![] }]));
}

// R8,R20,R22
#[test]
fn empty_expansion_activates_word() {
    assert_eq!(split("$M ${M} ${E} x$M", &[("E", "")]), Ok(vec![Word { text: "".to_string(), glob: vec![] }, Word { text: "".to_string(), glob: vec![] }, Word { text: "".to_string(), glob: vec![] }, Word { text: "x".to_string(), glob: vec![] }]));
}

// R23,R25,R26,R27
#[test]
fn default_decode_and_literal_syntax() {
    assert_eq!(split("${M:-'a b'\\}$X\\\n}z", &[]), Ok(vec![Word { text: "'a b'}$X\nz".to_string(), glob: vec![] }]));assert_eq!(split("${Q:-}z ${Q:-\\\\q}", &[]), Ok(vec![Word { text: "z".to_string(), glob: vec![] }, Word { text: "\\q".to_string(), glob: vec![] }]));assert_eq!(split("\"${Q:-x y}\"", &[]), Ok(vec![Word { text: "x y".to_string(), glob: vec![] }]));
}

// R23,R26,R27
#[test]
fn default_no_nesting_and_first_close() {
    assert_eq!(split("${M:-a${N}z}", &[]), Ok(vec![Word { text: "a${Nz}".to_string(), glob: vec![] }]));
}

// R20,R22,R25,R27
#[test]
fn unselected_default_and_atomic_values() {
    assert_eq!(split("${A:-ignored\\}} $B", &[("A", "yes"), ("B", "x $A \"q\" \\")]), Ok(vec![Word { text: "yes".to_string(), glob: vec![] }, Word { text: "x $A \"q\" \\".to_string(), glob: vec![] }]));assert_eq!(split("${A:-missing", &[("A", "set")]), Err(SplitError::UnterminatedExpansion { pos: 0 }));
}

// R14,R23,R24,R28,R29
#[test]
fn braced_errors_and_precedence() {
    assert_eq!(split("é ${9X", &[]), Err(SplitError::UnterminatedExpansion { pos: 3 }));
    assert_eq!(split("é ${9X}", &[]), Err(SplitError::InvalidExpansion { pos: 3 }));
    assert_eq!(split("\"${A?x}", &[]), Err(SplitError::InvalidExpansion { pos: 1 }));
    assert_eq!(split("${A\\}", &[]), Err(SplitError::UnterminatedExpansion { pos: 0 }));
    assert_eq!(split("${A\\}x}", &[]), Err(SplitError::InvalidExpansion { pos: 0 }));
    assert_eq!(split("${}", &[]), Err(SplitError::InvalidExpansion { pos: 0 }));
    assert_eq!(split("${é}", &[]), Err(SplitError::InvalidExpansion { pos: 0 }));
    assert_eq!(split("${A:+x}", &[]), Err(SplitError::InvalidExpansion { pos: 0 }));
    assert_eq!(split("${A x}", &[]), Err(SplitError::InvalidExpansion { pos: 0 }));
    assert_eq!(split("${A", &[]), Err(SplitError::UnterminatedExpansion { pos: 0 }));
    assert_eq!(split("${A:-x\\}", &[]), Err(SplitError::UnterminatedExpansion { pos: 0 }));
    assert_eq!(split("$A 'bad", &[("A", "long value")]), Err(SplitError::UnterminatedSingle { pos: 3 }));
}

// R3,R6,R20,R27,R28
#[test]
fn expansion_quoted_protection_and_nul_value() {
    assert_eq!(split("'${bad' \\${bad \"$N\"", &[("N", "x\0y")]), Ok(vec![Word { text: "${bad".to_string(), glob: vec![] }, Word { text: "${bad".to_string(), glob: vec![] }, Word { text: "x\0y".to_string(), glob: vec![] }]));
}

// R19,R30,R31,R32
#[test]
fn wildcard_origins_and_decoded_offsets() {
    assert_eq!(split("é*\"?\"\\[?]", &[]), Ok(vec![Word { text: "é*?[?]".to_string(), glob: vec![2, 5] }]));assert_eq!(split("''*\\\n?[", &[]), Ok(vec![Word { text: "*?[".to_string(), glob: vec![0, 1, 2] }]));assert_eq!(split("'*?['\"\\*?\"*", &[]), Ok(vec![Word { text: "*?[\\*?*".to_string(), glob: vec![6] }]));
}

// R20,R27,R30,R33
#[test]
fn expanded_wildcard_provenance() {
    assert_eq!(split("z$A \"x$A\" ${M:-\\*?[} \"${M:-*}\"", &[("A", "é*?[")]), Ok(vec![Word { text: "zé*?[".to_string(), glob: vec![3, 4, 5] }, Word { text: "xé*?[".to_string(), glob: vec![] }, Word { text: "*?[".to_string(), glob: vec![0, 1, 2] }, Word { text: "*".to_string(), glob: vec![] }]));
}

// R14,R34,R35,R36
#[test]
fn command_rejections_and_precedence() {
    assert_eq!(split("a $(x)", &[]), Err(SplitError::CommandSubstitution { pos: 2 }));
    assert_eq!(split("é $((1))", &[]), Err(SplitError::CommandSubstitution { pos: 3 }));
    assert_eq!(split("\"$(q)\"", &[]), Err(SplitError::CommandSubstitution { pos: 1 }));
    assert_eq!(split("\"x`q\"", &[]), Err(SplitError::CommandSubstitution { pos: 2 }));
    assert_eq!(split("\"a\\`b\"", &[]), Err(SplitError::CommandSubstitution { pos: 3 }));
    assert_eq!(split("`x\0", &[]), Err(SplitError::Nul { pos: 2 }));
    assert_eq!(split("${!} $(x)", &[]), Err(SplitError::InvalidExpansion { pos: 0 }));
    assert_eq!(split("$(x) \"", &[]), Err(SplitError::CommandSubstitution { pos: 0 }));
    assert_eq!(split("'$(x)`' \\$(y) \\` \"\\$(z)\" ${M:-`$(a)} $C", &[("C", "$(v)`")]), Ok(vec![Word { text: "$(x)`".to_string(), glob: vec![] }, Word { text: "$(y)".to_string(), glob: vec![] }, Word { text: "`".to_string(), glob: vec![] }, Word { text: "$(z)".to_string(), glob: vec![] }, Word { text: "`$(a)".to_string(), glob: vec![] }, Word { text: "$(v)`".to_string(), glob: vec![] }]));
}

// R37,R38,R39,R40,R41,R42,R43,R44,R45
#[test]
fn join_exact_candidates_and_ties() {
    assert_eq!(join(&vec![]), Ok("".to_string()));
    assert_eq!(join(&vec!["".to_string()]), Ok("''".to_string()));
    assert_eq!(join(&vec!["abc_-/.:=,+@%09".to_string()]), Ok("abc_-/.:=,+@%09".to_string()));
    assert_eq!(join(&vec!["a b".to_string()]), Ok("a\\ b".to_string()));
    assert_eq!(join(&vec!["a b c".to_string()]), Ok("a\\ b\\ c".to_string()));
    assert_eq!(join(&vec!["a b c d".to_string()]), Ok("'a b c d'".to_string()));
    assert_eq!(join(&vec!["é".to_string()]), Ok("\\é".to_string()));
    assert_eq!(join(&vec!["éé".to_string()]), Ok("\\é\\é".to_string()));
    assert_eq!(join(&vec!["ééé".to_string()]), Ok("'ééé'".to_string()));
    assert_eq!(join(&vec!["'".to_string()]), Ok("\\'".to_string()));
    assert_eq!(join(&vec!["a'b'c'd".to_string()]), Ok("\"a'b'c'd\"".to_string()));
    assert_eq!(join(&vec!["\"$\\".to_string()]), Ok("'\"$\\'".to_string()));
    assert_eq!(join(&vec!["\n".to_string()]), Ok("'\n'".to_string()));
    assert_eq!(join(&vec!["\r".to_string()]), Ok("\\\r".to_string()));
    assert_eq!(join(&vec!["a".to_string(), "".to_string(), "b".to_string()]), Ok("a '' b".to_string()));
    assert_eq!(join(&vec!["*?[]".to_string()]), Ok("'*?[]'".to_string()));
    assert_eq!(join(&vec!["`".to_string()]), Ok("\\`".to_string()));
    assert_eq!(join(&vec!["a` b` c".to_string()]), Ok("'a` b` c'".to_string()));
    assert_eq!(join(&vec!["\n'".to_string()]), Ok("\"\n'\"".to_string()));
    assert_eq!(join(&vec!["\n'`".to_string()]), Ok("'\n'\\''`'".to_string()));
}

// R37,R38,R44,R45,R46
#[test]
fn join_nul_and_round_trip() {
    assert_eq!(join(&vec!["ok".into(), "é\0x\0".into(), "\0".into()]), Err(JoinError::Nul { word: 1, pos: 2 }));
    let alphabet = ["", "a", " ", "'", "\"", "\\", "`", "$", "*?[", "é", "\n", "\r", "\t"];
    let mut words = Vec::new();
    for a in alphabet { for b in alphabet { for c in alphabet { words.push(format!("{a}{b}{c}")); } } }
    let line = join(&words).unwrap();
    let decoded = split(&line, &[("a", "bad*"), ("_", "wrong")]).unwrap();
    assert_eq!(decoded.len(), words.len());
    for (got,want) in decoded.iter().zip(&words) { assert_eq!(&got.text, want); assert!(got.glob.is_empty()); }
}
