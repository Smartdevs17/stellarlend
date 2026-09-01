//! # Benchmark Report Generation
//!
//! Handles result formatting, JSON output, Markdown output, baseline comparison,
//! historical trend storage, and regression detection for CI integration.

use crate::framework::{BenchmarkResult, Regression};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Full benchmark report written to JSON
#[derive(Debug, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub version: String,
    pub timestamp: String,
    pub total_benchmarks: usize,
    pub passed: usize,
    pub failed: usize,
    pub results: Vec<BenchmarkResult>,
    pub summary_by_contract: HashMap<String, ContractSummary>,
    #[serde(default)]
    pub optimization_findings: Vec<OptimizationFinding>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContractSummary {
    pub contract: String,
    pub total_operations: usize,
    pub max_instructions: u64,
    pub min_instructions: u64,
    pub avg_instructions: u64,
    pub over_budget_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FindingSeverity {
    Critical,
    High,
    Medium,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OptimizationFinding {
    pub operation: String,
    pub contract: String,
    pub severity: FindingSeverity,
    pub instruction_overage: u64,
    pub storage_operations: u32,
    pub recommendation: String,
}

pub fn generate_optimization_findings(results: &[BenchmarkResult]) -> Vec<OptimizationFinding> {
    let mut findings: Vec<OptimizationFinding> = results
        .iter()
        .filter_map(|result| {
            let instruction_overage = result.instructions.saturating_sub(result.budget);
            let storage_operations = result.storage_reads + result.storage_writes;
            let has_budget_overage = result.budget > 0 && instruction_overage > 0;
            let has_storage_pressure = storage_operations >= 8 || result.cold_storage;

            if !has_budget_overage && !has_storage_pressure {
                return None;
            }

            let severity = if has_budget_overage && instruction_overage > result.budget / 4 {
                FindingSeverity::Critical
            } else if has_budget_overage || storage_operations >= 12 {
                FindingSeverity::High
            } else {
                FindingSeverity::Medium
            };

            let recommendation = if has_budget_overage {
                "Profile this operation first; it exceeds its configured gas budget."
            } else if result.cold_storage {
                "Review persistent storage access and cache or batch cold reads where possible."
            } else {
                "Review storage access count and combine reads/writes where contract semantics allow."
            };

            Some(OptimizationFinding {
                operation: result.operation.clone(),
                contract: result.contract.clone(),
                severity,
                instruction_overage,
                storage_operations,
                recommendation: recommendation.into(),
            })
        })
        .collect();

    findings.sort_by(|a, b| {
        b.instruction_overage
            .cmp(&a.instruction_overage)
            .then_with(|| b.storage_operations.cmp(&a.storage_operations))
            .then_with(|| a.operation.cmp(&b.operation))
    });
    findings
}

/// Print a formatted summary table to stdout
pub fn print_summary(results: &[BenchmarkResult]) {
    println!("\n{}", "─".repeat(90));
    println!(
        "{:<45} {:>14} {:>12} {:>10} Status",
        "Operation", "Instructions", "Memory(B)", "Storage"
    );
    println!("{}", "─".repeat(90));

    let mut by_contract: HashMap<String, Vec<&BenchmarkResult>> = HashMap::new();
    for r in results {
        by_contract.entry(r.contract.clone()).or_default().push(r);
    }

    let mut contracts: Vec<&String> = by_contract.keys().collect();
    contracts.sort();

    for contract in contracts {
        let ops = &by_contract[contract];
        println!("\n  [{}]", contract.to_uppercase());
        for r in ops.iter() {
            let status = if r.within_budget {
                "✓ OK"
            } else {
                "✗ OVER BUDGET"
            };
            let storage = format!("R:{} W:{}", r.storage_reads, r.storage_writes);
            let cold_tag = if r.cold_storage { " (cold)" } else { "" };
            println!(
                "  {:<43} {:>14} {:>12} {:>10} {}{}",
                r.operation, r.instructions, r.memory_bytes, storage, status, cold_tag
            );
        }
    }

    println!("\n{}", "─".repeat(90));

    let total = results.len();
    let passed = results.iter().filter(|r| r.within_budget).count();
    let failed = total - passed;

    println!(
        "  Total: {}  |  Passed: {}  |  Failed: {}",
        total, passed, failed
    );
    println!("{}", "─".repeat(90));
}

/// Write benchmark results to a JSON file
pub fn write_json(results: &[BenchmarkResult], path: &str) {
    let total = results.len();
    let passed = results.iter().filter(|r| r.within_budget).count();
    let failed = total - passed;

    let mut summary_by_contract: HashMap<String, ContractSummary> = HashMap::new();
    for r in results {
        let entry = summary_by_contract
            .entry(r.contract.clone())
            .or_insert(ContractSummary {
                contract: r.contract.clone(),
                total_operations: 0,
                max_instructions: 0,
                min_instructions: u64::MAX,
                avg_instructions: 0,
                over_budget_count: 0,
            });
        entry.total_operations += 1;
        if r.instructions > entry.max_instructions {
            entry.max_instructions = r.instructions;
        }
        if r.instructions < entry.min_instructions {
            entry.min_instructions = r.instructions;
        }
        if !r.within_budget {
            entry.over_budget_count += 1;
        }
    }
    // Compute averages and fix min_instructions sentinel
    for (contract, summary) in summary_by_contract.iter_mut() {
        let ops: Vec<&BenchmarkResult> =
            results.iter().filter(|r| &r.contract == contract).collect();
        let total_insns: u64 = ops.iter().map(|r| r.instructions).sum();
        summary.avg_instructions = if ops.is_empty() {
            0
        } else {
            total_insns / ops.len() as u64
        };
        // Reset sentinel if no results were found
        if summary.min_instructions == u64::MAX {
            summary.min_instructions = 0;
        }
    }

    let report = BenchmarkReport {
        version: "0.1.0".into(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        total_benchmarks: total,
        passed,
        failed,
        results: results.to_vec(),
        summary_by_contract,
        optimization_findings: generate_optimization_findings(results),
    };

    let json = serde_json::to_string_pretty(&report).expect("Failed to serialize benchmark report");
    fs::write(path, json).expect("Failed to write benchmark report");
}

/// Compare results against a baseline JSON file.
/// Returns a list of regressions (operations that exceeded their budget or
/// increased by more than the configured threshold compared to baseline).
pub fn compare_baseline(
    results: &[BenchmarkResult],
    baseline_path: &str,
    threshold: f64,
) -> Vec<Regression> {
    let baseline_json = match fs::read_to_string(baseline_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Warning: Could not read baseline file '{}': {}",
                baseline_path, e
            );
            return Vec::new();
        }
    };

    let baseline_report: BenchmarkReport = match serde_json::from_str(&baseline_json) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Warning: Could not parse baseline file: {}", e);
            return Vec::new();
        }
    };

    let baseline_map: HashMap<String, u64> = baseline_report
        .results
        .iter()
        .map(|r| (r.operation.clone(), r.instructions))
        .collect();

    let mut regressions = Vec::new();

    for result in results {
        // Check hard budget violation
        if !result.within_budget && result.budget > 0 {
            regressions.push(Regression {
                operation: result.operation.clone(),
                actual: result.instructions,
                budget: result.budget,
                delta: result.instructions.saturating_sub(result.budget),
            });
            continue;
        }

        // Check regression vs baseline (configurable threshold triggers alert)
        if let Some(&baseline_insns) = baseline_map.get(&result.operation) {
            if baseline_insns > 0 {
                let increase_pct = (result.instructions as f64 - baseline_insns as f64)
                    / baseline_insns as f64
                    * 100.0;
                if increase_pct > threshold {
                    regressions.push(Regression {
                        operation: result.operation.clone(),
                        actual: result.instructions,
                        budget: baseline_insns,
                        delta: result.instructions.saturating_sub(baseline_insns),
                    });
                }
            }
        }
    }

    regressions
}

