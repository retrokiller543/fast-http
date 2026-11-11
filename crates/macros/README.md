# fast-http-macros

## Role

Controller macros for fast-http. `#[controller]` turns an inherent `impl` block of `async fn` handlers into a `routes` constructor that builds a `fast_http_router::Router` from the methods' route attributes. Use it through the `fast-http` facade (`macros` feature).

## Usage

```rust
struct Chat { db: Arc<Db> }

#[controller("/chat")]
impl Chat {
    #[post(when(params.stream == Some(true)))]
    async fn stream(&self, Query(params): Query<Params>, body: Body<ChatReq>) -> Sse<ChatChunks> { /* ... */ }

    #[post]
    async fn buffered(&self, body: Body<ChatReq>) -> Negotiated<ChatRes> { /* ... */ }

    #[get("/{id}", when(headers.get("x-api-version").is_some_and(|v| v == "2")))]
    async fn show_v2(&self, Path(id): Path<u64>, headers: &HeaderMap) -> String { /* ... */ }

    #[get("/{id}")]
    async fn show(&self, Path(id): Path<u64>) -> String { /* ... */ }
}

let router = Router::new().merge(Arc::new(chat).routes()?);
```

- The optional argument is a path prefix; `crate = path` names the crate providing the facade modules `router` and `core` (default `::fast_http`).
- `#[get]`, `#[post]`, `#[put]`, `#[patch]` and `#[delete]` mark a method as an endpoint. The path is relative to the prefix and may be omitted, as in `#[post]` or `#[post(when(..))]`, for the prefix itself. Several attributes on one method add several endpoints. Unmarked methods are left alone, other attributes are kept.
- A `&self` method is bound to the `Arc<Self>` that `routes` takes; a method without a receiver is used as it is. All other arguments are extractors, as for any handler. Methods must be `async fn` and non-generic.
- The generated `routes(self: Arc<Self>)` returns the router, or the `PatternError` of the first malformed pattern.

### Guards

`when(..)` takes boolean Rust expressions over the method's own parameters; several, separated by commas, must all hold. The expression is emitted exactly as written, so the editor highlights, completes and type-checks it like any other code.

- A guard sees the parameters it names by their binding: `params` for `Query(params): Query<Params>`, `headers` for `headers: &HeaderMap`, `order` for `Body { value: order, .. }: Body<Order>`. They are borrowed, so write `*id > 0` for a `Path<u64>`.
- Any head extractor works: `&HeaderMap`, `&Parts`, `&Uri`, `Path<T>`, `QueryParams<'_>` (a zero-copy view with `get`, `contains` and `iter`), `Query<T>` from `fast-http-format` (serde), or your own `FromParts` type. A body extractor works too, since the guard reads the already-parsed value.
- A guard cannot use `self`; take what it needs as a parameter. A parameter that only feeds a guard does not warn as unused.
- Variants of one method and path are tried in declaration order. A parameter that is absent matches no guard that reads it, so put an unguarded default last. An extractor that fails ends the request with its own error; it does not fall through to the next variant.
- Compile errors: a variant after an unguarded one for the same method and path (unreachable), a repeated endpoint, a non-`async` or generic method, a receiver other than `&self`, a guard using `self`, and a malformed prefix or path.

## Internal role

A method and path with one unguarded variant uses a hidden free-standing `async fn(Arc<Self>, ..)` wrapper and `fast_http_router::bind`, the same cost as writing the route by hand. A function item is what lets borrowed extractors such as `&Parts` or `Path<&str>` keep working.

Guarded variants compile to one generated `Endpoint` that does its own selection, and keeps work lazy:

- Arguments are extracted in the order the guards need them, so a guard on the head runs before any body is parsed, and a variant whose guard fails never extracts the rest.
- Each distinct argument type is extracted once, into a slot on the stack, shared by every variant that takes it, and moved into the winning handler. Nothing allocates and the controller `Arc` is borrowed, not cloned, per request.
- The request body is handed over untouched unless different variants need different body types. Only then is it buffered once, up to `DEFAULT_BODY_LIMIT`, and replayed to each extractor.
- When every variant misses, the response is the usual guard miss (400).

The runtime pieces live in `fast_http_router::dispatch`. Prefix and path are joined at expansion time, so the router receives one literal pattern.

## Feature flags

None.
