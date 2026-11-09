//! Format layer with real serde work: every enabled format, small and large payloads, and the
//! `Negotiated` and `Body` extractors that wrap them.

use criterion::{Criterion, criterion_group, criterion_main};
use fast_http_bench_support::{Budget, bench, ready};
use fast_http_core::{FromRequest, IntoOutcome};
use fast_http_format::{Body, Decode, Encode, Negotiated, formats::Json};
use http::{Request, header, request::Parts};
use http_body_util::Full;
use serde::{Deserialize, Serialize};

/// Customer embedded in an order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Customer {
    name: String,
    email: String,
}

/// One order line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Line {
    sku: String,
    quantity: u32,
    unit_price_cents: u64,
}

/// Nested payload shared by every case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Order {
    id: u64,
    customer: Customer,
    lines: Vec<Line>,
    notes: Option<String>,
}

const SIZES: [(&str, usize); 2] = [("small_1_line", 1), ("large_100_lines", 100)];

fn order(lines: usize) -> Order {
    Order {
        id: 1042,
        customer: Customer {
            name: "Ada Lovelace".to_owned(),
            email: "ada@example.com".to_owned(),
        },
        lines: (0..lines)
            .map(|index| Line {
                sku: format!("SKU-{index:05}"),
                quantity: (index % 7) as u32 + 1,
                unit_price_cents: 999 + index as u64 * 100,
            })
            .collect(),
        notes: Some("leave at the front door".to_owned()),
    }
}

fn head(media: &str) -> Parts {
    Request::builder()
        .uri("/orders/1042")
        .header(header::ACCEPT, media)
        .header(header::CONTENT_TYPE, media)
        .body(())
        .expect("valid request")
        .into_parts()
        .0
}

fn codec<F: Encode<Order> + Decode<Order>>(c: &mut Criterion, label: &str) {
    let media = F::MEDIA_TYPE.essence();

    for (size, lines) in SIZES {
        let value = order(lines);
        let parts = head(media);
        let bytes = F::encode(&value, &parts).expect("encodes");
        let group = format!("codec/{size}");

        assert_eq!(
            <F as Decode<Order>>::decode(&bytes).expect("decodes"),
            value,
            "{label} must round-trip"
        );
        println!("size {group}/{label}: {} bytes on the wire", bytes.len());

        bench(
            c,
            &group,
            &format!("{label}_encode"),
            Budget::Report,
            || (),
            |()| F::encode(&value, &parts),
        );
        bench(
            c,
            &group,
            &format!("{label}_decode"),
            Budget::Report,
            || (),
            |()| <F as Decode<Order>>::decode(&bytes),
        );
    }
}

fn extractors<F: Encode<Order> + Decode<Order>>(c: &mut Criterion, label: &str) {
    let media = F::MEDIA_TYPE.essence();

    for (size, lines) in SIZES {
        let value = order(lines);
        let bytes = F::encode(&value, &head(media)).expect("encodes");
        let group = format!("extractors/{size}");

        bench(
            c,
            &group,
            &format!("negotiated_{label}"),
            Budget::Report,
            || (value.clone(), head(media)),
            |(value, parts)| Negotiated::<Order>::new(value).into_outcome(&parts),
        );
        bench(
            c,
            &group,
            &format!("body_{label}"),
            Budget::Report,
            || (head(media), Full::new(bytes.clone())),
            |(parts, body)| ready(<Body<Order> as FromRequest>::from_request(&parts, body)),
        );
    }
}

fn formats(c: &mut Criterion) {
    codec::<Json>(c, "json");
    extractors::<Json>(c, "json");

    #[cfg(feature = "yaml")]
    {
        codec::<fast_http_format::formats::Yaml>(c, "yaml");
        extractors::<fast_http_format::formats::Yaml>(c, "yaml");
    }

    #[cfg(feature = "msgpack")]
    {
        codec::<fast_http_format::formats::MessagePack>(c, "msgpack");
        extractors::<fast_http_format::formats::MessagePack>(c, "msgpack");
    }

    #[cfg(feature = "cbor")]
    {
        codec::<fast_http_format::formats::Cbor>(c, "cbor");
        extractors::<fast_http_format::formats::Cbor>(c, "cbor");
    }
}

criterion_group!(benches, formats);
criterion_main!(benches);
