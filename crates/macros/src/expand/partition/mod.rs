use syn::{Error, LitStr};

use super::model::Variant;
use crate::route::HttpMethod;

/// The variants that answer one method on one pattern, in declaration order.
pub struct Partition<'a> {
    /// The HTTP method.
    pub method: HttpMethod,
    /// The variants, tried in order.
    pub variants: Vec<&'a Variant>,
}

/// The partitions of one route pattern.
pub struct RoutePlan<'a> {
    /// The route pattern.
    pub pattern: LitStr,
    /// One partition per method, in order of first appearance.
    pub partitions: Vec<Partition<'a>>,
}

/// How specific a pattern is, segment by segment: a literal beats a `{param}`, which beats `*`,
/// which beats a trailing `**`. A smaller key is more specific.
fn specificity(pattern: &str) -> Vec<u8> {
    pattern
        .trim_start_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| match segment {
            "**" => 3,
            "*" => 2,
            other if other.starts_with("{**") => 3,
            other if other.starts_with('{') => 1,
            _ => 0,
        })
        .collect()
}

/// Groups variants by pattern and then by method, and rejects variants that can never be
/// selected.
///
/// Routes come out most specific first, so `/items/orders` is tried before `/items/{id}` whatever
/// order they were declared in; the router uses the first route that matches and does not fall
/// through to a later one. Routes that are equally specific keep their declaration order, and so
/// do the variants within a route.
pub fn plan(variants: &[Variant]) -> syn::Result<Vec<RoutePlan<'_>>> {
    let mut plans: Vec<RoutePlan> = Vec::new();

    for variant in variants {
        let index = plans
            .iter()
            .position(|plan| plan.pattern.value() == variant.pattern.value());
        let plan = match index {
            Some(index) => &mut plans[index],
            None => {
                plans.push(RoutePlan {
                    pattern: variant.pattern.clone(),
                    partitions: Vec::new(),
                });

                plans.last_mut().expect("a plan was just pushed")
            }
        };

        match plan
            .partitions
            .iter_mut()
            .find(|partition| partition.method == variant.method)
        {
            Some(partition) => {
                check_reachable(partition, variant)?;
                partition.variants.push(variant);
            }
            None => plan.partitions.push(Partition {
                method: variant.method,
                variants: vec![variant],
            }),
        }
    }

    plans.sort_by_key(|plan| specificity(&plan.pattern.value()));

    Ok(plans)
}

#[cfg(test)]
mod tests;

fn check_reachable(partition: &Partition, variant: &Variant) -> syn::Result<()> {
    for earlier in &partition.variants {
        if earlier.when_text == variant.when_text {
            return Err(Error::new_spanned(
                &variant.pattern,
                "this route repeats an earlier endpoint with the same method, path and `when`",
            ));
        }

        if earlier.guards.is_empty() {
            return Err(Error::new_spanned(
                &variant.pattern,
                "this endpoint is unreachable: an earlier one for the same method and path has no `when` guard and always wins",
            ));
        }
    }

    Ok(())
}
