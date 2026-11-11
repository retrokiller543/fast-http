# fast-http-macros

## Role

Route macros for fast-http. `#[controller]` turns an inherent `impl` block of `async fn` handlers into a `routes` constructor that builds a `fast_http_router::Router` from the methods' route attributes. `#[routes]` does the same for a module of free functions, and `#[get]`, `#[post]`, `#[put]`, `#[patch]` and `#[delete]` make a lone free function an endpoint. All three share one engine, with the same typed guards. Use them through the `fast-http` facade (`macros` feature).

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
    async fn show_v2(&self, Path(id): Path<u64>) -> String { /* ... */ }

    #[get("/{id}")]
    async fn show(&self, Path(id): Path<u64>) -> String { /* ... */ }
}

let router = Router::new().merge(Arc::new(chat).routes()?);
```

### Three forms, one engine

- **`#[controller("/prefix")] impl T { .. }`** has state: a `&self` method is bound to the `Arc<Self>` that `routes(self: Arc<Self>)` takes, and a method without a receiver is used as it is.
- **`#[routes("/prefix")] mod name { .. }`** is the same without state. The free `async fn`s inside, with route attributes, become the module's `routes()` function (`name::routes()`). Functions that share a method and path are variants of one route. Types from the parent scope need `use super::*;`.
- **`#[route(METHOD, "/path", when(..))]`** is the general form of a route attribute, with `METHOD` one of `GET`, `POST`, `PUT`, `PATCH` or `DELETE` in any case. `#[get(..)]`, `#[post(..)]`, `#[put(..)]`, `#[patch(..)]` and `#[delete(..)]` are shorthands for it, and it works wherever they do: on a lone function, on a method of a controller and on a function of a `#[routes]` module.
- **`#[get("/path", when(..))] async fn name(..)`** on a lone function makes `name` a unit struct that implements `Endpoint` and `IntoRoute`, with its path as `name::PATH`: `Router::new().route(name)` registers it at that path, and `endpoints![a, b]` makes one route of several functions that share a path, which is checked at compile time. The body stays in a hidden function. A lone function is a complete endpoint, and when its guard misses it declines the request, so the next endpoint of the same route (`endpoints![a, b]`) gets its turn, and the guard miss status is the answer when none accepts. Each lone function extracts its own arguments, so a type that two of them take is extracted twice, and a function that already read the body cannot hand it on: its miss ends the request. That trade is yours to make; to share extraction and a single body parse across variants of one path, use a `#[routes]` module or a controller.

### Why a module for variants

Two functions are variants of one path only if one generated endpoint sees both, because that endpoint is what extracts each argument type once and runs the guards lazily in order. An attribute macro sees only the item it is on, so something has to group the variants: an `impl` block (`#[controller]`), a module (`#[routes]`), or nothing, in which case each function is its own complete endpoint.

The optional first argument is a path prefix (controller and module); `crate = path` names the crate providing the facade modules `router` and `core` (default `::fast_http`). `#[get]`, `#[post]`, `#[put]`, `#[patch]` and `#[delete]` mark a function as an endpoint. The path may be omitted, as in `#[post]` or `#[post(when(..))]`, for the prefix itself. Several attributes on one function add several endpoints. Functions must be `async fn` and non-generic. The generated `routes()` returns the router, or the `PatternError` of the first malformed pattern.

Routes are ordered most specific first: a literal segment beats `{param}`, which beats `*` and `**`. So `/items/orders` is tried before `/items/{id}` whatever order they are declared in, which matters because the router uses the first route that matches and does not fall through to a later one. Routes that are equally specific, and the variants within a route, keep their declaration order.

### Guards

`when(..)` takes boolean Rust expressions over the function's own parameters; several, separated by commas, must all hold. The expression is emitted exactly as written, so the editor highlights, completes and type-checks it like any other code.

