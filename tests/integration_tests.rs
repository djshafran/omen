use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn omen() -> Command {
    #[allow(deprecated)]
    Command::cargo_bin("omen").expect("binary exists")
}

fn fixtures_dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures")
}

// ---------------------------------------------------------------------------
// CLI smoke tests
// ---------------------------------------------------------------------------

#[test]
fn test_help_output() {
    omen()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("code analysis"));
}

#[test]
fn test_complexity_runs_successfully() {
    omen()
        .args(["-p", fixtures_dir(), "-f", "json", "complexity"])
        .assert()
        .success();
}

#[test]
fn test_complexity_json_output() {
    omen()
        .args(["-p", fixtures_dir(), "-f", "json", "complexity"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cyclomatic"));
}

#[test]
fn test_satd_runs_successfully() {
    omen()
        .args(["-p", fixtures_dir(), "-f", "json", "satd"])
        .assert()
        .success();
}

#[test]
fn test_deadcode_runs_successfully() {
    omen()
        .args(["-p", fixtures_dir(), "-f", "json", "deadcode"])
        .assert()
        .success();
}

#[test]
fn test_cohesion_runs_successfully() {
    omen()
        .args(["-p", fixtures_dir(), "-f", "json", "cohesion"])
        .assert()
        .success();
}

#[test]
fn test_flags_runs_successfully() {
    omen()
        .args(["-p", fixtures_dir(), "-f", "json", "flags"])
        .assert()
        .success();
}

#[test]
fn test_clones_runs_successfully() {
    omen()
        .args(["-p", fixtures_dir(), "-f", "json", "clones"])
        .assert()
        .success();
}

#[test]
fn test_defect_requires_git_repo() {
    omen()
        .args(["-p", fixtures_dir(), "-f", "json", "defect"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("git"));
}

#[test]
fn test_tdg_runs_successfully() {
    omen()
        .args(["-p", fixtures_dir(), "-f", "json", "tdg"])
        .assert()
        .success();
}

#[test]
fn test_graph_runs_successfully() {
    omen()
        .args(["-p", fixtures_dir(), "-f", "json", "graph"])
        .assert()
        .success();
}

#[test]
fn test_repomap_runs_successfully() {
    omen()
        .args(["-p", fixtures_dir(), "-f", "json", "repomap"])
        .assert()
        .success();
}

#[test]
fn test_smells_runs_successfully() {
    omen()
        .args(["-p", fixtures_dir(), "-f", "json", "smells"])
        .assert()
        .success();
}

#[test]
fn test_score_runs_successfully() {
    omen()
        .args(["-p", ".", "-f", "json", "score"])
        .assert()
        .success()
        .stdout(predicate::str::contains("overall_score"));
}

#[test]
fn test_all_analyzers_no_panic() {
    omen()
        .args(["-p", fixtures_dir(), "all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("analyzers"));
}

#[test]
fn test_json_output_is_valid_json() {
    let output = omen()
        .args(["-p", fixtures_dir(), "-f", "json", "complexity"])
        .output()
        .expect("command runs");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(parsed.is_ok(), "stdout is not valid JSON: {}", stdout);
}

#[test]
fn test_markdown_output() {
    omen()
        .args(["-p", fixtures_dir(), "-f", "markdown", "complexity"])
        .assert()
        .success()
        .stdout(predicate::str::contains("# "));
}

#[test]
fn test_text_output() {
    omen()
        .args(["-p", fixtures_dir(), "-f", "text", "complexity"])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Multi-language fixture tests
// ---------------------------------------------------------------------------

#[test]
fn test_complexity_rust_fixture() {
    let output = omen()
        .args([
            "-p",
            fixtures_dir(),
            "-f",
            "json",
            "complexity",
            "-g",
            "*.rs",
        ])
        .output()
        .expect("command runs");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("fibonacci"),
        "expected fibonacci in Rust output: {}",
        stdout
    );
    assert!(
        stdout.contains("validate"),
        "expected validate in Rust output: {}",
        stdout
    );
}

#[test]
fn test_complexity_python_fixture() {
    let output = omen()
        .args([
            "-p",
            fixtures_dir(),
            "-f",
            "json",
            "complexity",
            "-g",
            "*.py",
        ])
        .output()
        .expect("command runs");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("get_user"),
        "expected get_user in Python output: {}",
        stdout
    );
    assert!(
        stdout.contains("calculate_discount"),
        "expected calculate_discount in Python output: {}",
        stdout
    );
}

#[test]
fn test_complexity_go_fixture() {
    let output = omen()
        .args([
            "-p",
            fixtures_dir(),
            "-f",
            "json",
            "complexity",
            "-g",
            "*.go",
        ])
        .output()
        .expect("command runs");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("validate"),
        "expected validate in Go output: {}",
        stdout
    );
    assert!(
        stdout.contains("maxOf"),
        "expected maxOf in Go output: {}",
        stdout
    );
}

#[test]
fn test_complexity_ruby_fixture() {
    let output = omen()
        .args([
            "-p",
            fixtures_dir(),
            "-f",
            "json",
            "complexity",
            "-g",
            "*.rb",
        ])
        .output()
        .expect("command runs");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("process"),
        "expected process in Ruby output: {}",
        stdout
    );
}

#[test]
fn test_complexity_typescript_fixture() {
    let output = omen()
        .args([
            "-p",
            fixtures_dir(),
            "-f",
            "json",
            "complexity",
            "-g",
            "*.ts",
        ])
        .output()
        .expect("command runs");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("parseConfig"),
        "expected parseConfig in TypeScript output: {}",
        stdout
    );
}

#[test]
fn test_bdd_coverage_runs_successfully() {
    let output = omen()
        .args([
            "-p",
            fixtures_dir(),
            "-f",
            "json",
            "bdd-coverage",
            "-g",
            "*.feature",
        ])
        .output()
        .expect("command runs");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("bdd-coverage output should be valid JSON");

    let summary = parsed["summary"].as_object().expect("summary should exist");
    assert_eq!(summary["steps_total"].as_u64(), Some(6));
    assert_eq!(summary["steps_missing"].as_u64(), Some(0));
    assert_eq!(summary["steps_ambiguous"].as_u64(), Some(0));
    assert_eq!(summary["scenarios_total"].as_u64(), Some(2));
    assert_eq!(summary["scenarios_fully_implemented"].as_u64(), Some(2));
}

#[test]
fn test_bdd_coverage_reports_missing_and_orphan_steps() {
    let temp_dir = TempDir::new().expect("create temp dir");

    std::fs::write(
        temp_dir.path().join("features.feature"),
        r#"Feature: Demo
  Scenario: Missing step
    Given configured account
    When user triggers action
"#,
    )
    .unwrap();

    std::fs::write(
        temp_dir.path().join("steps.py"),
        r#"from behave import given, when, then

@given("configured account")
def configured_account(context): 
    pass

@then("unused verification")
def unused_verification(context): 
    pass
"#,
    )
    .unwrap();

    let output = omen()
        .args([
            "-p",
            temp_dir.path().to_str().unwrap(),
            "-f",
            "json",
            "bdd-coverage",
        ])
        .output()
        .expect("command runs");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("bdd-coverage output should be valid JSON");

    let summary = parsed["summary"].as_object().expect("summary should exist");
    assert_eq!(summary["steps_total"].as_u64(), Some(2));
    assert_eq!(summary["steps_missing"].as_u64(), Some(1));
    assert_eq!(summary["scenarios_with_missing_steps"].as_u64(), Some(1));
    let orphans = parsed["orphan_step_definitions"]
        .as_array()
        .expect("orphan_step_definitions should be an array");
    assert_eq!(orphans.len(), 1);
}

#[test]
fn test_bdd_coverage_execution_report_marks_statuses() {
    let temp_dir = TempDir::new().expect("create temp dir");

    std::fs::write(
        temp_dir.path().join("features.feature"),
        r#"Feature: Checkout
  Scenario: Login
    Given user has an account
    When user submits valid credentials
    Then dashboard is visible

  Scenario: Add product to cart
    Given product "Widget" exists
    When user adds "Widget" to cart
    Then cart contains "Widget"
"#,
    )
    .unwrap();

    std::fs::write(
        temp_dir.path().join("steps.py"),
        r#"from behave import given, when, then, parsers

@given("user has an account")
def user_has_account(context): 
    pass

@when("user submits valid credentials")
def user_submits_credentials(context): 
    pass

@then("dashboard is visible")
def dashboard_is_visible(context): 
    pass

@given(parsers.parse('product "{product}" exists'))
def product_exists(context, product): 
    pass

@when(parsers.parse('user adds "{product}" to cart'))
def add_to_cart(context, product): 
    pass

@then(parsers.parse('cart contains "{product}"'))
def cart_contains(context, product): 
    pass
"#,
    )
    .unwrap();

    std::fs::write(
        temp_dir.path().join("execution.json"),
        r#"{
  "scenarios": [
    {
      "name": "Login",
      "line": 2,
      "status": "passed",
      "steps": [
        {
          "text": "user has an account",
          "keyword": "Given",
          "line": 3,
          "status": "passed"
        },
        {
          "text": "user submits valid credentials",
          "keyword": "When",
          "line": 4,
          "status": "passed"
        },
        {
          "text": "dashboard is visible",
          "keyword": "Then",
          "line": 5,
          "status": "passed"
        }
      ]
    },
    {
      "name": "Add product to cart",
      "line": 7,
      "status": "failed",
      "steps": [
        {
          "text": "product \"Widget\" exists",
          "keyword": "Given",
          "line": 8,
          "status": "passed"
        },
        {
          "text": "user adds \"Widget\" to cart",
          "keyword": "When",
          "line": 9,
          "status": "failed"
        },
        {
          "text": "cart contains \"Widget\"",
          "keyword": "Then",
          "line": 10,
          "status": "skipped"
        }
      ]
    }
  ]
}"#,
    )
    .unwrap();

    let output = omen()
        .args([
            "-p",
            temp_dir.path().to_str().unwrap(),
            "-f",
            "json",
            "bdd-coverage",
            "-r",
            "execution.json",
            "--report-format",
            "json",
        ])
        .output()
        .expect("command runs");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("bdd-coverage output should be valid JSON");

    let summary = parsed["summary"].as_object().expect("summary should exist");
    assert_eq!(summary["scenarios_total"].as_u64(), Some(2));
    assert_eq!(summary["scenarios_executed"].as_u64(), Some(2));
    assert_eq!(summary["scenarios_passed"].as_u64(), Some(1));
    assert_eq!(summary["scenarios_failed"].as_u64(), Some(1));
    assert_eq!(summary["steps_total"].as_u64(), Some(6));
    assert_eq!(summary["steps_executed"].as_u64(), Some(6));
    assert_eq!(summary["steps_failed"].as_u64(), Some(1));

    let scenarios = parsed["scenarios"]
        .as_array()
        .expect("scenarios should be array");
    assert_eq!(scenarios.len(), 2);
    assert_eq!(
        scenarios[0]["execution_status"],
        serde_json::json!("passed")
    );
    assert_eq!(
        scenarios[1]["execution_status"],
        serde_json::json!("failed")
    );
    assert_eq!(
        scenarios[1]["steps"][1]["execution_status"],
        serde_json::json!("failed")
    );
    assert_eq!(
        scenarios[0]["steps"][0]["executed"],
        serde_json::json!(true)
    );
}

