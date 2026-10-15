// R1,R2,R4,R17
#[test]
fn empty_and_ascii_separators() {
    assert_eq!(split(""), Ok(vec![]));assert_eq!(split(" \t\r\n  "), Ok(vec![]));
}

// R4,R16
#[test]
fn unicode_whitespace_is_text() {
    assert_eq!(split("x y z"), Ok(vec!["x y z".to_string()]));
}

// R5,R6,R7,R17
#[test]
fn adjacent_fragments_merge() {
    assert_eq!(split("ab'cd'\"ef\"gh"), Ok(vec!["abcdefgh".to_string()]));
}

// R8
#[test]
fn empty_quote_activation() {
    assert_eq!(split("'' \"\" a''b ''\"\""), Ok(vec!["".to_string(), "".to_string(), "ab".to_string(), "".to_string()]));
}

// R6,R16
#[test]
fn single_quote_verbatim() {
    assert_eq!(split("'x\\q\n\"$Z`'"), Ok(vec!["x\\q\n\"$Z`".to_string()]));
}

// R7
#[test]
fn double_quote_separators() {
    assert_eq!(split("\"a\t b\rc\nd'e\""), Ok(vec!["a\t b\rc\nd'e".to_string()]));
}

// R9
#[test]
fn outside_escaped_scalars() {
    assert_eq!(split("x\\ y \\' \\\" \\\\ \\$Z"), Ok(vec!["x y".to_string(), "'".to_string(), "\"".to_string(), "\\".to_string(), "$Z".to_string()]));
}

// R8,R10
#[test]
fn continuation_does_not_create_words() {
    assert_eq!(split("\\\n \\\n"), Ok(vec![]));assert_eq!(split("aa\\\nbb"), Ok(vec!["aabb".to_string()]));assert_eq!(split("''\\\n"), Ok(vec!["".to_string()]));assert_eq!(split("\"\\\n\""), Ok(vec!["".to_string()]));
}

// R9,R10
#[test]
fn cr_is_not_continuation() {
    assert_eq!(split("a\\\r\nb"), Ok(vec!["a\r".to_string(), "b".to_string()]));
}

// R11
#[test]
fn double_unknown_escape_retained() {
    assert_eq!(split("\"a\\qb\\ c\\'d\""), Ok(vec!["a\\qb\\ c\\'d".to_string()]));
}

// R11
#[test]
fn double_known_escapes() {
    assert_eq!(split("\"a\\\"b\\\\c\\$Z\\\nd\""), Ok(vec!["a\"b\\c$Zd".to_string()]));
}

// R2,R12
#[test]
fn trailing_escape_after_multibyte() {
    assert_eq!(split("λ x\\"), Err(SplitError::TrailingEscape { pos: 4 }));
}

// R12,R14
#[test]
fn double_trailing_escape_precedence() {
    assert_eq!(split("xx \"a\\"), Err(SplitError::TrailingEscape { pos: 5 }));
}

// R6,R12,R13
#[test]
fn single_backslash_is_not_escape() {
    assert_eq!(split("zz 'a\\"), Err(SplitError::UnterminatedSingle { pos: 3 }));
}

// R2,R13
#[test]
fn quote_opening_byte_offset() {
    assert_eq!(split("é \"done\" z'bad"), Err(SplitError::UnterminatedSingle { pos: 11 }));
}

// R2,R13
#[test]
fn double_opening_byte_offset() {
    assert_eq!(split("λ 'ok' x\"tail"), Err(SplitError::UnterminatedDouble { pos: 9 }));
}

// R3,R14
#[test]
fn nul_preempts_earlier_quote() {
    assert_eq!(split("'bad\0"), Err(SplitError::Nul { pos: 4 }));
}

// R2,R3
#[test]
fn nul_first_byte_offset() {
    assert_eq!(split("é\0x\0"), Err(SplitError::Nul { pos: 2 }));
}

// R3
#[test]
fn nul_in_quoted_or_escaped_input() {
    assert_eq!(split("'a\0b'"), Err(SplitError::Nul { pos: 2 }));assert_eq!(split("\\\0"), Err(SplitError::Nul { pos: 1 }));
}

// R15
#[test]
fn ordinary_shell_punctuation() {
    assert_eq!(split("#a;|&=~(z)"), Ok(vec!["#a;|&=~(z)".to_string()]));
}

// R5,R13
#[test]
fn closed_fragments_before_unclosed() {
    assert_eq!(split("x''y\"z\"'t"), Err(SplitError::UnterminatedSingle { pos: 7 }));
}

