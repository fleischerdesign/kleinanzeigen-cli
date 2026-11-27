//! Category and location reference resolution.
//!
//! The mobile API accepts numeric category and location ids. This module turns
//! user-supplied names or postal codes into those ids and keeps the matching
//! rules out of the HTTP layer.

use std::collections::HashSet;

use crate::domain::{CategoryRef, LocationRef};
use crate::error::{Error, Result};

use super::dto::{CategoryNode, CategoryRoot, FlexibleString, LocationNode, LocationRoot};

/// Flattens the category response into leaf and branch categories.
pub(crate) fn categories(root: CategoryRoot) -> Vec<CategoryRef> {
    let mut out = Vec::new();
    if let Some(roots) = root.category {
        for root in roots.into_vec() {
            if let Some(children) = root.category {
                for child in children.into_vec() {
                    walk_category(*child, &mut Vec::new(), &mut out);
                }
            }
        }
    }
    out
}

fn walk_category(node: CategoryNode, path: &mut Vec<String>, out: &mut Vec<CategoryRef>) {
    let CategoryNode {
        id,
        id_name,
        localized_name,
        category,
    } = node;
    let name = localized_name
        .or(id_name)
        .and_then(|value| value.value)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());

    if let Some(name) = name {
        path.push(name.clone());
        if let Some(id) = id
            .map(FlexibleString::into_string)
            .filter(|id| !id.is_empty())
        {
            out.push(CategoryRef {
                id,
                name,
                path: path.join(" > "),
            });
        }
        if let Some(children) = category {
            for child in children.into_vec() {
                walk_category(*child, path, out);
            }
        }
        path.pop();
    } else if let Some(children) = category {
        for child in children.into_vec() {
            walk_category(*child, path, out);
        }
    }
}

/// Flattens the location response into a deduplicated list.
pub(crate) fn locations(root: LocationRoot) -> Vec<LocationRef> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    if let Some(nodes) = root.location {
        for node in nodes.into_vec() {
            walk_location(node, &mut out, &mut seen);
        }
    }
    out
}

fn walk_location(node: LocationNode, out: &mut Vec<LocationRef>, seen: &mut HashSet<String>) {
    let LocationNode {
        id,
        id_name,
        localized_name,
        location,
        ..
    } = node;
    let name = id_name
        .and_then(|value| value.value)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let label = localized_name
        .and_then(|value| value.value)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .or_else(|| name.clone());

    if let Some(id) = id
        .map(FlexibleString::into_string)
        .filter(|id| !id.is_empty())
    {
        if let (Some(name), Some(label)) = (name, label) {
            if seen.insert(id.clone()) {
                out.push(LocationRef { id, name, label });
            }
        }
    }

    if let Some(children) = location {
        for child in children.into_vec() {
            walk_location(*child, out, seen);
        }
    }
}

/// Resolves a category id or name to a numeric id string.
pub(crate) fn resolve_category(categories: &[CategoryRef], value: &str) -> Result<String> {
    let value = value.trim();
    let exact = categories
        .iter()
        .filter(|category| category.name.eq_ignore_ascii_case(value))
        .collect::<Vec<_>>();

    match exact.len() {
        1 => Ok(exact[0].id.clone()),
        count if count > 1 => Err(ambiguous_category(value, &exact)),
        _ => {
            let candidates = matching_categories(categories, value);
            match candidates.len() {
                0 => Err(Error::Input(format!(
                    "unknown category '{value}'; pass a numeric id or an exact category name"
                ))),
                1 => Ok(candidates[0].id.clone()),
                _ => Err(ambiguous_category(value, &candidates)),
            }
        }
    }
}

fn matching_categories<'a>(categories: &'a [CategoryRef], value: &str) -> Vec<&'a CategoryRef> {
    let lower = value.to_lowercase();
    let mut matches = categories
        .iter()
        .filter(|category| {
            category.name.to_lowercase().contains(&lower)
                || category.path.to_lowercase().contains(&lower)
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|category| (category.name.len(), category.name.as_str()));
    matches.truncate(6);
    matches
}

fn ambiguous_category(value: &str, matches: &[&CategoryRef]) -> Error {
    let options = matches
        .iter()
        .map(|category| format!("{} ({})", category.id, category.path))
        .collect::<Vec<_>>()
        .join("; ");
    Error::Input(format!(
        "category '{value}' is ambiguous; use a numeric id: {options}"
    ))
}

/// Resolves a location name or postal code to a numeric id string.
pub(crate) fn resolve_location(locations: &[LocationRef], value: &str) -> Result<String> {
    let value = value.trim();
    let exact = locations
        .iter()
        .filter(|location| {
            location.name.eq_ignore_ascii_case(value) || location.label.eq_ignore_ascii_case(value)
        })
        .collect::<Vec<_>>();

    match exact.len() {
        1 => Ok(exact[0].id.clone()),
        count if count > 1 => Err(ambiguous_location(value, &exact)),
        _ => {
            let candidates = matching_locations(locations, value);
            match candidates.len() {
                0 => Err(Error::Input(format!(
                    "unknown location '{value}'; pass a numeric location id or a known place name"
                ))),
                1 => Ok(candidates[0].id.clone()),
                _ => Err(ambiguous_location(value, &candidates)),
            }
        }
    }
}

