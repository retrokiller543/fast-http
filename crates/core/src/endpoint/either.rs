use std::pin::Pin;
use std::task::{Context, Poll};

/// Defines an endpoint-call future enum of one arity: exactly one variant is live, and polling
/// forwards to it.
///
/// [`EndpointSet::call`](super::EndpointSet::call) returns the selected endpoint's future through
/// one of these instead of awaiting inside an `async fn`, so the caller's state machine gains only
/// the chosen future and a discriminant rather than another nested frame.
macro_rules! endpoint_future {
    ($name:ident; $($variant:ident),+) => {
        /// One of several endpoint-call futures, chosen at dispatch time.
        pub enum $name<$($variant),+> {
            $(
                #[doc = concat!("The future of the endpoint selected as `", stringify!($variant), "`.")]
                $variant($variant),
            )+
        }

        impl<T, $($variant: Future<Output = T>),+> Future for $name<$($variant),+> {
            type Output = T;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
                // SAFETY: the inner future is never moved out of the pin; it is only projected in place and polled.
                unsafe {
                    match self.get_unchecked_mut() {
                        $(Self::$variant(fut) => Pin::new_unchecked(fut).poll(cx),)+
                    }
                }
            }
        }
    };
}

endpoint_future!(EndpointFuture1; A);
endpoint_future!(EndpointFuture2; A, B);
endpoint_future!(EndpointFuture3; A, B, C);
endpoint_future!(EndpointFuture4; A, B, C, D);
endpoint_future!(EndpointFuture5; A, B, C, D, E);
endpoint_future!(EndpointFuture6; A, B, C, D, E, F);
endpoint_future!(EndpointFuture7; A, B, C, D, E, F, G);
endpoint_future!(EndpointFuture8; A, B, C, D, E, F, G, H);
endpoint_future!(EndpointFuture9; A, B, C, D, E, F, G, H, I);
endpoint_future!(EndpointFuture10; A, B, C, D, E, F, G, H, I, J);
endpoint_future!(EndpointFuture11; A, B, C, D, E, F, G, H, I, J, K);
endpoint_future!(EndpointFuture12; A, B, C, D, E, F, G, H, I, J, K, L);
endpoint_future!(EndpointFuture13; A, B, C, D, E, F, G, H, I, J, K, L, M);
endpoint_future!(EndpointFuture14; A, B, C, D, E, F, G, H, I, J, K, L, M, N);
endpoint_future!(EndpointFuture15; A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
endpoint_future!(EndpointFuture16; A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);
