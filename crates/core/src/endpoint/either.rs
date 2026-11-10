use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;

/// Defines an endpoint-call future enum of one arity: exactly one variant is live, and polling
/// forwards to it.
///
/// [`EndpointSet::call`](super::EndpointSet::call) returns the selected endpoint's future through
/// one of these instead of awaiting inside an `async fn`, so the caller's state machine gains only
/// the chosen future and a discriminant rather than another nested frame.
macro_rules! endpoint_future {
    ($name:ident, $proj:ident; $($variant:ident),+) => {
        pin_project! {
            /// One of several endpoint-call futures, chosen at dispatch time.
            #[project = $proj]
            pub enum $name<$($variant),+> {
                $(
                    #[doc = concat!("The future of the endpoint selected as `", stringify!($variant), "`.")]
                    $variant { #[pin] fut: $variant },
                )+
            }
        }

        impl<T, $($variant: Future<Output = T>),+> Future for $name<$($variant),+> {
            type Output = T;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
                match self.project() {
                    $($proj::$variant { fut } => fut.poll(cx),)+
                }
            }
        }
    };
}

endpoint_future!(EndpointFuture1, EndpointFuture1Proj; A);
endpoint_future!(EndpointFuture2, EndpointFuture2Proj; A, B);
endpoint_future!(EndpointFuture3, EndpointFuture3Proj; A, B, C);
endpoint_future!(EndpointFuture4, EndpointFuture4Proj; A, B, C, D);
endpoint_future!(EndpointFuture5, EndpointFuture5Proj; A, B, C, D, E);
endpoint_future!(EndpointFuture6, EndpointFuture6Proj; A, B, C, D, E, F);
endpoint_future!(EndpointFuture7, EndpointFuture7Proj; A, B, C, D, E, F, G);
endpoint_future!(EndpointFuture8, EndpointFuture8Proj; A, B, C, D, E, F, G, H);
endpoint_future!(EndpointFuture9, EndpointFuture9Proj; A, B, C, D, E, F, G, H, I);
endpoint_future!(EndpointFuture10, EndpointFuture10Proj; A, B, C, D, E, F, G, H, I, J);
endpoint_future!(EndpointFuture11, EndpointFuture11Proj; A, B, C, D, E, F, G, H, I, J, K);
endpoint_future!(EndpointFuture12, EndpointFuture12Proj; A, B, C, D, E, F, G, H, I, J, K, L);
endpoint_future!(EndpointFuture13, EndpointFuture13Proj; A, B, C, D, E, F, G, H, I, J, K, L, M);
endpoint_future!(EndpointFuture14, EndpointFuture14Proj; A, B, C, D, E, F, G, H, I, J, K, L, M, N);
endpoint_future!(EndpointFuture15, EndpointFuture15Proj; A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
endpoint_future!(EndpointFuture16, EndpointFuture16Proj; A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);
