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

impl Partition<'_> {
    /// Whether the partition is one unguarded variant, which needs no dispatcher.
    pub fn is_simple(&self) -> bool {
        matches!(self.variants.as_slice(), [only] if only.guards.is_empty())
    }
}

/// The partitions of one route pattern.
pub struct RoutePlan<'a> {
    /// The route pattern.
    pub pattern: LitStr,
    /// One partition per method, in order of first appearance.
    pub partitions: Vec<Partition<'a>>,
}

/// Groups variants by pattern and then by method, keeping first-appearance and declaration
/// order, and rejects variants that can never be selected.
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

    Ok(plans)
}

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
