use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use nwws_rs::{NwsProduct, NwwsContent, NwwsOiMessage, WmoMessage, WmoStreamScanner};

struct WmoCase {
    name: &'static str,
    framed: String,
}

struct OiCase {
    name: &'static str,
    xml: &'static str,
}

fn frame_with_wmo_separators(bulletin: &str) -> String {
    let bulletin = bulletin.lines().collect::<Vec<_>>().join("\r\r\n");
    format!("\u{1}\r\r\n{bulletin}\r\r\n\u{3}")
}

fn bench_wmo(c: &mut Criterion) {
    let cases = [
        WmoCase {
            name: "basic",
            framed: "\u{1}\r\r\n111\r\r\nNOUS41 KWBC 201530 AAA\r\r\nPNSXXX\r\r\nHeadline\r\r\nBody line 1\r\r\nBody line 2\r\r\n\u{3}"
                .to_owned(),
        },
        WmoCase {
            name: "tornado_warning",
            framed: frame_with_wmo_separators(include_str!("../tests/fixtures/wmo_tornado_warning.txt")),
        },
        WmoCase {
            name: "segmented_svs",
            framed: frame_with_wmo_separators(include_str!("../tests/fixtures/wmo_segmented_svs.txt")),
        },
    ];

    let mut group = c.benchmark_group("wmo");
    for case in &cases {
        group.throughput(Throughput::Bytes(case.framed.len() as u64));

        group.bench_with_input(
            BenchmarkId::new("message_only", case.name),
            &case.framed,
            |b, framed| {
                b.iter(|| WmoMessage::parse(black_box(framed.as_bytes())).unwrap());
            },
        );

        group.bench_with_input(
            BenchmarkId::new("message_plus_product", case.name),
            &case.framed,
            |b, framed| {
                b.iter(|| {
                    let message = WmoMessage::parse(black_box(framed.as_bytes())).unwrap();
                    let product = NwsProduct::parse(&message).unwrap();
                    black_box((message.sequence_number, product.segments.len()));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("scan_plus_message", case.name),
            &case.framed,
            |b, framed| {
                let scanner = WmoStreamScanner::new();
                b.iter(|| {
                    let outcome = scanner.scan_next(black_box(framed.as_bytes())).unwrap();
                    let chunk = outcome.chunk.unwrap();
                    let message = chunk.parse().unwrap();
                    black_box((chunk.range.len(), message.sequence_number));
                });
            },
        );
    }
    group.finish();
}

fn bench_oi(c: &mut Criterion) {
    let cases = [
        OiCase {
            name: "example",
            xml: include_str!("../tests/fixtures/nwws_oi_example.xml"),
        },
        OiCase {
            name: "tornado_warning",
            xml: include_str!("../tests/fixtures/nwws_oi_tornado_warning.xml"),
        },
        OiCase {
            name: "segmented_svs",
            xml: include_str!("../tests/fixtures/nwws_oi_segmented_svs.xml"),
        },
    ];

    let mut group = c.benchmark_group("oi");
    for case in &cases {
        group.throughput(Throughput::Bytes(case.xml.len() as u64));

        group.bench_with_input(
            BenchmarkId::new("envelope_only", case.name),
            &case.xml,
            |b, xml| {
                b.iter_with_large_drop(|| NwwsOiMessage::parse(black_box(xml)).unwrap());
            },
        );

        group.bench_with_input(
            BenchmarkId::new("envelope_plus_bulletin_validate", case.name),
            &case.xml,
            |b, xml| {
                b.iter(|| {
                    let message = NwwsOiMessage::parse(black_box(xml)).unwrap();
                    message.validate().unwrap();
                    black_box(message.summary.as_deref());
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("end_to_end_product", case.name),
            &case.xml,
            |b, xml| {
                b.iter(|| {
                    let message = NwwsOiMessage::parse(black_box(xml)).unwrap();
                    let content = NwwsContent::from_oi_message(&message).unwrap();
                    black_box((
                        content.bulletin.sequence_number,
                        content.product.segments.len(),
                    ));
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_wmo, bench_oi);
criterion_main!(benches);
