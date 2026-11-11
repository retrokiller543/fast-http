/// Returns the path after `prefix` when it ends on a segment boundary.
pub fn strip_prefix<'a>(prefix: &str, path: &'a str) -> Option<&'a str> {
    let rest = path.strip_prefix(prefix)?;

    if rest.is_empty() || rest.starts_with('/') || prefix.is_empty() {
        return Some(rest);
    }

    None
}

#[cfg(test)]
mod tests;
