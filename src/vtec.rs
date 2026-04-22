use core::fmt;

use time::{Date, Month, PrimitiveDateTime, Time};

use crate::error::{ErrorKind, ParseError, Result};

const ZERO_TIME_GROUP: &str = "000000T0000Z";

macro_rules! code_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $(
                $variant:ident => $code:literal,
            )+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $(
                $variant,
            )+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[
                $(
                    Self::$variant,
                )+
            ];

            pub const fn code(self) -> &'static str {
                match self {
                    $(
                        Self::$variant => $code,
                    )+
                }
            }

            pub const fn as_str(self) -> &'static str {
                self.code()
            }

            pub fn from_code(input: &str) -> Option<Self> {
                match input {
                    $(
                        $code => Some(Self::$variant),
                    )+
                    _ => None,
                }
            }

            fn parse_field(input: &str, field: &'static str) -> Result<Self> {
                Self::from_code(input)
                    .ok_or_else(|| ParseError::new(ErrorKind::InvalidField(field)))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.code())
            }
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VtecTime {
    Unspecified,
    At(PrimitiveDateTime),
}

impl VtecTime {
    pub const fn is_unspecified(self) -> bool {
        matches!(self, Self::Unspecified)
    }

    pub fn datetime(self) -> Option<PrimitiveDateTime> {
        match self {
            Self::Unspecified => None,
            Self::At(value) => Some(value),
        }
    }
}

impl fmt::Display for VtecTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unspecified => f.write_str(ZERO_TIME_GROUP),
            Self::At(value) => write!(
                f,
                "{:02}{:02}{:02}T{:02}{:02}Z",
                value.year().rem_euclid(100),
                value.month() as u8,
                value.day(),
                value.hour(),
                value.minute()
            ),
        }
    }
}

code_enum!(
    pub enum EventClass {
        Operational => "O",
        Test => "T",
        Experimental => "E",
        ExperimentalVtecInOperationalProduct => "X",
    }
);

code_enum!(
    pub enum VtecAction {
        New => "NEW",
        Continue => "CON",
        ExtendTime => "EXT",
        ExtendArea => "EXA",
        ExtendAreaAndTime => "EXB",
        Upgrade => "UPG",
        Cancel => "CAN",
        Expire => "EXP",
        Correction => "COR",
        Routine => "ROU",
    }
);

code_enum!(
    pub enum Phenomenon {
        AshfallLand => "AF",
        AirStagnation => "AS",
        BeachHazard => "BH",
        BriskWind => "BW",
        Blizzard => "BZ",
        CoastalFlood => "CF",
        DebrisFlow => "DF",
        DustStorm => "DS",
        BlowingDust => "DU",
        ExtremeCold => "EC",
        ExcessiveHeat => "EH",
        ExtremeWind => "EW",
        Flood => "FA",
        FlashFlood => "FF",
        DenseFogLand => "FG",
        FloodForecastPoint => "FL",
        Frost => "FR",
        FireWeather => "FW",
        Freeze => "FZ",
        Gale => "GL",
        HurricaneForceWind => "HF",
        Heat => "HT",
        Hurricane => "HU",
        HighWind => "HW",
        Hydrologic => "HY",
        HardFreeze => "HZ",
        IceStorm => "IS",
        LakeEffectSnow => "LE",
        LowWater => "LO",
        LakeshoreFlood => "LS",
        LakeWind => "LW",
        Marine => "MA",
        DenseFogMarine => "MF",
        AshfallMarine => "MH",
        DenseSmokeMarine => "MS",
        RipCurrentRisk => "RP",
        SmallCraft => "SC",
        HazardousSeas => "SE",
        DenseSmokeLand => "SM",
        Storm => "SR",
        StormSurge => "SS",
        SnowSquall => "SQ",
        HighSurf => "SU",
        SevereThunderstorm => "SV",
        Tornado => "TO",
        TropicalStorm => "TR",
        Tsunami => "TS",
        Typhoon => "TY",
        HeavyFreezingSpray => "UP",
        WindChill => "WC",
        Wind => "WI",
        WinterStorm => "WS",
        WinterWeather => "WW",
        FreezingFog => "ZF",
        FreezingRain => "ZR",
        FreezingSpray => "ZY",
    }
);

