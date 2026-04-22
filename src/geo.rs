use core::fmt;
use core::str::SplitAsciiWhitespace;

use crate::error::{ErrorKind, ParseError, Result};

const LAT_LON_PREFIX: &str = "LAT...LON";
const TIME_MOT_LOC_PREFIX: &str = "TIME...MOT...LOC";
const MAX_LAT_LON_POINTS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeoPoint {
    latitude_hundredths: u16,
    longitude_hundredths: u32,
}

impl GeoPoint {
    pub const fn latitude_hundredths(self) -> u16 {
        self.latitude_hundredths
    }

    pub const fn longitude_hundredths(self) -> u32 {
        self.longitude_hundredths
    }

    pub fn latitude_degrees(self) -> f32 {
        f32::from(self.latitude_hundredths) / 100.0
    }

    pub fn longitude_degrees(self) -> f32 {
        self.longitude_hundredths as f32 / 100.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatLonBlock<'a> {
    raw: &'a str,
    coords: &'a str,
    point_count: u8,
}

impl<'a> LatLonBlock<'a> {
    pub fn parse(input: &'a str) -> Result<Self> {
        let raw = trim_ascii(input);
        let coords_payload = strip_required_prefix(raw, LAT_LON_PREFIX, "LAT...LON")?;
        let coords_offset =
            raw.len() - coords_payload.len() + leading_ascii_whitespace_len(coords_payload);
        let coords = trim_ascii(coords_payload);
        let point_count = validate_coordinate_pairs(
            coords,
            CoordinateContext {
                label: "LAT...LON coordinates",
                require_at_least_one: true,
                max_pairs: Some(MAX_LAT_LON_POINTS),
            },
            coords_offset,
        )?;

        Ok(Self {
            raw,
            coords,
            point_count: u8::try_from(point_count)
                .map_err(|_| ParseError::new(ErrorKind::Oversized("LAT...LON coordinates")))?,
        })
    }

    pub const fn raw(self) -> &'a str {
        self.raw
    }

    pub const fn point_count(self) -> usize {
        self.point_count as usize
    }

    pub fn points(self) -> GeoPointIter<'a> {
        GeoPointIter {
            tokens: self.coords.split_ascii_whitespace(),
            remaining: self.point_count as usize,
        }
    }
}

impl fmt::Display for LatLonBlock<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.raw)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeMotLocLine<'a> {
    raw: &'a str,
    time: ZuluTime,
    direction_degrees: u16,
    speed_knots: u8,
    locations: &'a str,
    location_count: u8,
}

impl<'a> TimeMotLocLine<'a> {
    pub fn parse(input: &'a str) -> Result<Self> {
        let raw = trim_ascii(input);
        let payload = strip_required_prefix(raw, TIME_MOT_LOC_PREFIX, "TIME...MOT...LOC")?;
        let payload_offset = raw.len() - payload.len();
        let mut scanner = TokenScanner::new(payload);

        let (time_token, time_offset) = scanner
            .next()
            .ok_or_else(|| ParseError::new(ErrorKind::MissingField("time")))?;
        let time = parse_time_token(time_token, payload_offset + time_offset)?;

        let (direction_token, direction_offset) = scanner
            .next()
            .ok_or_else(|| ParseError::new(ErrorKind::MissingField("direction")))?;
        let direction_degrees =
            parse_direction_token(direction_token, payload_offset + direction_offset)?;

        let (speed_token, speed_offset) = scanner
            .next()
            .ok_or_else(|| ParseError::new(ErrorKind::MissingField("speed")))?;
        let speed_knots = parse_speed_token(speed_token, payload_offset + speed_offset)?;

        let locations_offset = scanner.position();
        let locations_payload = &payload[locations_offset..];
        let locations_offset =
            payload_offset + locations_offset + leading_ascii_whitespace_len(locations_payload);
        let locations = trim_ascii(locations_payload);
        let location_count = validate_coordinate_pairs(
            locations,
            CoordinateContext {
                label: "TIME...MOT...LOC locations",
                require_at_least_one: true,
                max_pairs: None,
            },
            locations_offset,
        )?;

        Ok(Self {
            raw,
            time,
            direction_degrees,
            speed_knots,
            locations,
            location_count: u8::try_from(location_count)
                .map_err(|_| ParseError::new(ErrorKind::Oversized("TIME...MOT...LOC locations")))?,
        })
    }

