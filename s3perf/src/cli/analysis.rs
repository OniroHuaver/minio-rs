//! Offline analysis: load, compare, and merge benchmark result files.

use std::time::Duration;

/// Load and pretty-print aggregated results from `.csv.zst` / `.json.zst`.
pub fn analyze_file(path: &str) -> anyhow::Result<()> {
    println!("Analyzing benchmark file: {path}");
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);

    let ops = if path.ends_with(".csv.zst") {
        crate::aggregate::read_csv_zst(&mut reader)?
    } else if path.ends_with(".json.zst") {
        let agg = crate::aggregate::read_json_zst(&mut reader)?;
        println!("{agg:#?}");
        return Ok(());
    } else {
        anyhow::bail!("unsupported file format: {path} (expected .csv.zst or .json.zst)");
    };

    let agg = crate::aggregate::analyze(&ops, Duration::from_secs(1), 20);
    println!("{agg:#?}");
    Ok(())
}

/// Compare aggregates from two prior runs.
pub fn compare_files(before: &str, after: &str) -> anyhow::Result<()> {
    println!("Comparing benchmarks: {before} vs {after}");

    let read_agg = |path: &str| -> anyhow::Result<crate::aggregate::Aggregated> {
        let file = std::fs::File::open(path)?;
        let mut reader = std::io::BufReader::new(file);
        if path.ends_with(".json.zst") {
            Ok(crate::aggregate::read_json_zst(&mut reader)?)
        } else if path.ends_with(".csv.zst") {
            let ops = crate::aggregate::read_csv_zst(&mut reader)?;
            Ok(crate::aggregate::analyze(
                &ops,
                std::time::Duration::from_secs(1),
                20,
            ))
        } else {
            anyhow::bail!("unsupported file format: {path}")
        }
    };

    let before_agg = read_agg(before)?;
    let after_agg = read_agg(after)?;
    let result = crate::aggregate::compare(&before_agg, &after_agg);

    println!("Comparison:");
    for diff in &result.diffs {
        println!(
            "  {}: {:.2} → {:.2} MiB/s ({:+.1}%)  |  {:.2} → {:.2} obj/s ({:+.1}%)",
            diff.op_type,
            diff.before_mbps,
            diff.after_mbps,
            diff.mbps_diff_pct,
            diff.before_ops,
            diff.after_ops,
            diff.ops_diff_pct,
        );
    }
    Ok(())
}

/// Merge multiple compressed CSV benchmarks.
pub fn merge_files(files: &[String]) -> anyhow::Result<()> {
    println!("Merge: combining {} CSV.zst datasets...", files.len());
    let mut op_sets = Vec::new();

    for path in files {
        let file = std::fs::File::open(path)?;
        let mut reader = std::io::BufReader::new(file);
        let ops = if path.ends_with(".csv.zst") {
            crate::aggregate::read_csv_zst(&mut reader)?
        } else {
            anyhow::bail!("merge only supports .csv.zst files: {path}");
        };
        op_sets.push(ops);
    }

    let merged = crate::aggregate::merge(&op_sets);
    println!("Merged operation count: {}", merged.len());

    let agg = crate::aggregate::analyze(&merged, std::time::Duration::from_secs(1), 20);
    println!("{agg:#?}");
    Ok(())
}
