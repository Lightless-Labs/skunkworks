/// Returns `true` when `email` matches a pragmatic address format.
///
/// This validator is intentionally stricter than RFC 5322. It accepts common
/// mailbox formats and rejects addresses with whitespace, multiple `@`
/// separators, invalid domain labels, or dot-placement errors.
pub fn is_valid_email(email: &str) -> bool {
    if email.is_empty() || email.len() > 254 || email.trim() != email {
        return false;
    }

    let mut parts = email.split('@');
    let Some(local) = parts.next() else {
        return false;
    };
    let Some(domain) = parts.next() else {
        return false;
    };

    if parts.next().is_some() || local.is_empty() || local.len() > 64 || domain.is_empty() {
        return false;
    }

    is_valid_local_part(local) && is_valid_domain(domain)
}

fn is_valid_local_part(local: &str) -> bool {
    if local.starts_with('.') || local.ends_with('.') || local.contains("..") {
        return false;
    }

    local.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'!' | b'#'
                    | b'$'
                    | b'%'
                    | b'&'
                    | b'\''
                    | b'*'
                    | b'+'
                    | b'-'
                    | b'/'
                    | b'='
                    | b'?'
                    | b'^'
                    | b'_'
                    | b'`'
                    | b'{'
                    | b'|'
                    | b'}'
                    | b'~'
                    | b'.'
            )
    })
}

fn is_valid_domain(domain: &str) -> bool {
    if domain.len() > 253 || domain.starts_with('.') || domain.ends_with('.') {
        return false;
    }

    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() < 2 || labels.last().is_some_and(|label| label.len() < 2) {
        return false;
    }

    labels.into_iter().all(is_valid_domain_label)
}

fn is_valid_domain_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::is_valid_email;

    #[test]
    fn accepts_common_email_formats() {
        for email in [
            "user@example.com",
            "first.last+tag@example.co.uk",
            "USER_123@sub-domain.example",
            "name/o'reilly@example.io",
        ] {
            assert!(is_valid_email(email), "{email} should be valid");
        }
    }

    #[test]
    fn rejects_missing_or_multiple_at_signs() {
        for email in [
            "user.example.com",
            "user@@example.com",
            "@example.com",
            "user@",
        ] {
            assert!(!is_valid_email(email), "{email} should be invalid");
        }
    }

    #[test]
    fn rejects_whitespace_and_invalid_characters() {
        for email in [
            " user@example.com",
            "user@example.com ",
            "user name@example.com",
            "user@exa mple.com",
            "us\\er@example.com",
        ] {
            assert!(!is_valid_email(email), "{email} should be invalid");
        }
    }

    #[test]
    fn rejects_invalid_dot_placement() {
        for email in [
            ".user@example.com",
            "user.@example.com",
            "user..name@example.com",
            "user@example..com",
            "user@.example.com",
            "user@example.com.",
        ] {
            assert!(!is_valid_email(email), "{email} should be invalid");
        }
    }

    #[test]
    fn rejects_invalid_domain_labels() {
        for email in [
            "user@example",
            "user@example.c",
            "user@-example.com",
            "user@example-.com",
            "user@example!.com",
        ] {
            assert!(!is_valid_email(email), "{email} should be invalid");
        }
    }
}
