use std::{cmp::Ordering, collections::HashSet, fmt};

/// Most path parameters one route can capture.
pub const MAX_PARAMS: usize = 16;

/// One segment of a route pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment {
    /// Matches exactly this text.
    Literal(&'static str),
    /// Matches any one non-empty segment and captures it.
    Param(&'static str),
    /// Matches any one non-empty segment without capturing it (`*`).
    Any,
    /// Matches the remaining segments, at least one, optionally capturing them as one string (`**` or `{**name}`). Always last.
    Rest(Option<&'static str>),
}

/// Why a route pattern was rejected at startup.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PatternError {
    /// The pattern does not start with `/`.
    #[error("pattern `{0}` must start with `/`")]
    MissingLeadingSlash(&'static str),

    /// The pattern has an empty segment, such as `//` or a trailing `/`.
    #[error("pattern `{0}` has an empty segment")]
    EmptySegment(&'static str),

    /// A `{}` or `{**}` placeholder has no name.
    #[error("pattern `{0}` has an unnamed parameter")]
    UnnamedParam(&'static str),

    /// A `**` segment is not the last one.
    #[error("pattern `{0}` has a wildcard that is not last")]
    WildcardNotLast(&'static str),

    /// Two parameters share a name.
    #[error("pattern `{0}` repeats parameter `{1}`")]
    DuplicateParam(&'static str, &'static str),

    /// The pattern captures more than [`MAX_PARAMS`] parameters.
    #[error("pattern `{0}` captures more than {MAX_PARAMS} parameters")]
    TooManyParams(&'static str),
}

/// Fixed-capacity stack buffer of captured parameter values borrowed from the request path.
#[derive(Debug, Clone, Copy)]
pub struct ParamBuf<'a> {
    values: [&'a str; MAX_PARAMS],
    len: usize,
}

impl<'a> ParamBuf<'a> {
    /// An empty buffer.
    pub const fn new() -> Self {
        Self {
            values: [""; MAX_PARAMS],
            len: 0,
        }
    }

    /// Captured values in declaration order.
    pub fn as_slice(&self) -> &[&'a str] {
        &self.values[..self.len]
    }

    fn push(&mut self, value: &'a str) -> bool {
        if self.len == MAX_PARAMS {
            return false;
        }

        self.values[self.len] = value;
        self.len += 1;

        true
    }
}

impl Default for ParamBuf<'_> {
    fn default() -> Self {
        Self::new()
    }
}

/// A parsed route pattern such as `/users/{id}/files/{**path}`, matched without allocating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    segments: Vec<Segment>,
    names: Vec<&'static str>,
}

impl Pattern {
    /// Parses a pattern; `/` is the root.
    pub fn parse(source: &'static str) -> Result<Self, PatternError> {
        let mut segments = Vec::new();
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        let body = source
            .strip_prefix('/')
            .ok_or(PatternError::MissingLeadingSlash(source))?;

        if !body.is_empty() {
            for raw in body.split('/') {
                if matches!(segments.last(), Some(Segment::Rest(_))) {
                    return Err(PatternError::WildcardNotLast(source));
                }

                let segment = parse_segment(source, raw)?;

                if let Segment::Param(name) | Segment::Rest(Some(name)) = segment {
                    if !seen.insert(name) {
                        return Err(PatternError::DuplicateParam(source, name));
                    }

                    if names.len() == MAX_PARAMS {
                        return Err(PatternError::TooManyParams(source));
                    }

                    names.push(name);
                }

                segments.push(segment);
            }
        }

        Ok(Self { segments, names })
    }

    /// Parsed segments.
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// Parameter names in declaration order, matching the values in [`ParamBuf`].
    pub fn names(&self) -> &[&'static str] {
        &self.names
    }

    /// `base` followed by `path` as one pattern, such as a controller's path and a route under it.
    ///
    /// Each half must parse on its own. Together they must not repeat a parameter name, exceed
    /// [`MAX_PARAMS`], or put anything after a `**` in `base`.
    pub fn join(base: &'static str, path: &'static str) -> Result<Self, PatternError> {
        let base_pattern = Self::parse(base)?;
        let path_pattern = Self::parse(path)?;

        if matches!(base_pattern.segments.last(), Some(Segment::Rest(_)))
            && !path_pattern.segments.is_empty()
        {
            return Err(PatternError::WildcardNotLast(base));
        }

        if let Some(name) = path_pattern
            .names
            .iter()
            .find(|name| base_pattern.names.contains(name))
        {
            return Err(PatternError::DuplicateParam(path, name));
        }

        if base_pattern.names.len() + path_pattern.names.len() > MAX_PARAMS {
            return Err(PatternError::TooManyParams(path));
        }

        let Self {
            mut segments,
            mut names,
        } = base_pattern;

        segments.extend(path_pattern.segments);
        names.extend(path_pattern.names);

        Ok(Self { segments, names })
    }

    /// Orders patterns most specific first: segment by segment, a literal before `{param}`,
    /// before `*`, before `**`.
    pub fn cmp_specificity(&self, other: &Self) -> Ordering {
        self.segments
            .iter()
            .map(Segment::rank)
            .cmp(other.segments.iter().map(Segment::rank))
    }

    /// Whether both patterns match exactly the same paths, whatever their parameters are named.
    pub fn same_shape(&self, other: &Self) -> bool {
        self.segments.len() == other.segments.len()
            && self
                .segments
                .iter()
                .zip(&other.segments)
                .all(|pair| match pair {
                    (Segment::Literal(left), Segment::Literal(right)) => left == right,
                    (left, right) => {
                        !matches!(left, Segment::Literal(_)) && left.rank() == right.rank()
                    }
                })
    }

    /// The literal first segment, when the pattern starts with one.
    pub fn first_literal(&self) -> Option<&'static str> {
        match self.segments.first() {
            Some(Segment::Literal(text)) => Some(text),
            _ => None,
        }
    }

    /// Matches `path`, filling `out` with borrowed values on success.
    pub fn matches<'a>(&self, path: &'a str, out: &mut ParamBuf<'a>) -> bool {
        let Some(mut rest) = path.strip_prefix('/') else {
            return false;
        };

        out.len = 0;

        if self.segments.is_empty() {
            return rest.is_empty();
        }

        for (index, segment) in self.segments.iter().enumerate() {
            if let Segment::Rest(name) = segment {
                return !rest.is_empty() && (name.is_none() || out.push(rest));
            }

            let (head, tail) = match rest.split_once('/') {
                Some((head, tail)) => (head, Some(tail)),
                None => (rest, None),
            };

            match segment {
                Segment::Literal(text) if head != *text => return false,
                Segment::Param(_) if head.is_empty() || !out.push(head) => return false,
                Segment::Any if head.is_empty() => return false,
                _ => {}
            }

            match tail {
                Some(tail) => rest = tail,
                None => return index + 1 == self.segments.len(),
            }
        }

        false
    }
}

impl Segment {
    fn rank(&self) -> u8 {
        match self {
            Self::Literal(_) => 0,
            Self::Param(_) => 1,
            Self::Any => 2,
            Self::Rest(_) => 3,
        }
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.segments.is_empty() {
            return f.write_str("/");
        }

        for segment in &self.segments {
            match segment {
                Segment::Literal(text) => write!(f, "/{text}")?,
                Segment::Param(name) => write!(f, "/{{{name}}}")?,
                Segment::Any => f.write_str("/*")?,
                Segment::Rest(Some(name)) => write!(f, "/{{**{name}}}")?,
                Segment::Rest(None) => f.write_str("/**")?,
            }
        }

        Ok(())
    }
}

fn parse_segment(source: &'static str, raw: &'static str) -> Result<Segment, PatternError> {
    if raw.is_empty() {
        return Err(PatternError::EmptySegment(source));
    }

    if raw == "*" {
        return Ok(Segment::Any);
    }

    if raw == "**" {
        return Ok(Segment::Rest(None));
    }

    let Some(inner) = raw.strip_prefix('{').and_then(|raw| raw.strip_suffix('}')) else {
        return Ok(Segment::Literal(raw));
    };

    match inner.strip_prefix("**") {
        Some("") => Err(PatternError::UnnamedParam(source)),
        Some(name) => Ok(Segment::Rest(Some(name))),
        None if inner.is_empty() => Err(PatternError::UnnamedParam(source)),
        None => Ok(Segment::Param(inner)),
    }
}

#[cfg(test)]
mod tests;