/// Historical trend entry for tracking benchmark performance over time
#[derive(Debug, Serialize, Deserialize)]
pub struct HistoricalEntry {
    pub timestamp: String,
    pub operation: String,
    pub instructions: u64,
    pub memory_bytes: u64,
    pub git_commit: Option<String>,
    pub git_branch: Option<String>,
}

/// Append current benchmark results to historical trend storage
pub fn append_to_history(results: &[BenchmarkResult], history_path: &str) {
    let mut history: Vec<HistoricalEntry> = if Path::new(history_path).exists() {
        match fs::read_to_string(history_path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };

    let git_commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string());

    let git_branch = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string());

    for result in results {
        history.push(HistoricalEntry {
            timestamp: result.timestamp.clone(),
            operation: result.operation.clone(),
            instructions: result.instructions,
            memory_bytes: result.memory_bytes,
            git_commit: git_commit.clone(),
            git_branch: git_branch.clone(),
        });
    }

    let json = serde_json::to_string_pretty(&history).expect("Failed to serialize history");
    fs::write(history_path, json).expect("Failed to write history file");
}

/// Generate Markdown report from benchmark results
pub fn write_markdown(results: &[BenchmarkResult], path: &str) {
    let total = results.len();
    let passed = results.iter().filter(|r| r.within_budget).count();
    let failed = total - passed;

    let mut markdown = String::new();
    markdown.push_str("# Gas Benchmark Report\n\n");
    markdown.push_str(&format!(
        "**Generated:** {}\n\n",
        chrono::Utc::now().to_rfc3339()
    ));
    markdown.push_str(&format!("**Total Benchmarks:** {}\n", total));
    markdown.push_str(&format!("**Passed:** {}\n", passed));
    markdown.push_str(&format!("**Failed:** {}\n\n", failed));

    let findings = generate_optimization_findings(results);
    markdown.push_str("## Optimization Findings\n\n");
    if findings.is_empty() {
        markdown.push_str("No gas optimization findings were detected.\n\n");
    } else {
        markdown.push_str(
            "| Severity | Operation | Instruction Overage | Storage Ops | Recommendation |\n",
        );
        markdown.push_str(
            "|----------|-----------|----------------------|-------------|----------------|\n",
        );
        for finding in &findings {
            markdown.push_str(&format!(
                "| {:?} | {} | {:,} | {} | {} |\n",
                finding.severity,
                finding.operation,
                finding.instruction_overage,
                finding.storage_operations,
                finding.recommendation
            ));
        }
        markdown.push_str("\n");
    }

    // Group by contract
    let mut by_contract: HashMap<String, Vec<&BenchmarkResult>> = HashMap::new();
    for r in results {
        by_contract.entry(r.contract.clone()).or_default().push(r);
    }

    let mut contracts: Vec<&String> = by_contract.keys().collect();
    contracts.sort();

    for contract in contracts {
        let ops = &by_contract[contract];
        markdown.push_str(&format!("## {}\n\n", contract.to_uppercase()));
        markdown.push_str("| Operation | Instructions | Memory (B) | Storage | Status |\n");
        markdown.push_str("|-----------|--------------|-----------|---------|--------|\n");

        for r in ops {
            let status = if r.within_budget { "✓" } else { "✗" };
            let storage = format!("R:{} W:{}", r.storage_reads, r.storage_writes);
            let cold_tag = if r.cold_storage { " (cold)" } else { "" };
            markdown.push_str(&format!(
                "| {}{} | {:,} | {:,} | {} | {} |\n",
                r.operation, cold_tag, r.instructions, r.memory_bytes, storage, status
            ));
        }
        markdown.push_str("\n");
    }

    fs::write(path, markdown).expect("Failed to write markdown report");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(operation: &str, instructions: u64, budget: u64) -> BenchmarkResult {
        BenchmarkResult {
            operation: operation.into(),
            contract: "lending".into(),
            description: "test benchmark".into(),
            instructions,
            memory_bytes: 1024,
            storage_reads: 1,
            storage_writes: 1,
            cold_storage: false,
            budget,
            within_budget: budget == 0 || instructions <= budget,
            timestamp: "2026-01-01T00:00:00Z".into(),
            tags: vec![],
        }
    }

    #[test]
    fn flags_operations_over_budget() {
        let findings = generate_optimization_findings(&[result("lending::deposit", 130, 100)]);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].operation, "lending::deposit");
        assert_eq!(findings[0].severity, FindingSeverity::Critical);
        assert_eq!(findings[0].instruction_overage, 30);
    }

    #[test]
    fn flags_cold_storage_pressure_without_budget_overage() {
        let mut cold = result("lending::withdraw", 90, 100);
        cold.cold_storage = true;

        let findings = generate_optimization_findings(&[cold]);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, FindingSeverity::Medium);
        assert_eq!(findings[0].storage_operations, 2);
    }
}
