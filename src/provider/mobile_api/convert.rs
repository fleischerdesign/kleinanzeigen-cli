//! Conversion from provider DTOs to validated domain values.

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use url::Url;

use crate::domain::{
    Attribute, Category, FilterValue, Listing, ListingId, Location, Price, SearchFilter,
    SellerBadge, SellerProfile,
};
use crate::error::{Error, Result};
use crate::provider::{ListingList, SearchPage};

use super::dto::{
    Ad, Attribute as AttributeDto, DetailResponse, FlexibleString, Link, OneOrMany,
    ProfileResponse, SearchMetadataResponse, SearchResponse, Value, ViewsResponse,
    WatchlistCountersResponse,
};
use super::html;

/// Converts a search response into a search page.
pub(crate) fn search_page(response: SearchResponse, page: u32, size: u32) -> Result<SearchPage> {
    let block = response.ads.value;
    let items = block
        .ad
        .map(OneOrMany::into_vec)
        .unwrap_or_default()
        .into_iter()
        .map(ad_to_listing)
        .collect::<Result<Vec<_>>>()?;

    let (total, next_cursor) = match block.paging {
        Some(paging) => {
            let total = match paging.num_found {
                Some(value) => value
                    .into_string()
                    .parse::<u64>()
                    .map_err(|_| invalid("search total is not a number"))?,
                None => items.len() as u64,
            };
            (total, next_cursor(paging.link)?)
        }
        None => (items.len() as u64, None),
    };

    Ok(SearchPage {
        items,
        total,
        page,
        size,
        pages_fetched: 1,
        next_cursor,
    })
}

/// Converts a discovery response into a bounded listing list.
pub(crate) fn listing_list(response: SearchResponse) -> Result<ListingList> {
    let page = search_page(response, 1, 0)?;
    Ok(ListingList {
        items: page.items,
        total: page.total,
    })
}

/// Converts a detail response into a listing.
pub(crate) fn detail(response: DetailResponse) -> Result<Listing> {
    ad_to_listing(response.ad.value)
}

fn ad_to_listing(ad: Ad) -> Result<Listing> {
    let Ad {
        id,
        title,
        description,
        price,
        address,
        start_date_time,
        status,
        ad_type,
        poster_type,
        category,
        link,
        pictures,
        attributes,
        negotiation_enabled,
        user_id,
    } = ad;

    let id_value = id
        .ok_or_else(|| invalid("listing has no id"))?
        .into_string();
    let listing_id = ListingId::new(id_value.as_str()).map_err(|error| {
        Error::Format(format!("provider response for listing {id_value}: {error}"))
    })?;

    let title = title
        .and_then(|value| value.value)
        .map(|value| html::decode_entities(&value).trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(format!("listing {id_value} has no title")))?;

    let url = public_url(link, &id_value)?;
    let price_type = price
        .as_ref()
        .and_then(|price| price.price_type.as_ref())
        .and_then(|value| value.value.as_deref())
        .map(ToOwned::to_owned);
    let negotiable = price_type
        .as_deref()
        .is_some_and(|value| matches!(value, "PLEASE_CONTACT" | "NEGOTIABLE"))
        || negotiation_enabled
            .and_then(|value| value.value)
            .map(|value| value.is_true())
            .unwrap_or(false);
    let price = price_from_dto(price, &id_value, negotiable)?;
    let location = location_from_dto(address);
    let description = description
        .and_then(|value| value.value)
        .map(|value| html::to_text(&value))
        .filter(|value| !value.is_empty());
    let posted_at = start_date_time
        .and_then(|value| value.value)
        .map(|value| parse_timestamp(&value, &id_value))
        .transpose()?;
    let category = category_from_dto(category);
    let (rooms, size_m2) = rooms_and_size(attributes.as_ref());
    let images = images_from_dto(pictures)?;
    let attributes = attributes_from_dto(attributes);

    Ok(Listing {
        id: listing_id,
        title,
        url,
        price,
        price_type,
        location,
        description,
        posted_at,
        status: text_value(status),
        ad_type: text_value(ad_type),
        poster_type: text_value(poster_type),
        category,
        seller_id: text_value(user_id),
        rooms,
        size_m2,
        images,
        attributes,
    })
}

