//! Transport DTOs for the mobile API.
//!
//! These types mirror the provider payload and are deliberately separate from
//! the domain. They deserialize permissively where the API is inconsistent
//! (scalars as strings or numbers, one item or a list), and the conversion layer
//! validates and normalizes the result.

use serde::Deserialize;

/// A scalar wrapped in the provider's `{"value": ...}` envelope.
///
/// The provider uses an empty object for an absent scalar, for example
/// `"amount": {}`. The value is therefore optional even when the envelope is
/// present.
#[derive(Debug, Deserialize)]
pub(crate) struct Value<T> {
    pub(crate) value: Option<T>,
}

/// A scalar the API may return as a string or a JSON number.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum FlexibleString {
    String(String),
    Number(serde_json::Number),
}

impl FlexibleString {
    pub(crate) fn into_string(self) -> String {
        match self {
            Self::String(value) => value,
            Self::Number(value) => value.to_string(),
        }
    }

    pub(crate) fn to_plain_string(&self) -> String {
        match self {
            Self::String(value) => value.clone(),
            Self::Number(value) => value.to_string(),
        }
    }
}

/// A numeric scalar the API may return as a JSON number or a string.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum NumberOrString {
    Number(serde_json::Number),
    String(String),
}

impl NumberOrString {
    pub(crate) fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(value) => value.as_f64(),
            Self::String(value) => value.parse().ok(),
        }
    }
}

/// A boolean the API may return as a JSON boolean or a string.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum FlexibleBool {
    Bool(bool),
    String(String),
}

impl FlexibleBool {
    pub(crate) fn is_true(&self) -> bool {
        match self {
            Self::Bool(value) => *value,
            Self::String(value) => value.eq_ignore_ascii_case("true"),
        }
    }
}

/// One item or a list of items, as used by several provider fields.
///
/// The list variant comes first on purpose: a permissive struct can otherwise
/// deserialize from a one-element sequence as if it were the struct itself, and
/// a list would silently become a single item.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum OneOrMany<T> {
    Many(Vec<T>),
    One(T),
}

impl<T> OneOrMany<T> {
    pub(crate) fn into_vec(self) -> Vec<T> {
        match self {
            Self::One(value) => vec![value],
            Self::Many(values) => values,
        }
    }
}

