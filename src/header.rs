use core::fmt;

use crate::error::{ErrorKind, ParseError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WmoHeading<'a> {
    raw: &'a str,
    ttaaii: &'a str,
    cccc: &'a str,
    yygggg: &'a str,
    bbb: Option<&'a str>,
}

impl<'a> WmoHeading<'a> {
    pub fn parse(input: &'a str) -> Result<Self> {
        let trimmed = input.trim_matches(|ch: char| ch.is_ascii_whitespace());
        let mut parts = trimmed.split_ascii_whitespace();
        let ttaaii = parts
            .next()
            .ok_or_else(|| ParseError::new(ErrorKind::MissingField("ttaaii")))?;
        let cccc = parts
            .next()
            .ok_or_else(|| ParseError::new(ErrorKind::MissingField("cccc")))?;
        let yygggg = parts
            .next()
            .ok_or_else(|| ParseError::new(ErrorKind::MissingField("yygggg")))?;
        let bbb = parts.next();

        if parts.next().is_some() {
            return Err(ParseError::new(ErrorKind::InvalidField(
                "wmo heading token count",
            )));
        }

        validate_ttaaii(ttaaii)?;
        validate_cccc(cccc)?;
        validate_yygggg(yygggg)?;
        if let Some(bbb) = bbb {
            validate_bbb(bbb)?;
        }

        Ok(Self {
            raw: trimmed,
            ttaaii,
            cccc,
            yygggg,
            bbb,
        })
    }

    pub const fn raw(self) -> &'a str {
        self.raw
    }

    pub const fn ttaaii(self) -> &'a str {
        self.ttaaii
    }

    pub const fn cccc(self) -> &'a str {
        self.cccc
    }

    pub const fn yygggg(self) -> &'a str {
        self.yygggg
    }

    pub const fn bbb(self) -> Option<&'a str> {
        self.bbb
    }

    pub fn day(self) -> u8 {
        ascii_dec(self.yygggg.as_bytes()[0], self.yygggg.as_bytes()[1])
    }

    pub fn hour(self) -> u8 {
        ascii_dec(self.yygggg.as_bytes()[2], self.yygggg.as_bytes()[3])
    }

    pub fn minute(self) -> u8 {
        ascii_dec(self.yygggg.as_bytes()[4], self.yygggg.as_bytes()[5])
    }
}

impl fmt::Display for WmoHeading<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.raw)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AwipsId<'a> {
    raw: &'a str,
    nnn: &'a str,
    xxx: &'a str,
}

impl<'a> AwipsId<'a> {
    pub fn parse(input: &'a str) -> Result<Self> {
        let trimmed = input.trim_matches(|ch: char| ch.is_ascii_whitespace());
        validate_awips_id(trimmed)?;
        Ok(Self {
            raw: trimmed,
            nnn: &trimmed[..3],
            xxx: &trimmed[3..],
        })
    }

    pub const fn raw(self) -> &'a str {
        self.raw
    }

    pub const fn nnn(self) -> &'a str {
        self.nnn
    }

    pub const fn xxx(self) -> &'a str {
        self.xxx
    }
}

impl fmt::Display for AwipsId<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.raw)
    }
}

pub fn looks_like_wmo_heading(input: &str) -> bool {
    WmoHeading::parse(input).is_ok()
}

pub fn looks_like_awips_id(input: &str) -> bool {
    AwipsId::parse(input).is_ok()
}

fn validate_ttaaii(value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    if bytes.len() != 6 {
        return Err(ParseError::new(ErrorKind::InvalidField("ttaaii")));
    }

    if !bytes[..4].iter().all(|byte| byte.is_ascii_uppercase()) {
        return Err(ParseError::new(ErrorKind::InvalidField("ttaaii")));
    }

    if !bytes[4..].iter().all(|byte| byte.is_ascii_digit()) {
        return Err(ParseError::new(ErrorKind::InvalidField("ttaaii")));
    }

    Ok(())
}

fn validate_cccc(value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    if bytes.len() != 4 || !bytes.iter().all(|byte| byte.is_ascii_uppercase()) {
        return Err(ParseError::new(ErrorKind::InvalidField("cccc")));
    }
    Ok(())
}

fn validate_yygggg(value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    if bytes.len() != 6 || !bytes.iter().all(|byte| byte.is_ascii_digit()) {
        return Err(ParseError::new(ErrorKind::InvalidField("yygggg")));
    }
    Ok(())
}

fn validate_bbb(value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    if bytes.len() != 3
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        return Err(ParseError::new(ErrorKind::InvalidField("bbb")));
    }
    Ok(())
}

fn validate_awips_id(value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    if !(5..=6).contains(&bytes.len()) {
        return Err(ParseError::new(ErrorKind::InvalidField("awips id")));
    }

    if !bytes
        .iter()
        .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        return Err(ParseError::new(ErrorKind::InvalidField("awips id")));
    }

    Ok(())
}

fn ascii_dec(tens: u8, ones: u8) -> u8 {
    (tens - b'0') * 10 + (ones - b'0')
}

#[cfg(test)]
mod tests {
    use super::{AwipsId, WmoHeading, looks_like_awips_id, looks_like_wmo_heading};

    #[test]
    fn parses_heading_with_optional_bbb() {
        let heading = WmoHeading::parse("NOUS41 KWBC 201530 AAA").unwrap();
        assert_eq!(heading.ttaaii(), "NOUS41");
        assert_eq!(heading.cccc(), "KWBC");
        assert_eq!(heading.yygggg(), "201530");
        assert_eq!(heading.bbb(), Some("AAA"));
        assert_eq!(heading.day(), 20);
        assert_eq!(heading.hour(), 15);
        assert_eq!(heading.minute(), 30);
    }

    #[test]
    fn rejects_bad_heading_shapes() {
        assert!(WmoHeading::parse("NOUS4 KWBC 201530").is_err());
        assert!(WmoHeading::parse("NOUS41 KW3C 201530").is_err());
        assert!(WmoHeading::parse("NOUS41 KWBC 20153A").is_err());
    }

    #[test]
    fn parses_awips_id() {
        let awips = AwipsId::parse("RR8ARX").unwrap();
        assert_eq!(awips.nnn(), "RR8");
        assert_eq!(awips.xxx(), "ARX");
    }

    #[test]
    fn detects_shapes() {
        assert!(looks_like_wmo_heading("SRUS83 KARX 250220"));
        assert!(looks_like_awips_id("AFDLWX"));
        assert!(!looks_like_awips_id("AFD-LWX"));
    }
}