impl Phenomenon {
    pub const fn supports_hydrologic_vtec(self) -> bool {
        matches!(
            self,
            Self::Flood | Self::FlashFlood | Self::FloodForecastPoint | Self::Hydrologic
        )
    }
}

code_enum!(
    pub enum Significance {
        Warning => "W",
        Watch => "A",
        Advisory => "Y",
        Statement => "S",
        Forecast => "F",
        Outlook => "O",
        Synopsis => "N",
    }
);

code_enum!(
    pub enum FloodSeverity {
        NoneExpected => "N",
        NotClassified => "0",
        Minor => "1",
        Moderate => "2",
        Major => "3",
        Unknown => "U",
    }
);

code_enum!(
    pub enum ImmediateCause {
        ExcessiveRainfall => "ER",
        Snowmelt => "SM",
        RainAndSnowmelt => "RS",
        DamOrLeveeFailure => "DM",
        GlacierDammedLakeOutburst => "GO",
        IceJam => "IJ",
        RainSnowmeltIceJam => "IC",
        UpstreamFloodingStormSurge => "FS",
        UpstreamFloodingTidalEffects => "FT",
        ElevatedUpstreamFlowTidalEffects => "ET",
        WindOrTidalEffects => "WT",
        UpstreamDamOrReservoirRelease => "DR",
        OtherMultipleCauses => "MC",
        OtherEffects => "OT",
        Unknown => "UU",
    }
);

code_enum!(
    pub enum FloodRecord {
        NotApplicable => "OO",
        NotExpected => "NO",
        NearRecordOrRecordExpected => "NR",
        NoPeriodOfRecord => "UU",
    }
);

pub type ProductClass = EventClass;
pub type Action = VtecAction;
pub type PVtec = Pvtec;
pub type HVtec = Hvtec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pvtec {
    raw: Box<str>,
    office_id: [u8; 4],
    pub event_class: EventClass,
    pub action: VtecAction,
    pub phenomenon: Phenomenon,
    pub significance: Significance,
    pub event_tracking_number: u16,
    pub start_time: VtecTime,
    pub end_time: VtecTime,
}

impl Pvtec {
    pub fn parse(input: &str) -> Result<Self> {
        let (raw, body) = trim_and_strip_slashes(input, "p-vtec")?;
        let mut parts = body.split('.');
        let event_class =
            EventClass::parse_field(next_required(&mut parts, "product class")?, "product class")?;
        let action = VtecAction::parse_field(next_required(&mut parts, "action")?, "action")?;
        let office_id = next_required(&mut parts, "office id")?;
        validate_office_id(office_id)?;
        let phenomenon =
            Phenomenon::parse_field(next_required(&mut parts, "phenomenon")?, "phenomenon")?;
        let significance =
            Significance::parse_field(next_required(&mut parts, "significance")?, "significance")?;
        let event_tracking_number = parse_etn(next_required(&mut parts, "etn")?)?;
        let time_range = next_required(&mut parts, "event time range")?;

        if parts.next().is_some() {
            return Err(ParseError::new(ErrorKind::InvalidField(
                "p-vtec field count",
            )));
        }

        let (start_time, end_time) = parse_p_vtec_time_range(time_range)?;

        Ok(Self {
            raw: raw.into(),
            office_id: copy_ascii::<4>(office_id),
            event_class,
            action,
            phenomenon,
            significance,
            event_tracking_number,
            start_time,
            end_time,
        })
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }

    pub const fn event_class(&self) -> EventClass {
        self.event_class
    }

    pub const fn product_class(&self) -> EventClass {
        self.event_class
    }

    pub const fn action(&self) -> VtecAction {
        self.action
    }

    pub fn office_id(&self) -> &str {
        ascii_bytes_to_str(&self.office_id)
    }

    pub const fn phenomenon(&self) -> Phenomenon {
        self.phenomenon
    }

    pub const fn significance(&self) -> Significance {
        self.significance
    }

    pub const fn event_tracking_number(&self) -> u16 {
        self.event_tracking_number
    }

    pub const fn start_time(&self) -> VtecTime {
        self.start_time
    }

    pub const fn end_time(&self) -> VtecTime {
        self.end_time
    }
}