fn matching_locations<'a>(locations: &'a [LocationRef], value: &str) -> Vec<&'a LocationRef> {
    let lower = value.to_lowercase();
    let mut matches = locations
        .iter()
        .filter(|location| {
            location.name.to_lowercase().contains(&lower)
                || location.label.to_lowercase().contains(&lower)
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|location| (location.label.len(), location.label.as_str()));
    matches.truncate(6);
    matches
}

fn ambiguous_location(value: &str, matches: &[&LocationRef]) -> Error {
    let options = matches
        .iter()
        .map(|location| format!("{} ({})", location.id, location.label))
        .collect::<Vec<_>>()
        .join("; ");
    Error::Input(format!(
        "location '{value}' is ambiguous; use a numeric id: {options}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn category(id: &str, name: &str, path: &str) -> CategoryRef {
        CategoryRef {
            id: id.to_owned(),
            name: name.to_owned(),
            path: path.to_owned(),
        }
    }

    fn location(id: &str, name: &str, label: &str) -> LocationRef {
        LocationRef {
            id: id.to_owned(),
            name: name.to_owned(),
            label: label.to_owned(),
        }
    }

    #[test]
    fn resolves_an_exact_category_name() {
        let categories = vec![
            category(
                "217",
                "Fahrräder & Zubehör",
                "Auto, Rad & Boot > Fahrräder & Zubehör",
            ),
            category("80", "Sonstiges", "Haus & Garten > Sonstiges"),
        ];
        assert_eq!(
            resolve_category(&categories, "Fahrräder & Zubehör").expect("category"),
            "217"
        );
    }

    #[test]
    fn resolves_a_single_category_prefix() {
        let categories = vec![category(
            "217",
            "Fahrräder & Zubehör",
            "Auto, Rad & Boot > Fahrräder & Zubehör",
        )];
        assert_eq!(
            resolve_category(&categories, "Fahrräder").expect("category"),
            "217"
        );
    }

    #[test]
    fn rejects_ambiguous_and_unknown_categories() {
        let categories = vec![
            category("1", "Synthetic", "A > Synthetic"),
            category("2", "Synthetic", "B > Synthetic"),
        ];
        assert!(resolve_category(&categories, "Synthetic").is_err());
        assert!(resolve_category(&categories, "Unknown").is_err());
    }

    #[test]
    fn resolves_exact_and_partial_locations() {
        let locations = vec![
            location("3331", "Berlin", "Berlin"),
            location("23309", "Berlingen", "Berlingen (Rheinland-Pfalz)"),
        ];
        assert_eq!(
            resolve_location(&locations, "Berlin").expect("location"),
            "3331"
        );
        assert_eq!(
            resolve_location(&locations, "Rheinland-Pfalz").expect("location"),
            "23309"
        );
    }

    #[test]
    fn rejects_ambiguous_locations() {
        let locations = vec![
            location("1", "Teststadt", "Teststadt (Nord)"),
            location("2", "Teststadt", "Teststadt (Süd)"),
        ];
        assert!(resolve_location(&locations, "Teststadt").is_err());
    }

    #[test]
    fn flattens_the_synthetic_category_tree() {
        let json = r#"{
            "{http://www.ebayclassifiedsgroup.com/schema/category/v1}categories": {
                "value": {
                    "category": [
                        {
                            "id-name": { "value": "Alle Kategorien" },
                            "localized-name": { "value": "Alle Kategorien" },
                            "category": [
                                {
                                    "id": "100",
                                    "id-name": { "value": "Synthetic parent" },
                                    "localized-name": { "value": "Synthetic parent" },
                                    "category": [
                                        {
                                            "id": "101",
                                            "localized-name": { "value": "Synthetic child" }
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                }
            }
        }"#;
        let response: super::super::dto::CategoriesResponse =
            serde_json::from_str(json).expect("valid category response");
        let categories = categories(response.categories.value);
        assert_eq!(categories.len(), 2);
        assert_eq!(categories[0].id, "100");
        assert_eq!(categories[1].path, "Synthetic parent > Synthetic child");
    }

    #[test]
    fn flattens_the_synthetic_location_tree() {
        let json = r#"{
            "{http://www.ebayclassifiedsgroup.com/schema/location/v1}locations": {
                "value": {
                    "location": [
                        {
                            "id": "3331",
                            "id-name": { "value": "Teststadt" },
                            "localized-name": { "value": "Teststadt" },
                            "location": [
                                {
                                    "id": "3332",
                                    "id-name": { "value": "Testdorf" },
                                    "localized-name": { "value": "Testdorf (Testland)" }
                                }
                            ]
                        }
                    ]
                }
            }
        }"#;
        let response: super::super::dto::LocationsResponse =
            serde_json::from_str(json).expect("valid location response");
        let locations = locations(response.locations.value);
        assert_eq!(locations.len(), 2);
        assert_eq!(locations[0].id, "3331");
        assert_eq!(locations[1].label, "Testdorf (Testland)");
    }
}