#[test]
fn test_bdd_coverage_execution_report_xml_marks_statuses() {
    let temp_dir = TempDir::new().expect("create temp dir");

    std::fs::write(
        temp_dir.path().join("features.feature"),
        r#"Feature: Checkout
  Scenario: Login
    Given user has an account
    When user submits valid credentials
    Then dashboard is visible

  Scenario: Failed checkout
    Given user has an account
    When user submits invalid credentials
    Then error is shown
"#,
    )
    .unwrap();

    std::fs::write(
        temp_dir.path().join("steps.py"),
        r#"from behave import given, when, then, parsers

@given("user has an account")
def user_has_account(context):
    pass

@when("user submits valid credentials")
def user_submits_credentials(context):
    pass

@then("dashboard is visible")
def dashboard_is_visible(context):
    pass

@when("user submits invalid credentials")
def user_submits_invalid_credentials(context):
    pass

@then("error is shown")
def error_is_shown(context):
    pass
"#,
    )
    .unwrap();

    std::fs::write(
        temp_dir.path().join("execution.xml"),
        r#"<?xml version="1.0"?>
<testsuite name="bdd" tests="2">
  <testcase name="Login" line="2" status="passed">
    <system-out>Given user has an account
When user submits valid credentials
Then dashboard is visible</system-out>
  </testcase>
  <testcase name="Failed checkout" line="7" status="failed">
    <failure>
Then error is shown
    </failure>
    <system-out>Given user has an account
When user submits invalid credentials
Then error is shown</system-out>
  </testcase>
</testsuite>"#,
    )
    .unwrap();

    let output = omen()
        .args([
            "-p",
            temp_dir.path().to_str().unwrap(),
            "-f",
            "json",
            "bdd-coverage",
            "-r",
            "execution.xml",
            "--report-format",
            "xml",
        ])
        .output()
        .expect("command runs");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("bdd-coverage output should be valid JSON");

    let summary = parsed["summary"].as_object().expect("summary should exist");
    assert_eq!(summary["scenarios_total"].as_u64(), Some(2));
    assert_eq!(summary["scenarios_executed"].as_u64(), Some(2));
    assert_eq!(summary["scenarios_passed"].as_u64(), Some(1));
    assert_eq!(summary["scenarios_failed"].as_u64(), Some(1));
    assert_eq!(summary["steps_executed"].as_u64(), Some(6));
    assert_eq!(summary["steps_failed"].as_u64(), Some(1));

    let scenarios = parsed["scenarios"]
        .as_array()
        .expect("scenarios should be array");
    assert_eq!(scenarios.len(), 2);
    assert_eq!(
        scenarios[0]["execution_status"],
        serde_json::json!("passed")
    );
    assert_eq!(
        scenarios[1]["execution_status"],
        serde_json::json!("failed")
    );
    assert_eq!(
        scenarios[1]["steps"][2]["execution_status"],
        serde_json::json!("failed")
    );
}

