use std::fs;
use tokenowl_lib::domain::provider::{
    ProviderErrorKind, QuotaUnit, parse_openai_compatible_response,
};

fn read_fixture(name: &str) -> String {
    let path = format!("tests/fixtures/{name}");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read {path}: {e}"))
}

#[test]
fn openai_success_fixture_parses_all_fields() {
    let body = read_fixture("openai-success.json");
    let set = parse_openai_compatible_response(200, &body).expect("success fixture");
    let snapshot = &set.snapshots[0];
    assert_eq!(snapshot.used, Some(2500.0));
    assert_eq!(snapshot.limit, Some(10000.0));
    assert_eq!(snapshot.remaining, Some(7500.0));
    assert_eq!(snapshot.reset_at.as_deref(), Some("2026-10-10T12:00:00Z"));
    assert_eq!(snapshot.unit, Some(QuotaUnit::Token));
}

#[test]
fn openai_partial_fixture_preserves_missing_as_none() {
    let body = read_fixture("openai-partial.json");
    let set = parse_openai_compatible_response(200, &body).expect("partial fixture");
    let snapshot = &set.snapshots[0];
    assert_eq!(snapshot.used, None);
    assert_eq!(snapshot.limit, None);
    assert_eq!(snapshot.remaining, Some(42.5));
    assert_eq!(snapshot.unit, Some(QuotaUnit::Credit));
}

#[test]
fn openai_windowed_fixture_parses_window_spans() {
    let body = read_fixture("openai-windowed.json");
    let set = parse_openai_compatible_response(200, &body).expect("windowed fixture");
    assert_eq!(set.snapshots.len(), 2);
    let five_hour = &set.snapshots[0];
    assert_eq!(five_hour.used, Some(420_000.0));
    assert_eq!(five_hour.limit, Some(1_000_000.0));
    assert_eq!(five_hour.remaining, Some(580_000.0));
    let window = five_hour.window.as_ref().expect("5h window present");
    assert_eq!(window.starts_at.as_deref(), Some("2026-10-31T09:00:00Z"));
    assert_eq!(window.ends_at.as_deref(), Some("2026-10-31T14:00:00Z"));
    assert_eq!(five_hour.reset_at.as_deref(), Some("2026-10-31T14:00:00Z"));
    let week = &set.snapshots[1];
    let window = week.window.as_ref().expect("7d window present");
    assert_eq!(window.starts_at.as_deref(), Some("2026-10-27T00:00:00Z"));
    assert_eq!(window.ends_at.as_deref(), Some("2026-11-03T00:00:00Z"));
}

#[test]
fn openai_unauthorized_fixture_maps_to_auth_error() {
    let body = read_fixture("openai-unauthorized.json");
    let error = parse_openai_compatible_response(401, &body).expect_err("unauthorized");
    assert_eq!(error.code, ProviderErrorKind::AuthRequired);
    assert_eq!(error.http_status, Some(401));
}

#[test]
fn openai_rate_limit_fixture_maps_to_rate_limited() {
    let body = read_fixture("openai-rate-limit.json");
    let error = parse_openai_compatible_response(429, &body).expect_err("rate limit");
    assert_eq!(error.code, ProviderErrorKind::RateLimited);
    assert_eq!(error.http_status, Some(429));
}

#[test]
fn anthropic_success_fixture_parses() {
    let body = read_fixture("anthropic-success.json");
    let set = parse_openai_compatible_response(200, &body).expect("anthropic fixture");
    assert_eq!(set.snapshots.len(), 1);
    assert_eq!(set.snapshots[0].remaining, Some(100.0));
}

#[test]
fn gemini_success_fixture_parses() {
    let body = read_fixture("gemini-success.json");
    let set = parse_openai_compatible_response(200, &body).expect("gemini fixture");
    assert_eq!(set.snapshots.len(), 1);
    assert_eq!(set.snapshots[0].remaining, Some(50000.0));
}

#[test]
fn deepseek_success_fixture_parses() {
    let body = read_fixture("deepseek-success.json");
    let set = parse_openai_compatible_response(200, &body).expect("deepseek fixture");
    assert_eq!(set.snapshots.len(), 1);
    assert_eq!(set.snapshots[0].remaining, Some(25.0));
    assert_eq!(set.snapshots[0].currency.as_deref(), Some("USD"));
}
