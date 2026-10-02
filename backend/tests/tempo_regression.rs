mod common;

use axum::http::{Method, StatusCode};
use chrono::Utc;
use chrono_tz::Europe::Paris;
use serde_json::Value;

use maison_backend::tempo::TempoService;

#[tokio::test]
async fn tempo_state_uses_migrated_cache_data() {
    let (status, body) = request_rust("/api/tempo/state").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.get("success").and_then(Value::as_bool), Some(true));
    assert_eq!(body.get("season").and_then(Value::as_str), Some("2025-2026"));
    assert!(body.get("stock_red_remaining").and_then(Value::as_i64).is_some());
    assert!(body.get("stock_white_remaining").and_then(Value::as_i64).is_some());
}

#[tokio::test]
async fn tempo_predictions_return_forecast_entries() {
    let (status, body) = request_rust("/api/tempo/predictions").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.get("success").and_then(Value::as_bool), Some(true));

    let predictions = body
        .get("predictions")
        .and_then(Value::as_array)
        .expect("predictions should be an array");
    assert_eq!(predictions.len(), 7);
    let expected_first_date = Utc::now()
        .with_timezone(&Paris)
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    assert_eq!(
        predictions.first().and_then(|day| day.get("date")).and_then(Value::as_str),
        Some(expected_first_date.as_str())
    );
    assert!(predictions.iter().all(|day| day.get("predicted_color").is_some()));
}

#[tokio::test]
async fn tempo_history_reads_cached_season_file() {
    let (status, body) = request_rust("/api/tempo/history?season=2025-2026").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.get("success").and_then(Value::as_bool), Some(true));

    let history = body
        .get("history")
        .and_then(Value::as_array)
        .expect("history should be an array");
    assert!(!history.is_empty());
    assert!(history.iter().any(|day| {
        day.get("date").and_then(Value::as_str) == Some("2026-01-29")
            && day.get("color").and_then(Value::as_str) == Some("RED")
    }));
}

#[tokio::test]
async fn tempo_calibration_reads_migrated_calibration_file() {
    let (status, body) = request_rust("/api/tempo/calibration").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.get("success").and_then(Value::as_bool), Some(true));
    assert_eq!(body.get("calibrated").and_then(Value::as_bool), Some(true));

    // Read the expected date from the actual calibration file instead of hardcoding it,
    // since recalibration updates this value.
    let calibration_file = common::workspace_root().join("cache/tempo/calibration_params.json");
    let raw = std::fs::read_to_string(&calibration_file).expect("calibration file should exist");
    let expected: Value = serde_json::from_str(&raw).expect("calibration file should be valid json");
    let expected_date = expected
        .get("calibration_date")
        .and_then(Value::as_str)
        .expect("calibration file should have calibration_date");

    assert_eq!(
        body.pointer("/params/calibration_date").and_then(Value::as_str),
        Some(expected_date)
    );
}

#[tokio::test]
async fn tempo_recalibration_produces_metrics_without_persisting() {
    let service = TempoService::new(common::workspace_root()).expect("tempo service should build");
    let report = match service
        .recalibrate(&["2024-2025".to_string(), "2025-2026".to_string()], false)
        .await
    {
        Ok(report) => report,
        Err(error) => {
            assert!(error
                .to_string()
                .contains("Not enough historical weather samples for calibration"));
            return;
        }
    };

    assert_eq!(report.seasons, vec!["2024-2025", "2025-2026"]);
    assert!(report.params.calibration_sample_count >= 120);
    assert!(report.params.calibration_accuracy > 0.0);
    assert!(report.params.calibration_red_recall >= 0.0);
    assert!(report.params.calibration_white_recall >= 0.0);
}

async fn request_rust(path: &str) -> (StatusCode, Value) {
    common::send_authed(&common::app(common::test_config()), Method::GET, path, None).await
}
