use std::error::Error;

use nwws_rs::{NwwsContent, NwwsOiMessage, ProductSegment};

fn main() -> Result<(), Box<dyn Error>> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "all".to_owned());

    match mode.as_str() {
        "all" => {
            demo_tornado_wrapper()?;
            println!();
            demo_segmented_bulletin()?;
        }
        "tornado" => demo_tornado_wrapper()?,
        "segmented" => demo_segmented_bulletin()?,
        other => {
            eprintln!("unknown demo mode: {other}");
            eprintln!("usage: cargo run --example demo [all|tornado|segmented]");
            std::process::exit(2);
        }
    }

    Ok(())
}

fn demo_tornado_wrapper() -> Result<(), Box<dyn Error>> {
    let xml = include_str!("../tests/fixtures/nwws_oi_tornado_warning.xml");
    let message = NwwsOiMessage::parse(xml)?;
    let payload = message.payload.as_ref().expect("fixture carries payload");
    let content = NwwsContent::from_oi_message(&message)?;

    println!("=== NWWS-OI Tornado Warning Demo ===");
    println!(
        "summary: {}",
        message.summary.as_deref().unwrap_or("<no summary>")
    );
    println!(
        "wrapper: cccc={} ttaaii={} awipsid={} id={}.{}",
        payload.cccc, payload.ttaaii, payload.awips_id, payload.id.process_id, payload.id.sequence
    );
    print_content(&content);
    Ok(())
}

fn demo_segmented_bulletin() -> Result<(), Box<dyn Error>> {
    let raw = include_str!("../tests/fixtures/wmo_segmented_svs.txt");
    let content = NwwsContent::parse_bulletin(raw.as_bytes())?;

    println!("=== Segmented Raw Bulletin Demo ===");
    print_content(&content);
    Ok(())
}

fn print_content(content: &NwwsContent<'_>) {
    let bulletin = &content.bulletin;
    let product = &content.product;

    println!("frame kind: {:?}", bulletin.frame_kind);
    println!(
        "sequence: {}",
        bulletin
            .sequence_number
            .map(|value| value.to_string())
            .unwrap_or_else(|| "<none>".to_owned())
    );
    println!("wmo heading: {}", bulletin.heading);
    println!(
        "awips id: {}",
        bulletin
            .awips_id
            .map(|value| value.raw().to_owned())
            .unwrap_or_else(|| "<none>".to_owned())
    );
    println!("product family: {:?}", product.family);
    println!(
        "mnd header: {}",
        product.mnd_header.unwrap_or("<no mnd header detected>")
    );
    println!("segment count: {}", product.segments.len());

    for (index, segment) in product.segments.iter().enumerate() {
        print_segment(index + 1, segment);
    }
}

fn print_segment(index: usize, segment: &ProductSegment<'_>) {
    let ugc_codes = segment
        .ugc
        .codes
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");

    println!("segment {index}:");
    println!("  ugc raw: {}", segment.ugc.raw);
    println!("  ugc codes: {ugc_codes}");
    println!("  purge time: {}", segment.ugc.purge_time);
    println!(
        "  headline: {}",
        segment.headline.unwrap_or("<no headline>")
    );

    if segment.pvtec.is_empty() {
        println!("  p-vtec: <none>");
    } else {
        for code in &segment.pvtec {
            println!(
                "  p-vtec: {} office={} phenomenon={} significance={} etn={} start={} end={}",
                code,
                code.office_id(),
                code.phenomenon().as_str(),
                code.significance().as_str(),
                code.event_tracking_number(),
                code.start_time(),
                code.end_time()
            );
        }
    }

    if segment.hvtec.is_empty() {
        println!("  h-vtec: <none>");
    } else {
        for code in &segment.hvtec {
            println!(
                "  h-vtec: {} nwsli={} severity={} cause={} crest={} flood_record={}",
                code,
                code.nwsli(),
                code.severity().as_str(),
                code.immediate_cause().as_str(),
                code.crest_time(),
                code.flood_record().as_str()
            );
        }
    }

    if let Some(block) = segment.lat_lon {
        println!("  lat...lon points: {}", block.point_count());
        println!("  lat...lon raw: {}", block.raw().replace('\n', " "));
    } else {
        println!("  lat...lon: <none>");
    }

    if let Some(motion) = segment.time_mot_loc {
        println!(
            "  time...mot...loc: {} (time={:02}{:02}Z dir={} speed={}kt locs={})",
            motion,
            motion.hour(),
            motion.minute(),
            motion.direction_degrees(),
            motion.speed_knots(),
            motion.location_count()
        );
    } else {
        println!("  time...mot...loc: <none>");
    }

    println!("  tags: {:?}", segment.tags.tags);
}
