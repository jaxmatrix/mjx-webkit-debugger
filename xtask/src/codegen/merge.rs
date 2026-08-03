//! Collapsing platform-conditional duplicates into one item.
//!
//! WebKit declares some members twice under complementary `condition`s. At the
//! pinned ref there are four, all in `DOM`:
//!
//! ```text
//! highlightNode          condition:  defined(WTF_PLATFORM_IOS_FAMILY) && …
//! highlightNode          condition: !(defined(WTF_PLATFORM_IOS_FAMILY) && …)
//! ```
//!
//! The non-iOS variant is a strict superset — it adds an optional `showRulers`.
//!
//! Emitting both is impossible (same Rust name) and picking one is wrong (we
//! must drive both kinds of debuggee from one binary). So they are **merged by
//! union of fields, and any field absent from some variant becomes optional**.
//! Because an absent optional is not serialised, an iOS debuggee never receives
//! the field it does not know — the merge is safe in the direction that matters.

use std::collections::BTreeMap;

use anyhow::{Result, bail};

use super::schema::{Command, DomainFile, Event, Property, TypeDef};

/// A domain with its conditional duplicates collapsed.
#[derive(Debug)]
pub struct MergedDomain {
    pub name: String,
    pub description: Option<String>,
    pub types: Vec<TypeDef>,
    pub commands: Vec<Command>,
    pub events: Vec<Event>,
    pub debuggable_types: Vec<String>,
    pub target_types: Vec<String>,
}

/// Collapse every duplicated member in one domain.
pub fn domain(file: &DomainFile) -> Result<MergedDomain> {
    Ok(MergedDomain {
        name: file.domain.clone(),
        description: file.description.clone(),
        types: merge_by(&file.types, |t| t.id.clone(), merge_types)?,
        commands: merge_by(&file.commands, |c| c.name.clone(), merge_commands)?,
        events: merge_by(&file.events, |e| e.name.clone(), merge_events)?,
        debuggable_types: file.debuggable_types.clone(),
        target_types: file.target_types.clone(),
    })
}

/// Group items by name, preserving declaration order, and fold each group.
fn merge_by<T: Clone, K, F>(items: &[T], key: K, fold: F) -> Result<Vec<T>>
where
    K: Fn(&T) -> String,
    F: Fn(&T, &T) -> Result<T>,
{
    let mut order: Vec<String> = Vec::new();
    let mut groups: BTreeMap<String, T> = BTreeMap::new();

    for item in items {
        let k = key(item);
        match groups.get(&k) {
            None => {
                order.push(k.clone());
                groups.insert(k, item.clone());
            }
            Some(existing) => {
                let merged = fold(existing, item)?;
                groups.insert(k, merged);
            }
        }
    }

    Ok(order
        .into_iter()
        .filter_map(|k| groups.remove(&k))
        .collect())
}

/// Union two property lists. A property missing from either side, or optional
/// on either side, is optional in the result.
fn merge_properties(a: &[Property], b: &[Property]) -> Vec<Property> {
    let mut out: Vec<Property> = Vec::new();

    for prop in a.iter().chain(b.iter()) {
        let name = prop.name.clone().unwrap_or_default();
        match out.iter_mut().find(|p| p.name.as_deref() == Some(&name)) {
            Some(existing) => {
                // Present on both sides: optional if either side says so.
                existing.optional |= prop.optional;
            }
            None => {
                let mut cloned = prop.clone();
                // Present on one side only: it cannot be required.
                let on_both = a.iter().any(|p| p.name.as_deref() == Some(&name))
                    && b.iter().any(|p| p.name.as_deref() == Some(&name));
                if !on_both {
                    cloned.optional = true;
                }
                out.push(cloned);
            }
        }
    }
    out
}

fn merge_commands(a: &Command, b: &Command) -> Result<Command> {
    if a.name != b.name {
        bail!("cannot merge commands with different names");
    }
    Ok(Command {
        name: a.name.clone(),
        description: a.description.clone().or_else(|| b.description.clone()),
        parameters: merge_properties(&a.parameters, &b.parameters),
        returns: merge_properties(&a.returns, &b.returns),
        condition: merged_condition(&a.condition, &b.condition),
        target_types: union(&a.target_types, &b.target_types),
        is_async: a.is_async || b.is_async,
    })
}

fn merge_events(a: &Event, b: &Event) -> Result<Event> {
    Ok(Event {
        name: a.name.clone(),
        description: a.description.clone().or_else(|| b.description.clone()),
        parameters: merge_properties(&a.parameters, &b.parameters),
        condition: merged_condition(&a.condition, &b.condition),
        target_types: union(&a.target_types, &b.target_types),
    })
}

