/// Returns the path after `prefix` when it ends on a segment boundary.
///
/// A trailing `/` on the prefix is ignored, so `/admin/` covers what `/admin` covers and `/`
/// covers every path, like the empty prefix.
pub fn strip_prefix<'a>(prefix: &str, path: &'a str) -> Option<&'a str> {
    let prefix = prefix.trim_end_matches('/');
    let rest = path.strip_prefix(prefix)?;

    if rest.is_empty() || rest.starts_with('/') || prefix.is_empty() {
        return Some(rest);
    }

    None
}

#[cfg(test)]
mod tests;