    pub const fn raw(self) -> &'a str {
        self.raw
    }

    pub const fn time(self) -> ZuluTime {
        self.time
    }

    pub const fn hour(self) -> u8 {
        self.time.hour()
    }

    pub const fn minute(self) -> u8 {
        self.time.minute()
    }

    pub const fn direction_degrees(self) -> u16 {
        self.direction_degrees
    }

    pub const fn speed_knots(self) -> u8 {
        self.speed_knots
    }

    pub const fn location_count(self) -> usize {
        self.location_count as usize
    }

    pub fn locations(self) -> GeoPointIter<'a> {
        GeoPointIter {
            tokens: self.locations.split_ascii_whitespace(),
            remaining: self.location_count as usize,
        }
    }
}

impl fmt::Display for TimeMotLocLine<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.raw)
    }
}

pub type MotionLocation = GeoPoint;
pub type TimeMotLoc<'a> = TimeMotLocLine<'a>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZuluTime {
    hour: u8,
    minute: u8,
}

impl ZuluTime {
    pub const fn hour(self) -> u8 {
        self.hour
    }

    pub const fn minute(self) -> u8 {
        self.minute
    }
}

#[derive(Debug, Clone)]
pub struct GeoPointIter<'a> {
    tokens: SplitAsciiWhitespace<'a>,
    remaining: usize,
}

impl<'a> Iterator for GeoPointIter<'a> {
    type Item = GeoPoint;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let latitude = parse_latitude_token(self.tokens.next()?).ok()?;
        let longitude = parse_longitude_token(self.tokens.next()?).ok()?;
        self.remaining -= 1;

        Some(GeoPoint {
            latitude_hundredths: latitude,
            longitude_hundredths: longitude,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl ExactSizeIterator for GeoPointIter<'_> {}

#[derive(Debug, Clone, Copy)]
struct CoordinateContext {
    label: &'static str,
    require_at_least_one: bool,
    max_pairs: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
struct TokenScanner<'a> {
    input: &'a str,
    position: usize,
}

impl<'a> TokenScanner<'a> {
    const fn new(input: &'a str) -> Self {
        Self { input, position: 0 }
    }

    fn next(&mut self) -> Option<(&'a str, usize)> {
        let bytes = self.input.as_bytes();
        while self.position < bytes.len() && bytes[self.position].is_ascii_whitespace() {
            self.position += 1;
        }

        if self.position >= bytes.len() {
            return None;
        }

        let start = self.position;
        while self.position < bytes.len() && !bytes[self.position].is_ascii_whitespace() {
            self.position += 1;
        }

        Some((&self.input[start..self.position], start))
    }

    const fn position(self) -> usize {
        self.position
    }
}

fn strip_required_prefix<'a>(
    input: &'a str,
    prefix: &str,
    field_name: &'static str,
) -> Result<&'a str> {
    let Some(rest) = input.strip_prefix(prefix) else {
        return Err(ParseError::new(ErrorKind::InvalidField(field_name)));
    };

    if rest.is_empty() {
        return Ok(rest);
    }

    if !rest.as_bytes().first().is_some_and(u8::is_ascii_whitespace) {
        return Err(ParseError::at(
            ErrorKind::InvalidField(field_name),
            prefix.len(),
        ));
    }

    Ok(rest)
}

fn validate_coordinate_pairs(
    input: &str,
    context: CoordinateContext,
    base_offset: usize,
) -> Result<usize> {
    let mut scanner = TokenScanner::new(input);
    let mut tokens = 0usize;

    while let Some((token, offset)) = scanner.next() {
        if tokens.is_multiple_of(2) {
            parse_latitude_token_at(token, base_offset + offset)?;
        } else {
            parse_longitude_token_at(token, base_offset + offset)?;
        }
        tokens += 1;
    }

    if tokens == 0 && context.require_at_least_one {
        return Err(ParseError::at(
            ErrorKind::MissingField(context.label),
            base_offset,
        ));
    }

    if !tokens.is_multiple_of(2) {
        return Err(ParseError::at(
            ErrorKind::InvalidField(context.label),
            base_offset + input.len(),
        ));
    }

    let pairs = tokens / 2;
    if let Some(max_pairs) = context.max_pairs
        && pairs > max_pairs
    {
        return Err(ParseError::new(ErrorKind::Oversized(context.label)));
    }

    Ok(pairs)
}

fn parse_time_token(token: &str, offset: usize) -> Result<ZuluTime> {
    let bytes = token.as_bytes();
    if bytes.len() != 5 || bytes[4] != b'Z' || !bytes[..4].iter().all(u8::is_ascii_digit) {
        return Err(ParseError::at(ErrorKind::InvalidField("time"), offset));
    }

    let hour = ascii_dec(bytes[0], bytes[1]);
    let minute = ascii_dec(bytes[2], bytes[3]);
    if hour > 23 || minute > 59 {
        return Err(ParseError::at(ErrorKind::InvalidField("time"), offset));
    }

    Ok(ZuluTime { hour, minute })
}

fn parse_direction_token(token: &str, offset: usize) -> Result<u16> {
    let bytes = token.as_bytes();
    if bytes.len() != 6 || !bytes[..3].iter().all(u8::is_ascii_digit) || &bytes[3..] != b"DEG" {
        return Err(ParseError::at(ErrorKind::InvalidField("direction"), offset));
    }

    let direction = u16::from(ascii_dec(bytes[0], bytes[1])) * 10 + u16::from(bytes[2] - b'0');
    if direction > 360 {
        return Err(ParseError::at(ErrorKind::InvalidField("direction"), offset));
    }

    Ok(direction)
}

fn parse_speed_token(token: &str, offset: usize) -> Result<u8> {
    let bytes = token.as_bytes();
    if !(3..=4).contains(&bytes.len()) || &bytes[bytes.len() - 2..] != b"KT" {
        return Err(ParseError::at(ErrorKind::InvalidField("speed"), offset));
    }

    let digits = &bytes[..bytes.len() - 2];
    if digits.is_empty()
        || digits.len() > 2
        || !digits.iter().all(u8::is_ascii_digit)
        || (digits.len() == 2 && digits[0] == b'0')
    {
        return Err(ParseError::at(ErrorKind::InvalidField("speed"), offset));
    }

    Ok(parse_ascii_number(digits) as u8)
}

fn parse_latitude_token(token: &str) -> Result<u16> {
    parse_latitude_token_at(token, 0)
}

fn parse_latitude_token_at(token: &str, offset: usize) -> Result<u16> {
    let bytes = token.as_bytes();
    if bytes.len() != 4 || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(ParseError::at(ErrorKind::InvalidField("latitude"), offset));
    }

