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

    check_segments(&joined).map_err(|message| Error::new_spanned(route, message))?;

    Ok(LitStr::new(&joined, route.span()))
}

/// Most path parameters one route can capture, as the router enforces.
const MAX_PARAMS: usize = 16;

/// Applies the router's segment rules at compile time, so a route attribute's pattern always
/// parses: no empty segment, every `{}` named, `**` last, no repeated name, at most
/// [`MAX_PARAMS`] parameters.
fn check_segments(pattern: &str) -> Result<(), String> {
    let mut names: Vec<&str> = Vec::new();
    let body = &pattern[1..];

    if body.is_empty() {
        return Ok(());
    }

    let segments: Vec<&str> = body.split('/').collect();

    for (index, raw) in segments.iter().enumerate() {
        let last = index + 1 == segments.len();
        let name = match *raw {
            "" => return Err(format!("pattern `{pattern}` has an empty segment")),
            "*" => None,
            "**" if last => None,
            "**" => {
                return Err(format!(
                    "pattern `{pattern}` has a wildcard that is not last"
                ));
            }
            _ => match raw.strip_prefix('{').and_then(|raw| raw.strip_suffix('}')) {
                None => None,
                Some("" | "**") => {
                    return Err(format!("pattern `{pattern}` has an unnamed parameter"));
                }
                Some(inner) => match inner.strip_prefix("**") {
                    Some(_) if !last => {
                        return Err(format!(
                            "pattern `{pattern}` has a wildcard that is not last"
                        ));
                    }
                    Some(name) => Some(name),
                    None => Some(inner),
                },
            },
        };

        if let Some(name) = name {
            if names.contains(&name) {
                return Err(format!("pattern `{pattern}` repeats parameter `{name}`"));
            }

            names.push(name);
        }
    }

    if names.len() > MAX_PARAMS {
        return Err(format!(
            "pattern `{pattern}` captures more than {MAX_PARAMS} parameters"
        ));
    }

    Ok(())
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