impl fmt::Display for Pvtec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hvtec {
    raw: Box<str>,
    nwsli: [u8; 5],
    pub severity: FloodSeverity,
    pub immediate_cause: ImmediateCause,
    pub begin_time: VtecTime,
    pub crest_time: VtecTime,
    pub end_time: VtecTime,
    pub flood_record: FloodRecord,
}

impl Hvtec {
    pub fn parse(input: &str) -> Result<Self> {
        let (raw, body) = trim_and_strip_slashes(input, "h-vtec")?;
        let mut parts = body.split('.');
        let nwsli = next_required(&mut parts, "nwsli")?;
        validate_nwsli(nwsli)?;
        let severity = FloodSeverity::parse_field(
            next_required(&mut parts, "flood severity")?,
            "flood severity",
        )?;
        let immediate_cause = ImmediateCause::parse_field(
            next_required(&mut parts, "immediate cause")?,
            "immediate cause",
        )?;
        let begin_time = parse_time_group(
            next_required(&mut parts, "flood begin time")?,
            "flood begin time",
        )?;
        let crest_time = parse_time_group(
            next_required(&mut parts, "flood crest time")?,
            "flood crest time",
        )?;
        let end_time = parse_time_group(
            next_required(&mut parts, "flood end time")?,
            "flood end time",
        )?;
        let flood_record =
            FloodRecord::parse_field(next_required(&mut parts, "flood record")?, "flood record")?;

        if parts.next().is_some() {
            return Err(ParseError::new(ErrorKind::InvalidField(
                "h-vtec field count",
            )));
        }

        Ok(Self {
            raw: raw.into(),
            nwsli: copy_ascii::<5>(nwsli),
            severity,
            immediate_cause,
            begin_time,
            crest_time,
            end_time,
            flood_record,
        })
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }

    pub fn nwsli(&self) -> &str {
        ascii_bytes_to_str(&self.nwsli)
    }

    pub const fn severity(&self) -> FloodSeverity {
        self.severity
    }

    pub const fn immediate_cause(&self) -> ImmediateCause {
        self.immediate_cause
    }

    pub const fn begin_time(&self) -> VtecTime {
        self.begin_time
    }

    pub const fn crest_time(&self) -> VtecTime {
        self.crest_time
    }

    pub const fn end_time(&self) -> VtecTime {
        self.end_time
    }

    pub const fn flood_record(&self) -> FloodRecord {
        self.flood_record
    }
}

impl fmt::Display for Hvtec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HydrologicVtecPair {
    primary: Pvtec,
    hydrologic: Hvtec,
}

impl HydrologicVtecPair {
    pub fn parse(primary: &str, hydrologic: &str) -> Result<Self> {
        let primary = Pvtec::parse(primary)?;
        if !primary.phenomenon.supports_hydrologic_vtec() {
            return Err(ParseError::new(ErrorKind::InvalidField(
                "h-vtec trigger phenomenon",
            )));
        }

        let hydrologic = Hvtec::parse(hydrologic)?;
        Ok(Self {
            primary,
            hydrologic,
        })
    }

    pub fn primary(&self) -> &Pvtec {
        &self.primary
    }

    pub fn hydrologic(&self) -> &Hvtec {
        &self.hydrologic
    }
}

pub fn looks_like_p_vtec(input: &str) -> bool {
    Pvtec::parse(input).is_ok()
}

pub fn looks_like_h_vtec(input: &str) -> bool {
    Hvtec::parse(input).is_ok()
}

fn trim_and_strip_slashes<'a>(input: &'a str, field: &'static str) -> Result<(&'a str, &'a str)> {
    let trimmed = input.trim_matches(|ch: char| ch.is_ascii_whitespace());
    if trimmed.len() < 2 || !trimmed.starts_with('/') || !trimmed.ends_with('/') {
        return Err(ParseError::new(ErrorKind::InvalidField(field)));
    }
    Ok((trimmed, &trimmed[1..trimmed.len() - 1]))
}

fn next_required<'a, I>(parts: &mut I, field: &'static str) -> Result<&'a str>
where
    I: Iterator<Item = &'a str>,
{
    parts
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ParseError::new(ErrorKind::MissingField(field)))
}

