use std::borrow::Cow;

use core::fmt;

use crate::error::{ErrorKind, ParseError, Result};

/// Universal Geographic Code string as defined by NWSI 10-1702.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UgcString<'a> {
    pub raw: &'a str,
    pub kind: UgcKind,
    pub codes: Vec<UgcCode<'a>>,
    pub purge_time: UgcPurgeTime,
}

impl<'a> UgcString<'a> {
    pub fn parse(input: &'a str) -> Result<Self> {
        let raw = input.trim_matches(|ch: char| ch.is_ascii_whitespace());
        if raw.is_empty() {
            return Err(ParseError::new(ErrorKind::MissingField("ugc string")));
        }

        let compact = normalize_ugc(raw)?;
        let body = compact
            .strip_suffix('-')
            .ok_or_else(|| ParseError::new(ErrorKind::InvalidField("ugc terminator")))?;
        if body.is_empty() {
            return Err(ParseError::new(ErrorKind::MissingField("ugc string")));
        }

        let mut groups = body.split('-').collect::<Vec<_>>();
        if groups.is_empty() || groups.iter().any(|group| group.is_empty()) {
            return Err(ParseError::new(ErrorKind::InvalidField("ugc group")));
        }

        let purge_time =
            UgcPurgeTime::parse(groups.pop().expect("ugc groups cannot be empty here"))?;
        let first_group = groups
            .first()
            .copied()
            .ok_or_else(|| ParseError::new(ErrorKind::MissingField("ugc code")))?;

        let (kind, current_state, first_code) = parse_first_group(first_group)?;
        let mut seen_states = vec![current_state.clone()];
        let mut state_rules = StateRules::default();
        state_rules.observe(&first_code)?;

        let mut codes = Vec::with_capacity(groups.len());
        codes.push(first_code);

        let mut current_state = current_state;
        for group in groups.iter().copied().skip(1) {
            let (next_state, code) = parse_group(group, kind, &current_state, &seen_states)?;

            if let Some(next_state) = next_state {
                seen_states.push(next_state.clone());
                current_state = next_state;
                state_rules = StateRules::default();
            }

            state_rules.observe(&code)?;
            codes.push(code);
        }

        Ok(Self {
            raw,
            kind,
            codes,
            purge_time,
        })
    }

    pub const fn raw(&self) -> &'a str {
        self.raw
    }

    pub const fn kind(&self) -> UgcKind {
        self.kind
    }

    pub fn codes(&self) -> &[UgcCode<'a>] {
        &self.codes
    }

    pub const fn purge_time(&self) -> UgcPurgeTime {
        self.purge_time
    }
}

impl fmt::Display for UgcString<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.raw)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UgcKind {
    County,
    Zone,
}

