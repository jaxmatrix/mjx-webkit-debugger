//! Serde types for WebKit's protocol descriptions.
//!
//! These mirror `Source/JavaScriptCore/inspector/protocol/*.json` exactly. Any
//! member we do not model would be silently dropped, so the structs are
//! deliberately complete rather than minimal — including `condition`, which
//! drives the variant-merging rule in [`super::merge`].

use serde::Deserialize;

/// One `*.json` file: a domain, or the pseudo-domain `GenericTypes`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DomainFile {
    pub domain: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub types: Vec<TypeDef>,
    #[serde(default)]
    pub commands: Vec<Command>,
    #[serde(default)]
    pub events: Vec<Event>,
    /// The kinds of debuggable that expose this domain at all. This is the axis
    /// `activateDomain` uses in the inspector frontend.
    #[serde(default, rename = "debuggableTypes")]
    pub debuggable_types: Vec<String>,
    /// The kinds of target within a debuggable that expose this domain. A
    /// strict superset of `debuggableTypes` — see `Domain`'s docs in
    /// `mjx-wk-protocol`.
    #[serde(default, rename = "targetTypes")]
    pub target_types: Vec<String>,
    /// A C preprocessor condition gating the whole domain.
    ///
    /// Parsed but unused: `deny_unknown_fields` is what makes this generator
    /// fail loudly when WebKit adds a schema member, so every member is
    /// modelled even when nothing reads it yet.
    #[serde(default)]
    #[allow(dead_code)]
    pub condition: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub version: Option<u32>,
}

/// A named type declared by a domain.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeDef {
    pub id: String,
    /// `"object"`, `"string"`, `"integer"`, `"number"`, `"array"`, `"boolean"`.
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub properties: Vec<Property>,
    /// Present when this is a closed set of string values.
    #[serde(default, rename = "enum")]
    pub enum_values: Option<Vec<String>>,
    /// Present when `ty == "array"`.
    #[serde(default)]
    pub items: Option<Box<Property>>,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default, rename = "minItems")]
    pub min_items: Option<u32>,
    #[serde(default, rename = "maxItems")]
    pub max_items: Option<u32>,
}

/// A struct field, a command parameter, a return value, or an array's element.
///
/// Array elements are the reason `name` is optional: `{"items": {"$ref": "X"}}`
/// is a `Property` with no name.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Property {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, rename = "type")]
    pub ty: Option<String>,
    #[serde(default, rename = "$ref")]
    pub reference: Option<String>,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub description: Option<String>,
    /// An anonymous closed set of values. The generator synthesises a named
    /// enum for these — see `super::emit::inline_enum_name`.
    #[serde(default, rename = "enum")]
    pub enum_values: Option<Vec<String>>,
    #[serde(default)]
    pub items: Option<Box<Property>>,
    /// See the note on [`DomainFile::condition`].
    #[serde(default)]
    #[allow(dead_code)]
    pub condition: Option<String>,
}

/// A request the debugger can send.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parameters: Vec<Property>,
    #[serde(default)]
    pub returns: Vec<Property>,
    /// A C preprocessor condition. Commands appearing twice under complementary
    /// conditions are merged rather than emitted twice.
    #[serde(default)]
    pub condition: Option<String>,
    /// Target types this *command* is available on, narrowing the domain's own
    /// list. A panel can use this to know a command exists only for, say, page
    /// targets, rather than discovering it by getting an error back.
    #[serde(default, rename = "targetTypes")]
    pub target_types: Vec<String>,
    #[serde(default, rename = "async")]
    pub is_async: bool,
}

/// An unsolicited message from the debuggee.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parameters: Vec<Property>,
    #[serde(default)]
    pub condition: Option<String>,
    /// Target types this *event* is emitted for. See [`Command::target_types`].
    #[serde(default, rename = "targetTypes")]
    pub target_types: Vec<String>,
}
