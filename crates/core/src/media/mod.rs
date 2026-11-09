/// A concrete media type such as `application/json`, stored as its parameter-free essence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MediaType(&'static str);

impl MediaType {
    /// Creates a media type from a `type/subtype` essence.
    pub const fn new(essence: &'static str) -> Self {
        Self(essence)
    }

    /// The `type/subtype` string, usable as a `Content-Type` value.
    pub const fn essence(&self) -> &'static str {
        self.0
    }

    fn split(&self) -> (&'static str, &'static str) {
        self.0.split_once('/').unwrap_or((self.0, ""))
    }
}

/// Specificity of the `Accept` range that applies to a media type; higher wins.
fn specificity(range: &str, media: MediaType) -> Option<u8> {
    let (kind, subtype) = media.split();
    let (range_kind, range_subtype) = range.split_once('/')?;

    if range_kind == "*" && range_subtype == "*" {
        return Some(0);
    }

    if !range_kind.eq_ignore_ascii_case(kind) {
        return None;
    }

    if range_subtype == "*" {
        return Some(1);
    }

    range_subtype.eq_ignore_ascii_case(subtype).then_some(2)
}

/// Parses a `q` weight into thousandths; a missing or malformed weight means 1.0.
fn weight(params: &str) -> u16 {
    for param in params.split(';') {
        let Some((name, value)) = param.split_once('=') else {
            continue;
        };

        if name.trim().eq_ignore_ascii_case("q") {
            return value
                .trim()
                .parse::<f32>()
                .map(|q| (q.clamp(0.0, 1.0) * 1000.0).round() as u16)
                .unwrap_or(1000);
        }
    }

    1000
}

/// Weight and specificity the header assigns to `media`, or `None` when no range matches.
fn rank(accept: &str, media: MediaType) -> Option<(u8, u16)> {
    let mut best: Option<(u8, u16)> = None;

    for item in accept.split(',') {
        let (range, params) = item.split_once(';').unwrap_or((item, ""));
        let Some(specific) = specificity(range.trim(), media) else {
            continue;
        };

        if best.is_none_or(|(current, _)| specific > current) {
            best = Some((specific, weight(params)));
        }
    }

    best
}

/// Picks the index of the best offered media type for an `Accept` header.
///
/// Without a header the first offer wins. Ties on weight keep the earlier offer, so offers are
/// ordered by server preference.
pub fn negotiate(
    accept: Option<&str>,
    count: usize,
    media_type: impl Fn(usize) -> MediaType,
) -> Option<usize> {
    let Some(accept) = accept else {
        return (count > 0).then_some(0);
    };
    let mut best: Option<(usize, u16)> = None;

    for index in 0..count {
        let Some((_, weight)) = rank(accept, media_type(index)) else {
            continue;
        };

        if weight > 0 && best.is_none_or(|(_, current)| weight > current) {
            best = Some((index, weight));
        }
    }

    best.map(|(index, _)| index)
}

/// Picks the index of the offered media type equal to a `Content-Type` header, ignoring parameters.
pub fn match_content_type(
    content_type: &str,
    count: usize,
    media_type: impl Fn(usize) -> MediaType,
) -> Option<usize> {
    let essence = content_type.split(';').next().unwrap_or("").trim();

    (0..count).find(|&index| media_type(index).essence().eq_ignore_ascii_case(essence))
}

#[cfg(test)]
mod tests;