impl UgcKind {
    const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            b'C' => Some(Self::County),
            b'Z' => Some(Self::Zone),
            _ => None,
        }
    }

    const fn as_char(self) -> char {
        match self {
            Self::County => 'C',
            Self::Zone => 'Z',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UgcCode<'a> {
    Single {
        state: Cow<'a, str>,
        kind: UgcKind,
        number: u16,
    },
    Range {
        state: Cow<'a, str>,
        kind: UgcKind,
        start: u16,
        end: u16,
    },
    All {
        state: Cow<'a, str>,
        kind: UgcKind,
    },
    Unspecified {
        state: Cow<'a, str>,
        kind: UgcKind,
    },
}

impl<'a> UgcCode<'a> {
    pub fn state(&self) -> &str {
        match self {
            Self::Single { state, .. }
            | Self::Range { state, .. }
            | Self::All { state, .. }
            | Self::Unspecified { state, .. } => state,
        }
    }

    pub const fn kind(&self) -> UgcKind {
        match self {
            Self::Single { kind, .. }
            | Self::Range { kind, .. }
            | Self::All { kind, .. }
            | Self::Unspecified { kind, .. } => *kind,
        }
    }

    pub const fn start_number(&self) -> Option<u16> {
        match self {
            Self::Single { number, .. } => Some(*number),
            Self::Range { start, .. } => Some(*start),
            Self::All { .. } | Self::Unspecified { .. } => None,
        }
    }

    pub const fn end_number(&self) -> Option<u16> {
        match self {
            Self::Single { number, .. } => Some(*number),
            Self::Range { end, .. } => Some(*end),
            Self::All { .. } | Self::Unspecified { .. } => None,
        }
    }

    pub const fn is_range(&self) -> bool {
        matches!(self, Self::Range { .. })
    }
}

impl fmt::Display for UgcCode<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Single {
                state,
                kind,
                number,
            } => write!(f, "{state}{}{number:03}", kind.as_char()),
            Self::Range {
                state,
                kind,
                start,
                end,
            } => write!(f, "{state}{}{start:03}>{end:03}", kind.as_char()),
            Self::All { state, kind } => write!(f, "{state}{}ALL", kind.as_char()),
            Self::Unspecified { state, kind } => write!(f, "{state}{}000", kind.as_char()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UgcPurgeTime {
    day: u8,
    hour: u8,
    minute: u8,
}

impl UgcPurgeTime {
    pub fn parse(input: &str) -> Result<Self> {
        let bytes = input.as_bytes();
        if bytes.len() != 6 || !bytes.iter().all(|byte| byte.is_ascii_digit()) {
            return Err(ParseError::new(ErrorKind::InvalidField("ugc purge time")));
        }

        let day = ascii_dec(bytes[0], bytes[1]);
        let hour = ascii_dec(bytes[2], bytes[3]);
        let minute = ascii_dec(bytes[4], bytes[5]);

        if !(1..=31).contains(&day) || hour > 23 || minute > 59 {
            return Err(ParseError::new(ErrorKind::InvalidField("ugc purge time")));
        }

        Ok(Self { day, hour, minute })
    }

    pub const fn day(self) -> u8 {
        self.day
    }

    pub const fn hour(self) -> u8 {
        self.hour
    }

    pub const fn minute(self) -> u8 {
        self.minute
    }
}

impl fmt::Display for UgcPurgeTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}{:02}{:02}", self.day, self.hour, self.minute)
    }
}

pub fn looks_like_ugc_string(input: &str) -> bool {
    UgcString::parse(input).is_ok()
}

#[derive(Debug, Default, Clone, Copy)]
struct StateRules {
    last_number: Option<u16>,
    has_special_code: bool,
}

impl StateRules {
    fn observe(&mut self, code: &UgcCode<'_>) -> Result<()> {
        match code {
            UgcCode::All { .. } | UgcCode::Unspecified { .. } => {
                if self.has_special_code || self.last_number.is_some() {
                    return Err(ParseError::new(ErrorKind::InvalidField("ugc state group")));
                }
                self.has_special_code = true;
            }
            UgcCode::Single { number, .. } => {
                self.observe_numeric(*number, *number)?;
            }
            UgcCode::Range { start, end, .. } => {
                if end <= start {
                    return Err(ParseError::new(ErrorKind::InvalidField("ugc range")));
                }
                self.observe_numeric(*start, *end)?;
            }
        }

        Ok(())
    }

    fn observe_numeric(&mut self, start: u16, end: u16) -> Result<()> {
        if self.has_special_code {
            return Err(ParseError::new(ErrorKind::InvalidField("ugc state group")));
        }

        if let Some(last) = self.last_number
            && start <= last
        {
            return Err(ParseError::new(ErrorKind::InvalidField("ugc ordering")));
        }

        self.last_number = Some(end);
        Ok(())
    }
}

fn normalize_ugc(input: &str) -> Result<String> {
    let mut normalized = String::with_capacity(input.len());
    let mut lines = input.lines().peekable();

    while let Some(line) = lines.next() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            return Err(ParseError::new(ErrorKind::InvalidField(
                "ugc line continuation",
            )));
        }

        if line.bytes().any(|byte| byte.is_ascii_whitespace()) {
            return Err(ParseError::new(ErrorKind::InvalidField("ugc whitespace")));
        }

        if lines.peek().is_some() && !line.ends_with('-') {
            return Err(ParseError::new(ErrorKind::InvalidField(
                "ugc line continuation",
            )));
        }

        normalized.push_str(line);
    }

    Ok(normalized)
}