#[test]
fn test_satd_detects_todo_in_python() {
    let output = omen()
        .args(["-p", fixtures_dir(), "-f", "json", "satd", "-g", "*.py"])
        .output()
        .expect("command runs");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("TODO"),
        "expected SATD to detect TODO in Python fixture: {}",
        stdout
    );
}

// ---------------------------------------------------------------------------
// Semantic search E2E tests
// ---------------------------------------------------------------------------

#[test]
fn test_semantic_search_python_e2e_branch_and_complexity_metrics() {
    let temp_dir = TempDir::new().expect("create temp dir");
    std::fs::write(
        temp_dir.path().join("sample.py"),
        r#"def process(items):
    for item in items:
        if item.retries > 3:
            raise ValueError("too many retries")
    return True
"#,
    )
    .expect("write sample.py");

    omen()
        .args(["-p", temp_dir.path().to_str().unwrap(), "search", "index"])
        .assert()
        .success();

    let search_output = omen()
        .args([
            "-p",
            temp_dir.path().to_str().unwrap(),
            "-f",
            "json",
            "search",
            "query",
            "too many retries",
            "--top-k",
            "10",
            "--min-score",
            "0.0",
        ])
        .output()
        .expect("search query runs");
    assert!(
        search_output.status.success(),
        "search query should succeed: {}",
        String::from_utf8_lossy(&search_output.stderr)
    );

    let search_stdout = String::from_utf8_lossy(&search_output.stdout);
    let search_json: serde_json::Value =
        serde_json::from_str(&search_stdout).expect("search query output should be valid JSON");
    let results = search_json["results"]
        .as_array()
        .expect("results should be an array");
    assert!(
        !results.is_empty(),
        "expected semantic search results, got: {}",
        search_stdout
    );

    let function_result = results
        .iter()
        .find(|r| r["symbol_type"] == "function" && r["symbol_name"] == "process")
        .expect("expected function result for process");

    let function_cyclomatic = function_result["cyclomatic_complexity"]
        .as_u64()
        .expect("function cyclomatic_complexity should be present");
    let function_cognitive = function_result["cognitive_complexity"]
        .as_u64()
        .expect("function cognitive_complexity should be present");
    assert_eq!(function_result["start_line"].as_u64(), Some(1));
    assert_eq!(function_result["end_line"].as_u64(), Some(5));

    let branch_results: Vec<&serde_json::Value> = results
        .iter()
        .filter(|r| r["symbol_type"] == "branch")
        .collect();
    assert!(
        !branch_results.is_empty(),
        "expected at least one branch result, got: {}",
        search_stdout
    );

    for branch in &branch_results {
        assert_eq!(
            branch["cyclomatic_complexity"].as_u64(),
            Some(function_cyclomatic),
            "branch should inherit function cyclomatic complexity"
        );
        assert_eq!(
            branch["cognitive_complexity"].as_u64(),
            Some(function_cognitive),
            "branch should inherit function cognitive complexity"
        );
        let signature = branch["signature"].as_str().unwrap_or("");
        assert!(
            signature.contains("[branch:"),
            "branch signature should contain branch marker, got: {signature}"
        );
    }

    let complexity_output = omen()
        .args([
            "-p",
            temp_dir.path().to_str().unwrap(),
            "-f",
            "json",
            "complexity",
        ])
        .output()
        .expect("complexity runs");
    assert!(
        complexity_output.status.success(),
        "complexity should succeed: {}",
        String::from_utf8_lossy(&complexity_output.stderr)
    );

    let complexity_stdout = String::from_utf8_lossy(&complexity_output.stdout);
    let complexity_json: serde_json::Value =
        serde_json::from_str(&complexity_stdout).expect("complexity output should be valid JSON");

    let files = complexity_json["files"]
        .as_array()
        .expect("complexity files should be an array");
    let mut baseline_cyclomatic = None;
    let mut baseline_cognitive = None;
    for file in files {
        if let Some(functions) = file["functions"].as_array() {
            for function in functions {
                if function["name"] == "process" {
                    baseline_cyclomatic = function["metrics"]["cyclomatic"].as_u64();
                    baseline_cognitive = function["metrics"]["cognitive"].as_u64();
                }
            }
        }
    }

    assert_eq!(
        baseline_cyclomatic,
        Some(function_cyclomatic),
        "semantic_search function cyclomatic should match complexity analyzer"
    );
    assert_eq!(
        baseline_cognitive,
        Some(function_cognitive),
        "semantic_search function cognitive should match complexity analyzer"
    );
}

