use nwws_rs::error::ErrorKind;
use nwws_rs::header::{AwipsId, WmoHeading};
use nwws_rs::oi::NwwsOiMessage;
use nwws_rs::product::SegmentTag;
use nwws_rs::stream::WmoStreamScanner;
use nwws_rs::wmo::{WmoFrameKind, WmoMessage};

fn frame_with_wmo_separators(bulletin: &str) -> String {
    let bulletin = bulletin.lines().collect::<Vec<_>>().join("\r\r\n");
    format!("\u{1}\r\r\n{bulletin}\r\r\n\u{3}")
}

#[test]
fn parses_fixture_nwws_oi_message() {
    let xml = include_str!("fixtures/nwws_oi_example.xml");
    let message = NwwsOiMessage::parse(xml).unwrap();
    message.validate().unwrap();

    let payload = message.payload.unwrap();
    let bulletin = payload.parse_bulletin().unwrap();
    assert_eq!(bulletin.sequence_number, Some(111));
    assert_eq!(
        bulletin.heading,
        WmoHeading::parse("SRUS83 KARX 250220").unwrap()
    );
    assert_eq!(bulletin.awips_id, Some(AwipsId::parse("RR8ARX").unwrap()));
}

#[test]
fn parses_framed_fixture_and_scanner() {
    let framed = frame_with_wmo_separators(include_str!("fixtures/wmo_bulletin.txt"));
    let scanner = WmoStreamScanner::new();
    let outcome = scanner.scan_next(framed.as_bytes()).unwrap();
    let chunk = outcome.chunk.unwrap();
    let message = chunk.parse().unwrap();

    assert_eq!(message.frame_kind, WmoFrameKind::Framed);
    assert_eq!(message.sequence_number, Some(111));
    assert_eq!(message.heading.ttaaii(), "NOUS41");
    assert_eq!(message.heading.bbb(), Some("AAA"));
    assert_eq!(message.awips_id.unwrap().raw(), "PNSXXX");
}

#[test]
fn rejects_mismatched_payload_metadata() {
    let xml = include_str!("fixtures/nwws_oi_example.xml")
        .replace("awipsid='RR8ARX'", "awipsid='AFDLWX'");
    let message = NwwsOiMessage::parse(&xml).unwrap();
    let payload = message.payload.unwrap();
    assert!(payload.validate().is_err());
}

#[test]
fn parses_bare_bulletin_without_awips_line() {
    let input = "123\nSXUS01 KWBC 010600\nFirst body line\nSecond body line";
    let message = WmoMessage::parse_str(input).unwrap();
    assert_eq!(message.frame_kind, WmoFrameKind::Bare);
    assert_eq!(message.sequence_number, Some(123));
    assert!(message.awips_id.is_none());
    assert_eq!(message.body, "First body line\nSecond body line");
}

#[test]
fn parses_ugc_vtec_warning_fixture_with_geometry_lines() {
    let bulletin = include_str!("fixtures/wmo_tornado_warning.txt");
    let message = WmoMessage::parse_str(bulletin).unwrap();
    let product = nwws_rs::NwsProduct::parse(&message).unwrap();

    assert_eq!(message.frame_kind, WmoFrameKind::Bare);
    assert_eq!(message.sequence_number, Some(401));
    assert_eq!(
        message.heading,
        WmoHeading::parse("WUUS53 KLOT 211600").unwrap()
    );
    assert_eq!(message.awips_id, Some(AwipsId::parse("TORLOT").unwrap()));
    assert!(message.body.starts_with("ILC031-043-197-211630-"));
    assert!(
        message
            .body
            .contains("/O.NEW.KLOT.TO.W.0001.260421T1600Z-260421T1630Z/")
    );
    assert!(
        message
            .body
            .contains("LAT...LON 4215 8850 4203 8820 4194 8810")
    );
    assert!(
        message
            .body
            .contains("TIME...MOT...LOC 1600Z 265DEG 31KT 4208 8837")
    );
    assert!(message.body.contains("TORNADO...RADAR INDICATED"));
    assert_eq!(product.segments.len(), 1);
    assert!(
        product.segments[0]
            .tags
            .tags
            .contains(&SegmentTag::HailInches(1.0))
    );
}