fn parse_first_group<'a>(group: &str) -> Result<(UgcKind, String, UgcCode<'a>)> {
    let (state, kind, remainder) = parse_prefixed_group(group)?;
    let code = parse_designator(&state, kind, remainder)?;
    Ok((kind, state, code))
}

fn parse_group<'a>(
    group: &str,
    expected_kind: UgcKind,
    current_state: &str,
    seen_states: &[String],
) -> Result<(Option<String>, UgcCode<'a>)> {
    if has_prefix(group) {
        let (state, kind, remainder) = parse_prefixed_group(group)?;
        if kind != expected_kind {
            return Err(ParseError::new(ErrorKind::Mismatch("ugc kind")));
        }

        if seen_states.iter().any(|seen| seen == &state) {
            return Err(ParseError::new(ErrorKind::Mismatch("ugc state prefix")));
        }

        let code = parse_designator(&state, expected_kind, remainder)?;
        return Ok((Some(state), code));
    }

    let code = parse_designator(current_state, expected_kind, group)?;
    Ok((None, code))
}

fn parse_prefixed_group(group: &str) -> Result<(String, UgcKind, &str)> {
    if !has_prefix(group) {
        return Err(ParseError::new(ErrorKind::InvalidField("ugc prefix")));
    }

    let bytes = group.as_bytes();
    let state = group[..2].to_owned();
    let kind = UgcKind::from_byte(bytes[2])
        .ok_or_else(|| ParseError::new(ErrorKind::InvalidField("ugc kind")))?;

    Ok((state, kind, &group[3..]))
}

fn has_prefix(group: &str) -> bool {
    let bytes = group.as_bytes();
    bytes.len() >= 6
        && bytes[0].is_ascii_uppercase()
        && bytes[1].is_ascii_uppercase()
        && matches!(bytes[2], b'C' | b'Z')
}

fn parse_designator<'a>(state: &str, kind: UgcKind, designator: &str) -> Result<UgcCode<'a>> {
    match designator {
        "ALL" => {
            return Ok(UgcCode::All {
                state: Cow::Owned(state.to_owned()),
                kind,
            });
        }
        "000" => {
            return Ok(UgcCode::Unspecified {
                state: Cow::Owned(state.to_owned()),
                kind,
            });
        }
        _ => {}
    }

    if let Some((start, end)) = designator.split_once('>') {
        if kind != UgcKind::Zone {
            return Err(ParseError::new(ErrorKind::InvalidField("ugc range")));
        }

        let start = parse_number(start)?;
        let end = parse_number(end)?;
        return Ok(UgcCode::Range {
            state: Cow::Owned(state.to_owned()),
            kind,
            start,
            end,
        });
    }

    let number = parse_number(designator)?;
    Ok(UgcCode::Single {
        state: Cow::Owned(state.to_owned()),
        kind,
        number,
    })
}

fn parse_number(input: &str) -> Result<u16> {
    let bytes = input.as_bytes();
    if bytes.len() != 3 || !bytes.iter().all(|byte| byte.is_ascii_digit()) {
        return Err(ParseError::new(ErrorKind::InvalidField("ugc code")));
    }

    Ok(((bytes[0] - b'0') as u16) * 100
        + ((bytes[1] - b'0') as u16) * 10
        + (bytes[2] - b'0') as u16)
}

const fn ascii_dec(tens: u8, ones: u8) -> u8 {
    (tens - b'0') * 10 + (ones - b'0')
}

#[cfg(test)]
mod tests {
    use super::{UgcCode, UgcKind, UgcPurgeTime, UgcString, looks_like_ugc_string};

    #[test]
    fn parses_county_explicit_list() {
        let ugc = UgcString::parse("MOC001-005-009-121530-").unwrap();

        assert_eq!(ugc.kind(), UgcKind::County);
        assert_eq!(ugc.purge_time(), UgcPurgeTime::parse("121530").unwrap());
        assert_eq!(ugc.codes().len(), 3);
        assert_eq!(
            ugc.codes(),
            &[
                UgcCode::Single {
                    state: "MO".into(),
                    kind: UgcKind::County,
                    number: 1,
                },
                UgcCode::Single {
                    state: "MO".into(),
                    kind: UgcKind::County,
                    number: 5,
                },
                UgcCode::Single {
                    state: "MO".into(),
                    kind: UgcKind::County,
                    number: 9,
                },
            ]
        );
    }

