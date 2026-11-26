//! Small HTML-to-text helpers for provider descriptions.
//!
//! The mobile API returns description and title text with HTML entities and
//! occasional `<br>` markers even when it labels the content as plain text.
//! This module is deliberately conservative: it decodes entities, turns a small
//! set of block tags into line breaks, and drops every other tag. It is not an
//! HTML renderer.

/// Converts provider text to a plain-text value.
pub(crate) fn to_text(input: &str) -> String {
    let mut text = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = rest.find('<') {
        text.push_str(&rest[..start]);
        let after = &rest[start..];
        let Some(end) = after.find('>') else {
            text.push_str(after);
            rest = "";
            break;
        };

        let tag = after[1..end].trim().to_ascii_lowercase();
        if tag.starts_with("br") {
            text.push('\n');
        } else if tag == "p" || tag == "/p" {
            text.push_str("\n\n");
        } else if tag == "li" {
            text.push_str("\n- ");
        } else if tag == "/li" {
            text.push('\n');
        }

        rest = &after[end + 1..];
    }
    text.push_str(rest);

    let decoded = decode_entities(&text);
    normalize_lines(&decoded)
}

/// Decodes common named and numeric HTML entities in one pass.
pub(crate) fn decode_entities(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = rest.find('&') {
        output.push_str(&rest[..start]);
        let after = &rest[start..];
        let Some(end) = after.find(';') else {
            output.push_str(after);
            return output;
        };

        let entity = &after[1..end];
        if let Some(character) = decode_entity(entity) {
            output.push(character);
            rest = &after[end + 1..];
        } else {
            output.push('&');
            rest = &after[1..];
        }
    }
    output.push_str(rest);
    output
}

fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            let number = entity.strip_prefix('#')?;
            let hex = number
                .strip_prefix('x')
                .or_else(|| number.strip_prefix('X'));
            let value = if let Some(hex) = hex {
                u32::from_str_radix(hex, 16).ok()?
            } else {
                number.parse::<u32>().ok()?
            };
            char::from_u32(value)
        }
    }
}

fn normalize_lines(input: &str) -> String {
    let input = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut output = String::with_capacity(input.len());
    let mut blank_lines = 0;

    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() {
            if !output.is_empty() && blank_lines < 2 {
                output.push('\n');
                blank_lines += 1;
            }
        } else {
            if !output.is_empty() && !output.ends_with('\n') {
                output.push('\n');
            }
            output.push_str(line);
            blank_lines = 0;
        }
    }

    output.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_named_and_numeric_entities() {
        assert_eq!(
            decode_entities("Tom &amp; Jerry &#x2F; &#47; &quot;ok&quot;"),
            "Tom & Jerry / / \"ok\""
        );
    }

    #[test]
    fn leaves_unknown_entities_untouched() {
        assert_eq!(decode_entities("&unknown; &amp;lt;"), "&unknown; &lt;");
    }

    #[test]
    fn converts_tags_and_paragraphs_to_lines() {
        assert_eq!(
            to_text("<p>Erste Zeile</p><p>Zweite<br />Zeile</p>"),
            "Erste Zeile\n\nZweite\nZeile"
        );
    }

    #[test]
    fn collapses_excess_blank_lines_to_one() {
        assert_eq!(to_text("A<br><br><br>B"), "A\n\nB");
    }

    #[test]
    fn keeps_text_when_a_tag_is_not_closed() {
        assert_eq!(to_text("A <b B"), "A <b B");
    }
}
