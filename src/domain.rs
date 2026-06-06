use crate::api::Error;

/// Split a registrable domain into the (SLD, TLD) pair the dns.* API methods
/// require. IDN input is normalized to punycode first, and the split uses the
/// Public Suffix List so multi-label suffixes like co.uk land in TLD whole.
/// Subdomains are rejected rather than silently trimmed: querying
/// "www.example.com" when you own "example.com" is a mistake worth surfacing.
pub fn split_sld_tld(input: &str) -> Result<(String, String), Error> {
    let trimmed = input.trim().trim_end_matches('.');
    let ascii = idna::domain_to_ascii(trimmed)
        .map_err(|_| Error::Usage(format!("invalid domain name {input:?}")))?;
    let registrable = psl::domain(ascii.as_bytes()).ok_or_else(|| {
        Error::Usage(format!(
            "cannot determine a registrable domain in {input:?}"
        ))
    })?;
    let reg = std::str::from_utf8(registrable.as_bytes())
        .map_err(|_| Error::Usage(format!("invalid domain name {input:?}")))?;
    if reg != ascii {
        return Err(Error::Usage(format!(
            "{input:?} is not a registrable domain (did you mean {reg}?)"
        )));
    }
    let suffix = std::str::from_utf8(registrable.suffix().as_bytes())
        .map_err(|_| Error::Usage(format!("invalid domain name {input:?}")))?;
    let sld = &reg[..reg.len() - suffix.len() - 1];
    Ok((sld.to_owned(), suffix.to_owned()))
}

/// Normalize a domain argument for DomainName/DomainList params: same
/// IDN-to-punycode and PSL validation as the SLD/TLD path, joined back up,
/// so every command treats domain input identically.
pub fn normalize(input: &str) -> Result<String, Error> {
    let (sld, tld) = split_sld_tld(input)?;
    Ok(format!("{sld}.{tld}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_com_splits() {
        assert_eq!(
            split_sld_tld("example.com").unwrap(),
            ("example".into(), "com".into())
        );
    }

    #[test]
    fn multi_label_suffix_stays_whole() {
        assert_eq!(
            split_sld_tld("example.co.uk").unwrap(),
            ("example".into(), "co.uk".into())
        );
    }

    #[test]
    fn idn_input_is_punycoded() {
        assert_eq!(
            split_sld_tld("münchen.de").unwrap(),
            ("xn--mnchen-3ya".into(), "de".into())
        );
    }

    #[test]
    fn uppercase_and_trailing_dot_normalize() {
        assert_eq!(
            split_sld_tld("Example.COM.").unwrap(),
            ("example".into(), "com".into())
        );
    }

    #[test]
    fn subdomain_is_rejected_with_suggestion() {
        let err = split_sld_tld("www.example.co.uk").unwrap_err();
        assert_eq!(err.exit_code(), 2);
        assert!(err.to_string().contains("example.co.uk"));
    }

    #[test]
    fn bare_suffix_is_rejected() {
        assert!(split_sld_tld("co.uk").is_err());
        assert!(split_sld_tld("localhost").is_err());
    }
}