    #[test]
    fn parses_multistate_zone_string_with_ranges() {
        let ugc = UgcString::parse(
            "DCZ001-MDZ003>007-009>011-013-014-016>018-501-502-VAZ021-025>031-036>040-042-050>057-501-502-WVZ050>055-501>504-182200-",
        )
        .unwrap();

        assert_eq!(ugc.kind(), UgcKind::Zone);
        assert_eq!(ugc.codes().len(), 17);
        assert!(matches!(
            &ugc.codes()[1],
            UgcCode::Range {
                state,
                kind: UgcKind::Zone,
                start: 3,
                end: 7,
            } if state == "MD"
        ));
        assert!(matches!(
            ugc.codes().last().unwrap(),
            UgcCode::Range {
                state,
                kind: UgcKind::Zone,
                start: 501,
                end: 504,
            } if state == "WV"
        ));
        assert_eq!(ugc.purge_time().day(), 18);
        assert_eq!(ugc.purge_time().hour(), 22);
        assert_eq!(ugc.purge_time().minute(), 0);
    }

    #[test]
    fn parses_multiline_ugc_continuation() {
        let ugc = UgcString::parse(
            "VAZ021-025>031-036>040-042-050>057-\r\n501-502-WVZ050>055-501>504-182200-",
        )
        .unwrap();

        assert_eq!(ugc.codes().len(), 9);
        assert_eq!(ugc.codes()[6].state(), "VA");
        assert_eq!(ugc.codes()[7].state(), "WV");
        assert_eq!(ugc.codes()[8].state(), "WV");
    }

    #[test]
    fn parses_all_and_unspecified_codes() {
        let zones = UgcString::parse("COZALL-220000-").unwrap();
        let mixed_states = UgcString::parse("LAZ000-TXZ000-260600-").unwrap();

        assert_eq!(
            zones.codes(),
            &[UgcCode::All {
                state: "CO".into(),
                kind: UgcKind::Zone,
            }]
        );
        assert_eq!(
            mixed_states.codes(),
            &[
                UgcCode::Unspecified {
                    state: "LA".into(),
                    kind: UgcKind::Zone,
                },
                UgcCode::Unspecified {
                    state: "TX".into(),
                    kind: UgcKind::Zone,
                },
            ]
        );
    }

    #[test]
    fn rejects_county_ranges() {
        assert!(UgcString::parse("MOC001>005-121530-").is_err());
    }

    #[test]
    fn rejects_repeated_state_prefixes() {
        assert!(UgcString::parse("MOC001-MOC005-121530-").is_err());
        assert!(UgcString::parse("MOC001-KSC001-MOC003-121530-").is_err());
    }

    #[test]
    fn rejects_mixed_ugc_kinds() {
        assert!(UgcString::parse("MDZ003-VAC001-182200-").is_err());
    }

    #[test]
    fn rejects_invalid_purge_time() {
        assert!(UgcString::parse("MOC001-322460-").is_err());
    }

    #[test]
    fn rejects_special_codes_mixed_with_explicit_codes() {
        assert!(UgcString::parse("COZALL-001-220000-").is_err());
        assert!(UgcString::parse("TXZ000-003-220000-").is_err());
    }

    #[test]
    fn rejects_non_monotonic_or_overlapping_groups() {
        assert!(UgcString::parse("MDZ003>007-006-182200-").is_err());
        assert!(UgcString::parse("MDZ007-003-182200-").is_err());
    }

    #[test]
    fn rejects_bad_line_continuation() {
        assert!(UgcString::parse("MDZ003>007\r\n009-182200-").is_err());
        assert!(UgcString::parse("MDZ003>007-\r\n\r\n009-182200-").is_err());
    }

    #[test]
    fn detects_valid_shapes() {
        assert!(looks_like_ugc_string("MOC001-005-009-121530-"));
        assert!(looks_like_ugc_string("COZALL-220000-"));
        assert!(!looks_like_ugc_string("moc001-005-009-121530-"));
    }
}
