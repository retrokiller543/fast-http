# fast-http-macros

## Role

Controller macros for fast-http. `#[controller]` turns an inherent `impl` block of `async fn` handlers into a `routes` constructor that builds a `fast_http_router::Router` from the methods' route attributes. The expansion is plain router builder calls, so anything it generates can also be written by hand. Use it through the `fast-http` facade (`macros` feature).

## Usage

```rust
struct Chat { db: Arc<Db> }

#[controller("/chat")]
impl Chat {
    #[post("", when(query.stream = true))]
    async fn stream(&self, body: Body<ChatReq>) -> Sse<ChatChunks> { /* ... */ }

    #[post("")]
    async fn buffered(&self, body: Body<ChatReq>) -> Negotiated<ChatRes> { /* ... */ }

    #[get("/{id}")]
    async fn show(&self, Path(id): Path<u64>, parts: &Parts) -> String { /* ... */ }
}

let router = Router::new().merge(Arc::new(chat).routes()?);
```

- The optional argument is a path prefix; `crate = path` names the crate providing the facade modules `router` and `core` (default `::fast_http`).
- `#[get]`, `#[post]`, `#[put]`, `#[patch]` and `#[delete]` mark a method as an endpoint. Several attributes on one method add several endpoints. Unmarked methods are left alone, other attributes are kept.
- A `&self` method is bound to the `Arc<Self>` that `routes` takes; a method without a receiver is used as it is. All other arguments are extractors, as for any handler. Methods must be `async fn` and non-generic.
- Endpoints that share a path form one route, tried in declaration order. The generated `routes(self: Arc<Self>)` returns the router, or the `PatternError` of the first malformed pattern.
- `when(..)` restricts an endpoint to requests whose head matches; several conditions in one `when` must all hold:
  - `query.name` and `header.name` check presence; `= value` checks the exact value (string, integer or boolean for queries, string for headers). Query names and values are compared as sent, without percent-decoding. Header identifiers turn `_` into `-`.
  - `not(..)`, `any(..)` and `all(..)` combine conditions.
  - `guard(Type)` runs a `HeadGuard` type.
- A parameter that is absent matches no `= value` condition. Put an unguarded endpoint last for the default.
- Compile errors: a variant after an unguarded one for the same method and path (unreachable), a repeated endpoint, a non-`async` or generic method, a receiver other than `&self`, and a malformed prefix or path. Body guards are not supported by `when`; implement `Endpoint` by hand.

## Internal role

Each `&self` method gets a hidden free-standing wrapper `async fn(Arc<Self>, ...)` with the same argument types, which is what `fast_http_router::bind` accepts. A free-standing function item is what lets borrowed extractors such as `&Parts` or `Path<&str>` keep working. The guards compile to non-capturing closures over helpers in `fast_http_core::guard`. Prefix and path are joined at expansion time, so the router receives one literal pattern.

## Feature flags

None.
