use std::env;
use std::fs;
use std::path::Path;

use nwws_rs::geo::GeoPoint;
use nwws_rs::product::{NwwsContent, SegmentTag};
use nwws_rs::ugc::{UgcCode, UgcKind};
use serde::Serialize;

fn main() {
    let mut args = env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: cargo run --example summary -- <path>");
        std::process::exit(2);
    };

    let text = fs::read_to_string(&path).unwrap_or_else(|err| {
        eprintln!("failed to read {path}: {err}");
        std::process::exit(1);
    });

    let summary = if looks_like_oi(&text) {
        let message = nwws_rs::NwwsOiMessage::parse(&text).unwrap_or_else(|err| {
            eprintln!("failed to parse NWWS-OI payload in {path}: {err}");
            std::process::exit(1);
        });
        let content = NwwsContent::from_oi_message(&message).unwrap_or_else(|err| {
            eprintln!("failed to parse embedded bulletin in {path}: {err}");
            std::process::exit(1);
        });
        build_summary(
            "nwws-oi",
            &content,
            message.payload.as_ref().map(|payload| WrapperSummary {
                cccc: payload.cccc.clone(),
                ttaaii: payload.ttaaii.clone(),
                awips_id: payload.awips_id.clone(),
                id: format!("{}.{}", payload.id.process_id, payload.id.sequence),
            }),
        )
    } else {
        let content = NwwsContent::parse_bulletin(text.as_bytes()).unwrap_or_else(|err| {
            eprintln!("failed to parse bulletin in {path}: {err}");
            std::process::exit(1);
        });
        build_summary("wmo", &content, None)
    };

    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
}

fn looks_like_oi(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with('<')
        && (trimmed.contains("xmlns='nwws-oi'") || trimmed.contains("xmlns=\"nwws-oi\""))
}

fn build_summary(
    input_kind: &'static str,
    content: &NwwsContent<'_>,
    wrapper: Option<WrapperSummary>,
) -> Summary {
    let bulletin = &content.bulletin;
    let office = bulletin.heading.cccc();
    let segments = content
        .product
        .segments
        .iter()
        .map(|segment| SegmentSummary::from_segment(segment, office))
        .collect::<Vec<_>>();

    Summary {
        input_kind,
        path_kind: format!("{:?}", bulletin.frame_kind).to_ascii_lowercase(),
        sequence_number: bulletin.sequence_number,
        ttaaii: bulletin.heading.ttaaii().to_owned(),
        cccc: bulletin.heading.cccc().to_owned(),
        yygggg: bulletin.heading.yygggg().to_owned(),
        bbb: bulletin.heading.bbb().map(str::to_owned),
        awips_id: bulletin
            .awips_id
            .as_ref()
            .map(|value| value.raw().to_owned()),
        family: format!("{:?}", content.product.family),
        structured_segment_count: segments.len(),
        segments,
        wrapper,
    }
}

#[derive(Debug, Serialize)]
struct Summary {
    input_kind: &'static str,
    path_kind: String,
    sequence_number: Option<u16>,
    ttaaii: String,
    cccc: String,
    yygggg: String,
    bbb: Option<String>,
    awips_id: Option<String>,
    family: String,
    structured_segment_count: usize,
    segments: Vec<SegmentSummary>,
    wrapper: Option<WrapperSummary>,
}

#[derive(Debug, Serialize)]
struct WrapperSummary {
    cccc: String,
    ttaaii: String,
    awips_id: String,
    id: String,
}

#[derive(Debug, Serialize)]
struct SegmentSummary {
    ugcs: Vec<String>,
    pvtec: Vec<String>,
    hvtec: Vec<String>,
    tornado_tag: Option<&'static str>,
    flash_flood_observed: bool,
    flash_flood_emergency: bool,
    hail_inches: Option<f32>,
    wind_mph: Option<u16>,
    damage_threat: Option<String>,
    lat_lon: Option<Vec<Point>>,
    time_mot_loc: Option<TimeMotLocSummary>,
}

