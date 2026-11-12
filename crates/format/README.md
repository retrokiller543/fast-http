# fast-http-format

## Role
Data formats for requests and responses: serde formats behind Cargo features, `Accept`/`Content-Type` negotiation, SSE and NDJSON streaming, and opt-in RFC 9457 problem details.

## Usage
- `Negotiated<T>` as a handler return type encodes `T` in the format the `Accept` header prefers; `Body<T>` as the last argument decodes by `Content-Type`. Both default to every enabled format.
- `Negotiated<T, (Json, Yaml)>` restricts the set; `Negotiated<T, Cons<MyFormat, Enabled>>` adds a custom format.
- Every serde format is also its own wrapper: `Json<T>`, `Form<T>`, `Yaml<T>`, `Toml<T>`, `Cbor<T>`, `MessagePack<T>` and `Xml<T>`. As an argument it accepts only that format (other content types are `415`); as a return value it is always that format, whatever `Accept` says. The bare name, as in `(Json, Yaml)`, is the same type naming the format in a set. `(StatusCode, Json(value))` sets the status.
- The wrappers, `Body`, `Query` and `Negotiated` reach their value through `Deref` and `AsRef` and convert with `From<T>` and `into_inner`. The pure wrappers also implement `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash` and `Default` whenever `T` does, whatever the format set.
- A custom format implements `Format` plus `Encode<T>` and/or `Decode<T>`, with any bound on `T` it needs.
- `Query<T>` (the `form` feature) deserializes the query string into `T` as a head extractor. A plain `&str` field whose value needs percent-decoding fails with `BorrowedQueryField`, which keeps serde's error as its source; borrow with `#[serde(borrow)] Cow<'_, str>` instead. For a look at one parameter without deserializing a struct, take `fast_http_core::QueryParams`.
- `Sse<S>` and `NdJson<S>` stream any `Stream<Item = Result<_, _>>`. `Event<Encoded<T, Json>>` encodes each event's data with a format.
- `problem::ProblemErrorResponse` is optional; no part of the framework requires problem details.

## Internal role
Builds on `fast-http-core` (`IntoOutcome`, `FromRequest`, `ErrorResponse`, media-type negotiation). Format sets are static type lists, so dispatch is a `match` with no `dyn`. One internal `transparent!` macro derives the std traits of every wrapper from a small `Wrapper` access trait, bounded on the value alone, which `#[derive]` cannot do for a type that also carries a format-set marker.

## Feature flags
`json` (default), `form`, `yaml`, `toml`, `cbor`, `msgpack`, `xml`, `problem` (implies `json`), `full`. A format's dependencies are only compiled when its feature is on.
