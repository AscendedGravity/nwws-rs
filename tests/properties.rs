use nwws_rs::oi::NwwsOiMessage;
use nwws_rs::stream::WmoStreamScanner;
use nwws_rs::vtec::{
    EventClass, FloodRecord, FloodSeverity, Hvtec, ImmediateCause, Phenomenon, Pvtec, Significance,
    VtecAction, looks_like_h_vtec, looks_like_p_vtec,
};
use nwws_rs::wmo::{WmoFrameKind, WmoMessage};
use proptest::prelude::*;
use proptest::sample::select;
use proptest::string::string_regex;

fn ttaaii_strategy() -> impl Strategy<Value = String> {
    string_regex("[A-Z]{4}[0-9]{2}").unwrap()
}

fn cccc_strategy() -> impl Strategy<Value = String> {
    string_regex("[A-Z]{4}").unwrap()
}

fn yygggg_strategy() -> impl Strategy<Value = String> {
    string_regex("[0-9]{6}").unwrap()
}

fn bbb_strategy() -> impl Strategy<Value = String> {
    string_regex("[A-Z0-9]{3}").unwrap()
}

fn awips_strategy() -> impl Strategy<Value = String> {
    string_regex("[A-Z0-9]{5,6}").unwrap()
}

fn office_strategy() -> impl Strategy<Value = String> {
    string_regex("[A-Z]{4}").unwrap()
}

fn nwsli_strategy() -> impl Strategy<Value = String> {
    string_regex("[A-Z0-9]{5}").unwrap()
}

fn body_line_strategy() -> impl Strategy<Value = String> {
    string_regex("[A-Z0-9 ./:,*-]{0,40}")
        .unwrap()
        .prop_map(|suffix| format!("LINE {suffix}"))
}

fn text_without_controls() -> impl Strategy<Value = String> {
    string_regex("[A-Za-z0-9 ]{0,24}").unwrap()
}

fn separator_strategy() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("\n"), Just("\r\n"), Just("\r\r\n")]
}

fn time_group_strategy() -> impl Strategy<Value = String> {
    (0u8..100, 1u8..13, 1u8..29, 0u8..24, 0u8..60).prop_map(|(year, month, day, hour, minute)| {
        format!("{year:02}{month:02}{day:02}T{hour:02}{minute:02}Z")
    })
}

fn optional_time_group_strategy() -> impl Strategy<Value = String> {
    prop_oneof![Just("000000T0000Z".to_owned()), time_group_strategy()]
}

fn build_bare_bulletin(
    separator: &str,
    sequence: u16,
    heading: &str,
    awips: Option<&str>,
    body_lines: &[String],
) -> String {
    let mut lines = vec![format!("{sequence:03}"), heading.to_owned()];
    if let Some(awips) = awips {
        lines.push(awips.to_owned());
    }
    lines.extend(body_lines.iter().cloned());
    lines.join(separator)
}

fn build_framed_bulletin(body_lines: &[String]) -> String {
    let mut lines = vec![
        "111".to_owned(),
        "NOUS41 KWBC 201530 AAA".to_owned(),
        "PNSXXX".to_owned(),
    ];
    lines.extend(body_lines.iter().cloned());
    format!("\u{1}\r\r\n{}\r\r\n\u{3}", lines.join("\r\r\n"))
}