impl SegmentSummary {
    fn from_segment(segment: &nwws_rs::ProductSegment<'_>, office: &str) -> Self {
        let mut tornado_tag = None;
        let mut flash_flood_observed = false;
        let mut explicit_flash_flood_emergency = false;
        let mut hail_inches = None;
        let mut wind_mph = None;
        let mut raw_damage_threat = None;

        for tag in &segment.tags.tags {
            match tag {
                SegmentTag::TornadoObserved => tornado_tag = Some("OBSERVED"),
                SegmentTag::TornadoRadarIndicated => tornado_tag = Some("RADAR INDICATED"),
                SegmentTag::TornadoPossible => tornado_tag = Some("POSSIBLE"),
                SegmentTag::FlashFloodObserved => flash_flood_observed = true,
                SegmentTag::FlashFloodEmergency => explicit_flash_flood_emergency = true,
                SegmentTag::HailInches(value) => hail_inches = Some(round2(*value)),
                SegmentTag::WindMph(value) => wind_mph = Some(*value),
                SegmentTag::DamageThreat(value) => raw_damage_threat = Some((*value).to_owned()),
            }
        }

        let is_flash_flood_product = segment
            .pvtec
            .iter()
            .any(|pvtec| pvtec.phenomenon().as_str() == "FF");
        let flash_flood_emergency = explicit_flash_flood_emergency
            || (is_flash_flood_product && raw_damage_threat.as_deref() == Some("CATASTROPHIC"));
        let damage_threat = if is_flash_flood_product {
            None
        } else {
            raw_damage_threat
        };

        Self {
            ugcs: expand_ugc_codes(&segment.ugc),
            pvtec: segment
                .pvtec
                .iter()
                .map(|value| value.raw().to_owned())
                .collect(),
            hvtec: segment
                .hvtec
                .iter()
                .map(|value| value.raw().to_owned())
                .collect(),
            tornado_tag,
            flash_flood_observed,
            flash_flood_emergency,
            hail_inches,
            wind_mph,
            damage_threat,
            lat_lon: segment.lat_lon.as_ref().map(|block| {
                block
                    .points()
                    .map(|point| Point::from_geo_point(point, office))
                    .collect()
            }),
            time_mot_loc: segment.time_mot_loc.as_ref().map(|line| TimeMotLocSummary {
                time: format!("{:02}{:02}Z", line.hour(), line.minute()),
                direction_degrees: line.direction_degrees(),
                speed_knots: line.speed_knots(),
                locations: line
                    .locations()
                    .map(|point| Point::from_geo_point(point, office))
                    .collect(),
            }),
        }
    }
}

#[derive(Debug, Serialize)]
struct TimeMotLocSummary {
    time: String,
    direction_degrees: u16,
    speed_knots: u8,
    locations: Vec<Point>,
}

#[derive(Debug, Serialize)]
struct Point {
    lat: f32,
    lon: f32,
}

impl Point {
    fn from_geo_point(point: GeoPoint, office: &str) -> Self {
        Self {
            lat: round2(point.latitude_degrees()),
            lon: round2(normalize_longitude(point.longitude_degrees(), office)),
        }
    }
}

fn expand_ugc_codes(ugc: &nwws_rs::UgcString<'_>) -> Vec<String> {
    let mut values = Vec::new();

    for code in ugc.codes() {
        match code {
            UgcCode::Single { .. } | UgcCode::All { .. } | UgcCode::Unspecified { .. } => {
                values.push(code.to_string());
            }
            UgcCode::Range {
                state,
                kind,
                start,
                end,
            } => {
                let kind = ugc_kind_char(*kind);
                for number in *start..=*end {
                    values.push(format!("{state}{kind}{number:03}"));
                }
            }
        }
    }

    values
}

fn ugc_kind_char(kind: UgcKind) -> char {
    match kind {
        UgcKind::County => 'C',
        UgcKind::Zone => 'Z',
    }
}

fn normalize_longitude(raw: f32, office: &str) -> f32 {
    let mut longitude = raw;
    if longitude < 40.0 {
        longitude += 100.0;
    }
    if office == "PGUM" {
        longitude
    } else {
        -longitude
    }
}

fn round2(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

#[allow(dead_code)]
fn _path_exists(path: &Path) -> bool {
    path.exists()
}