fn public_url(link: Option<OneOrMany<Link>>, id: &str) -> Result<Url> {
    let links = link.map(OneOrMany::into_vec).unwrap_or_default();
    let href = links
        .iter()
        .find(|link| link.rel.as_deref() == Some("self-public-website"))
        .and_then(|link| link.href.clone());

    match href {
        Some(href) => Url::parse(&href)
            .map_err(|error| invalid(format!("invalid public listing URL: {error}"))),
        None => Url::parse(&format!("https://www.kleinanzeigen.de/s-anzeige/{id}"))
            .map_err(|error| invalid(format!("invalid fallback listing URL: {error}"))),
    }
}

fn price_from_dto(
    price: Option<super::dto::Price>,
    id: &str,
    negotiable: bool,
) -> Result<Option<Price>> {
    let Some(price) = price else {
        return Ok(None);
    };
    let Some(amount) = price.amount.as_ref().and_then(|value| value.value.as_ref()) else {
        return Ok(None);
    };
    let Some(amount) = amount.as_f64() else {
        return Err(invalid(format!("listing {id} has a non-numeric price")));
    };

    if !amount.is_finite() || amount < 0.0 {
        return Err(invalid(format!("listing {id} has an invalid price")));
    }

    let cents = (amount * 100.0).round();
    if cents > i64::MAX as f64 {
        return Err(invalid(format!("listing {id} price is out of range")));
    }

    let currency = price
        .currency_iso_code
        .as_ref()
        .and_then(|currency| currency.value.as_ref())
        .and_then(|value| value.value.as_deref())
        .filter(|currency| !currency.is_empty())
        .ok_or_else(|| invalid(format!("listing {id} has no price currency")))?;

    Price::new(cents as i64, currency, negotiable)
        .map(Some)
        .map_err(|error| Error::Format(format!("provider response for listing {id}: {error}")))
}

fn location_from_dto(address: Option<super::dto::Address>) -> Option<Location> {
    let address = address?;
    let city = address
        .state
        .and_then(|value| value.value)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let postal_code = address
        .zip_code
        .and_then(|value| value.value)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());

    if city.is_none() && postal_code.is_none() {
        None
    } else {
        Some(Location::new(city, postal_code))
    }
}

fn category_from_dto(category: Option<super::dto::Category>) -> Option<Category> {
    let category = category?;
    let id = category
        .id
        .map(|value| value.into_string())
        .filter(|value| !value.is_empty())?;
    let name = category
        .localized_name
        .or(category.id_name)
        .and_then(|value| value.value)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| id.clone());

    Some(Category { id, name })
}

fn images_from_dto(pictures: Option<super::dto::Pictures>) -> Result<Vec<Url>> {
    let pictures = pictures
        .and_then(|pictures| pictures.picture)
        .map(OneOrMany::into_vec)
        .unwrap_or_default();
    let mut images = Vec::with_capacity(pictures.len());

    for picture in pictures {
        let links = picture.link.map(OneOrMany::into_vec).unwrap_or_default();
        let Some(href) = best_picture_link(&links) else {
            continue;
        };
        let url =
            Url::parse(&href).map_err(|error| invalid(format!("invalid image URL: {error}")))?;
        images.push(url);
    }

    Ok(images)
}

fn best_picture_link(links: &[Link]) -> Option<String> {
    const PREFERRED: [&str; 4] = ["large", "XXL", "teaser", "thumbnail"];

    PREFERRED
        .iter()
        .find_map(|preferred| {
            links
                .iter()
                .find(|link| link.rel.as_deref() == Some(*preferred))
                .and_then(|link| link.href.clone())
        })
        .or_else(|| links.first().and_then(|link| link.href.clone()))
}

fn attributes_from_dto(attributes: Option<super::dto::Attributes>) -> Vec<Attribute> {
    let attributes = attributes
        .and_then(|attributes| attributes.attribute)
        .map(OneOrMany::into_vec)
        .unwrap_or_default();

    attributes
        .into_iter()
        .filter_map(attribute_from_dto)
        .collect()
}

