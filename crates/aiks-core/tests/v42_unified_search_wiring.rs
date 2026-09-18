#[test]
fn legacy_search_is_only_a_compatibility_wrapper_over_unified_search() {
    let source = include_str!("../src/pipeline/search.rs");

    assert!(source.contains("UnifiedSearchService"));
    assert!(!source.contains("fn fts_search("));
    assert!(!source.contains("fn vector_search("));
    assert!(!source.contains("0.35 * existing.score"));
    assert!(!source.contains("0.65 * hit.score"));
}