// R4,R5,R17
#[test]
fn large_deterministic_sequence() {
    assert_eq!(split("'v0' 'v1' 'v2' 'v3' 'v4' 'v5' 'v6' 'v7' 'v8' 'v9' 'v10' 'v11' 'v12' 'v13' 'v14' 'v15' 'v16' 'v17' 'v18' 'v19' 'v20' 'v21' 'v22' 'v23' 'v24' 'v25' 'v26' 'v27' 'v28' 'v29' 'v30' 'v31' 'v32' 'v33' 'v34' 'v35' 'v36' 'v37' 'v38' 'v39' 'v40' 'v41' 'v42' 'v43' 'v44' 'v45' 'v46' 'v47' 'v48' 'v49' 'v50' 'v51' 'v52' 'v53' 'v54' 'v55' 'v56' 'v57' 'v58' 'v59' 'v60' 'v61' 'v62' 'v63' 'v64' 'v65' 'v66' 'v67' 'v68' 'v69' 'v70' 'v71' 'v72' 'v73' 'v74' 'v75' 'v76' 'v77' 'v78' 'v79' 'v80' 'v81' 'v82' 'v83' 'v84' 'v85' 'v86' 'v87' 'v88' 'v89' 'v90' 'v91' 'v92' 'v93' 'v94' 'v95' 'v96' 'v97' 'v98' 'v99' 'v100' 'v101' 'v102' 'v103' 'v104' 'v105' 'v106' 'v107' 'v108' 'v109' 'v110' 'v111' 'v112' 'v113' 'v114' 'v115' 'v116' 'v117' 'v118' 'v119' 'v120' 'v121' 'v122' 'v123' 'v124' 'v125' 'v126' 'v127' 'v128' 'v129' 'v130' 'v131' 'v132' 'v133' 'v134' 'v135' 'v136' 'v137' 'v138' 'v139' 'v140' 'v141' 'v142' 'v143' 'v144' 'v145' 'v146' 'v147' 'v148' 'v149'"), Ok(vec!["v0".to_string(), "v1".to_string(), "v2".to_string(), "v3".to_string(), "v4".to_string(), "v5".to_string(), "v6".to_string(), "v7".to_string(), "v8".to_string(), "v9".to_string(), "v10".to_string(), "v11".to_string(), "v12".to_string(), "v13".to_string(), "v14".to_string(), "v15".to_string(), "v16".to_string(), "v17".to_string(), "v18".to_string(), "v19".to_string(), "v20".to_string(), "v21".to_string(), "v22".to_string(), "v23".to_string(), "v24".to_string(), "v25".to_string(), "v26".to_string(), "v27".to_string(), "v28".to_string(), "v29".to_string(), "v30".to_string(), "v31".to_string(), "v32".to_string(), "v33".to_string(), "v34".to_string(), "v35".to_string(), "v36".to_string(), "v37".to_string(), "v38".to_string(), "v39".to_string(), "v40".to_string(), "v41".to_string(), "v42".to_string(), "v43".to_string(), "v44".to_string(), "v45".to_string(), "v46".to_string(), "v47".to_string(), "v48".to_string(), "v49".to_string(), "v50".to_string(), "v51".to_string(), "v52".to_string(), "v53".to_string(), "v54".to_string(), "v55".to_string(), "v56".to_string(), "v57".to_string(), "v58".to_string(), "v59".to_string(), "v60".to_string(), "v61".to_string(), "v62".to_string(), "v63".to_string(), "v64".to_string(), "v65".to_string(), "v66".to_string(), "v67".to_string(), "v68".to_string(), "v69".to_string(), "v70".to_string(), "v71".to_string(), "v72".to_string(), "v73".to_string(), "v74".to_string(), "v75".to_string(), "v76".to_string(), "v77".to_string(), "v78".to_string(), "v79".to_string(), "v80".to_string(), "v81".to_string(), "v82".to_string(), "v83".to_string(), "v84".to_string(), "v85".to_string(), "v86".to_string(), "v87".to_string(), "v88".to_string(), "v89".to_string(), "v90".to_string(), "v91".to_string(), "v92".to_string(), "v93".to_string(), "v94".to_string(), "v95".to_string(), "v96".to_string(), "v97".to_string(), "v98".to_string(), "v99".to_string(), "v100".to_string(), "v101".to_string(), "v102".to_string(), "v103".to_string(), "v104".to_string(), "v105".to_string(), "v106".to_string(), "v107".to_string(), "v108".to_string(), "v109".to_string(), "v110".to_string(), "v111".to_string(), "v112".to_string(), "v113".to_string(), "v114".to_string(), "v115".to_string(), "v116".to_string(), "v117".to_string(), "v118".to_string(), "v119".to_string(), "v120".to_string(), "v121".to_string(), "v122".to_string(), "v123".to_string(), "v124".to_string(), "v125".to_string(), "v126".to_string(), "v127".to_string(), "v128".to_string(), "v129".to_string(), "v130".to_string(), "v131".to_string(), "v132".to_string(), "v133".to_string(), "v134".to_string(), "v135".to_string(), "v136".to_string(), "v137".to_string(), "v138".to_string(), "v139".to_string(), "v140".to_string(), "v141".to_string(), "v142".to_string(), "v143".to_string(), "v144".to_string(), "v145".to_string(), "v146".to_string(), "v147".to_string(), "v148".to_string(), "v149".to_string()]));
}

// R18
#[test]
fn dollars_backticks_remain_literal() {
    assert_eq!(split("$A ${A:-d} `x` $(y)"), Ok(vec!["$A".to_string(), "${A:-d}".to_string(), "`x`".to_string(), "$(y)".to_string()]));
}

// R19
#[test]
fn wildcards_preserved() {
    assert_eq!(split("* a? [bc] \\* \"?\""), Ok(vec!["*".to_string(), "a?".to_string(), "[bc]".to_string(), "*".to_string(), "?".to_string()]));
}