fn validate_office_id(input: &str) -> Result<()> {
    let bytes = input.as_bytes();
    if bytes.len() != 4 || !bytes.iter().all(|byte| byte.is_ascii_uppercase()) {
        return Err(ParseError::new(ErrorKind::InvalidField("office id")));
    }
    Ok(())
}

fn validate_nwsli(input: &str) -> Result<()> {
    let bytes = input.as_bytes();
    if bytes.len() != 5
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        return Err(ParseError::new(ErrorKind::InvalidField("nwsli")));
    }
    Ok(())
}

fn copy_ascii<const N: usize>(input: &str) -> [u8; N] {
    let mut bytes = [0u8; N];
    bytes.copy_from_slice(input.as_bytes());
    bytes
}

fn ascii_bytes_to_str<const N: usize>(bytes: &[u8; N]) -> &str {
    core::str::from_utf8(bytes).expect("validated ASCII VTEC field")
}

fn parse_etn(input: &str) -> Result<u16> {
    let bytes = input.as_bytes();
    if bytes.len() != 4 || !bytes.iter().all(|byte| byte.is_ascii_digit()) {
        return Err(ParseError::new(ErrorKind::InvalidField("etn")));
    }

    Ok(bytes
        .iter()
        .fold(0u16, |value, digit| (value * 10) + u16::from(digit - b'0')))
}

fn parse_p_vtec_time_range(input: &str) -> Result<(VtecTime, VtecTime)> {
    let (start, end) = input
        .split_once('-')
        .ok_or_else(|| ParseError::new(ErrorKind::InvalidField("event time range")))?;
    if start.is_empty() || end.is_empty() || end.contains('-') {
        return Err(ParseError::new(ErrorKind::InvalidField("event time range")));
    }

    Ok((
        parse_time_group(start, "event start time")?,
        parse_time_group(end, "event end time")?,
    ))
}

fn parse_time_group(input: &str, field: &'static str) -> Result<VtecTime> {
    if input == ZERO_TIME_GROUP {
        return Ok(VtecTime::Unspecified);
    }

    let bytes = input.as_bytes();
    if bytes.len() != 12 || bytes[6] != b'T' || bytes[11] != b'Z' {
        return Err(ParseError::new(ErrorKind::InvalidField(field)));
    }

    if !bytes[..6].iter().all(|byte| byte.is_ascii_digit())
        || !bytes[7..11].iter().all(|byte| byte.is_ascii_digit())
    {
        return Err(ParseError::new(ErrorKind::InvalidField(field)));
    }

    let year = 2000 + i32::from(parse_two_digits(bytes[0], bytes[1]));
    let month = parse_two_digits(bytes[2], bytes[3]);
    let day = parse_two_digits(bytes[4], bytes[5]);
    let hour = parse_two_digits(bytes[7], bytes[8]);
    let minute = parse_two_digits(bytes[9], bytes[10]);

    // VTEC encodes a two-digit year; interpret it as 2000-based UTC.
    let month =
        Month::try_from(month).map_err(|_| ParseError::new(ErrorKind::InvalidField(field)))?;
    let date = Date::from_calendar_date(year, month, day)
        .map_err(|_| ParseError::new(ErrorKind::InvalidField(field)))?;
    let time = Time::from_hms(hour, minute, 0)
        .map_err(|_| ParseError::new(ErrorKind::InvalidField(field)))?;

    Ok(VtecTime::At(PrimitiveDateTime::new(date, time)))
}

fn parse_two_digits(tens: u8, ones: u8) -> u8 {
    ((tens - b'0') * 10) + (ones - b'0')
}

#[cfg(test)]
mod tests {
    use super::{
        EventClass, FloodRecord, FloodSeverity, Hvtec, HydrologicVtecPair, ImmediateCause,
        Phenomenon, Pvtec, Significance, VtecAction, VtecTime, looks_like_h_vtec,
        looks_like_p_vtec,
    };
    use time::{Date, Month, PrimitiveDateTime, Time};

    fn dt(year: i32, month: u8, day: u8, hour: u8, minute: u8) -> PrimitiveDateTime {
        PrimitiveDateTime::new(
            Date::from_calendar_date(year, Month::try_from(month).unwrap(), day).unwrap(),
            Time::from_hms(hour, minute, 0).unwrap(),
        )
    }