- A guard sees the parameters it names by their binding: `params` for `Query(params): Query<Params>`, `order` for `Body { value: order, .. }: Body<Order>`. They are borrowed, so write `*id > 0` for a `Path<u64>`.
- `headers` and `query` are reserved names. When a guard uses one and the function has no parameter binding that name, whatever its type, the macro supplies it: `headers` is the request's `&HeaderMap`, `query` a `QueryParams` view. The view is built inside the guard that names it, so it costs nothing when unused, and it is decided per variant. A parameter of the same name always takes precedence. Anything else of the head, such as the method or URI, is a normal parameter: `parts: &Parts`.
- Any head extractor works as a parameter: `&HeaderMap`, `&Parts`, `&Uri`, `Path<T>`, `QueryParams<'_>` (a zero-copy view with `get`, `contains` and `iter`), `Query<T>` from `fast-http-format` (serde), or your own `FromParts` type. A body extractor works too, since the guard reads the already-parsed value.
- A guard cannot use `self`; take what it needs as a parameter. A parameter that only feeds a guard does not warn as unused.
- Variants of one method and path are tried in declaration order. A parameter that is absent matches no guard that reads it, so put an unguarded default last. An extractor that fails ends the request with its own error; it does not fall through to the next variant.

### One body type per route

Variants of one method and path that read the request body must all take the same body type, which is parsed lazily, once. This is checked at compile time: taking two different body types is an error. If a body can legitimately take several shapes, define one enum for it, deserialize that, and guard on its variants:

```rust
#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Job { Fast { units: u32 }, Slow { units: u32 } }

#[post("/job", when(matches!(job.value, Job::Fast { .. }))]
async fn fast(&self, job: Body<Job>) -> .. { }

#[post("/job")]
async fn slow(&self, job: Body<Job>) -> .. { }
```

### Compile errors

A variant after an unguarded one for the same method and path (unreachable), a repeated endpoint, a non-`async` function or one with type or const parameters, a receiver other than `&self` (and any receiver in a module or on a lone function), a guard using `self`, two different body types on one route, and a malformed prefix or path, by the same rules the router parses with: an empty segment, an unnamed `{}`, a `**` that is not last, a repeated parameter name, or more than 16 parameters.

## Borrowed returns

A handler may return data borrowed from its arguments, `async fn echo(Path(name): Path<&str>) -> &str`, with named lifetimes too (`fn echo<'a>(..) -> &'a str`). Arguments live as long as the request does, and a borrowed `&str` is copied into the response body. Type and const parameters are still rejected.

`Query<T>` is one extractor for both cases: `Query<Params>` owns what it reads, and `Query<Params<'_>>` with `#[serde(borrow)] Cow<'a, str>` fields points into the request and copies a value only when percent-decoding changes it. A plain `&'a str` field cannot hold decoded text, so a request that needs decoding fails with an error that says to use `Cow`.

## Internal role

Every method and path compiles to one generated `Endpoint`, guarded or not, so every argument is asked for as the type the handler wrote, with the request lifetime inferred at that spot. That is what lets `Query<Params<'_>>` borrow from the request, which the router's plain `Handler` path cannot express. An endpoint without guards declines nothing and does the same work as a hand-written one.

Guarded variants, and every lone function, compile to one generated `Endpoint` that does its own selection and keeps work lazy:

- A guard fills the arguments it reads, and the winning variant takes the rest, so a guard on the head runs before any body is parsed and a variant whose guard fails never extracts what it did not need.
- Each distinct argument type is extracted once, into a typed slot on the stack, shared by every variant that takes it, and moved into the winner. Nothing allocates, the request body is never buffered, and the controller `Arc` is borrowed, not cloned, per request.
- When every variant misses, the response is the usual guard miss (400).

The generated code is small and its locals use unlikely names (`__parts`, `__s0`, ...), so a guard cannot reach them by accident. Each slot is annotated with the parameter's declared type, which is also what lets an editor infer the types of guard bindings. If the macro cannot expand, for example while a guard is half typed, it emits the item as written plus the error rather than only the error, so the editor keeps its view of the rest of the controller.

The runtime pieces live in `fast_http_router::dispatch`.

## Feature flags

None.