// ---------------------------------------------------------------------------
// Error handling tests
// ---------------------------------------------------------------------------

#[test]
fn test_invalid_path_returns_error() {
    omen()
        .args(["-p", "/nonexistent/path/that/does/not/exist", "complexity"])
        .assert()
        .failure();
}

#[test]
fn test_nonexistent_path_error() {
    omen()
        .args(["-p", "/tmp/__omen_nonexistent_xyz__", "complexity"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error"));
}

#[test]
fn test_empty_directory() {
    let tmp = TempDir::new().expect("create temp dir");
    omen()
        .args([
            "-p",
            tmp.path().to_str().unwrap(),
            "-f",
            "json",
            "complexity",
        ])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Glob and exclude filter tests
// ---------------------------------------------------------------------------

#[test]
fn test_glob_filter() {
    let output = omen()
        .args([
            "-p",
            fixtures_dir(),
            "-f",
            "json",
            "complexity",
            "-g",
            "*.py",
        ])
        .output()
        .expect("command runs");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(".py"),
        "expected .py files in filtered output"
    );
}

// ---------------------------------------------------------------------------
// Score analyzer tests
// ---------------------------------------------------------------------------

#[test]
fn test_score_json_structure() {
    let output = omen()
        .args(["-p", ".", "-f", "json", "score"])
        .output()
        .expect("command runs");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("score output should be valid JSON");

    assert!(
        parsed.get("overall_score").is_some(),
        "missing overall_score"
    );
    assert!(parsed.get("grade").is_some(), "missing grade");
    assert!(parsed.get("components").is_some(), "missing components");
}

#[test]
fn test_score_grade_is_valid() {
    let output = omen()
        .args(["-p", ".", "-f", "json", "score"])
        .output()
        .expect("command runs");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let grade = parsed["grade"].as_str().unwrap();
    assert!(
        ["A", "B", "C", "D", "F"].contains(&grade),
        "unexpected grade: {}",
        grade,
    );
}

// ---------------------------------------------------------------------------
// All command output structure
// ---------------------------------------------------------------------------

#[test]
fn test_all_json_has_analyzers_array() {
    let output = omen()
        .args(["-p", fixtures_dir(), "all"])
        .output()
        .expect("command runs");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("all output should be valid JSON");

    let analyzers = parsed["analyzers"]
        .as_array()
        .expect("analyzers should be an array");
    assert!(!analyzers.is_empty(), "analyzers array should not be empty");

    for entry in analyzers {
        assert!(
            entry.get("analyzer").is_some(),
            "each entry needs an analyzer name"
        );
    }
}

// ---------------------------------------------------------------------------
// Output format consistency
// ---------------------------------------------------------------------------

#[test]
fn test_all_three_formats_succeed() {
    for format in &["json", "markdown", "text"] {
        omen()
            .args(["-p", fixtures_dir(), "-f", format, "complexity"])
            .assert()
            .success();
    }
}