    let latitude = parse_ascii_number(bytes);
    if latitude > 9000 {
        return Err(ParseError::at(ErrorKind::InvalidField("latitude"), offset));
    }

    Ok(latitude as u16)
}

fn parse_longitude_token(token: &str) -> Result<u32> {
    parse_longitude_token_at(token, 0)
}

fn parse_longitude_token_at(token: &str, offset: usize) -> Result<u32> {
    let bytes = token.as_bytes();
    if !(4..=5).contains(&bytes.len()) || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(ParseError::at(ErrorKind::InvalidField("longitude"), offset));
    }

    let longitude = parse_ascii_number(bytes);
    if longitude > 18100 {
        return Err(ParseError::at(ErrorKind::InvalidField("longitude"), offset));
    }

    Ok(longitude)
}

fn trim_ascii(input: &str) -> &str {
    input.trim_matches(|ch: char| ch.is_ascii_whitespace())
}

fn leading_ascii_whitespace_len(input: &str) -> usize {
    input
        .as_bytes()
        .iter()
        .take_while(|byte| byte.is_ascii_whitespace())
        .count()
}

fn ascii_dec(tens: u8, ones: u8) -> u8 {
    (tens - b'0') * 10 + (ones - b'0')
}

fn parse_ascii_number(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |value, digit| (value * 10) + u32::from(*digit - b'0'))
}

#[cfg(test)]
mod tests {
    use super::{GeoPoint, LatLonBlock, TimeMotLocLine, ZuluTime};