fn attribute_from_dto(attribute: AttributeDto) -> Option<Attribute> {
    let name = attribute.name.filter(|name| !name.is_empty())?;
    let label = attribute
        .localized_label
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| name.clone());
    let values = attribute.value.map(OneOrMany::into_vec).unwrap_or_default();
    let value = values
        .into_iter()
        .filter_map(|value| {
            value
                .localized_label
                .filter(|label| !label.is_empty())
                .or_else(|| value.value.map(|value| value.into_string()))
        })
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(", ");

    if value.is_empty() {
        None
    } else {
        Some(Attribute { name, label, value })
    }
}

/// Converts a public seller profile.
pub(crate) fn profile(response: ProfileResponse) -> Result<SellerProfile> {
    let id = response
        .id
        .ok_or_else(|| invalid("seller profile has no id"))?
        .into_string();
    if id.is_empty() {
        return Err(invalid("seller profile has an empty id"));
    }

    let (historical_ads, online_ads) = response
        .counters
        .map(|counters| (counters.historical_ads, counters.online_ads))
        .unwrap_or((None, None));
    let badges = response
        .user_badges
        .and_then(|badges| badges.badges)
        .map(OneOrMany::into_vec)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|badge| {
            let name = text_plain(badge.name)?;
            Some(SellerBadge {
                name,
                level: badge.level,
                value: text_plain(badge.value),
            })
        })
        .collect();

    Ok(SellerProfile {
        id,
        name: text_plain(response.contact_name),
        since: text_plain(response.user_since),
        poster_type: text_plain(response.poster_type),
        initials: text_plain(response.initials),
        historical_ads,
        online_ads,
        reply_speed: response
            .reply_indicators
            .and_then(|indicators| indicators.reply_speed)
            .and_then(|value| text_plain(Some(value))),
        badges,
        secure_payment: response.secure_payment.unwrap_or(false),
        seller_account_type: text_plain(response.seller_account_type),
    })
}

/// Converts category search metadata into stable filters.
pub(crate) fn search_metadata(response: SearchMetadataResponse) -> Vec<SearchFilter> {
    let mut filters = response
        .options
        .value
        .into_iter()
        .map(|(name, filter)| {
            let values = filter
                .supported_value
                .map(OneOrMany::into_vec)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|entry| {
                    let value = entry
                        .value
                        .map(FlexibleString::into_string)
                        .filter(|value| !value.is_empty())?;
                    Some(FilterValue {
                        value,
                        label: text_plain(entry.localized_label),
                    })
                })
                .collect();

            SearchFilter {
                name,
                label: text_plain(filter.label),
                kind: text_plain(filter.kind),
                search_param: text_plain(filter.search_param),
                deprecated: filter
                    .deprecated
                    .map(|deprecated| deprecated.is_true())
                    .unwrap_or(false),
                values,
            }
        })
        .collect::<Vec<_>>();
    filters.sort_by(|left, right| left.name.cmp(&right.name));
    filters
}

/// Reads the view count.
pub(crate) fn views(response: ViewsResponse) -> Result<u64> {
    response
        .value
        .ok_or_else(|| invalid("view counter has no value"))
}

/// Finds one listing's watchlist count in a counter response.
pub(crate) fn watchlist_count(response: WatchlistCountersResponse, id: &ListingId) -> Option<u64> {
    response
        .counters
        .map(OneOrMany::into_vec)
        .unwrap_or_default()
        .into_iter()
        .find(|counter| {
            counter
                .ad_id
                .as_ref()
                .is_some_and(|counter_id| counter_id.to_plain_string() == id.as_str())
        })
        .and_then(|counter| counter.value)
}

fn rooms_and_size(attributes: Option<&super::dto::Attributes>) -> (Option<f64>, Option<f64>) {
    let Some(attributes) = attributes.and_then(|attributes| attributes.attribute.as_ref()) else {
        return (None, None);
    };
    let attributes = match attributes {
        OneOrMany::One(attribute) => std::slice::from_ref(attribute),
        OneOrMany::Many(attributes) => attributes.as_slice(),
    };

    let mut rooms = None;
    let mut size_m2 = None;
    for attribute in attributes {
        let Some(name) = attribute.name.as_deref() else {
            continue;
        };
        let name = name.to_ascii_lowercase();
        if rooms.is_none() && name.ends_with(".zimmer") {
            rooms = attribute_number(attribute);
        } else if size_m2.is_none() && name.ends_with(".qm") {
            size_m2 = attribute_number(attribute);
        }
    }

    (rooms, size_m2)
}

