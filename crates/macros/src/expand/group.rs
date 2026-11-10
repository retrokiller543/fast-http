use proc_macro2::TokenStream;
use syn::{Error, LitStr};

use crate::{route::HttpMethod, when::Condition};

/// One endpoint of a controller: a method, a pattern, its guard and the handler to call.
pub struct Endpoint {
    /// The HTTP method.
    pub method: HttpMethod,
    /// The full route pattern, prefix included.
    pub pattern: LitStr,
    /// Conditions on the request head, all of which must hold.
    pub when: Vec<Condition>,
    /// The `when` clause as written, for spotting duplicates.
    pub when_text: String,
    /// Expression that produces the handler.
    pub handler: TokenStream,
}

/// The endpoints that share one pattern, in declaration order.
pub struct Group {
    /// The route pattern.
    pub pattern: LitStr,
    /// The endpoints of the route.
    pub endpoints: Vec<Endpoint>,
}

/// Groups endpoints by pattern, keeping first-appearance and declaration order, and rejects
/// endpoints that can never be selected.
pub fn group(endpoints: Vec<Endpoint>) -> syn::Result<Vec<Group>> {
    let mut groups: Vec<Group> = Vec::new();

    for endpoint in endpoints {
        let position = groups
            .iter()
            .position(|group| group.pattern.value() == endpoint.pattern.value());

        match position {
            Some(index) => {
                check_reachable(&groups[index], &endpoint)?;
                groups[index].endpoints.push(endpoint);
            }
            None => groups.push(Group {
                pattern: endpoint.pattern.clone(),
                endpoints: vec![endpoint],
            }),
        }
    }

    Ok(groups)
}

fn check_reachable(group: &Group, endpoint: &Endpoint) -> syn::Result<()> {
    for earlier in group
        .endpoints
        .iter()
        .filter(|earlier| earlier.method == endpoint.method)
    {
        if earlier.when_text == endpoint.when_text {
            return Err(Error::new_spanned(
                &endpoint.pattern,
                "this route repeats an earlier endpoint with the same method, path and `when`",
            ));
        }

        if earlier.when.is_empty() {
            return Err(Error::new_spanned(
                &endpoint.pattern,
                "this endpoint is unreachable: an earlier one for the same method and path has no `when` guard and always wins",
            ));
        }
    }

    Ok(())
}
