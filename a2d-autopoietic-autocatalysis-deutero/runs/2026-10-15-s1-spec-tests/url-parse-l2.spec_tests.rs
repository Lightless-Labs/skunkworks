#[test]
fn spec_r1_api_shapes() {
    let _: Result<Url, ParseError> = parse_url("x://h");
    let _: Result<String, CodecError> = decode_component(Component::Path, "");
    let _: String = encode_component(Component::Path, "");
}

#[test]
fn spec_r2_absolute_url() {
    let u = parse_url("x://h/p").unwrap();
    assert_eq!(u.scheme, "x");
    assert_eq!(u.host, "h");
    assert_eq!(u.path, "/p");
}

#[test]
fn spec_r3_error_validation_order() {
    assert_eq!(parse_url("1://[bad]@"), Err(ParseError::InvalidScheme { at: 0 }));
    assert_eq!(parse_url("x:/@"), Err(ParseError::MissingAuthority { at: 2 }));
    assert_eq!(parse_url("x://@"), Err(ParseError::InvalidUserInfo { at: 4 }));
    assert_eq!(parse_url("x://!"), Err(ParseError::InvalidHost { at: 4 }));
}

#[test]
fn spec_r4_empty_and_global_byte_scan() {
    assert_eq!(parse_url(""), Err(ParseError::Empty));
    assert_eq!(parse_url("x://h/a b"), Err(ParseError::InvalidByte { at: 7 }));
    assert_eq!(parse_url("x://é"), Err(ParseError::InvalidByte { at: 4 }));
}

#[test]
fn spec_r5_scheme_rules_and_lowercase() {
    assert_eq!(parse_url("x+Y.2://h").unwrap().scheme, "x+y.2");
    assert_eq!(parse_url("://h"), Err(ParseError::InvalidScheme { at: 0 }));
    assert_eq!(parse_url("1x://h"), Err(ParseError::InvalidScheme { at: 0 }));
    assert_eq!(parse_url("ab^://h"), Err(ParseError::InvalidScheme { at: 2 }));
    assert_eq!(parse_url("abc"), Err(ParseError::InvalidScheme { at: 3 }));
}

#[test]
fn spec_r6_authority_marker_and_extra_slashes() {
    assert_eq!(parse_url("x:/h"), Err(ParseError::MissingAuthority { at: 2 }));
    assert_eq!(parse_url("x://").unwrap().host, "");
    assert_eq!(parse_url("x:///p").unwrap().path, "/p");
    assert_eq!(parse_url("x:////p").unwrap().host, "");
}

#[test]
fn spec_r7_authority_stops_at_component_delimiters() {
    let u = parse_url("x://host/path?query#fragment").unwrap();
    assert_eq!(u.host, "host");
    assert_eq!(u.path, "/path");
    assert_eq!(u.query.as_deref(), Some("query"));
    assert_eq!(u.fragment.as_deref(), Some("fragment"));
}

#[test]
fn spec_r8_userinfo_structure_and_verbatim_value() {
    let u = parse_url("x://A:B:C@Host").unwrap();
    assert_eq!(u.userinfo.as_deref(), Some("A:B:C"));
    assert_eq!(parse_url("x://a@b@c"), Err(ParseError::InvalidUserInfo { at: 7 }));
    assert_eq!(parse_url("x://@h"), Err(ParseError::InvalidUserInfo { at: 4 }));
    assert_eq!(parse_url("x://h").unwrap().userinfo, None);
}

#[test]
fn spec_r9_ordinary_host_characters_and_case() {
    let u = parse_url("x://.-_A..-").unwrap();
    assert_eq!(u.host, ".-_a..-");
    assert!(!u.ipv6);
    assert_eq!(parse_url("x://ho!st"), Err(ParseError::InvalidHost { at: 6 }));
    assert_eq!(parse_url("x://h/").unwrap().host, "h");
    assert_eq!(parse_url("x://").unwrap_err(), ParseError::InvalidHost { at: 4 });
}

#[test]
fn spec_r10_port_presence_and_empty_port() {
    assert_eq!(parse_url("x://h").unwrap().port, None);
    assert_eq!(parse_url("x://h:").unwrap(), Err(ParseError::InvalidPort { at: 6 }));
}

#[test]
fn spec_r11_port_validation_order_and_bounds() {
    assert_eq!(parse_url("x://h:0"), Err(ParseError::InvalidPort { at: 6 }));
    assert_eq!(parse_url("x://h:01"), Err(ParseError::InvalidPort { at: 6 }));
    assert_eq!(parse_url("x://h:12x"), Err(ParseError::InvalidPort { at: 8 }));
    assert_eq!(parse_url("x://h:65535").unwrap().port, Some(65535));
    assert_eq!(parse_url("x://h:65536"), Err(ParseError::InvalidPort { at: 6 }));
    assert_eq!(
        parse_url("x://h:999999999999999999999999999999"),
        Err(ParseError::InvalidPort { at: 6 })
    );
}