/// Union of two lists, preserving order and dropping duplicates.
///
/// Used for `targetTypes`: a member available on either variant's targets is
/// available on their union, since the variants are complementary builds.
fn union(a: &[String], b: &[String]) -> Vec<String> {
    let mut out = a.to_vec();
    out.extend(b.iter().filter(|s| !a.contains(s)).cloned());
    out
}

fn merge_types(a: &TypeDef, b: &TypeDef) -> Result<TypeDef> {
    if a.ty != b.ty {
        bail!(
            "type `{}` is declared as both `{}` and `{}`; the generator has no rule for that",
            a.id,
            a.ty,
            b.ty
        );
    }
    let enum_values = match (&a.enum_values, &b.enum_values) {
        (Some(x), Some(y)) => {
            let mut v = x.clone();
            v.extend(y.iter().filter(|s| !x.contains(s)).cloned());
            Some(v)
        }
        (some, None) | (None, some) => some.clone(),
    };
    Ok(TypeDef {
        id: a.id.clone(),
        ty: a.ty.clone(),
        description: a.description.clone().or_else(|| b.description.clone()),
        properties: merge_properties(&a.properties, &b.properties),
        enum_values,
        items: a.items.clone().or_else(|| b.items.clone()),
        condition: merged_condition(&a.condition, &b.condition),
        min_items: a.min_items.or(b.min_items),
        max_items: a.max_items.or(b.max_items),
    })
}

/// Record that the merged item came from variants, so the doc comment can say
/// so. Complementary conditions cancel out to "always present".
fn merged_condition(a: &Option<String>, b: &Option<String>) -> Option<String> {
    match (a, b) {
        (Some(x), Some(y)) => Some(format!("{x} | {y}")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prop(name: &str, optional: bool) -> Property {
        Property {
            name: Some(name.into()),
            ty: Some("boolean".into()),
            reference: None,
            optional,
            description: None,
            enum_values: None,
            items: None,
            condition: None,
        }
    }

    fn cmd(name: &str, params: Vec<Property>, condition: &str) -> Command {
        Command {
            name: name.into(),
            description: None,
            parameters: params,
            returns: vec![],
            condition: Some(condition.into()),
            target_types: vec![],
            is_async: false,
        }
    }

    #[test]
    fn a_field_present_on_only_one_variant_becomes_optional() {
        // This is the real `DOM.highlightNode` case: the iOS build has no
        // `showRulers`. Merging must not make it a required field, or every
        // call would serialise a member iOS rejects.
        let ios = cmd("highlightNode", vec![prop("nodeId", true)], "IOS");
        let other = cmd(
            "highlightNode",
            vec![prop("nodeId", true), prop("showRulers", true)],
            "!IOS",
        );
        let merged = merge_commands(&ios, &other).unwrap();
        let rulers = merged
            .parameters
            .iter()
            .find(|p| p.name.as_deref() == Some("showRulers"))
            .unwrap();
        assert!(rulers.optional);
        assert_eq!(merged.parameters.len(), 2);
    }

    #[test]
    fn a_required_field_on_both_variants_stays_required() {
        let a = cmd("x", vec![prop("enabled", false)], "A");
        let b = cmd("x", vec![prop("enabled", false)], "!A");
        let merged = merge_commands(&a, &b).unwrap();
        assert!(!merged.parameters[0].optional);
    }

    #[test]
    fn a_field_optional_on_either_side_is_optional_in_the_merge() {
        let a = cmd("x", vec![prop("enabled", false)], "A");
        let b = cmd("x", vec![prop("enabled", true)], "!A");
        assert!(merge_commands(&a, &b).unwrap().parameters[0].optional);
    }

    #[test]
    fn merging_preserves_declaration_order_and_collapses_the_group() {
        let items = vec![
            cmd("b", vec![], "A"),
            cmd("a", vec![], "A"),
            cmd("b", vec![], "!A"),
        ];
        let merged = merge_by(&items, |c| c.name.clone(), merge_commands).unwrap();
        assert_eq!(
            merged.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["b", "a"]
        );
    }

    #[test]
    fn complementary_conditions_cancel_to_unconditional() {
        let a = cmd("x", vec![], "IOS");
        let b = cmd("x", vec![], "!IOS");
        assert!(merge_commands(&a, &b).unwrap().condition.is_some());
        // A single unconditional command keeps no condition at all.
        let lone = Command {
            condition: None,
            ..cmd("y", vec![], "")
        };
        assert!(merged_condition(&lone.condition, &None).is_none());
    }
}