proptest! {
    #[test]
    fn parses_generated_bare_bulletins_across_supported_separators(
        separator in separator_strategy(),
        sequence in 0u16..1000,
        ttaaii in ttaaii_strategy(),
        cccc in cccc_strategy(),
        yygggg in yygggg_strategy(),
        bbb in prop::option::of(bbb_strategy()),
        awips in prop::option::of(awips_strategy()),
        body_lines in prop::collection::vec(body_line_strategy(), 1..6),
    ) {
        let mut heading = format!("{ttaaii} {cccc} {yygggg}");
        if let Some(ref bbb) = bbb {
            heading.push(' ');
            heading.push_str(bbb);
        }

        let bulletin = build_bare_bulletin(
            separator,
            sequence,
            &heading,
            awips.as_deref(),
            &body_lines,
        );
        let message = WmoMessage::parse_str(&bulletin).unwrap();

        prop_assert_eq!(message.frame_kind, WmoFrameKind::Bare);
        prop_assert_eq!(message.sequence_number, Some(sequence));
        prop_assert_eq!(message.heading.ttaaii(), ttaaii.as_str());
        prop_assert_eq!(message.heading.cccc(), cccc.as_str());
        prop_assert_eq!(message.heading.yygggg(), yygggg.as_str());
        prop_assert_eq!(message.heading.bbb(), bbb.as_deref());
        prop_assert_eq!(message.awips_id.as_ref().map(|awips| awips.raw()), awips.as_deref());
        prop_assert_eq!(message.body, body_lines.join(separator));
    }

    #[test]
    fn scanner_finds_generated_framed_messages_after_safe_junk(
        junk in text_without_controls(),
        tail in text_without_controls(),
        body_lines in prop::collection::vec(body_line_strategy(), 1..5),
    ) {
        let framed = build_framed_bulletin(&body_lines);
        let mut bytes = junk.clone().into_bytes();
        bytes.extend_from_slice(framed.as_bytes());
        bytes.extend_from_slice(tail.as_bytes());

        let scanner = WmoStreamScanner::new();
        let outcome = scanner.scan_next(&bytes).unwrap();
        let chunk = outcome.chunk.unwrap();
        let message = chunk.parse().unwrap();

        prop_assert_eq!(outcome.junk_prefix, junk.len());
        prop_assert_eq!(chunk.range.start, junk.len());
        prop_assert_eq!(message.frame_kind, WmoFrameKind::Framed);
        prop_assert_eq!(message.sequence_number, Some(111));
        prop_assert_eq!(message.awips_id.unwrap().raw(), "PNSXXX");
        prop_assert_eq!(message.body, body_lines.join("\r\r\n"));
        prop_assert_eq!(outcome.pending, tail.as_bytes());
    }

    #[test]
    fn scanner_iterates_generated_multiple_messages(
        junk in text_without_controls(),
        middle_junk in text_without_controls(),
        tail in text_without_controls(),
        first_body in prop::collection::vec(body_line_strategy(), 1..5),
        second_body in prop::collection::vec(body_line_strategy(), 1..5),
    ) {
        let first = build_framed_bulletin(&first_body);
        let second = build_framed_bulletin(&second_body);
        let input = format!("{junk}{first}{middle_junk}{second}{tail}");

        let scanner = WmoStreamScanner::new();
        let mut iter = scanner.iter(input.as_bytes());
        let first_chunk = iter.next().unwrap().unwrap();
        let second_chunk = iter.next().unwrap().unwrap();
        prop_assert!(iter.next().is_none());

        let first_message = first_chunk.parse().unwrap();
        let second_message = second_chunk.parse().unwrap();
        prop_assert_eq!(first_message.body, first_body.join("\r\r\n"));
        prop_assert_eq!(second_message.body, second_body.join("\r\r\n"));
        prop_assert_eq!(first_chunk.range.start, junk.len());
        prop_assert_eq!(
            second_chunk.range.start,
            junk.len() + first.len() + middle_junk.len()
        );
        prop_assert!(iter.pending().is_empty());
    }

    #[test]
    fn generated_primary_vtec_roundtrips(
        event_class in select(EventClass::ALL),
        action in select(VtecAction::ALL),
        office in office_strategy(),
        phenomenon in select(Phenomenon::ALL),
        significance in select(Significance::ALL),
        event_tracking_number in 0u16..10000,
        start_time in time_group_strategy(),
        end_time in optional_time_group_strategy(),
    ) {
        let input = format!(
            "/{}.{}.{}.{}.{}.{:04}.{}-{}/",
            event_class.code(),
            action.code(),
            office,
            phenomenon.code(),
            significance.code(),
            event_tracking_number,
            start_time,
            end_time,
        );

        let parsed = Pvtec::parse(&input).unwrap();
        prop_assert!(looks_like_p_vtec(&input));
        prop_assert_eq!(parsed.to_string(), input);
        prop_assert_eq!(parsed.product_class(), event_class);
        prop_assert_eq!(parsed.action(), action);
        prop_assert_eq!(parsed.office_id(), office);
        prop_assert_eq!(parsed.phenomenon(), phenomenon);
        prop_assert_eq!(parsed.significance(), significance);
        prop_assert_eq!(parsed.event_tracking_number(), event_tracking_number);
    }

    #[test]
    fn generated_hydrologic_vtec_roundtrips(
        nwsli in nwsli_strategy(),
        severity in select(FloodSeverity::ALL),
        cause in select(ImmediateCause::ALL),
        begin_time in optional_time_group_strategy(),
        crest_time in optional_time_group_strategy(),
        end_time in optional_time_group_strategy(),
        record in select(FloodRecord::ALL),
    ) {
        let input = format!(
            "/{}.{}.{}.{}.{}.{}.{}/",
            nwsli,
            severity.code(),
            cause.code(),
            begin_time,
            crest_time,
            end_time,
            record.code(),
        );

        let parsed = Hvtec::parse(&input).unwrap();
        prop_assert!(looks_like_h_vtec(&input));
        prop_assert_eq!(parsed.to_string(), input);
        prop_assert_eq!(parsed.nwsli(), nwsli);
        prop_assert_eq!(parsed.severity(), severity);
        prop_assert_eq!(parsed.immediate_cause(), cause);
        prop_assert_eq!(parsed.flood_record(), record);
    }

    #[test]
    fn generated_nwws_oi_payload_validates_embedded_bulletin(
        sequence in 0u16..1000,
        ttaaii in ttaaii_strategy(),
        cccc in cccc_strategy(),
        yygggg in yygggg_strategy(),
        awips in awips_strategy(),
        body_lines in prop::collection::vec(body_line_strategy(), 1..6),
        process_id in 1u32..100000,
        message_sequence in 1u64..1000,
    ) {
        let bulletin = build_bare_bulletin(
            "\n",
            sequence,
            &format!("{ttaaii} {cccc} {yygggg}"),
            Some(&awips),
            &body_lines,
        );
        let xml = format!(
            "<message type='groupchat'><x xmlns='nwws-oi' cccc='{cccc}' ttaaii='{ttaaii}' issue='2026-04-21T16:00:00Z' awipsid='{awips}' id='{process_id}.{message_sequence}'>{bulletin}</x></message>"
        );

        let message = NwwsOiMessage::parse(&xml).unwrap();
        message.validate().unwrap();

        let payload = message.payload.unwrap();
        let parsed = payload.parse_bulletin().unwrap();
        prop_assert_eq!(parsed.sequence_number, Some(sequence));
        prop_assert_eq!(parsed.heading.ttaaii(), ttaaii.as_str());
        prop_assert_eq!(parsed.heading.cccc(), cccc.as_str());
        prop_assert_eq!(parsed.heading.yygggg(), yygggg.as_str());
        prop_assert_eq!(parsed.awips_id.unwrap().raw(), awips.as_str());
        prop_assert_eq!(parsed.body, body_lines.join("\n"));
    }
}
