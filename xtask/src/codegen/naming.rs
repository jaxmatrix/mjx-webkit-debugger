//! Turning protocol names into Rust identifiers.
//!
//! The rule, from `CLAUDE.md`: **the wire token is preserved exactly and never
//! guessed; the Rust identifier is self-explanatory.** Every emitted item
//! carries a `#[serde(rename = "…")]` with the original token, so the mapping
//! here only has to be *stable and unambiguous*, never reversible by eye.

use heck::{ToSnakeCase, ToUpperCamelCase};

/// Rust keywords that can be written as raw identifiers (`r#type`).
///
/// `self`, `super`, `crate` and `Self` cannot, so they are handled by the
/// trailing-underscore fallback in [`field_ident`].
const RAW_ESCAPABLE: &[&str] = &[
    "as", "break", "const", "continue", "else", "enum", "extern", "false", "fn", "for", "if",
    "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "static",
    "struct", "trait", "true", "type", "unsafe", "use", "where", "while", "async", "await", "dyn",
    "abstract", "become", "box", "do", "final", "macro", "override", "priv", "typeof", "unsized",
    "virtual", "yield", "try", "union",
];

/// Keywords that are not valid as raw identifiers.
const NEVER_RAW: &[&str] = &["self", "Self", "super", "crate"];

/// A domain name to its module name: `"DOMDebugger"` → `"dom_debugger"`.
pub fn module_ident(domain: &str) -> String {
    domain.to_snake_case()
}

/// A type, command, or event name to a type identifier: `"setBreakpointByUrl"`
/// → `"SetBreakpointByUrl"`, `"-webkit-scrollbar"` → `"WebkitScrollbar"`.
pub fn type_ident(name: &str) -> String {
    let camel = name.to_upper_camel_case();
    // A leading digit cannot start an identifier. None occur at the pinned ref,
    // but a future protocol addition must not silently emit broken Rust.
    if camel.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("V{camel}")
    } else {
        camel
    }
}

/// A field name to a struct-field identifier, escaping keywords.
///
/// Returns the identifier as it must appear in source — already `r#`-prefixed
/// where that is what Rust requires.
pub fn field_ident(name: &str) -> String {
    let snake = name.to_snake_case();
    if NEVER_RAW.contains(&snake.as_str()) {
        format!("{snake}_")
    } else if RAW_ESCAPABLE.contains(&snake.as_str()) {
        format!("r#{snake}")
    } else if snake.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("f_{snake}")
    } else {
        snake
    }
}

/// An enum variant identifier for a wire value.
///
/// `"alternate-reverse"` → `"AlternateReverse"`, `"-webkit-scrollbar"` →
/// `"WebkitScrollbar"`.
pub fn variant_ident(value: &str) -> String {
    type_ident(value)
}

/// The name of the enum synthesised for an anonymous inline enum.
///
/// `owner` is the type, command, or event the property belongs to. So
/// `Debugger.paused`'s `reason` property becomes `PausedReason`.
pub fn inline_enum_ident(owner: &str, property: &str) -> String {
    format!("{}{}", type_ident(owner), type_ident(property))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acronym_domains_become_readable_modules() {
        assert_eq!(module_ident("CSS"), "css");
        assert_eq!(module_ident("DOM"), "dom");
        assert_eq!(module_ident("DOMDebugger"), "dom_debugger");
        assert_eq!(module_ident("DOMStorage"), "dom_storage");
        assert_eq!(module_ident("CPUProfiler"), "cpu_profiler");
        assert_eq!(module_ident("IndexedDB"), "indexed_db");
        assert_eq!(module_ident("GenericTypes"), "generic_types");
        assert_eq!(module_ident("ServiceWorker"), "service_worker");
    }

    #[test]
    fn the_one_keyword_field_in_the_protocol_is_escaped() {
        // `type` is the only Rust keyword appearing as a field name at the
        // pinned ref, and it appears often.
        assert_eq!(field_ident("type"), "r#type");
    }

    #[test]
    fn keywords_that_cannot_be_raw_get_a_trailing_underscore() {
        assert_eq!(field_ident("self"), "self_");
        assert_eq!(field_ident("crate"), "crate_");
    }

    #[test]
    fn ordinary_fields_are_just_snake_case() {
        assert_eq!(field_ident("scriptId"), "script_id");
        assert_eq!(field_ident("sourceMapURL"), "source_map_url");
        assert_eq!(field_ident("lineNumber"), "line_number");
    }

    #[test]
    fn dashed_enum_values_become_camel_variants() {
        // The seven `-webkit-*` pseudo-element values are the only wire tokens
        // at the pinned ref that need more than a case change.
        assert_eq!(variant_ident("-webkit-scrollbar"), "WebkitScrollbar");
        assert_eq!(variant_ident("alternate-reverse"), "AlternateReverse");
        assert_eq!(variant_ident("service-worker"), "ServiceWorker");
    }

    #[test]
    fn inline_enums_are_named_after_their_owner_and_property() {
        assert_eq!(inline_enum_ident("paused", "reason"), "PausedReason");
        assert_eq!(
            inline_enum_ident("setPauseOnExceptions", "state"),
            "SetPauseOnExceptionsState"
        );
    }

    #[test]
    fn identifiers_never_start_with_a_digit() {
        assert_eq!(type_ident("2xx"), "V2xx");
        assert_eq!(field_ident("2fa"), "f_2fa");
    }
}
