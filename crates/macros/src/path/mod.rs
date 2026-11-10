use syn::{Error, LitStr};

/// Joins a controller prefix and a route path into the pattern the router parses.
///
/// The prefix is empty, `/`, or starts with `/` and does not end with one. A route path is empty
/// or starts with `/`; a lone `/` under a non-empty prefix is the prefix itself.
pub fn join(prefix: Option<&LitStr>, route: &LitStr) -> syn::Result<LitStr> {
    let prefix_text = prefix.map(LitStr::value).unwrap_or_default();
    let prefix_text = if prefix_text == "/" {
        String::new()
    } else {
        prefix_text
    };
    let route_text = route.value();

    if let Some(prefix) = prefix {
        check_prefix(prefix, &prefix_text)?;
    }

    if !route_text.is_empty() && !route_text.starts_with('/') {
        return Err(Error::new_spanned(
            route,
            "a route path must start with `/`",
        ));
    }

    let route_text = if route_text == "/" && !prefix_text.is_empty() {
        String::new()
    } else {
        route_text
    };
    let joined = format!("{prefix_text}{route_text}");
    let joined = if joined.is_empty() {
        String::from("/")
    } else {
        joined
    };

    Ok(LitStr::new(&joined, route.span()))
}

fn check_prefix(prefix: &LitStr, text: &str) -> syn::Result<()> {
    if !text.is_empty() && !text.starts_with('/') {
        return Err(Error::new_spanned(
            prefix,
            "a path prefix must start with `/`",
        ));
    }

    if text.ends_with('/') {
        return Err(Error::new_spanned(
            prefix,
            "a path prefix must not end with `/`",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests;