#[test]
fn spec_r12_fragment_delimiter_and_remainder() {
    let u = parse_url("x://h#").unwrap();
    assert_eq!(u.fragment.as_deref(), Some(""));
    let u = parse_url("x://h#a#?b").unwrap();
    assert_eq!(u.fragment.as_deref(), Some("a#?b"));
    assert_eq!(parse_url("x://h").unwrap().fragment, None);
}

#[test]
fn spec_r13_query_delimiter_and_remainder() {
    let u = parse_url("x://h?").unwrap();
    assert_eq!(u.query.as_deref(), Some(""));
    let u = parse_url("x://h?a?b#f").unwrap();
    assert_eq!(u.query.as_deref(), Some("a?b"));
    assert_eq!(parse_url("x://h").unwrap().query, None);
}

#[test]
fn spec_r14_path_default_and_verbatim() {
    assert_eq!(parse_url("x://h").unwrap().path, "/");
    assert_eq!(parse_url("x://h/a//b").unwrap().path, "/a//b");
    assert_eq!(parse_url("x://h?").unwrap().path, "/");
}

#[test]
fn spec_r15_delimiters_end_authority_before_component_content() {
    let u = parse_url("x://h/@:?#").unwrap();
    assert_eq!(u.host, "h");
    assert_eq!(u.path, "/@:");
    assert_eq!(u.query.as_deref(), Some(""));
    assert_eq!(u.fragment.as_deref(), Some(""));
}

#[test]
fn spec_r16_level_one_percent_bytes_are_literal_components() {
    let u = parse_url("x://h/%zz").unwrap();
    assert_eq!(u.path, "/%zz");
    assert_eq!(u.query, None);
}

#[test]
fn spec_r17_preserve_optional_component_spelling() {
    let u = parse_url("HtTp://UsEr@MiXeD:80/A?Q#F").unwrap();
    assert_eq!(u.scheme, "http");
    assert_eq!(u.host, "mixed");
    assert_eq!(u.userinfo.as_deref(), Some("UsEr"));
    assert_eq!(u.port, Some(80));
    assert_eq!(u.path, "/A");
    assert_eq!(u.query.as_deref(), Some("Q"));
    assert_eq!(u.fragment.as_deref(), Some("F"));
}

#[test]
fn spec_r18_value_equality_and_empty_optionals() {
    let a = parse_url("x://h").unwrap();
    let b = parse_url("x://h").unwrap();
    assert_eq!(a, b);
    assert_eq!(a.path, "/");
    assert_eq!(a.userinfo, None);
    assert_eq!(a.query, None);
    assert_eq!(a.fragment, None);

    let empty = parse_url("x://h?#").unwrap();
    assert_eq!(empty.query.as_deref(), Some(""));
    assert_eq!(empty.fragment.as_deref(), Some(""));
    assert_ne!(empty, a);
    assert_eq!(
        parse_url("x://@"),
        parse_url("x://@")
    );
}

#[test]
fn spec_r19_ipv6_flag_and_extended_errors() {
    let ordinary = parse_url("x://host").unwrap();
    assert!(!ordinary.ipv6);
    assert_eq!(
        parse_url("x://[bad]"),
        Err(ParseError::InvalidIpv6 { at: 4 })
    );
    assert_eq!(decode_component(Component::Path, "%").unwrap_err(), CodecError::BadEscape { at: 0 });
}

#[test]
fn spec_r20_ipv6_validation_and_preserved_spelling() {
    let u = parse_url("x://[2001:DB8::A]").unwrap();
    assert!(u.ipv6);
    assert_eq!(u.host, "2001:db8::a");
    assert_eq!(parse_url("x://[::ffff:192.0.2.1]").unwrap().host, "::ffff:192.0.2.1");
    assert_eq!(parse_url("x://[fe80::1%25eth0]"), Err(ParseError::InvalidIpv6 { at: 4 }));
    assert_eq!(parse_url("x://[::1"), Err(ParseError::InvalidIpv6 { at: 4 }));
}

#[test]
fn spec_r21_ipv6_suffix_and_bracket_positions() {
    let u = parse_url("x://[::1]:443").unwrap();
    assert!(u.ipv6);
    assert_eq!(u.port, Some(443));
    assert_eq!(parse_url("x://[::1]x"), Err(ParseError::InvalidHost { at: 9 }));
    assert_eq!(parse_url("x://a[b"), Err(ParseError::InvalidHost { at: 5 }));
    assert_eq!(parse_url("x://::1").unwrap_err(), ParseError::InvalidHost { at: 5 });
}