/// Search response envelope.
#[derive(Debug, Deserialize)]
pub(crate) struct SearchResponse {
    #[serde(rename = "{http://www.ebayclassifiedsgroup.com/schema/ad/v1}ads")]
    pub(crate) ads: AdsEnvelope,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AdsEnvelope {
    pub(crate) value: AdsBlock,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AdsBlock {
    pub(crate) ad: Option<OneOrMany<Ad>>,
    pub(crate) paging: Option<Paging>,
}

/// Detail response envelope.
#[derive(Debug, Deserialize)]
pub(crate) struct DetailResponse {
    #[serde(rename = "{http://www.ebayclassifiedsgroup.com/schema/ad/v1}ad")]
    pub(crate) ad: AdEnvelope,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AdEnvelope {
    pub(crate) value: Ad,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Paging {
    #[serde(rename = "numFound")]
    pub(crate) num_found: Option<FlexibleString>,
    pub(crate) link: Option<OneOrMany<Link>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Ad {
    pub(crate) id: Option<FlexibleString>,
    pub(crate) title: Option<Value<String>>,
    pub(crate) description: Option<Value<String>>,
    pub(crate) price: Option<Price>,
    #[serde(rename = "ad-address")]
    pub(crate) address: Option<Address>,
    #[serde(rename = "start-date-time")]
    pub(crate) start_date_time: Option<Value<String>>,
    #[serde(rename = "ad-status")]
    pub(crate) status: Option<Value<String>>,
    #[serde(rename = "ad-type")]
    pub(crate) ad_type: Option<Value<String>>,
    #[serde(rename = "poster-type")]
    pub(crate) poster_type: Option<Value<String>>,
    pub(crate) category: Option<Category>,
    pub(crate) link: Option<OneOrMany<Link>>,
    pub(crate) pictures: Option<Pictures>,
    pub(crate) attributes: Option<Attributes>,
    #[serde(rename = "negotiation-enabled")]
    pub(crate) negotiation_enabled: Option<Value<FlexibleBool>>,
    #[serde(rename = "user-id")]
    pub(crate) user_id: Option<Value<String>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Price {
    pub(crate) amount: Option<Value<NumberOrString>>,
    #[serde(rename = "currency-iso-code")]
    pub(crate) currency_iso_code: Option<Currency>,
    #[serde(rename = "price-type")]
    pub(crate) price_type: Option<Value<String>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Currency {
    pub(crate) value: Option<Value<String>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Address {
    pub(crate) state: Option<Value<String>>,
    #[serde(rename = "zip-code")]
    pub(crate) zip_code: Option<Value<String>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Category {
    pub(crate) id: Option<FlexibleString>,
    #[serde(rename = "localized-name")]
    pub(crate) localized_name: Option<Value<String>>,
    #[serde(rename = "id-name")]
    pub(crate) id_name: Option<Value<String>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Link {
    pub(crate) href: Option<String>,
    pub(crate) rel: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Pictures {
    pub(crate) picture: Option<OneOrMany<Picture>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Picture {
    pub(crate) link: Option<OneOrMany<Link>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Attributes {
    pub(crate) attribute: Option<OneOrMany<Attribute>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Attribute {
    pub(crate) name: Option<String>,
    #[serde(rename = "localized-label")]
    pub(crate) localized_label: Option<String>,
    pub(crate) value: Option<OneOrMany<AttributeValue>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AttributeValue {
    pub(crate) value: Option<FlexibleString>,
    #[serde(rename = "localized-label")]
    pub(crate) localized_label: Option<String>,
}

/// Category reference response.
#[derive(Debug, Deserialize)]
pub(crate) struct CategoriesResponse {
    #[serde(rename = "{http://www.ebayclassifiedsgroup.com/schema/category/v1}categories")]
    pub(crate) categories: CategoryEnvelope,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CategoryEnvelope {
    pub(crate) value: CategoryRoot,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CategoryRoot {
    pub(crate) category: Option<OneOrMany<CategoryNode>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CategoryNode {
    pub(crate) id: Option<FlexibleString>,
    #[serde(rename = "id-name")]
    pub(crate) id_name: Option<Value<String>>,
    #[serde(rename = "localized-name")]
    pub(crate) localized_name: Option<Value<String>>,
    pub(crate) category: Option<OneOrMany<Box<CategoryNode>>>,
}

/// Location reference response.
#[derive(Debug, Deserialize)]
pub(crate) struct LocationsResponse {
    #[serde(rename = "{http://www.ebayclassifiedsgroup.com/schema/location/v1}locations")]
    pub(crate) locations: LocationEnvelope,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LocationEnvelope {
    pub(crate) value: LocationRoot,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LocationRoot {
    pub(crate) location: Option<OneOrMany<LocationNode>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LocationNode {
    pub(crate) id: Option<FlexibleString>,
    #[serde(rename = "id-name")]
    pub(crate) id_name: Option<Value<String>>,
    #[serde(rename = "localized-name")]
    pub(crate) localized_name: Option<Value<String>>,
    pub(crate) location: Option<OneOrMany<Box<LocationNode>>>,
}

/// Public seller profile response.
#[derive(Debug, Deserialize)]
pub(crate) struct ProfileResponse {
    pub(crate) id: Option<FlexibleString>,
    #[serde(rename = "userSince")]
    pub(crate) user_since: Option<String>,
    #[serde(rename = "contactName")]
    pub(crate) contact_name: Option<String>,
    #[serde(rename = "posterType")]
    pub(crate) poster_type: Option<String>,
    pub(crate) initials: Option<String>,
    pub(crate) counters: Option<ProfileCounters>,
    #[serde(rename = "replyIndicators")]
    pub(crate) reply_indicators: Option<ReplyIndicators>,
    #[serde(rename = "userBadges")]
    pub(crate) user_badges: Option<UserBadges>,
    #[serde(rename = "securePayment")]
    pub(crate) secure_payment: Option<bool>,
    #[serde(rename = "sellerAccountType")]
    pub(crate) seller_account_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProfileCounters {
    #[serde(rename = "historicalAds")]
    pub(crate) historical_ads: Option<u64>,
    #[serde(rename = "onlineAds")]
    pub(crate) online_ads: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReplyIndicators {
    #[serde(rename = "replySpeed")]
    pub(crate) reply_speed: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UserBadges {
    pub(crate) badges: Option<OneOrMany<ProfileBadge>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProfileBadge {
    pub(crate) name: Option<String>,
    pub(crate) level: Option<u64>,
    pub(crate) value: Option<String>,
}

/// Search metadata response.
#[derive(Debug, Deserialize)]
pub(crate) struct SearchMetadataResponse {
    #[serde(rename = "{http://www.ebayclassifiedsgroup.com/schema/ad/v1}ads-search-options")]
    pub(crate) options: SearchMetadataEnvelope,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SearchMetadataEnvelope {
    pub(crate) value: std::collections::HashMap<String, SearchFilter>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SearchFilter {
    #[serde(rename = "localized-label")]
    pub(crate) label: Option<String>,
    #[serde(rename = "type")]
    pub(crate) kind: Option<String>,
    #[serde(rename = "search-param")]
    pub(crate) search_param: Option<String>,
    #[serde(rename = "supported-value")]
    pub(crate) supported_value: Option<OneOrMany<FilterValue>>,
    pub(crate) deprecated: Option<FlexibleBool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FilterValue {
    pub(crate) value: Option<FlexibleString>,
    #[serde(rename = "localized-label")]
    pub(crate) localized_label: Option<String>,
}

/// View counter response.
#[derive(Debug, Deserialize)]
pub(crate) struct ViewsResponse {
    pub(crate) value: Option<u64>,
}

/// Watchlist counter response.
#[derive(Debug, Deserialize)]
pub(crate) struct WatchlistCountersResponse {
    pub(crate) counters: Option<OneOrMany<WatchlistCounter>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WatchlistCounter {
    #[serde(rename = "adId")]
    pub(crate) ad_id: Option<FlexibleString>,
    pub(crate) value: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_synthetic_search_fixture() {
        let response: SearchResponse =
            serde_json::from_str(include_str!("../../../tests/fixtures/search.json"))
                .expect("valid fixture");
        let ads = response.ads.value.ad.expect("ads").into_vec();
        assert_eq!(ads.len(), 2);

        let pictures = ads[0].pictures.as_ref().expect("pictures");
        match pictures.picture.as_ref().expect("picture") {
            OneOrMany::One(_) => panic!("expected a list of pictures"),
            OneOrMany::Many(pictures) => {
                assert_eq!(pictures.len(), 1);
                match pictures[0].link.as_ref().expect("picture links") {
                    OneOrMany::One(_) => panic!("expected a list of picture links"),
                    OneOrMany::Many(links) => assert_eq!(links.len(), 2),
                }
            }
        }

        let attributes = ads[0].attributes.as_ref().expect("attributes");
        match attributes.attribute.as_ref().expect("attribute") {
            OneOrMany::One(_) => panic!("expected a list of attributes"),
            OneOrMany::Many(attributes) => assert_eq!(attributes.len(), 1),
        }
    }
}
