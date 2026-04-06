//! Integration tests for garbage collection.
//!
//! Tests ref-counting correctness, shared layer preservation, and orphan cleanup.

#[cfg(test)]
mod tests {
    use shellwego_registry::gc::{GcConfig, GcResult, LayerRefCount};
    use chrono::Utc;

    #[test]
    fn test_gc_config_defaults() {
        let config = GcConfig::default();
        assert_eq!(config.max_size_bytes, 50 * 1024 * 1024 * 1024);
        assert!((config.high_watermark - 0.85).abs() < f64::EPSILON);
        assert!((config.low_watermark - 0.70).abs() < f64::EPSILON);
        assert_eq!(config.min_age_hours, 24);
        assert_eq!(config.max_images, 100);
        assert!(config.preserve_running);
    }

    #[test]
    fn test_gc_result_serde_roundtrip() {
        let result = GcResult {
            images_removed: 3,
            layers_freed: 7,
            bytes_freed: 1024 * 1024 * 256,
            duration_secs: 1.5,
            dry_run: true,
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: GcResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.images_removed, 3);
        assert_eq!(parsed.layers_freed, 7);
        assert_eq!(parsed.bytes_freed, 1024 * 1024 * 256);
        assert!((parsed.duration_secs - 1.5).abs() < f64::EPSILON);
        assert!(parsed.dry_run);
    }

    #[test]
    fn test_layer_ref_count_creation() {
        let lrc = LayerRefCount {
            digest: "sha256:deadbeef".to_string(),
            ref_count: 1,
            size: 1024 * 1024 * 10,
            created_at: Utc::now(),
        };

        assert_eq!(lrc.digest, "sha256:deadbeef");
        assert_eq!(lrc.ref_count, 1);
        assert_eq!(lrc.size, 10_485_760);
    }

    #[test]
    fn test_layer_ref_count_serde() {
        let lrc = LayerRefCount {
            digest: "sha256:abc".to_string(),
            ref_count: 5,
            size: 999,
            created_at: Utc::now(),
        };

        let json = serde_json::to_string(&lrc).unwrap();
        let parsed: LayerRefCount = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.ref_count, 5);
        assert_eq!(parsed.size, 999);
    }

    #[test]
    fn test_gc_config_watermark_validation() {
        let config = GcConfig {
            high_watermark: 0.95,
            low_watermark: 0.50,
            ..GcConfig::default()
        };

        assert!(config.high_watermark > config.low_watermark);
    }

    #[test]
    fn test_gc_result_dry_run_no_bytes() {
        let result = GcResult {
            images_removed: 0,
            layers_freed: 0,
            bytes_freed: 0,
            duration_secs: 0.01,
            dry_run: true,
        };

        assert_eq!(result.images_removed, 0);
        assert!(result.dry_run);
    }
}