#[test]
fn validates_nwws_oi_warning_fixture_against_embedded_bulletin() {
    let xml = include_str!("fixtures/nwws_oi_tornado_warning.xml");
    let message = NwwsOiMessage::parse(xml).unwrap();
    message.validate().unwrap();

    let payload = message.payload.unwrap();
    let bulletin = payload.parse_bulletin().unwrap();
    assert_eq!(payload.id.process_id, 41001);
    assert_eq!(payload.id.sequence, 17);
    assert_eq!(bulletin.heading.ttaaii(), "WUUS53");
    assert_eq!(bulletin.heading.cccc(), "KLOT");
    assert_eq!(bulletin.awips_id.unwrap().raw(), "TORLOT");
    assert!(
        bulletin
            .body
            .contains("LAT...LON 4215 8850 4203 8820 4194 8810")
    );
}

#[test]
fn parses_segmented_warning_fixture_through_scanner() {
    let framed = frame_with_wmo_separators(include_str!("fixtures/wmo_segmented_svs.txt"));
    let scanner = WmoStreamScanner::new();
    let outcome = scanner.scan_next(framed.as_bytes()).unwrap();
    let chunk = outcome.chunk.unwrap();
    let message = chunk.parse().unwrap();

    assert_eq!(message.frame_kind, WmoFrameKind::Framed);
    assert_eq!(message.sequence_number, Some(402));
    assert_eq!(
        message.heading,
        WmoHeading::parse("WWUS73 KLOT 211620").unwrap()
    );
    assert_eq!(message.awips_id, Some(AwipsId::parse("SVSLOT").unwrap()));
    assert_eq!(message.body.matches("$$").count(), 2);
    assert!(
        message
            .body
            .contains("/O.CON.KLOT.TO.W.0001.000000T0000Z-260421T1630Z/")
    );
    assert!(
        message
            .body
            .contains("/O.NEW.KLOT.SV.W.0002.260421T1620Z-260421T1700Z/")
    );
    assert!(
        message
            .body
            .contains("LAT...LON 4220 8850 4214 8825 4207 8797 4217 8788 4226 8829")
    );
    assert!(
        message
            .body
            .contains("LAT...LON 4208 8840 4201 8808 4215 8798 4220 8830")
    );
}

#[test]
fn validates_segmented_nwws_oi_fixture() {
    let xml = include_str!("fixtures/nwws_oi_segmented_svs.xml");
    let message = NwwsOiMessage::parse(xml).unwrap();
    message.validate().unwrap();

    let payload = message.payload.unwrap();
    let bulletin = payload.parse_bulletin().unwrap();
    assert_eq!(payload.id.process_id, 41001);
    assert_eq!(payload.id.sequence, 18);
    assert_eq!(bulletin.awips_id.unwrap().raw(), "SVSLOT");
    assert_eq!(bulletin.body.matches("$$").count(), 2);
    assert!(bulletin.body.contains("Severe Weather Statement"));
    assert!(bulletin.body.contains("Severe Thunderstorm Warning"));
}

#[test]
fn invalid_awips_variant_stays_in_body_and_breaks_wrapper_validation() {
    let bare = include_str!("fixtures/wmo_tornado_warning.txt").replace("TORLOT\n", "TOR-LOT\n");
    let message = WmoMessage::parse_str(&bare).unwrap();
    assert!(message.awips_id.is_none());
    assert!(message.body.starts_with("TOR-LOT\nILC031-043-197-211630-"));

    let xml = include_str!("fixtures/nwws_oi_tornado_warning.xml").replace("TORLOT\n", "TOR-LOT\n");
    let payload = NwwsOiMessage::parse(&xml).unwrap().payload.unwrap();
    let err = payload.validate().unwrap_err();
    assert_eq!(err.kind, ErrorKind::Mismatch("awips id"));
}

#[test]
fn rejects_warning_wrapper_with_ttaaii_mismatch() {
    let xml = include_str!("fixtures/nwws_oi_tornado_warning.xml")
        .replace("ttaaii='WUUS53'", "ttaaii='WUUS54'");
    let payload = NwwsOiMessage::parse(&xml).unwrap().payload.unwrap();
    let err = payload.validate().unwrap_err();
    assert_eq!(err.kind, ErrorKind::Mismatch("ttaaii"));
}

#[test]
fn rejects_warning_wrapper_with_cccc_mismatch() {
    let xml =
        include_str!("fixtures/nwws_oi_tornado_warning.xml").replace("cccc='KLOT'", "cccc='KLSX'");
    let payload = NwwsOiMessage::parse(&xml).unwrap().payload.unwrap();
    let err = payload.validate().unwrap_err();
    assert_eq!(err.kind, ErrorKind::Mismatch("cccc"));
}
