//! Static tuple growth, so builders can append without erasing types.

/// Appends `T` to a tuple, producing the tuple one element longer.
///
/// Implemented for `()` through tuples of 15 elements, so builders grow to at most 16.
pub trait TuplePush<T> {
    /// The tuple with `T` appended.
    type Output;

    /// Appends `value`.
    fn push(self, value: T) -> Self::Output;
}

impl<T> TuplePush<T> for () {
    type Output = (T,);

    fn push(self, value: T) -> Self::Output {
        (value,)
    }
}

macro_rules! tuple_push {
    ($($name:ident),+) => {
        impl<$($name,)+ T> TuplePush<T> for ($($name,)+) {
            type Output = ($($name,)+ T);

            #[allow(non_snake_case)]
            fn push(self, value: T) -> Self::Output {
                let ($($name,)+) = self;

                ($($name,)+ value)
            }
        }
    };
}

tuple_push!(A);
tuple_push!(A, B);
tuple_push!(A, B, C);
tuple_push!(A, B, C, D);
tuple_push!(A, B, C, D, E);
tuple_push!(A, B, C, D, E, F);
tuple_push!(A, B, C, D, E, F, G);
tuple_push!(A, B, C, D, E, F, G, H);
tuple_push!(A, B, C, D, E, F, G, H, I);
tuple_push!(A, B, C, D, E, F, G, H, I, J);
tuple_push!(A, B, C, D, E, F, G, H, I, J, K);
tuple_push!(A, B, C, D, E, F, G, H, I, J, K, L);
tuple_push!(A, B, C, D, E, F, G, H, I, J, K, L, M);
tuple_push!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
tuple_push!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
