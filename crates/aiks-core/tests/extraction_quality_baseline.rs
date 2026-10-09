use aiks_core::ai::schema_v3::V3ExtractionResult;
use serde_json::Value;
use std::collections::HashSet;
use std::time::Instant;

#[test]
fn extraction_baseline_has_diverse_reproducible_cases_and_valid_outputs() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/extraction_quality_baseline.json"))
        .unwrap();
    assert_eq!(fixture["version"], 1);
    let cases = fixture["cases"].as_array().unwrap();
    assert!(cases.len() >= 5);
    let mut ids = HashSet::new();
    let mut scenarios = HashSet::new();
    let mut valid_count = 0usize;
    let mut reusable_count = 0usize;
    let mut term_hits = 0usize;
    let mut term_total = 0usize;
    let started = Instant::now();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        assert!(ids.insert(id), "duplicate benchmark case ID: {id}");
        scenarios.insert(case["scenario"].as_str().unwrap());
        let extracted: V3ExtractionResult =
            serde_json::from_value(case["output"].clone()).unwrap();
        assert!(extracted.knowledge_score.is_finite());
        assert!((0.0..=1.0).contains(&extracted.knowledge_score));
        assert!(!extracted.session_summary.trim().is_empty());
        if !extracted.worth_extracting {
            assert!(extracted.items.is_empty());
        } else {
            assert!(!extracted.items.is_empty());
            reusable_count += 1;
        }
        let text = extracted
            .items
            .iter()
            .map(|item| {
                assert!(!item.title.trim().is_empty());
                assert!(!item.content.trim().is_empty());
                assert!(item.confidence.is_finite());
                assert!((0.0..=1.0).contains(&item.confidence));
                format!("{} {} {}", item.title, item.content, item.tags.join(" "))
            })
            .collect::<Vec<_>>()
            .join(" ");
        for term in case["expected_terms"].as_array().unwrap() {
            term_total += 1;
            if text.contains(term.as_str().unwrap()) {
                term_hits += 1;
            }
        }
        valid_count += 1;
    }
    assert_eq!(scenarios.len(), 5);
    assert_eq!(valid_count, cases.len());
    assert_eq!(term_hits, term_total);
    assert!(reusable_count < cases.len(), "noise case must be skipped");
    eprintln!(
        "AIKS_EXTRACTION_BASELINE cases={} valid_json={} reusable={} term_hits={}/{} schema_validation_ms={}",
        cases.len(),
        valid_count,
        reusable_count,
        term_hits,
        term_total,
        started.elapsed().as_millis()
    );
}