fn attribute_number(attribute: &AttributeDto) -> Option<f64> {
    let values = attribute.value.as_ref()?;
    let first = match values {
        OneOrMany::One(value) => value,
        OneOrMany::Many(values) => values.first()?,
    };

    if let Some(value) = &first.value {
        if let Some(number) = parse_number(&value.to_plain_string()) {
            return Some(number);
        }
    }
    first.localized_label.as_deref().and_then(parse_number)
}

fn parse_number(value: &str) -> Option<f64> {
    let normalized = value.trim().replace(',', ".");
    let numeric = normalized
        .chars()
        .take_while(|character| {
            character.is_ascii_digit() || *character == '.' || *character == '-'
        })
        .collect::<String>();
    numeric
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
}

fn text_value(value: Option<Value<String>>) -> Option<String> {
    value
        .and_then(|value| value.value)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn text_plain(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn parse_timestamp(value: &str, id: &str) -> Result<OffsetDateTime> {
    let mut value = value.trim().to_owned();
    if value.len() >= 5 {
        let split = value.len() - 5;
        let bytes = value.as_bytes();
        let is_offset = matches!(bytes[split], b'+' | b'-')
            && bytes[split + 1..].iter().all(u8::is_ascii_digit);
        if is_offset {
            value.insert(split + 3, ':');
        }
    }

    OffsetDateTime::parse(&value, &Rfc3339)
        .map_err(|error| invalid(format!("listing {id} has an invalid timestamp: {error}")))
}

fn next_cursor(link: Option<OneOrMany<Link>>) -> Result<Option<String>> {
    let links = link.map(OneOrMany::into_vec).unwrap_or_default();
    let Some(next) = links
        .into_iter()
        .find(|link| link.rel.as_deref() == Some("next"))
    else {
        return Ok(None);
    };
    let href = next
        .href
        .ok_or_else(|| invalid("next-page link has no href"))?;
    let url =
        Url::parse(&href).map_err(|error| invalid(format!("invalid next-page link: {error}")))?;
    let page = url
        .query_pairs()
        .find(|(key, _)| key == "page")
        .and_then(|(_, value)| value.parse::<u32>().ok())
        .filter(|page| *page > 0)
        .ok_or_else(|| invalid("next-page link has no valid page"))?;

    Ok(Some(page.to_string()))
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Format(format!("provider response: {}", message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_amount_envelope_means_no_price() {
        let json = r#"{
            "{http://www.ebayclassifiedsgroup.com/schema/ad/v1}ads": {
                "value": {
                    "ad": {
                        "id": "1234567890",
                        "title": { "value": "Synthetic listing" },
                        "link": [
                            {
                                "href": "https://www.example.invalid/s-anzeige/synthetic/1234567890",
                                "rel": "self-public-website"
                            }
                        ],
                        "price": {
                            "amount": {},
                            "currency-iso-code": { "value": { "value": "EUR" } },
                            "price-type": { "value": "PLEASE_CONTACT" }
                        }
                    }
                }
            }
        }"#;
        let response: super::super::dto::SearchResponse =
            serde_json::from_str(json).expect("valid response");
        let page = search_page(response, 1, 25).expect("search page");
        assert_eq!(page.items.len(), 1);
        assert!(page.items[0].price.is_none());
        assert_eq!(page.items[0].price_type.as_deref(), Some("PLEASE_CONTACT"));
    }

    #[test]
    fn extracts_rooms_and_size_and_uses_a_url_fallback() {
        let json = r#"{
            "{http://www.ebayclassifiedsgroup.com/schema/ad/v1}ads": {
                "value": {
                    "ad": {
                        "id": "1234567890",
                        "title": { "value": "Synthetic apartment" },
                        "attributes": {
                            "attribute": [
                                {
                                    "name": "wohnung_mieten.zimmer",
                                    "value": [{ "value": "2,5" }]
                                },
                                {
                                    "name": "wohnung_mieten.qm",
                                    "value": [{ "value": "75,5" }]
                                }
                            ]
                        }
                    }
                }
            }
        }"#;
        let response: SearchResponse = serde_json::from_str(json).expect("valid response");
        let page = search_page(response, 1, 1).expect("search page");
        let listing = &page.items[0];
        assert_eq!(listing.rooms, Some(2.5));
        assert_eq!(listing.size_m2, Some(75.5));
        assert_eq!(
            listing.url.as_str(),
            "https://www.kleinanzeigen.de/s-anzeige/1234567890"
        );
    }

    #[test]
    fn converts_a_synthetic_profile() {
        let json = r#"{
            "id": "42",
            "contactName": "Synthetic Seller",
            "userSince": "2020-01-01",
            "posterType": "PRIVATE",
            "initials": "SS",
            "counters": { "historicalAds": 3, "onlineAds": 2 },
            "replyIndicators": { "replySpeed": "3h" },
            "userBadges": {
                "badges": [
                    { "name": "rating", "level": 2, "value": "good" }
                ]
            },
            "securePayment": true,
            "sellerAccountType": "private"
        }"#;
        let response: ProfileResponse = serde_json::from_str(json).expect("valid profile");
        let profile = profile(response).expect("profile conversion");
        assert_eq!(profile.id, "42");
        assert_eq!(profile.name.as_deref(), Some("Synthetic Seller"));
        assert_eq!(profile.historical_ads, Some(3));
        assert_eq!(profile.online_ads, Some(2));
        assert_eq!(profile.reply_speed.as_deref(), Some("3h"));
        assert_eq!(profile.badges.len(), 1);
        assert!(profile.secure_payment);
    }

    #[test]
    fn converts_sorted_search_metadata() {
        let json = r#"{
            "{http://www.ebayclassifiedsgroup.com/schema/ad/v1}ads-search-options": {
                "value": {
                    "price": {
                        "localized-label": "Price",
                        "type": "STRING",
                        "deprecated": "false"
                    },
                    "adType": {
                        "localized-label": "Type",
                        "type": "ENUM",
                        "search-param": "optional",
                        "supported-value": [
                            { "value": "OFFERED", "localized-label": "Offer" },
                            { "value": "WANTED", "localized-label": "Want" }
                        ],
                        "deprecated": "false"
                    }
                }
            }
        }"#;
        let response: SearchMetadataResponse = serde_json::from_str(json).expect("valid metadata");
        let filters = search_metadata(response);
        assert_eq!(filters.len(), 2);
        assert_eq!(filters[0].name, "adType");
        assert_eq!(filters[0].values.len(), 2);
        assert_eq!(filters[1].name, "price");
    }

    #[test]
    fn reads_view_and_watchlist_counters() {
        let views_response: ViewsResponse = serde_json::from_str(r#"{"value": 7}"#).expect("views");
        assert_eq!(views(views_response).expect("view count"), 7);

        let counters: WatchlistCountersResponse =
            serde_json::from_str(r#"{"counters":[{"adId":"123","value":2}]}"#).expect("counters");
        let id = ListingId::new("123").expect("id");
        assert_eq!(watchlist_count(counters, &id), Some(2));
    }

    #[test]
    fn parses_mobile_api_timestamps_with_compact_offsets() {
        let timestamp =
            parse_timestamp("2026-11-26T23:48:37.000+0100", "1").expect("valid timestamp");
        assert_eq!(timestamp.year(), 2026);
        assert_eq!(timestamp.offset().whole_hours(), 1);
    }

    #[test]
    fn parses_rfc3339_with_colon_and_z() {
        assert!(parse_timestamp("2026-11-26T23:48:37+01:00", "1").is_ok());
        assert!(parse_timestamp("2026-11-26T22:48:37Z", "1").is_ok());
    }

    #[test]
    fn rejects_invalid_timestamps() {
        assert!(parse_timestamp("not a date", "1").is_err());
    }
}
