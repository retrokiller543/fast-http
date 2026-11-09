# fast-http-router

## Role

Static request routing. A `Router` is a statically typed list of `Route`s (pattern plus endpoints), matched without allocating and without `dyn`. It implements `fast_http_core::Routes`, so any transport can drive it. No macros are required; Upwell's macros are expected to emit the same builder calls.

## Usage

```rust
let router = Router::new()
    .route(Route::new("/users/{id}")?.get(show).put(update))
    .route(Route::new("/users")?.post(create));
```

Routes and endpoints are tried in the order they are added. `Route::endpoint(e)` adds any `Endpoint`, such as a guarded `get(handler).guard(..)`.

For large tables, build routes in groups (one function per controller, say) and join them with `Router::merge`:

```rust
fn users() -> Router<impl RouteList> {
    Router::new().route(Route::new("/users/{id}")?.get(show))
}

let router = Router::new().merge(users()).merge(orgs());
```

Every `.route` step produces a bigger type. Compile time and debug-build stack use therefore grow with the square of one chain's length: one 320-route chain takes about 35 s to compile and overflows a 2 MB debug thread. Grouping keeps each chain short. The same 320 routes as 16 groups compile in about 11 s, and 1 280 routes as 64 groups compile in about 40 s. Runtime cost is the same either way.

Handlers are plain `async fn`s. Arguments are extractors (`Path<T>`, `Method`, `Uri`, `HeaderMap`, `Extension<T>`, and so on); the last one may consume the body (`Body<T>` from `fast-http-format`). Return anything that implements `IntoOutcome`, including `Result<_, E>` for any `E: ErrorResponse`.

Patterns: `/users/{id}` captures one segment, `*` matches one segment without capturing, `**` matches the remaining segments, and `{**name}` captures them (read as the borrowed `Segments<'_>` or via `PathParams::segments`).

Handlers are plain `async fn`s and may borrow from the request: `Path<(&str, u64)>`, `Path<Segments<'_>>`, `Path<BTreeMap<&str, &str>>`, `&Parts`, `&HeaderMap`, `&Method`, `&Uri` and `&Extensions` all extract without allocating.

Run `cargo run -p fast-http-router --example users` for a mocked-request walkthrough.

## Internal role

As routes are added, the router extends an index from the first literal segment to the candidate routes, kept in registration order. The route list grows like a base-16 counter (`Level`): elements fill a tuple of up to 16, and full tuples carry upward, so `n` routes nest only log16(n) levels deep. Dispatching a position costs one comparison chain per level. A request does a binary search on that index, matches the candidates' patterns into a stack `ParamBuf` of borrowed values, and hands off to `dispatch_set`. Misses become `Rejection::NotFound` through the configured `ErrorHandler`.

## Feature flags

None.
