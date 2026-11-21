use dea_engine::json_filter::{FilterError, filter_json};
use dea_engine::pretty::{pretty_json, pretty_markup};

const SHIPMENT: &str = r#"{"id":"shp_8KQ2M4","status":"in_transit","origin":{"city":"Jakarta","hub":"CGK-02"},"destination":{"city":"Singapore","hub":"SIN-01"},"carrier":"Pacific Freight","weight_kg":12.4,"insured":true,"eta":"2026-11-05T09:30:00Z","events":[{"at":"2026-11-03T02:14:00Z","code":"picked_up"},{"at":"2026-11-03T08:40:00Z","code":"departed_hub"}]}"#;

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn pretty_json_keeps_numbers_and_key_order() {
    let out = pretty_json(r#"{"b":1.50,"a":[1e3,{}],"c":"x\"y"}"#).unwrap();
    assert_eq!(
        out,
        "{\n  \"b\": 1.50,\n  \"a\": [\n    1e3,\n    {}\n  ],\n  \"c\": \"x\\\"y\"\n}"
    );
}

#[test]
fn pretty_json_refuses_text_that_is_not_json() {
    assert_eq!(pretty_json("<html>"), None);
    assert_eq!(pretty_json("{\"a\":"), None);
    assert_eq!(pretty_json(""), None);
}

#[test]
fn filter_follows_rfc_9535() {
    let doc = SHIPMENT;
    assert_eq!(
        filter_json(doc, "$.events[*].code").unwrap().text,
        "[\n  \"picked_up\",\n  \"departed_hub\"\n]"
    );
    assert_eq!(filter_json(doc, "$..city").unwrap().count, 2);
    assert_eq!(
        filter_json(doc, "$.events[?@.code=='picked_up'].at")
            .unwrap()
            .count,
        1
    );
    assert!(matches!(
        filter_json(doc, "$.events["),
        Err(FilterError::InvalidPath(_))
    ));
    assert!(matches!(
        filter_json("<html>", "$"),
        Err(FilterError::NotJson)
    ));
}

#[test]
fn filter_without_matches_is_an_empty_array() {
    let out = filter_json(SHIPMENT, "$.nothing").unwrap();
    assert_eq!((out.text.as_str(), out.count), ("[]", 0));
}

#[test]
fn filter_results_keep_key_order() {
    let out = filter_json(
        r#"{"origin":{"hub":"CGK-02","city":"Jakarta"}}"#,
        "$.origin",
    )
    .unwrap();
    let hub = out.text.find("hub").unwrap();
    let city = out.text.find("city").unwrap();
    assert!(hub < city);
}

#[test]
fn pretty_xml_indents_nested_elements() {
    assert_eq!(
        pretty_markup("<a><b>t</b><c/></a>", false),
        "<a>\n  <b>t</b>\n  <c/>\n</a>"
    );
}

#[test]
fn markup_keeps_every_character_but_whitespace() {
    let xml = "<?xml version=\"1.0\"?><r><!-- a  note --><d><![CDATA[ x < y ]]></d><e a=\"1 2\">text</e></r>";
    let out = pretty_markup(xml, false);
    assert_eq!(squash(&out), squash(xml));
    assert!(out.contains("<!-- a  note -->"));
    assert!(out.contains("<![CDATA[ x < y ]]>"));

    let pre = "<pre>  line1\n    line2  </pre>";
    let html = format!(
        "<html><body><br><p>hi</p>{pre}<script>if (a<b) {{ x(); }}</script><img src=a.png><div><span>z</span></div></body></html>"
    );
    let out = pretty_markup(&html, true);
    assert_eq!(squash(&out), squash(&html));
    assert!(out.contains(pre));
    assert!(out.contains("<script>if (a<b) { x(); }</script>"));
    // void elements add no indent: <p> sits at the same level as <br>
    let indent = |needle: &str| {
        let line = out.lines().find(|l| l.contains(needle)).unwrap();
        line.len() - line.trim_start().len()
    };
    assert_eq!(indent("<br>"), indent("<p>"));
}

#[test]
fn malformed_markup_does_not_panic() {
    for s in [
        "<a><b></a>",
        "</x></y><z>",
        "<a",
        "<!-- open",
        "<![CDATA[ x",
        "<<>>",
        "<pre>x",
        "",
    ] {
        let out = pretty_markup(s, true);
        assert_eq!(squash(&out), squash(s));
    }
}
