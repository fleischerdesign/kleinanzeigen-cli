//! Public reference and metadata values.

use serde::Serialize;

/// One provider category in a flattened tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CategoryRef {
    /// Provider category identifier.
    pub id: String,
    /// Human-readable category name.
    pub name: String,
    /// Full category path.
    pub path: String,
}

/// One provider location in a flattened tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocationRef {
    /// Provider location identifier.
    pub id: String,
    /// Short provider name, for example a postal code.
    pub name: String,
    /// Human-readable location label.
    pub label: String,
}

/// One search filter exposed by the provider for a category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchFilter {
    /// Provider filter name.
    pub name: String,
    /// Human-readable filter label.
    pub label: Option<String>,
    /// Provider filter type, for example `ENUM`.
    pub kind: Option<String>,
    /// Provider support hint for the query parameter.
    pub search_param: Option<String>,
    /// Whether the provider marks the filter as deprecated.
    pub deprecated: bool,
    /// Supported values for an enumerated filter.
    pub values: Vec<FilterValue>,
}

/// One supported value of a search filter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FilterValue {
    /// Provider value.
    pub value: String,
    /// Human-readable label.
    pub label: Option<String>,
}
