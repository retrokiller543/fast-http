use super::Path;

/// An extractor that only carries a value, such as `Path<T>` or a body wrapper, so a route guard
/// sees that value instead of the extractor: a guard on `job: Body<Job>` reads `job` as `&Job`.
///
/// The handler still receives the extractor as declared. Implement it for your own wrapper
/// extractors to give them the same guard view.
pub trait Transparent {
    /// The carried value a guard sees.
    type Inner;

    /// Borrows the carried value.
    fn inner(&self) -> &Self::Inner;
}

impl<T> Transparent for Path<T> {
    type Inner = T;

    fn inner(&self) -> &T {
        &self.0
    }
}
