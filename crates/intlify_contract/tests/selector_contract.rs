// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use intlify_contract::{
    CatalogKey, CatalogKeyDomain, CatalogKeyPattern, CatalogKeyPrefix, CatalogKeyToken,
    MessageSelector, PatternToken,
};

struct KeyFixture {
    domain: CatalogKeyDomain,
    input: &'static str,
    decoded_tokens: &'static [&'static str],
}

#[test]
fn public_key_constructors_accept_the_cross_domain_conformance_fixture() {
    let fixtures = [
        KeyFixture {
            domain: CatalogKeyDomain::JsonPointer,
            input: "",
            decoded_tokens: &[],
        },
        KeyFixture {
            domain: CatalogKeyDomain::JsonPointer,
            input: "/checkout/a~1b~0c",
            decoded_tokens: &["checkout", "a/b~c"],
        },
        KeyFixture {
            domain: CatalogKeyDomain::YamlTypedPath,
            input: "/k:str:checkout/k:int:1/i:0",
            decoded_tokens: &["k:str:checkout", "k:int:1", "i:0"],
        },
        KeyFixture {
            domain: CatalogKeyDomain::Xliff12,
            input: "/file:original:app.json/group:id:menu/unit:id:welcome",
            decoded_tokens: &["file:original:app.json", "group:id:menu", "unit:id:welcome"],
        },
        KeyFixture {
            domain: CatalogKeyDomain::Xliff2,
            input: "/file:id:f1/group:i:0/unit:id:u1/segment:id:s1",
            decoded_tokens: &["file:id:f1", "group:i:0", "unit:id:u1", "segment:id:s1"],
        },
    ];

    for fixture in fixtures {
        let key = CatalogKey::try_new(fixture.domain, fixture.input).unwrap();
        assert_eq!(key.domain(), fixture.domain);
        assert_eq!(key.as_str(), fixture.input);
        assert_eq!(
            key.tokens()
                .iter()
                .map(CatalogKeyToken::as_str)
                .collect::<Vec<_>>(),
            fixture.decoded_tokens
        );
    }
}

#[test]
fn public_key_constructors_reject_noncanonical_cross_domain_spellings() {
    for (domain, input) in [
        (CatalogKeyDomain::JsonPointer, "checkout/title"),
        (CatalogKeyDomain::JsonPointer, "/checkout/~2"),
        (CatalogKeyDomain::YamlTypedPath, "/k:int:01"),
        (CatalogKeyDomain::YamlTypedPath, "/i:+1"),
        (
            CatalogKeyDomain::Xliff12,
            "/file:original:app.json/segment:id:s1",
        ),
        (
            CatalogKeyDomain::Xliff2,
            "/file:id:f1/unit:id:u1/side:source",
        ),
    ] {
        assert!(
            CatalogKey::try_new(domain, input).is_err(),
            "{domain:?} unexpectedly accepted {input:?}"
        );
    }
}

#[test]
fn public_selector_fixture_preserves_structural_payloads_and_variant_order() {
    let exact = MessageSelector::Exact(
        CatalogKey::try_new(CatalogKeyDomain::JsonPointer, "/checkout/title").unwrap(),
    );
    let prefix = MessageSelector::Prefix(
        CatalogKeyPrefix::try_new(CatalogKeyDomain::JsonPointer, "/checkout").unwrap(),
    );
    let pattern = MessageSelector::Pattern(
        CatalogKeyPattern::try_new(CatalogKeyDomain::JsonPointer, "/checkout/*/**/~2").unwrap(),
    );

    assert_eq!(exact.kind_str(), "exact");
    assert_eq!(prefix.kind_str(), "prefix");
    assert_eq!(pattern.kind_str(), "pattern");
    assert!(exact < prefix);
    assert!(prefix < pattern);
    assert!(pattern < MessageSelector::AllInScope);
    assert!(MessageSelector::AllInScope < MessageSelector::UnboundedDynamic);

    let MessageSelector::Pattern(pattern) = pattern else {
        unreachable!("the fixture constructs a pattern");
    };
    assert!(matches!(
        pattern.tokens(),
        [
            PatternToken::Literal(_),
            PatternToken::AnyOne,
            PatternToken::AnyMany,
            PatternToken::Literal(_)
        ]
    ));
}

#[test]
fn equality_and_order_use_exact_utf8_without_normalization() {
    let composed = CatalogKey::try_new(CatalogKeyDomain::JsonPointer, "/é").unwrap();
    let decomposed = CatalogKey::try_new(CatalogKeyDomain::JsonPointer, "/e\u{301}").unwrap();
    let ascii = CatalogKey::try_new(CatalogKeyDomain::JsonPointer, "/z").unwrap();

    assert_ne!(composed, decomposed);
    assert!(ascii < composed);
    assert!(decomposed < composed);
}
