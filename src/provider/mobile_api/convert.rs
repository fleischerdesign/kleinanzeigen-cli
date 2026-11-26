//! Conversion from provider DTOs to validated domain values.

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use url::Url;

use crate::domain::{Attribute, Category, Listing, ListingId, Location, Price};
use crate::error::{Error, Result};
use crate::provider::SearchPage;

use super::dto::{
    Ad, Attribute as AttributeDto, DetailResponse, Link, OneOrMany, SearchResponse, Value,
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
        next_cursor,
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

    let url = public_url(link)?;
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
        images,
        attributes,
    })
}

fn public_url(link: Option<OneOrMany<Link>>) -> Result<Url> {
    let links = link.map(OneOrMany::into_vec).unwrap_or_default();
    let href = links
        .iter()
        .find(|link| link.rel.as_deref() == Some("self-public-website"))
        .and_then(|link| link.href.clone());

    match href {
        Some(href) => Url::parse(&href)
            .map_err(|error| invalid(format!("invalid public listing URL: {error}"))),
        None => Err(invalid("listing has no public website URL")),
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

fn text_value(value: Option<Value<String>>) -> Option<String> {
    value
        .and_then(|value| value.value)
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
