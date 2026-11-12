/// Access to the value a format wrapper carries, from which [`transparent!`] derives the std
/// traits with bounds on that value alone, never on a format-set marker or a `const` limit.
pub(crate) trait Wrapper: Sized {
    /// The carried value.
    type Inner;

    /// Borrows the value.
    fn get(&self) -> &Self::Inner;

    /// Borrows the value mutably.
    fn get_mut(&mut self) -> &mut Self::Inner;

    /// Moves the value out.
    fn unwrap(self) -> Self::Inner;

    /// Wraps a value.
    fn wrap(inner: Self::Inner) -> Self;
}

/// Implements the std traits of a wrapper whose carried value is its parameter `T`, bounded on `T`
/// only, given its [`Wrapper`] impl.
///
/// `access` gives `Transparent` (route guards see the value), `Deref`, `DerefMut`, `AsRef`,
/// `AsMut`, `From<T>` and `into_inner`, for wrappers
/// that hold more than the value. `value` adds `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`,
/// `PartialOrd`, `Ord`, `Hash` and `Default`, for wrappers that are nothing but the value.
macro_rules! transparent {
    (access $name:ident [$($params:tt)*] [$($args:tt)*]) => {
        impl<$($params)*> ::fast_http_core::Transparent for $name<$($args)*> {
            type Inner = T;

            fn inner(&self) -> &T {
                $crate::wrapper::Wrapper::get(self)
            }
        }

        impl<$($params)*> ::std::ops::Deref for $name<$($args)*> {
            type Target = T;

            fn deref(&self) -> &T {
                $crate::wrapper::Wrapper::get(self)
            }
        }

        impl<$($params)*> ::std::ops::DerefMut for $name<$($args)*> {
            fn deref_mut(&mut self) -> &mut T {
                $crate::wrapper::Wrapper::get_mut(self)
            }
        }

        impl<$($params)*> ::std::convert::AsRef<T> for $name<$($args)*> {
            fn as_ref(&self) -> &T {
                $crate::wrapper::Wrapper::get(self)
            }
        }

        impl<$($params)*> ::std::convert::AsMut<T> for $name<$($args)*> {
            fn as_mut(&mut self) -> &mut T {
                $crate::wrapper::Wrapper::get_mut(self)
            }
        }

        impl<$($params)*> ::std::convert::From<T> for $name<$($args)*> {
            fn from(value: T) -> Self {
                $crate::wrapper::Wrapper::wrap(value)
            }
        }

        impl<$($params)*> $name<$($args)*> {
            /// Moves the carried value out.
            pub fn into_inner(self) -> T {
                $crate::wrapper::Wrapper::unwrap(self)
            }
        }
    };
    (value $name:ident [$($params:tt)*] [$($args:tt)*]) => {
        $crate::wrapper::transparent!(access $name [$($params)*] [$($args)*]);

        impl<$($params)*> ::std::fmt::Debug for $name<$($args)*>
        where
            T: ::std::fmt::Debug,
        {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.debug_tuple(::std::stringify!($name))
                    .field($crate::wrapper::Wrapper::get(self))
                    .finish()
            }
        }

        impl<$($params)*> ::std::clone::Clone for $name<$($args)*>
        where
            T: ::std::clone::Clone,
        {
            fn clone(&self) -> Self {
                $crate::wrapper::Wrapper::wrap($crate::wrapper::Wrapper::get(self).clone())
            }
        }

        impl<$($params)*> ::std::marker::Copy for $name<$($args)*> where T: ::std::marker::Copy {}

        impl<$($params)*> ::std::cmp::PartialEq for $name<$($args)*>
        where
            T: ::std::cmp::PartialEq,
        {
            fn eq(&self, other: &Self) -> bool {
                $crate::wrapper::Wrapper::get(self) == $crate::wrapper::Wrapper::get(other)
            }
        }

        impl<$($params)*> ::std::cmp::Eq for $name<$($args)*> where T: ::std::cmp::Eq {}

        impl<$($params)*> ::std::cmp::PartialOrd for $name<$($args)*>
        where
            T: ::std::cmp::PartialOrd,
        {
            fn partial_cmp(&self, other: &Self) -> ::std::option::Option<::std::cmp::Ordering> {
                $crate::wrapper::Wrapper::get(self)
                    .partial_cmp($crate::wrapper::Wrapper::get(other))
            }
        }

        impl<$($params)*> ::std::cmp::Ord for $name<$($args)*>
        where
            T: ::std::cmp::Ord,
        {
            fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
                $crate::wrapper::Wrapper::get(self).cmp($crate::wrapper::Wrapper::get(other))
            }
        }

        impl<$($params)*> ::std::hash::Hash for $name<$($args)*>
        where
            T: ::std::hash::Hash,
        {
            fn hash<H: ::std::hash::Hasher>(&self, state: &mut H) {
                $crate::wrapper::Wrapper::get(self).hash(state);
            }
        }

        impl<$($params)*> ::std::default::Default for $name<$($args)*>
        where
            T: ::std::default::Default,
        {
            fn default() -> Self {
                $crate::wrapper::Wrapper::wrap(T::default())
            }
        }
    };
}

pub(crate) use transparent;

#[cfg(test)]
mod tests;
