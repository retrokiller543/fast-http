use fast_http_core::{
    DispatchOptions, ErrorHandler, PathParams, ReqBody, Request, Response, TuplePush,
};
use http::request::Parts;

use super::RouteList;
use crate::Pattern;

/// One base-16 digit of a growing route list: up to 15 elements in `digit`, after the earlier
/// routes in `up`.
///
/// [`Append`] treats a route list like a base-16 counter. Elements fill the bottom digit; when a
/// 16th arrives, the full 16-tuple carries into `up` as one element and the digit empties. Every
/// element at one level therefore has the same shape, so `n` routes nest only `log16(n)` levels
/// deep and dispatch makes one comparison chain per level.
#[derive(Debug)]
pub struct Level<D, U> {
    digit: D,
    up: U,
}

impl<D: RouteList, U: RouteList> RouteList for Level<D, U> {
    const LEN: usize = U::LEN + D::LEN;

    fn pattern(&self, index: usize) -> &Pattern {
        match index.checked_sub(U::LEN) {
            None => self.up.pattern(index),
            Some(rest) => self.digit.pattern(rest),
        }
    }

    fn passthrough(&self, index: usize) -> bool {
        match index.checked_sub(U::LEN) {
            None => self.up.passthrough(index),
            Some(rest) => self.digit.passthrough(rest),
        }
    }

    async fn call<B: ReqBody, H: ErrorHandler>(
        &self,
        index: usize,
        parts: &Parts,
        params: PathParams<'_>,
        body: B,
        handler: &H,
        options: DispatchOptions,
    ) -> Response {
        match index.checked_sub(U::LEN) {
            None => {
                self.up
                    .call(index, parts, params, body, handler, options)
                    .await
            }
            Some(rest) => {
                self.digit
                    .call(rest, parts, params, body, handler, options)
                    .await
            }
        }
    }

    async fn call_layered<B: ReqBody, H: ErrorHandler>(
        &self,
        index: usize,
        req: Request<B>,
        handler: &H,
        options: DispatchOptions,
    ) -> Response {
        match index.checked_sub(U::LEN) {
            None => self.up.call_layered(index, req, handler, options).await,
            Some(rest) => self.digit.call_layered(rest, req, handler, options).await,
        }
    }
}

/// Appends an element, a [`Route`](crate::Route) or a whole route list, after every existing
/// route, statically.
///
/// `()` is the empty list; see [`Level`] for how the list stays shallow as it grows.
pub trait Append<X> {
    /// The list with `X` appended.
    type Output: RouteList;

    /// Appends `element`.
    fn append(self, element: X) -> Self::Output;
}

impl<X: RouteList> Append<X> for () {
    type Output = Level<(X,), ()>;

    fn append(self, element: X) -> Self::Output {
        Level {
            digit: (element,),
            up: (),
        }
    }
}

impl<U: RouteList, X: RouteList> Append<X> for Level<(), U> {
    type Output = Level<(X,), U>;

    fn append(self, element: X) -> Self::Output {
        Level {
            digit: (element,),
            up: self.up,
        }
    }
}

macro_rules! append_digit {
    ($($list:ident),+) => {
        impl<U: RouteList, $($list: RouteList,)+ X: RouteList> Append<X> for Level<($($list,)+), U> {
            type Output = Level<($($list,)+ X), U>;

            fn append(self, element: X) -> Self::Output {
                Level {
                    digit: self.digit.push(element),
                    up: self.up,
                }
            }
        }
    };
}

macro_rules! append_carry {
    ($($list:ident),+) => {
        impl<U, $($list: RouteList,)+ X: RouteList> Append<X> for Level<($($list,)+), U>
        where
            U: Append<($($list,)+ X)>,
        {
            type Output = Level<(), U::Output>;

            fn append(self, element: X) -> Self::Output {
                Level {
                    digit: (),
                    up: self.up.append(self.digit.push(element)),
                }
            }
        }
    };
}

append_digit!(A);
append_digit!(A, B);
append_digit!(A, B, C);
append_digit!(A, B, C, D);
append_digit!(A, B, C, D, E);
append_digit!(A, B, C, D, E, F);
append_digit!(A, B, C, D, E, F, G);
append_digit!(A, B, C, D, E, F, G, H);
append_digit!(A, B, C, D, E, F, G, H, I);
append_digit!(A, B, C, D, E, F, G, H, I, J);
append_digit!(A, B, C, D, E, F, G, H, I, J, K);
append_digit!(A, B, C, D, E, F, G, H, I, J, K, L);
append_digit!(A, B, C, D, E, F, G, H, I, J, K, L, M);
append_digit!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
append_carry!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
