//! Metrics tests: Metric serialization, histogram, LastMinute stats, Leak detection

// ---- metrics-v2_gen (MessagePack serde) ----

/// Verifies MetricDescription Marshal/Unmarshal round-trip.
#[test]
#[ignore]
fn test_marshal_unmarshal_metric_description() {
    // v.MarshalMsg(nil) -> v.UnmarshalMsg(bts) -> assert no leftover
    //   msgp.Skip(bts) -> assert no leftover
    // TODO: implement when MetricDescription + msgpack is available
}

/// Verifies MetricV2 Marshal/Unmarshal round-trip.
#[test]
#[ignore]
fn test_marshal_unmarshal_metric_v2() {
    // MetricV2 serialization/deserialization
    // TODO: implement when MetricV2 + msgpack is available
}

/// Verifies MetricsGroupOpts Marshal/Unmarshal round-trip.
#[test]
#[ignore]
fn test_marshal_unmarshal_metrics_group_opts() {
    // MetricsGroupOpts serialization/deserialization
    // TODO: implement when MetricsGroupOpts + msgpack is available
}

/// Verifies MetricsGroupV2 Marshal/Unmarshal round-trip.
#[test]
#[ignore]
fn test_marshal_unmarshal_metrics_group_v2() {
    // MetricsGroupV2 serialization/deserialization
    // TODO: implement when MetricsGroupV2 + msgpack is available
}

// ---- metrics-v2 (Prometheus histogram) ----

/// Verifies `getHistogramMetrics()` returns correct bucket count.
#[test]
#[ignore]
fn test_get_histogram_metrics_bucket_count() {
    // Prometheus HistogramVec observed then metrics extracted
    //   verify bucket count = labels * (buckets + 1) (including +Inf)
    // TODO: implement when getHistogramMetrics equivalent is available
}

/// Verifies `getHistogramMetrics()` returns correct values.
#[test]
#[ignore]
fn test_get_histogram_metrics_values() {
    // observe multiple values on PutObject and CopyObject
    //   verify each bucket (le) count correct
    // TODO: implement when getHistogramMetrics equivalent is available
}

// ---- last-minute_gen (MessagePack serde) ----

/// Verifies AccElem Marshal/Unmarshal round-trip.
#[test]
#[ignore]
fn test_marshal_unmarshal_acc_elem() {
    // AccElem serialization/deserialization
    // TODO: implement when AccElem + msgpack is available
}

/// Verifies AccElem Encode/Decode round-trip.
#[test]
#[ignore]
fn test_encode_decode_acc_elem() {
    // AccElem via msgp.Encode/Decode
    // TODO: implement when AccElem + msgpack is available
}

/// Verifies LastMinuteHistogram Marshal/Unmarshal round-trip.
#[test]
#[ignore]
fn test_marshal_unmarshal_last_minute_histogram() {
    // LastMinuteHistogram serialization/deserialization
    // TODO: implement when LastMinuteHistogram + msgpack is available
}

/// Verifies LastMinuteHistogram Encode/Decode round-trip.
#[test]
#[ignore]
fn test_encode_decode_last_minute_histogram() {
    // LastMinuteHistogram via msgp.Encode/Decode
    // TODO: implement when LastMinuteHistogram + msgpack is available
}

/// Verifies lastMinuteLatency Marshal/Unmarshal round-trip.
#[test]
#[ignore]
fn test_marshal_unmarshal_last_minute_latency() {
    // lastMinuteLatency serialization/deserialization
    // TODO: implement when lastMinuteLatency + msgpack is available
}

/// Verifies lastMinuteLatency Encode/Decode round-trip.
#[test]
#[ignore]
fn test_encode_decode_last_minute_latency() {
    // lastMinuteLatency via msgp.Encode/Decode
    // TODO: implement when lastMinuteLatency + msgpack is available
}