    #[test]
    fn parses_multiline_lat_lon_block() {
        let block = LatLonBlock::parse(
            "LAT...LON 4896 10015 4789 10017 4787 9995 4842 9987\n\
             4842 9955 4897 9958",
        )
        .unwrap();

        assert_eq!(block.point_count(), 6);

        let points: Vec<_> = block.points().collect();
        assert_eq!(
            points,
            vec![
                GeoPoint {
                    latitude_hundredths: 4896,
                    longitude_hundredths: 10015,
                },
                GeoPoint {
                    latitude_hundredths: 4789,
                    longitude_hundredths: 10017,
                },
                GeoPoint {
                    latitude_hundredths: 4787,
                    longitude_hundredths: 9995,
                },
                GeoPoint {
                    latitude_hundredths: 4842,
                    longitude_hundredths: 9987,
                },
                GeoPoint {
                    latitude_hundredths: 4842,
                    longitude_hundredths: 9955,
                },
                GeoPoint {
                    latitude_hundredths: 4897,
                    longitude_hundredths: 9958,
                },
            ]
        );
        assert_eq!(points[0].latitude_degrees(), 48.96);
        assert_eq!(points[0].longitude_degrees(), 100.15);
    }

    #[test]
    fn accepts_east_longitude_shape() {
        let block =
            LatLonBlock::parse("LAT...LON 1360 14509 1371 14495 1348 14463 1325 14492").unwrap();

        let points: Vec<_> = block.points().collect();
        assert_eq!(points.len(), 4);
        assert_eq!(points[0].longitude_hundredths(), 14509);
    }

    #[test]
    fn rejects_bad_lat_lon_shapes() {
        assert!(LatLonBlock::parse("LAT...LON").is_err());
        assert!(LatLonBlock::parse("LAT...LON 3480").is_err());
        assert!(LatLonBlock::parse("LAT...LON 348A 10318").is_err());
        assert!(LatLonBlock::parse("LAT...LON 3480 18101").is_err());
        assert!(LatLonBlock::parse("LAT...LON 3480 99999").is_err());
        assert!(LatLonBlock::parse("LAT...LON3480 10318").is_err());
    }

    #[test]
    fn enforces_lat_lon_point_limit() {
        let input = "LAT...LON 3000 9000 3001 9001 3002 9002 3003 9003 3004 9004 \
3005 9005 3006 9006 3007 9007 3008 9008 3009 9009 3010 9010 3011 9011 \
3012 9012 3013 9013 3014 9014 3015 9015 3016 9016 3017 9017 3018 9018 \
3019 9019 3020 9020";
        assert!(LatLonBlock::parse(input).is_err());
    }

    #[test]
    fn parses_time_mot_loc_with_line_locations() {
        let line =
            TimeMotLocLine::parse("TIME...MOT...LOC 2113Z 345DEG 4KT 2760 8211 2724 8198").unwrap();

        assert_eq!(
            line.time(),
            ZuluTime {
                hour: 21,
                minute: 13
            }
        );
        assert_eq!(line.hour(), 21);
        assert_eq!(line.minute(), 13);
        assert_eq!(line.direction_degrees(), 345);
        assert_eq!(line.speed_knots(), 4);
        assert_eq!(line.location_count(), 2);

        let points: Vec<_> = line.locations().collect();
        assert_eq!(
            points,
            vec![
                GeoPoint {
                    latitude_hundredths: 2760,
                    longitude_hundredths: 8211,
                },
                GeoPoint {
                    latitude_hundredths: 2724,
                    longitude_hundredths: 8198,
                },
            ]
        );
    }

    #[test]
    fn parses_stationary_motion() {
        let line = TimeMotLocLine::parse("TIME...MOT...LOC 1959Z 254DEG 0KT 3253 11464").unwrap();
        assert_eq!(line.direction_degrees(), 254);
        assert_eq!(line.speed_knots(), 0);
        assert_eq!(
            line.locations().next(),
            Some(GeoPoint {
                latitude_hundredths: 3253,
                longitude_hundredths: 11464,
            })
        );
    }

    #[test]
    fn rejects_bad_time_mot_loc_shapes() {
        assert!(TimeMotLocLine::parse("TIME...MOT...LOC").is_err());
        assert!(TimeMotLocLine::parse("TIME...MOT...LOC 2460Z 004DEG 9KT 3480 10318").is_err());
        assert!(TimeMotLocLine::parse("TIME...MOT...LOC 0128Z 361DEG 9KT 3480 10318").is_err());
        assert!(TimeMotLocLine::parse("TIME...MOT...LOC 0128Z 004DEG 09KT 3480 10318").is_err());
        assert!(TimeMotLocLine::parse("TIME...MOT...LOC 0128Z 004DEG 9KT 3480").is_err());
        assert!(TimeMotLocLine::parse("TIME...MOT...LOC0128Z 004DEG 9KT 3480 10318").is_err());
    }
}