    #[test]
    fn parses_primary_vtec_fields() {
        let parsed = Pvtec::parse("/O.NEW.KBMX.SV.A.0002.051013T1424Z-051013T1700Z/").unwrap();

        assert_eq!(parsed.product_class(), EventClass::Operational);
        assert_eq!(parsed.action(), VtecAction::New);
        assert_eq!(parsed.office_id(), "KBMX");
        assert_eq!(parsed.phenomenon(), Phenomenon::SevereThunderstorm);
        assert_eq!(parsed.significance(), Significance::Watch);
        assert_eq!(parsed.event_tracking_number(), 2);
        assert_eq!(parsed.start_time(), VtecTime::At(dt(2005, 10, 13, 14, 24)));
        assert_eq!(parsed.end_time(), VtecTime::At(dt(2005, 10, 13, 17, 0)));
    }

    #[test]
    fn parses_all_primary_code_tables() {
        for class in EventClass::ALL {
            for action in VtecAction::ALL {
                let input = format!(
                    "/{}.{}.KOUN.TO.W.0001.240421T1200Z-240421T1800Z/",
                    class.code(),
                    action.code()
                );
                let parsed = Pvtec::parse(&input).unwrap();
                assert_eq!(parsed.product_class(), *class);
                assert_eq!(parsed.action(), *action);
            }
        }

        for phenomenon in Phenomenon::ALL {
            for significance in Significance::ALL {
                let input = format!(
                    "/O.NEW.KOUN.{}.{}.4321.240421T1200Z-240421T1800Z/",
                    phenomenon.code(),
                    significance.code()
                );
                let parsed = Pvtec::parse(&input).unwrap();
                assert_eq!(parsed.phenomenon(), *phenomenon);
                assert_eq!(parsed.significance(), *significance);
            }
        }
    }

    #[test]
    fn parses_zeroed_primary_time_groups() {
        let parsed = Pvtec::parse("/X.NEW.KPHI.FL.W.0035.070415T2000Z-000000T0000Z/").unwrap();

        assert_eq!(
            parsed.product_class(),
            EventClass::ExperimentalVtecInOperationalProduct
        );
        assert_eq!(parsed.start_time(), VtecTime::At(dt(2007, 4, 15, 20, 0)));
        assert!(parsed.end_time().is_unspecified());
        assert_eq!(parsed.end_time().to_string(), "000000T0000Z");
    }

    #[test]
    fn parses_hydrologic_vtec_fields() {
        let parsed =
            Hvtec::parse("/BKWN4.2.ER.070415T2000Z.070416T1800Z.000000T0000Z.NO/").unwrap();

        assert_eq!(parsed.nwsli(), "BKWN4");
        assert_eq!(parsed.severity(), FloodSeverity::Moderate);
        assert_eq!(parsed.immediate_cause(), ImmediateCause::ExcessiveRainfall);
        assert_eq!(parsed.begin_time(), VtecTime::At(dt(2007, 4, 15, 20, 0)));
        assert_eq!(parsed.crest_time(), VtecTime::At(dt(2007, 4, 16, 18, 0)));
        assert!(parsed.end_time().is_unspecified());
        assert_eq!(parsed.flood_record(), FloodRecord::NotExpected);
    }

    #[test]
    fn parses_all_hydrologic_code_tables() {
        for severity in FloodSeverity::ALL {
            for cause in ImmediateCause::ALL {
                for record in FloodRecord::ALL {
                    let input = format!(
                        "/ABCDE.{}.{}.240421T1200Z.240421T1300Z.240421T1400Z.{}/",
                        severity.code(),
                        cause.code(),
                        record.code()
                    );
                    let parsed = Hvtec::parse(&input).unwrap();
                    assert_eq!(parsed.severity(), *severity);
                    assert_eq!(parsed.immediate_cause(), *cause);
                    assert_eq!(parsed.flood_record(), *record);
                }
            }
        }
    }