#[test]
fn spec_r22_codec_decodes_escapes_and_keeps_plus() {
    assert_eq!(
        decode_component(Component::Query, "a+b%2f").unwrap(),
        "a+b/"
    );
    assert_eq!(decode_component(Component::Fragment, "%41").unwrap(), "A");
    assert_eq!(decode_component(Component::Path, "é").unwrap(), "é");
}

#[test]
fn spec_r23_escape_syntax_scanned_before_utf8() {
    assert_eq!(
        decode_component(Component::Path, "%FF%"),
        Err(CodecError::BadEscape { at: 4 })
    );
    assert_eq!(
        decode_component(Component::Path, "%"),
        Err(CodecError::BadEscape { at: 0 })
    );
    assert_eq!(
        decode_component(Component::Path, "%0G"),
        Err(CodecError::BadEscape { at: 0 })
    );
}

#[test]
fn spec_r24_utf8_error_offset_maps_to_original_input() {
    assert_eq!(
        decode_component(Component::Path, "a%FFz"),
        Err(CodecError::InvalidUtf8 { at: 1 })
    );
    assert_eq!(
        decode_component(Component::Path, "é%FF"),
        Err(CodecError::InvalidUtf8 { at: 2 })
    );
    assert_eq!(decode_component(Component::Path, "%00").unwrap(), "\0");
    assert_eq!(decode_component(Component::Path, "%0A").unwrap(), "\n");
}

#[test]
fn spec_r25_component_specific_encoding_safe_sets() {
    assert_eq!(encode_component(Component::UserInfo, "a:b@%+"), "a:b%40%25%2B");
    assert_eq!(encode_component(Component::Path, "/a?b"), "/a%3Fb");
    assert_eq!(encode_component(Component::Query, "/a?b"), "/a?b");
    assert_eq!(encode_component(Component::Fragment, "/a?b#c"), "/a?b#c");
    assert_eq!(encode_component(Component::Path, "AZaz09-._~"), "AZaz09-._~");
    assert_eq!(encode_component(Component::Path, " é"), "%20%C3%A9");
    assert_eq!(encode_component(Component::Query, ""), "");
}

#[test]
fn spec_r26_url_component_validation_order_and_rebased_offsets() {
    assert_eq!(parse_url("x://u%FF@h"), Err(ParseError::InvalidUtf8 { at: 5 }));
    assert_eq!(parse_url("x://h/%"), Err(ParseError::BadEscape { at: 7 }));
    assert_eq!(parse_url("x://h?q%"), Err(ParseError::BadEscape { at: 7 }));
    assert_eq!(parse_url("x://h#%"), Err(ParseError::BadEscape { at: 6 }));
    assert_eq!(parse_url("x://h/%FF"), Err(ParseError::InvalidUtf8 { at: 7 }));
    assert_eq!(parse_url("x://h"), Ok(parse_url("x://h").unwrap()));
}

#[test]
fn spec_r27_percent_is_not_decoded_in_authority() {
    assert_eq!(parse_url("x://h%20"), Err(ParseError::InvalidHost { at: 5 }));
    assert_eq!(parse_url("x://h:%31"), Err(ParseError::InvalidPort { at: 6 }));
    let u = parse_url("x://h/%2F").unwrap();
    assert_eq!(u.path, "/%2F");
}

#[test]
fn spec_r28_component_validation_precedence() {
    assert_eq!(
        parse_url("x://u%FF@h/%"),
        Err(ParseError::InvalidUtf8 { at: 5 })
    );
    assert_eq!(
        parse_url("x://h/%FF?%"),
        Err(ParseError::InvalidUtf8 { at: 7 })
    );
    assert_eq!(
        parse_url("x://h/% "),
        Err(ParseError::InvalidByte { at: 8 })
    );
}

#[test]
fn spec_r29_long_codec_inputs_and_round_trip() {
    for component in [
        Component::UserInfo,
        Component::Path,
        Component::Query,
        Component::Fragment,
    ] {
        let text = "é+ /?#".repeat(2048);
        let encoded = encode_component(component, &text);
        assert_eq!(decode_component(component, &encoded).unwrap(), text);
    }
    let long = "a".repeat(100_000);
    assert_eq!(decode_component(Component::Path, &long).unwrap(), long);
}

#[test]
fn spec_r30_pure_deterministic_calls() {
    let input = String::from("HTTP://Example/a%20b?q#f");
    let before = input.clone();
    let first = parse_url(&input);
    let second = parse_url(&input);
    assert_eq!(first, second);
    assert_eq!(input, before);

    let text = String::from("é +");
    let text_before = text.clone();
    let encoded_a = encode_component(Component::Query, &text);
    let encoded_b = encode_component(Component::Query, &text);
    assert_eq!(encoded_a, encoded_b);
    assert_eq!(text, text_before);
    assert_eq!(
        decode_component(Component::Query, &encoded_a),
        decode_component(Component::Query, &encoded_b)
    );
}
