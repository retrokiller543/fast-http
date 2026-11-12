/// A type whose route groups share one base path.
///
/// `#[controller("/library", groups(..))]` on a struct implements it and adds a `routes`
/// constructor that merges the listed groups; each `#[controller(group = name)]` impl block is one
/// group, whose routes sit under [`PATH`](Controller::PATH). It can also be implemented by hand.
pub trait Controller: Send + Sync + 'static {
    /// The base path of every route of the controller, such as `/library`; `/` for none.
    const PATH: &'static str;
}