    #[test]
    fn parses_zeroed_hydrologic_fields_for_areal_products() {
        let parsed =
            Hvtec::parse("/00000.0.MC.000000T0000Z.000000T0000Z.000000T0000Z.OO/").unwrap();

        assert_eq!(parsed.nwsli(), "00000");
        assert_eq!(parsed.severity(), FloodSeverity::NotClassified);
        assert_eq!(
            parsed.immediate_cause(),
            ImmediateCause::OtherMultipleCauses
        );
        assert!(parsed.begin_time().is_unspecified());
        assert!(parsed.crest_time().is_unspecified());
        assert!(parsed.end_time().is_unspecified());
        assert_eq!(parsed.flood_record(), FloodRecord::NotApplicable);
    }

    #[test]
    fn pairs_hydrologic_and_primary_vtec() {
        let pair = HydrologicVtecPair::parse(
            "/O.NEW.KPHI.FL.W.0035.070415T2000Z-000000T0000Z/",
            "/BKWN4.2.ER.070415T2000Z.070416T1800Z.000000T0000Z.NO/",
        )
        .unwrap();

        assert_eq!(pair.primary().phenomenon(), Phenomenon::FloodForecastPoint);
        assert_eq!(pair.hydrologic().nwsli(), "BKWN4");
    }

    #[test]
    fn rejects_non_hydrologic_pairing() {
        let error = HydrologicVtecPair::parse(
            "/O.NEW.KOUN.TO.W.0001.240421T1200Z-240421T1800Z/",
            "/ABCDE.1.ER.240421T1200Z.240421T1300Z.240421T1400Z.NO/",
        )
        .unwrap_err();

        assert_eq!(
            error.kind,
            crate::error::ErrorKind::InvalidField("h-vtec trigger phenomenon")
        );
    }

    #[test]
    fn rejects_invalid_primary_vtec_values() {
        assert!(Pvtec::parse("O.NEW.KOUN.TO.W.0001.240421T1200Z-240421T1800Z").is_err());
        assert!(Pvtec::parse("/O.NEW.KO1N.TO.W.0001.240421T1200Z-240421T1800Z/").is_err());
        assert!(Pvtec::parse("/O.NEW.KOUN.ZZ.W.0001.240421T1200Z-240421T1800Z/").is_err());
        assert!(Pvtec::parse("/O.NEW.KOUN.TO.Z.0001.240421T1200Z-240421T1800Z/").is_err());
        assert!(Pvtec::parse("/O.NEW.KOUN.TO.W.00A1.240421T1200Z-240421T1800Z/").is_err());
        assert!(Pvtec::parse("/O.NEW.KOUN.TO.W.0001.240431T1200Z-240421T1800Z/").is_err());
        assert!(Pvtec::parse("/O.NEW.KOUN.TO.W.0001.240421T1200Z240421T1800Z/").is_err());
    }

    #[test]
    fn rejects_invalid_hydrologic_vtec_values() {
        assert!(Hvtec::parse("ABCDE.1.ER.240421T1200Z.240421T1300Z.240421T1400Z.NO").is_err());
        assert!(Hvtec::parse("/ABCD.1.ER.240421T1200Z.240421T1300Z.240421T1400Z.NO/").is_err());
        assert!(Hvtec::parse("/abc12.1.ER.240421T1200Z.240421T1300Z.240421T1400Z.NO/").is_err());
        assert!(Hvtec::parse("/ABCDE.Z.ER.240421T1200Z.240421T1300Z.240421T1400Z.NO/").is_err());
        assert!(Hvtec::parse("/ABCDE.1.ZZ.240421T1200Z.240421T1300Z.240421T1400Z.NO/").is_err());
        assert!(Hvtec::parse("/ABCDE.1.ER.240421T1200Z.240421T1300Z.240431T1400Z.NO/").is_err());
        assert!(Hvtec::parse("/ABCDE.1.ER.240421T1200Z.240421T1300Z.240421T1400Z.ZZ/").is_err());
    }

    #[test]
    fn shape_detectors_match_parser() {
        assert!(looks_like_p_vtec(
            "/O.NEW.KOUN.TO.W.0001.240421T1200Z-240421T1800Z/"
        ));
        assert!(looks_like_h_vtec(
            "/ABCDE.1.ER.240421T1200Z.240421T1300Z.240421T1400Z.NO/"
        ));
        assert!(!looks_like_p_vtec("/O.NEW.KOUN.TO.W.0001.240421T1200Z/"));
        assert!(!looks_like_h_vtec("/ABCDE.1.ER.240421T1200Z.240421T1300Z/"));
    }
}
