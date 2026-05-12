//! Aggregate 模块单元测试。

#[cfg(test)]
mod tests {
    use crate::aggregate::*;
    use crate::bench::Operation;
    use chrono::{Duration, Utc};

    fn make_op(op_type: &str, size: i64, duration_ms: i64, offset_secs: i64) -> Operation {
        let start = Utc::now() + Duration::seconds(offset_secs);
        let end = start + Duration::milliseconds(duration_ms);
        Operation {
            start,
            end,
            first_byte: Some(start + Duration::milliseconds(duration_ms / 3)),
            last_byte: None,
            op_type: op_type.to_string(),
            err: String::new(),
            file: format!("obj-{offset_secs}"),
            client_id: "c1".to_string(),
            endpoint: "localhost:9000".to_string(),
            obj_per_op: 1,
            size,
            thread: 0,
            categories: 0,
        }
    }

    #[test]
    fn test_analyze_basic() {
        let mut ops = Vec::new();
        for i in 0..10 {
            ops.push(make_op("GET", 1024 * 1024, 100 + i as i64, i as i64));
        }
        let agg = analyze(&ops, std::time::Duration::from_secs(1), 1);
        assert!(!agg.operations.is_empty());
        let get_analysis = agg.operations.iter().find(|a| a.op_type == "GET").unwrap();
        assert_eq!(get_analysis.throughput.total_ops, 10);
        assert!(get_analysis.throughput.avg_ops > 0.0);
    }

    #[test]
    fn test_csv_roundtrip() {
        let now = Utc::now();
        let ops: Vec<Operation> = (0..5)
            .map(|i| Operation {
                start: now + Duration::milliseconds(i * 10),
                end: now + Duration::milliseconds(i * 10 + 5),
                first_byte: None,
                last_byte: None,
                op_type: "GET".into(),
                err: String::new(),
                file: format!("obj-{i}"),
                client_id: "c1".into(),
                endpoint: "localhost:9000".into(),
                obj_per_op: 1,
                size: 1024,
                thread: 0,
                categories: 0,
            })
            .collect();
        let mut buf = Vec::new();
        write_csv_zst(&ops, &mut buf).unwrap();
        let decoded = read_csv_zst(&mut std::io::Cursor::new(buf)).unwrap();
        assert_eq!(decoded.len(), ops.len());
        assert_eq!(decoded[0].op_type, "GET");
    }

    #[test]
    fn test_merge_overlapping() {
        let now = Utc::now();
        let a: Vec<Operation> = (0..3)
            .map(|i| Operation {
                start: now + Duration::milliseconds(i * 10),
                end: now + Duration::milliseconds(i * 10 + 5),
                first_byte: None, last_byte: None,
                op_type: "GET".into(), err: String::new(),
                file: format!("a-{i}"), client_id: "c1".into(),
                endpoint: "localhost:9000".into(),
                obj_per_op: 1, size: 1024, thread: 0, categories: 0,
            })
            .collect();
        let b: Vec<Operation> = (0..3)
            .map(|i| Operation {
                start: now + Duration::milliseconds(i * 10),
                end: now + Duration::milliseconds(i * 10 + 5),
                first_byte: None, last_byte: None,
                op_type: "GET".into(), err: String::new(),
                file: format!("b-{i}"), client_id: "c2".into(),
                endpoint: "localhost:9000".into(),
                obj_per_op: 1, size: 1024, thread: 0, categories: 0,
            })
            .collect();
        let merged = merge(&[a, b]);
        assert!(merged.len() > 0, "merge should produce overlapping ops");
    }

    #[test]
    fn test_analyze_mixed_ops() {
        let mut ops = Vec::new();
        for i in 0..5 {
            ops.push(make_op("GET", 1024, 10, i as i64));
            ops.push(make_op("PUT", 2048, 20, i as i64 + 100));
        }
        let agg = analyze(&ops, std::time::Duration::from_secs(1), 1);
        let op_types: Vec<&str> = agg.operations.iter().map(|a| a.op_type.as_str()).collect();
        assert!(op_types.contains(&"GET"));
        assert!(op_types.contains(&"PUT"));
    }
}
