//! BDD coverage analyzer.
//!
//! - extracts `Scenario`/`Scenario Outline` as symbols for semantic search
//! - parses Python step definitions used by behave/pytest-bdd (`given`, `when`, `then`, `step`)
//! - maps scenarios and steps to definitions (exact/regex/parse)
//! - produces implementation coverage plus orphan definition report

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

use regex::Regex;
use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use crate::core::FileSet;
use crate::core::{AnalysisContext, Analyzer as AnalyzerTrait, Language, Result, SourceFile};
use crate::parser::{ParseResult, Parser};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatternKind {
    Exact,
    Parse,
    Regex,
    Unknown,
}

impl PatternKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Parse => "parse",
            Self::Regex => "regex",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepDefinition {
    pub name: String,
    pub keyword: String,
    pub pattern: String,
    pub pattern_type: String,
    pub pattern_kind: PatternKind,
    pub line: u32,
    pub file: String,
    pub signature: String,
    pub raw_decorator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StepStatus {
    Implemented,
    Missing,
    Ambiguous,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub text: String,
    pub keyword: String,
    pub line: u32,
    pub status: ExecutionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionScenario {
    pub name: String,
    pub line: u32,
    pub status: ExecutionStatus,
    pub steps: Vec<ExecutionStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReport {
    pub scenarios: Vec<ExecutionScenario>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionStatus {
    Passed,
    Failed,
    Error,
    Skipped,
    Undefined,
    Unknown,
    NotRun,
}

impl ExecutionStatus {
    fn from_raw(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "passed" | "ok" => Self::Passed,
            "failed" | "failure" | "fail" => Self::Failed,
            "error" | "errors" => Self::Error,
            "skipped" | "pending" => Self::Skipped,
            "undefined" => Self::Undefined,
            "notrun" => Self::NotRun,
            _ => Self::Unknown,
        }
    }

    fn is_failed(self) -> bool {
        matches!(self, Self::Failed | Self::Error)
    }

    fn is_executed(self) -> bool {
        matches!(
            self,
            Self::Passed | Self::Failed | Self::Error | Self::Skipped | Self::Undefined
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioStep {
    pub file: String,
    pub line: u32,
    pub keyword: String,
    pub text: String,
    pub matched_definitions: Vec<MatchedDefinitionRef>,
    pub status: StepStatus,
    pub executed: bool,
    pub execution_status: Option<ExecutionStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedDefinitionRef {
    pub file: String,
    pub line: u32,
    pub pattern: String,
    pub pattern_type: String,
    pub keyword: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioCoverage {
    pub file: String,
    pub line: u32,
    pub name: String,
    pub scenario_type: String,
    pub tags: Vec<String>,
    pub steps: Vec<ScenarioStep>,
    pub executed: bool,
    pub execution_status: Option<ExecutionStatus>,
    pub is_fully_implemented: bool,
    pub has_missing_steps: bool,
    pub has_ambiguous_steps: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageMetrics {
    pub steps_total: usize,
    pub steps_implemented: usize,
    pub steps_missing: usize,
    pub steps_ambiguous: usize,
    pub scenarios_total: usize,
    pub scenarios_fully_implemented: usize,
    pub scenarios_executed: usize,
    pub scenarios_passed: usize,
    pub scenarios_failed: usize,
    pub steps_executed: usize,
    pub steps_failed: usize,
    pub scenarios_with_missing_steps: usize,
    pub scenarios_with_ambiguous_steps: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Analysis {
    pub summary: CoverageMetrics,
    pub scenarios: Vec<ScenarioCoverage>,
    pub steps: Vec<ScenarioStep>,
    pub step_definitions: Vec<StepDefinition>,
    pub missing_steps: Vec<ScenarioStep>,
    pub ambiguous_steps: Vec<ScenarioStep>,
    pub orphan_step_definitions: Vec<StepDefinition>,
}

#[derive(Debug, Clone)]
pub struct Analyzer {
    execution_report: Option<PathBuf>,
    report_format: Option<String>,
}

impl Default for Analyzer {
    fn default() -> Self {
        Self {
            execution_report: None,
            report_format: None,
        }
    }
}

impl Analyzer {
    pub fn with_execution_report(
        mut self,
        execution_report: Option<PathBuf>,
        report_format: Option<String>,
    ) -> Self {
        self.execution_report = execution_report;
        self.report_format = report_format;
        self
    }

    fn collect_gherkin_scenarios(&self, ctx: &AnalysisContext<'_>) -> Vec<ScenarioCoverage> {
        let parser = Parser::new();
        let mut scenarios = Vec::new();

        for path in ctx.files.iter() {
            if Language::detect(path) != Some(Language::Gherkin) {
                continue;
            }

            let parse_result = match parse_file_with_context(ctx, &parser, path) {
                Some(result) => result,
                None => continue,
            };

            let relative_file = rel_path(path, ctx.root);
            scenarios.extend(extract_gherkin_scenarios(&parse_result, &relative_file));
        }

        scenarios
    }

    fn collect_step_definitions(&self, ctx: &AnalysisContext<'_>) -> Vec<StepDefinition> {
        let parser = Parser::new();
        let mut definitions = Vec::new();
        let all_paths = FileSet::from_path(ctx.root, ctx.config)
            .map(|set| set.files().to_vec())
            .unwrap_or_else(|_| ctx.files.files().to_vec());

        for path in all_paths.iter() {
            if Language::detect(path) != Some(Language::Python) {
                continue;
            }

            let parse_result = match parse_file_with_context(ctx, &parser, path) {
                Some(result) => result,
                None => continue,
            };

            let relative_file = rel_path(path, ctx.root);
            let mut parsed = extract_python_step_definitions(&parse_result, &relative_file);

            if parsed.is_empty() {
                parsed = extract_python_step_definitions_via_regex(&parse_result, &relative_file);
            }

            definitions.extend(parsed);
        }

        definitions
    }

    fn collect_execution_report(&self, ctx: &AnalysisContext<'_>) -> Option<ExecutionReport> {
        let report_path = self.execution_report.as_ref()?;
        let resolved_path = if report_path.is_absolute() {
            report_path.clone()
        } else {
            ctx.root.join(report_path)
        };

        let content = std::fs::read_to_string(&resolved_path).ok()?;
        let format = self.report_format.as_deref().unwrap_or_else(|| {
            resolved_path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("json")
        });

        parse_execution_report(&content, format).ok()
    }

    fn apply_execution_report(&self, scenarios: &mut [ScenarioCoverage], report: &ExecutionReport) {
        let mut used = vec![false; scenarios.len()];

        for exec_scenario in &report.scenarios {
            let idx = find_matching_scenario_index(scenarios, exec_scenario, &mut used);

            if let Some(idx) = idx {
                let matched = &mut scenarios[idx];
                matched.executed = true;
                matched.execution_status = Some(exec_scenario.status);

                for step in &mut matched.steps {
                    if let Some(exec_step) =
                        find_matching_execution_step(step, &exec_scenario.steps)
                    {
                        step.executed = true;
                        step.execution_status = Some(exec_step.status);
                    }
                }
            }
        }
    }

    fn flatten_steps(scenarios: &[ScenarioCoverage]) -> Vec<ScenarioStep> {
        scenarios
            .iter()
            .flat_map(|scenario| scenario.steps.clone())
            .collect()
    }
}

fn parse_file_with_context<'a>(
    ctx: &AnalysisContext<'a>,
    parser: &Parser,
    path: &Path,
) -> Option<ParseResult> {
    if ctx.content_source.is_some() {
        let content = ctx.read_file(path).ok()?;
        let language = Language::detect(path)?;
        let source_file = SourceFile::from_content(path, language, content);
        parser.parse_source(&source_file).ok()
    } else {
        let full_path = ctx.root.join(path);
        parser.parse_file(full_path).ok()
    }
}

impl AnalyzerTrait for Analyzer {
    type Output = Analysis;

    fn name(&self) -> &'static str {
        "bdd_coverage"
    }

    fn description(&self) -> &'static str {
        "Coverage analysis for Gherkin features and Python step definitions"
    }

    fn analyze(&self, ctx: &AnalysisContext<'_>) -> Result<Self::Output> {
        let mut scenarios = self.collect_gherkin_scenarios(ctx);
        let step_defs = self.collect_step_definitions(ctx);

        let mut missing_steps = Vec::new();
        let mut ambiguous_steps = Vec::new();
        let mut used_defs = HashSet::<String>::new();

        for scenario in &mut scenarios {
            for step in &mut scenario.steps {
                let matched = match_step(step, &step_defs);
                step.matched_definitions = matched.clone();

                if matched.is_empty() {
                    step.status = StepStatus::Missing;
                } else if matched.len() == 1 {
                    step.status = StepStatus::Implemented;
                    used_defs.insert(definition_fingerprint(&matched[0]));
                } else {
                    step.status = StepStatus::Ambiguous;
                    matched.iter().for_each(|d| {
                        used_defs.insert(definition_fingerprint(d));
                    });
                    ambiguous_steps.push(step.clone());
                }

                if step.status == StepStatus::Missing {
                    missing_steps.push(step.clone());
                }
            }

            scenario.has_missing_steps = scenario
                .steps
                .iter()
                .any(|s| s.status == StepStatus::Missing);
            scenario.has_ambiguous_steps = scenario
                .steps
                .iter()
                .any(|s| s.status == StepStatus::Ambiguous);
            scenario.is_fully_implemented =
                !scenario.has_missing_steps && !scenario.has_ambiguous_steps;
        }

        if let Some(execution_report) = self.collect_execution_report(ctx) {
            self.apply_execution_report(&mut scenarios, &execution_report);
        }

        let steps = Self::flatten_steps(&scenarios);
        let steps_total = steps.len();
        let steps_implemented = steps
            .iter()
            .filter(|step| step.status == StepStatus::Implemented)
            .count();
        let steps_missing = steps
            .iter()
            .filter(|step| step.status == StepStatus::Missing)
            .count();
        let steps_ambiguous = steps
            .iter()
            .filter(|step| step.status == StepStatus::Ambiguous)
            .count();
        let steps_executed = steps
            .iter()
            .filter(|step| {
                step.execution_status
                    .is_some_and(|status| status.is_executed())
            })
            .count();
        let steps_failed = steps
            .iter()
            .filter(|step| {
                step.execution_status
                    .is_some_and(|status| status.is_failed())
            })
            .count();

        let scenarios_total = scenarios.len();
        let scenarios_fully_implemented =
            scenarios.iter().filter(|s| s.is_fully_implemented).count();
        let scenarios_executed = scenarios.iter().filter(|s| s.executed).count();
        let scenarios_passed = scenarios
            .iter()
            .filter(|s| s.execution_status == Some(ExecutionStatus::Passed))
            .count();
        let scenarios_failed = scenarios
            .iter()
            .filter(|s| s.execution_status.is_some_and(|status| status.is_failed()))
            .count();
        let scenarios_with_missing_steps = scenarios.iter().filter(|s| s.has_missing_steps).count();
        let scenarios_with_ambiguous_steps =
            scenarios.iter().filter(|s| s.has_ambiguous_steps).count();

        let orphan_step_definitions = step_defs
            .iter()
            .filter(|def| {
                let signature = def_fingerprint(def);
                !used_defs.contains(&signature)
            })
            .cloned()
            .collect();

        let summary = CoverageMetrics {
            steps_total,
            steps_implemented,
            steps_missing,
            steps_ambiguous,
            scenarios_total,
            scenarios_fully_implemented,
            scenarios_executed,
            scenarios_passed,
            scenarios_failed,
            steps_executed,
            steps_failed,
            scenarios_with_missing_steps,
            scenarios_with_ambiguous_steps,
        };

        Ok(Analysis {
            summary,
            scenarios,
            steps,
            step_definitions: step_defs,
            missing_steps,
            ambiguous_steps,
            orphan_step_definitions,
        })
    }
}

fn rel_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn parse_execution_report(
    content: &str,
    format: &str,
) -> std::result::Result<ExecutionReport, String> {
    match format.to_lowercase().as_str() {
        "json" => parse_execution_report_json(content),
        "xml" => parse_execution_report_xml(content),
        _ if format.ends_with("json") => parse_execution_report_json(content),
        _ if format.ends_with("xml") => parse_execution_report_xml(content),
        _ => parse_execution_report_json(content),
    }
}

fn parse_execution_report_json(content: &str) -> std::result::Result<ExecutionReport, String> {
    let value: Value = serde_json::from_str(content)
        .map_err(|e| format!("failed to parse execution report json: {e}"))?;

    let mut scenario_records = Vec::new();
    collect_execution_scenarios(&value, &mut scenario_records);

    if scenario_records.is_empty() {
        return Err("no execution scenarios found in report".to_string());
    }

    Ok(ExecutionReport {
        scenarios: scenario_records,
    })
}

fn parse_execution_report_xml(content: &str) -> std::result::Result<ExecutionReport, String> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("<") {
        return Err("XML execution report must be XML content".to_string());
    }

    let testcase_re =
        Regex::new(r#"(?is)<testcase\b([^>]*?)(?:>(.*?)</testcase>|/>)|<testcase\b([^>]*)/>"#)
            .map_err(|e| format!("failed to build xml testcase regex: {e}"))?;

    let mut scenarios = Vec::new();

    for captures in testcase_re.captures_iter(trimmed) {
        let attributes = captures
            .get(1)
            .map(|m| m.as_str())
            .or_else(|| captures.get(3).map(|m| m.as_str()))
            .unwrap_or_default();
        let body = captures.get(2).map_or("", |m| m.as_str());

        if let Some(scenario) = parse_execution_scenario_from_testcase(attributes, body) {
            scenarios.push(scenario);
        }
    }

    if scenarios.is_empty() {
        // Fallback to embedded JSON payload in mixed-content files, if any.
        if let Some(start) = trimmed.find('{') {
            if let Some(end) = trimmed.rfind('}') {
                return parse_execution_report_json(&trimmed[start..=end]);
            }
        }

        return Err("no execution scenarios found in JUnit XML report".to_string());
    }

    Ok(ExecutionReport { scenarios })
}

fn parse_execution_scenario_from_testcase(
    attributes: &str,
    body: &str,
) -> Option<ExecutionScenario> {
    let raw_name = get_xml_attr(attributes, "name")?;
    let name = normalize_xml_scenario_name(&raw_name);
    if name.is_empty() {
        return None;
    }

    let raw_status = get_xml_attr(attributes, "status");
    let line = get_xml_attr(attributes, "line")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    let status = parse_execution_xml_status(raw_status.as_deref(), body);

    let mut steps = parse_execution_steps_from_testcase_xml(body, status);

    if status == ExecutionStatus::Failed || status == ExecutionStatus::Error {
        let failed_step = parse_failed_step_from_testcase_xml(body);
        if let Some(failed_step) = failed_step {
            let normalized_failed = normalize_match_text(&failed_step);
            let mut failed_index = None;

            for (idx, step) in steps.iter().enumerate() {
                let candidate = normalize_match_text(&format!("{} {}", step.keyword, step.text));
                if candidate == normalized_failed {
                    failed_index = Some(idx);
                    break;
                }
            }

            match failed_index {
                Some(index) => {
                    for (idx, step) in steps.iter_mut().enumerate() {
                        if idx == index {
                            step.status = ExecutionStatus::Failed;
                        } else if idx > index && step.status == ExecutionStatus::Passed {
                            step.status = ExecutionStatus::Skipped;
                        }
                    }
                }
                None => {
                    for step in &mut steps {
                        step.status = ExecutionStatus::Failed;
                    }
                }
            }
        }
    }

    Some(ExecutionScenario {
        name,
        line,
        status,
        steps,
    })
}

fn parse_execution_steps_from_testcase_xml(
    body: &str,
    scenario_status: ExecutionStatus,
) -> Vec<ExecutionStep> {
    let mut steps = Vec::new();
    let mut output_text = String::new();
    let mut error_text = String::new();

    for (_, content) in extract_xml_tag_content(body, "system-out") {
        output_text.push_str(&content);
        output_text.push('\n');
    }

    for (_, content) in extract_xml_tag_content(body, "system-err") {
        error_text.push_str(&content);
        error_text.push('\n');
    }

    let parsed_output = parse_gherkin_step_lines(&output_text);
    let parsed_error = parse_gherkin_step_lines(&error_text);

    let default_status = match scenario_status {
        ExecutionStatus::Passed => ExecutionStatus::Passed,
        ExecutionStatus::Failed | ExecutionStatus::Error => ExecutionStatus::Passed,
        ExecutionStatus::Skipped | ExecutionStatus::Undefined => ExecutionStatus::Skipped,
        ExecutionStatus::Unknown | ExecutionStatus::NotRun => ExecutionStatus::Unknown,
    };

    for (keyword, text) in parsed_output.into_iter().chain(parsed_error) {
        steps.push(ExecutionStep {
            text,
            keyword,
            line: 0,
            status: default_status,
        });
    }

    steps
}

fn parse_failed_step_from_testcase_xml(body: &str) -> Option<String> {
    for (attributes, content) in extract_xml_tag_content(body, "failure") {
        for line in content.lines() {
            if let Some((keyword, text)) = parse_gherkin_step_line(line) {
                return Some(format!("{} {}", keyword, text));
            }
        }
        if let Some(msg) = get_xml_attr(&attributes, "message") {
            if let Some((keyword, text)) = parse_gherkin_step_line(&msg) {
                return Some(format!("{} {}", keyword, text));
            }
        }
    }

    for (attributes, content) in extract_xml_tag_content(body, "error") {
        for line in content.lines() {
            if let Some((keyword, text)) = parse_gherkin_step_line(line) {
                return Some(format!("{} {}", keyword, text));
            }
        }
        if let Some(msg) = get_xml_attr(&attributes, "message") {
            if let Some((keyword, text)) = parse_gherkin_step_line(&msg) {
                return Some(format!("{} {}", keyword, text));
            }
        }
    }

    None
}

fn parse_gherkin_step_lines(text: &str) -> Vec<(String, String)> {
    text.lines().filter_map(parse_gherkin_step_line).collect()
}

fn parse_gherkin_step_line(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let keyword = parts.next()?.to_lowercase();
    let rest = parts.next().unwrap_or("").trim();
    if rest.is_empty() {
        return None;
    }

    let keyword = match keyword.as_str() {
        "given" | "when" | "then" | "and" | "but" | "*" => keyword,
        _ => return None,
    };

    Some((
        keyword[..1].to_uppercase() + &keyword[1..],
        rest.to_string(),
    ))
}

fn parse_execution_xml_status(raw_status: Option<&str>, body: &str) -> ExecutionStatus {
    if let Some(status) = raw_status {
        if !status.is_empty() {
            return ExecutionStatus::from_raw(status);
        }
    }

    if body.contains("<failure") || body.contains("<error") {
        return ExecutionStatus::Failed;
    }

    if body.contains("<skipped") || body.contains("<ignored") || body.contains("<pending") {
        return ExecutionStatus::Skipped;
    }

    ExecutionStatus::Passed
}

fn get_xml_attr(tag: &str, key: &str) -> Option<String> {
    let lower_key = key.to_ascii_lowercase();
    let mut cursor = 0usize;
    let mut haystack = tag;

    while let Some(eq) = haystack.find('=') {
        let before = &haystack[..eq];
        let name_start = before
            .rfind(|c: char| c.is_whitespace() || c == '<' || c == '/' || c == '>')
            .map(|idx| idx + 1)
            .unwrap_or(0);
        let name = before[name_start..].trim();
        if name.eq_ignore_ascii_case(&lower_key) {
            let mut tail = haystack[eq + 1..].trim_start();
            if tail.is_empty() {
                return None;
            }

            let quote = tail.chars().next().unwrap();
            if quote == '\'' || quote == '"' {
                tail = &tail[quote.len_utf8()..];
                let end = tail.find(quote)?;
                return Some(tail[..end].to_string());
            }

            let unquoted_len = tail.find(|c: char| c.is_whitespace()).unwrap_or(tail.len());
            return Some(tail[..unquoted_len].to_string());
        }

        cursor += eq + 1;
        haystack = &tag[cursor..];
    }

    None
}

fn extract_xml_tag_content<'a>(xml: &'a str, tag: &'a str) -> Vec<(String, String)> {
    let open_close = Regex::new(&format!(
        r#"(?is)<{}\b([^>]*)>(.*?)</{}>"#,
        regex::escape(tag),
        regex::escape(tag)
    ))
    .ok();
    let self_closing = Regex::new(&format!(r#"(?is)<{}\b([^>]*)/>"#, regex::escape(tag))).ok();

    let mut out = Vec::new();
    if let Some(re) = open_close {
        for caps in re.captures_iter(xml) {
            let attrs = caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let content = caps
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            out.push((attrs, content));
        }
    }

    if let Some(re) = self_closing {
        for caps in re.captures_iter(xml) {
            let attrs = caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            out.push((attrs, String::new()));
            let _ = attrs;
        }
    }

    out
}

fn normalize_xml_scenario_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let collapsed = trimmed.rsplit("::").next().unwrap_or(trimmed).trim();
    let maybe_scenario = collapsed
        .strip_prefix("Scenario: ")
        .or_else(|| collapsed.strip_prefix("Scenario "));

    if let Some(stripped) = maybe_scenario {
        stripped.trim().to_string()
    } else {
        collapsed.to_string()
    }
}

fn collect_execution_scenarios(value: &Value, out: &mut Vec<ExecutionScenario>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_execution_scenarios(item, out);
            }
        }
        Value::Object(map) => {
            if map.contains_key("name") && map.contains_key("steps") {
                if let Some(parsed) = parse_execution_scenario_obj(value) {
                    out.push(parsed);
                    return;
                }
            }

            if let Some(scenarios) = map.get("scenarios").and_then(|v| v.as_array()) {
                for scenario in scenarios {
                    if let Some(parsed) = parse_execution_scenario_obj(scenario) {
                        out.push(parsed);
                    }
                }
                return;
            }

            if let Some(elements) = map.get("elements").and_then(|v| v.as_array()) {
                for element in elements {
                    if let Some(parsed) = parse_execution_scenario_obj(element) {
                        out.push(parsed);
                    }
                }
            }

            if let Some(features) = map.get("features").and_then(|v| v.as_array()) {
                for feature in features {
                    collect_execution_scenarios(feature, out);
                }
            }

            for (key, nested) in map {
                if key == "scenarios" || key == "elements" || key == "features" {
                    continue;
                }
                collect_execution_scenarios(nested, out);
            }
        }
        _ => {}
    }
}

fn parse_execution_scenario_obj(value: &Value) -> Option<ExecutionScenario> {
    let map = value.as_object()?;
    let status = map
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("unknown");
    let name = map.get("name").and_then(|n| n.as_str())?;

    let line = map
        .get("line")
        .and_then(|v| v.as_u64())
        .or_else(|| {
            map.get("location")
                .and_then(|v| v.as_object())
                .and_then(|loc| loc.get("line"))
                .and_then(|v| v.as_u64())
        })
        .map(|line| line as u32)
        .unwrap_or(0);

    let mut steps = Vec::new();
    if let Some(raw_steps) = map.get("steps").and_then(|v| v.as_array()) {
        for step in raw_steps {
            if let Some(parsed) = parse_execution_step_obj(step) {
                steps.push(parsed);
            }
        }
    }

    Some(ExecutionScenario {
        name: name.trim().to_string(),
        line,
        status: ExecutionStatus::from_raw(status),
        steps,
    })
}

fn parse_execution_step_obj(value: &Value) -> Option<ExecutionStep> {
    let map = value.as_object()?;
    let text = map
        .get("text")
        .or_else(|| map.get("name"))
        .and_then(|v| v.as_str())?;

    let status = map
        .get("status")
        .and_then(|s| s.as_str())
        .or_else(|| {
            map.get("result")
                .and_then(|r| r.as_object())
                .and_then(|result| result.get("status"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("unknown");

    let line = map
        .get("line")
        .and_then(|v| v.as_u64())
        .or_else(|| {
            map.get("location")
                .and_then(|v| v.as_object())
                .and_then(|loc| loc.get("line"))
                .and_then(|v| v.as_u64())
        })
        .map(|line| line as u32)
        .unwrap_or(0);

    let keyword = map
        .get("keyword")
        .and_then(|v| v.as_str())
        .unwrap_or("Given")
        .trim()
        .to_string();

    Some(ExecutionStep {
        text: text.trim().to_string(),
        keyword,
        line,
        status: ExecutionStatus::from_raw(status),
    })
}

fn normalize_match_text(value: &str) -> String {
    value
        .split_whitespace()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn find_matching_scenario_index(
    scenarios: &[ScenarioCoverage],
    execution: &ExecutionScenario,
    used: &mut [bool],
) -> Option<usize> {
    let target_name = normalize_match_text(&execution.name);

    if execution.line > 0 {
        for (idx, scenario) in scenarios.iter().enumerate() {
            if !used[idx]
                && scenario.line == execution.line
                && normalize_match_text(&scenario.name) == target_name
            {
                return Some(idx);
            }
        }
    }

    for (idx, scenario) in scenarios.iter().enumerate() {
        if !used[idx] && normalize_match_text(&scenario.name) == target_name {
            return Some(idx);
        }
    }

    None
}

fn find_matching_execution_step(
    step: &ScenarioStep,
    candidates: &[ExecutionStep],
) -> Option<ExecutionStep> {
    if step.line > 0 {
        for candidate in candidates {
            if candidate.line == step.line {
                return Some(candidate.clone());
            }
        }
    }

    let target_keyword = step.keyword.to_lowercase();
    let target_text = normalize_match_text(&step.text);

    candidates.iter().find_map(|candidate| {
        if candidate.line == 0 || candidate.line == step.line {
            let candidate_keyword = candidate.keyword.to_lowercase();
            let candidate_text = normalize_match_text(&candidate.text);
            if candidate_keyword == target_keyword && candidate_text == target_text {
                Some(candidate.clone())
            } else {
                None
            }
        } else {
            None
        }
    })
}

fn def_fingerprint(definition: &StepDefinition) -> String {
    format!(
        "{}:{}:{}:{}",
        definition.file, definition.line, definition.keyword, definition.pattern
    )
}

fn definition_fingerprint(definition: &MatchedDefinitionRef) -> String {
    format!(
        "{}:{}:{}:{}",
        definition.file, definition.line, definition.keyword, definition.pattern
    )
}

fn extract_gherkin_scenarios(parse_result: &ParseResult, file: &str) -> Vec<ScenarioCoverage> {
    let mut scenarios = Vec::new();
    let source = &parse_result.source;
    let root = parse_result.root_node();

    fn visit(node: Node<'_>, source: &[u8], file: &str, out: &mut Vec<ScenarioCoverage>) {
        if node.kind() == "scenario" || node.kind() == "scenario_outline" {
            if let Some(scenario) = parse_gherkin_scenario(&node, source, file) {
                out.push(scenario);
            }
            return;
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            visit(child, source, file, out);
        }
    }

    visit(root, source, file, &mut scenarios);
    scenarios
}

fn parse_gherkin_scenario(node: &Node<'_>, source: &[u8], file: &str) -> Option<ScenarioCoverage> {
    let name = extract_gherkin_scenario_name(node, source)?;
    let line = node.start_position().row as u32 + 1;
    let scenario_type = node.kind().to_string();
    let tags = collect_gherkin_tags(node, source);

    let mut step_nodes = Vec::new();
    collect_gherkin_step_nodes(
        &node.children(&mut node.walk()).collect::<Vec<_>>(),
        &mut step_nodes,
    );

    let mut steps = Vec::new();
    let mut last_keyword = "Given".to_string();

    for step_node in step_nodes {
        let parsed = match extract_gherkin_step(step_node, source) {
            Some(v) => v,
            None => continue,
        };
        let effective_keyword = match parsed.keyword.as_str() {
            "And" | "But" | "*" => last_keyword.clone(),
            other => {
                last_keyword = other.to_string();
                parsed.keyword.clone()
            }
        };

        steps.push(ScenarioStep {
            file: file.to_string(),
            line: parsed.line,
            keyword: effective_keyword,
            text: parsed.text,
            matched_definitions: Vec::new(),
            status: StepStatus::Missing,
            executed: false,
            execution_status: None,
        });
    }

    Some(ScenarioCoverage {
        file: file.to_string(),
        line,
        name,
        scenario_type,
        tags,
        steps,
        executed: false,
        execution_status: None,
        is_fully_implemented: false,
        has_missing_steps: false,
        has_ambiguous_steps: false,
    })
}

fn collect_gherkin_step_nodes<'a>(children: &[Node<'a>], out: &mut Vec<Node<'a>>) {
    for child in children {
        match child.kind() {
            "given_step" | "when_step" | "then_step" | "and_step" | "but_step"
            | "asterisk_step" => {
                out.push(*child);
            }
            _ => {
                collect_gherkin_step_nodes(
                    &child.children(&mut child.walk()).collect::<Vec<_>>(),
                    out,
                );
            }
        }
    }
}

struct ParsedStep {
    keyword: String,
    text: String,
    line: u32,
}

fn extract_gherkin_step(node: Node<'_>, source: &[u8]) -> Option<ParsedStep> {
    let mut keyword = "Given".to_string();
    let mut text_parts = Vec::new();

    collect_step_text_and_keyword(node, source, &mut keyword, &mut text_parts);

    let text = normalize_step_text(&text_parts);
    let fallback = fallback_step_text(node, source).unwrap_or_default();

    let final_text = match (text.is_empty(), fallback.is_empty()) {
        (true, true) => return None,
        (true, false) => fallback,
        (false, false) => {
            if fallback.len() > text.len() {
                fallback
            } else {
                text
            }
        }
        (false, true) => text,
    };

    if final_text.is_empty() {
        None
    } else {
        Some(ParsedStep {
            keyword,
            text: final_text,
            line: node.start_position().row as u32 + 1,
        })
    }
}

fn collect_step_text_and_keyword(
    node: Node<'_>,
    source: &[u8],
    keyword: &mut String,
    text_parts: &mut Vec<String>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "given_kw" => *keyword = "Given".to_string(),
            "when_kw" => *keyword = "When".to_string(),
            "then_kw" => *keyword = "Then".to_string(),
            "and_kw" => *keyword = "And".to_string(),
            "but_kw" => *keyword = "But".to_string(),
            "step" | "given_line" | "when_line" | "then_line" | "and_line" | "but_line"
            | "asterisk_line" => {
                collect_step_text_and_keyword(child, source, keyword, text_parts);
            }
            "step_param" | "context" => {
                if let Ok(text) = child.utf8_text(source) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        text_parts.push(trimmed.to_string());
                    }
                }
            }
            _ => {
                if child.kind() == "identifier"
                    || child.kind() == "string"
                    || child.kind() == "string_content"
                    || child.kind() == "int"
                    || child.kind() == "float"
                {
                    if let Ok(text) = child.utf8_text(source) {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            text_parts.push(trimmed.to_string());
                        }
                    }
                    continue;
                }
                collect_step_text_and_keyword(child, source, keyword, text_parts);
            }
        }
    }
}

fn normalize_step_text(parts: &[String]) -> String {
    parts
        .iter()
        .map(|part| part.trim())
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

fn fallback_step_text(node: Node<'_>, source: &[u8]) -> Option<String> {
    let text = node.utf8_text(source).ok()?;
    let first = text.lines().next()?.trim();
    let mut chunks = first.splitn(2, char::is_whitespace);
    chunks.next()?;
    chunks
        .next()
        .map(|rest| rest.trim().to_string())
        .filter(|rest| !rest.is_empty())
}

fn extract_gherkin_scenario_name(node: &Node<'_>, source: &[u8]) -> Option<String> {
    let mut header = None;
    for child in node.children(&mut node.walk()) {
        if child.kind() == "scenario_line" || child.kind() == "scenario_outline_line" {
            header = Some(child);
            break;
        }
    }

    let header = header?;
    let mut parts = Vec::new();

    for child in header.children(&mut header.walk()) {
        if child.kind() == "context" {
            if let Ok(text) = child.utf8_text(source) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
        }
    }

    if !parts.is_empty() {
        return Some(parts.join(" "));
    }

    let full = header.utf8_text(source).ok()?;
    let after_colon = full.splitn(2, ':').nth(1)?.trim();
    if after_colon.is_empty() {
        None
    } else {
        Some(after_colon.to_string())
    }
}

fn collect_gherkin_tags(node: &Node<'_>, source: &[u8]) -> Vec<String> {
    let mut tags = Vec::new();
    for child in node.children(&mut node.walk()) {
        if child.kind() == "tags" {
            for tag in child.children(&mut child.walk()) {
                if tag.kind() == "tag" {
                    if let Ok(tag_text) = tag.utf8_text(source) {
                        tags.push(tag_text.to_string());
                    }
                }
            }
        }
    }
    tags
}

fn extract_python_step_definitions(parse_result: &ParseResult, file: &str) -> Vec<StepDefinition> {
    let mut definitions = Vec::new();
    let root = parse_result.root_node();
    let source = &parse_result.source;

    fn visit<'a>(node: Node<'a>, source: &'a [u8], file: &str, out: &mut Vec<StepDefinition>) {
        if node.kind() == "decorated_definition" {
            let mut decorators = Vec::new();
            let mut function_node = None;

            for child in node.children(&mut node.walk()) {
                match child.kind() {
                    "decorator" => decorators.push(child),
                    "function_definition" | "class_definition" => function_node = Some(child),
                    _ => {}
                }
            }

            if let Some(func) = function_node {
                let func_name = func
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("unknown")
                    .to_string();
                let signature = signature_from_node(func, source).unwrap_or_default();

                for decorator in decorators {
                    if let Ok(raw_decorator) = decorator.utf8_text(source) {
                        if let Some((keyword, pattern_kind, pattern)) =
                            parse_step_decorator(raw_decorator)
                        {
                            out.push(StepDefinition {
                                name: func_name.clone(),
                                keyword,
                                pattern,
                                pattern_type: pattern_kind.as_str().to_string(),
                                pattern_kind,
                                line: func.start_position().row as u32 + 1,
                                file: file.to_string(),
                                signature: signature.clone(),
                                raw_decorator: raw_decorator.to_string(),
                            });
                        }
                    }
                }
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            visit(child, source, file, out);
        }
    }

    visit(root, source, file, &mut definitions);
    definitions
}

fn signature_from_node(node: Node<'_>, source: &[u8]) -> Option<String> {
    node.utf8_text(source)
        .ok()
        .map(|text| text.lines().next().map(str::trim).unwrap_or("").to_string())
}

fn extract_python_step_definitions_via_regex(
    parse_result: &ParseResult,
    relative_file: &str,
) -> Vec<StepDefinition> {
    let source = String::from_utf8_lossy(&parse_result.source).to_string();
    let source_lines: Vec<&str> = source.lines().collect();

    let decorator_re = match Regex::new(r"^\s*@(?P<decorator>.+)\s*$") {
        Ok(re) => re,
        Err(_) => return Vec::new(),
    };
    let fn_re = match Regex::new(r"^\s*def\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)") {
        Ok(re) => re,
        Err(_) => return Vec::new(),
    };
    let mut pending: Vec<String> = Vec::new();
    let mut out = Vec::new();

    for (idx, raw_line) in source_lines.iter().enumerate() {
        if let Some(captures) = decorator_re.captures(raw_line) {
            let deco = captures
                .name("decorator")
                .map(|m| format!("@{}", m.as_str()));
            if let Some(deco) = deco {
                if let Some((keyword, kind, pattern)) = parse_step_decorator(&deco) {
                    pending.push(format!("{}|{}|{}", keyword, kind.as_str(), pattern));
                }
            }
            continue;
        }

        if let Some(cap) = fn_re.captures(raw_line) {
            if let Some(name) = cap.name("name") {
                let line = idx as u32 + 1;
                let signature = raw_line.trim().to_string();
                let mut to_append = Vec::new();

                for item in pending.drain(..) {
                    let mut parts = item.splitn(3, '|');
                    let keyword = parts.next().unwrap_or("").to_string();
                    let pattern_type = parts.next().unwrap_or("exact").to_string();
                    let pattern = parts.next().unwrap_or("").to_string();
                    let pattern_kind = match pattern_type.as_str() {
                        "parse" => PatternKind::Parse,
                        "regex" => PatternKind::Regex,
                        "exact" => PatternKind::Exact,
                        _ => PatternKind::Unknown,
                    };

                    to_append.push(StepDefinition {
                        name: name.as_str().to_string(),
                        keyword: keyword.clone(),
                        pattern: pattern.clone(),
                        pattern_type,
                        pattern_kind,
                        line,
                        file: relative_file.to_string(),
                        signature: signature.clone(),
                        raw_decorator: String::new(),
                    });
                }

                out.extend(to_append);
            }
        } else if !raw_line.trim().is_empty() && raw_line.trim().chars().next() != Some('@') {
            // stop collecting decorators on other statements
            pending.clear();
        }
    }

    out.retain(|d| d.pattern_kind != PatternKind::Unknown);
    out
}

fn parse_step_decorator(text: &str) -> Option<(String, PatternKind, String)> {
    let raw = text.trim();
    if !raw.starts_with('@') {
        return None;
    }

    let call = raw.trim_start_matches('@').trim();
    let (callee, args) = split_call(call)?;
    let mut name = callee.trim().to_string();
    if let Some(last) = name.rsplit('.').next() {
        name = last.to_string();
    }

    let keyword = normalize_keyword(&name)?;
    let arg = first_call_arg(args)?;

    let (kind, pattern) = parse_pattern_expr(arg)?;
    Some((keyword, kind, pattern))
}

fn parse_pattern_expr(expr: &str) -> Option<(PatternKind, String)> {
    let expr = expr.trim();
    if let Some((callee, inner)) = split_call(expr) {
        let name = callee.rsplit('.').next().unwrap_or(callee);
        let pattern = first_call_arg(inner)?;
        return match name {
            "parse" => Some((PatternKind::Parse, extract_first_quoted_string(pattern)?)),
            "re" => Some((PatternKind::Regex, extract_first_quoted_string(pattern)?)),
            _ => {
                let pattern = extract_first_quoted_string(pattern)?;
                if is_parse_template(&pattern) {
                    (PatternKind::Parse, pattern)
                } else {
                    (PatternKind::Exact, pattern)
                }
                .into()
            }
        };
    }

    let quoted = extract_first_quoted_string(expr)?;
    if is_parse_template(&quoted) {
        Some((PatternKind::Parse, quoted))
    } else {
        Some((PatternKind::Exact, quoted))
    }
}

fn is_parse_template(pattern: &str) -> bool {
    let mut has_placeholder = false;
    let mut index = 0usize;

    while let Some(open) = pattern[index..].find('{') {
        let open = index + open;
        let after_open = open + 1;
        let close = match pattern[after_open..].find('}') {
            Some(close_rel) => after_open + close_rel,
            None => return false,
        };

        if close <= open + 1 {
            return false;
        }

        let token = pattern[open + 1..close].trim();
        if token.is_empty() || token.contains('{') || token.contains('}') {
            return false;
        }

        has_placeholder = true;
        index = close + 1;
    }

    has_placeholder
}

fn split_call(text: &str) -> Option<(&str, &str)> {
    let open = text.find('(')?;
    let name = text[..open].trim();
    if name.is_empty() {
        return None;
    }

    let mut depth = 0usize;
    let mut iter = text[open + 1..].char_indices();
    while let Some((idx, ch)) = iter.next() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    let args = &text[open + 1..open + 1 + idx];
                    return Some((name, args));
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    None
}

fn first_call_arg(args: &str) -> Option<&str> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut quote = '\0';
    let mut escape = false;

    let mut end = args.len();
    for (idx, ch) in args.char_indices() {
        if escape {
            escape = false;
            continue;
        }

        if in_string {
            if ch == '\\' {
                escape = true;
            } else if ch == quote {
                in_string = false;
                quote = '\0';
            }
            continue;
        }

        match ch {
            '"' | '\'' => {
                in_string = true;
                quote = ch;
            }
            '(' => depth += 1,
            ')' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            ',' if depth == 0 => {
                end = idx;
                break;
            }
            _ => {}
        }
    }

    let first = args[..end].trim();
    if first.is_empty() {
        None
    } else {
        Some(first)
    }
}

fn extract_first_quoted_string(value: &str) -> Option<String> {
    let value = value.trim();
    let bytes = value.as_bytes();
    let mut idx = 0;

    while idx < bytes.len() {
        let ch = bytes[idx];
        if ch == b'\'' || ch == b'"' {
            if let Some(value) = unquote_value(&value[idx..], ch as char, false) {
                return Some(value);
            }
        }

        if (ch == b'r' || ch == b'R')
            && idx + 1 < bytes.len()
            && (bytes[idx + 1] == b'"' || bytes[idx + 1] == b'\'')
        {
            let quote = bytes[idx + 1] as char;
            if let Some(value) = unquote_value(&value[idx + 1..], quote, true) {
                return Some(value);
            }
        }

        idx += 1;
    }

    None
}

fn unquote_value(value: &str, quote: char, raw: bool) -> Option<String> {
    if value.is_empty() || !value.starts_with(quote) {
        return None;
    }

    let mut chars = value[quote.len_utf8()..].chars();
    let mut output = String::new();

    while let Some(ch) = chars.next() {
        if raw {
            if ch == quote {
                return Some(output);
            }
            output.push(ch);
            continue;
        }

        if ch == '\\' {
            if let Some(next) = chars.next() {
                output.push(match next {
                    'n' => '\n',
                    't' => '\t',
                    '\\' => '\\',
                    '\'' if quote == '\'' => '\'',
                    '"' if quote == '"' => '"',
                    _ => next,
                });
            }
            continue;
        }

        if ch == quote {
            return Some(output);
        }

        output.push(ch);
    }

    None
}

fn normalize_keyword(name: &str) -> Option<String> {
    match name.trim().to_ascii_lowercase().as_str() {
        "given" => Some("Given".to_string()),
        "when" => Some("When".to_string()),
        "then" => Some("Then".to_string()),
        "step" => Some("step".to_string()),
        "and" => Some("And".to_string()),
        "but" => Some("But".to_string()),
        _ => None,
    }
}

fn step_keyword_matches(step_keyword: &str, definition_keyword: &str) -> bool {
    if definition_keyword.eq_ignore_ascii_case("step") {
        return true;
    }
    if definition_keyword.eq_ignore_ascii_case("and")
        || definition_keyword.eq_ignore_ascii_case("but")
    {
        return true;
    }
    step_keyword.eq_ignore_ascii_case(definition_keyword)
}

fn match_step(step: &ScenarioStep, defs: &[StepDefinition]) -> Vec<MatchedDefinitionRef> {
    defs.iter()
        .filter(|def| {
            step_keyword_matches(&step.keyword, &def.keyword)
                && step_definition_matches(step.text.as_str(), def)
        })
        .map(|def| MatchedDefinitionRef {
            file: def.file.clone(),
            line: def.line,
            pattern: def.pattern.clone(),
            pattern_type: def.pattern_type.clone(),
            keyword: def.keyword.clone(),
        })
        .collect()
}

fn step_definition_matches(step_text: &str, def: &StepDefinition) -> bool {
    match def.pattern_kind {
        PatternKind::Exact | PatternKind::Unknown => def.pattern == step_text,
        PatternKind::Regex => Regex::new(&def.pattern)
            .map(|re| re.is_match(step_text))
            .unwrap_or(false),
        PatternKind::Parse => to_parse_regex(&def.pattern)
            .ok()
            .and_then(|pattern| Regex::new(&pattern).ok())
            .is_some_and(|re| re.is_match(step_text)),
    }
}

fn to_parse_regex(pattern: &str) -> std::result::Result<String, regex::Error> {
    let mut output = String::from("^");
    let mut cursor = 0usize;

    while let Some(open) = pattern[cursor..].find('{') {
        let open = cursor + open;

        if open > cursor {
            output.push_str(&regex::escape(&pattern[cursor..open]));
        }

        let after_open = open + 1;
        if let Some(close_rel) = pattern[after_open..].find('}') {
            let close = after_open + close_rel;
            output.push_str("(.+)");
            cursor = close + 1;
            continue;
        }

        output.push_str(&regex::escape(&pattern[cursor..]));
        cursor = pattern.len();
    }

    if cursor < pattern.len() {
        output.push_str(&regex::escape(&pattern[cursor..]));
    }

    output.push('$');
    Regex::new(&output).map(|re| re.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_parse_regex() {
        let pattern = to_parse_regex("there are {count:d} results").unwrap();
        assert!(pattern.starts_with("^"));
        assert!(pattern.ends_with('$'));
        assert!(pattern.contains("(.+)"));
    }

    #[test]
    fn test_parse_step_decorator_exact() {
        let parsed = parse_step_decorator("@given(\"user logs in\")").unwrap();
        assert_eq!(
            parsed,
            (
                "Given".to_string(),
                PatternKind::Exact,
                "user logs in".to_string()
            )
        );
    }

    #[test]
    fn test_parse_step_decorator_parse() {
        let parsed =
            parse_step_decorator("@then(parsers.parse(\"the result is {status}\"))").unwrap();
        assert_eq!(
            parsed,
            (
                "Then".to_string(),
                PatternKind::Parse,
                "the result is {status}".to_string()
            )
        );
    }

    #[test]
    fn test_parse_step_decorator_parse_template() {
        let parsed = parse_step_decorator("@given(\"Integrity builds the quotes grid window from fixtures \\\"{start_bar_idx}\\\" to \\\"{end_bar_idx}\\\"\")").unwrap();
        assert_eq!(
            parsed,
            (
                "Given".to_string(),
                PatternKind::Parse,
                "Integrity builds the quotes grid window from fixtures \"{start_bar_idx}\" to \"{end_bar_idx}\"".to_string()
            )
        );
    }

    #[test]
    fn test_parse_step_decorator_regex() {
        let parsed = parse_step_decorator("@step(parsers.re(\"^item (\\\\w+)$\"))").unwrap();
        assert_eq!(
            parsed,
            (
                "step".to_string(),
                PatternKind::Regex,
                "^item (\\w+)$".to_string()
            )
        );
    }

    #[test]
    fn test_parse_step_decorator_raw_regex() {
        let parsed = parse_step_decorator(r#"@step(parsers.re(r"^item (\w+)$"))"#).unwrap();
        assert_eq!(
            parsed,
            (
                "step".to_string(),
                PatternKind::Regex,
                "^item (\\w+)$".to_string()
            )
        );
    }

    #[test]
    fn test_extract_gherkin_scenario_name() {
        let parser = Parser::new();
        let source = b"Feature: Demo\n  Scenario: User can login\n    Given user exists";
        let parse_result = parser
            .parse(source, Language::Gherkin, Path::new("sample.feature"))
            .unwrap();
        let scenarios = extract_gherkin_scenarios(&parse_result, "sample.feature");
        assert_eq!(scenarios.len(), 1);
        assert_eq!(scenarios[0].name, "User can login");
        assert_eq!(scenarios[0].steps.len(), 1);
        assert_eq!(scenarios[0].steps[0].keyword, "Given");
        assert_eq!(scenarios[0].steps[0].text, "user exists");
    }

    #[test]
    fn test_python_step_definition_tree_fallback() {
        let source = b"\"\"\"doc\"\"\"\nfrom behave import given\n\n@given(\"start\")\ndef step_start():\n    pass\n";
        let parse_result = Parser::new()
            .parse(source, Language::Python, Path::new("steps.py"))
            .unwrap();
        let defs = extract_python_step_definitions(&parse_result, "steps.py");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].pattern, "start");
        assert_eq!(defs[0].keyword, "Given");
    }

    #[test]
    fn test_parse_execution_report_json() {
        let report_json = serde_json::json!({
            "scenarios": [
                {
                    "name": "Successful login",
                    "line": 2,
                    "status": "failed",
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
                            "status": "failed"
                        }
                    ]
                }
            ]
        });

        let report = parse_execution_report_json(&report_json.to_string()).unwrap();
        assert_eq!(report.scenarios.len(), 1);
        assert_eq!(report.scenarios[0].name, "Successful login");
        assert_eq!(report.scenarios[0].status, ExecutionStatus::Failed);
        assert_eq!(report.scenarios[0].steps.len(), 2);
    }

    #[test]
    fn test_parse_execution_report_xml() {
        assert_eq!(
            get_xml_attr(r#" name="Login" line="2" status="passed""#, "name"),
            Some("Login".to_string())
        );
        let report_xml = r#"<testsuite>
  <testcase name="Login" line="2" status="passed">
    <system-out>Given user has an account
Then user submits valid credentials</system-out>
  </testcase>
  <testcase name="Checkout" line="10" status="failed">
    <failure>
      Then dashboard is visible
    </failure>
    <system-out>Given user has an account
When user submits valid credentials
Then dashboard is visible</system-out>
  </testcase>
</testsuite>"#;

        let report = parse_execution_report_xml(report_xml).unwrap();
        assert_eq!(report.scenarios.len(), 2);
        assert_eq!(report.scenarios[0].name, "Login");
        assert_eq!(report.scenarios[0].status, ExecutionStatus::Passed);
        assert_eq!(report.scenarios[0].steps.len(), 2);
        assert_eq!(report.scenarios[0].steps[0].status, ExecutionStatus::Passed);
        assert_eq!(report.scenarios[1].status, ExecutionStatus::Failed);
        assert_eq!(report.scenarios[1].steps[2].status, ExecutionStatus::Failed);
    }

    #[test]
    fn test_parse_execution_report_xml_with_scenario_attributes_fallback_status() {
        let report_xml = r#"<testsuite>
  <testcase name="tests.vivasvan.feature::test_flow" status="failed">
    <system-out>Given context exists
When action happens</system-out>
  </testcase>
</testsuite>"#;

        let report = parse_execution_report_xml(report_xml).unwrap();
        assert_eq!(report.scenarios.len(), 1);
        assert_eq!(report.scenarios[0].name, "test_flow");
        assert_eq!(report.scenarios[0].status, ExecutionStatus::Failed);
        assert_eq!(report.scenarios[0].steps.len(), 2);
    }

    #[test]
    fn test_apply_execution_report_marks_steps_and_scenarios() {
        let parser = Parser::new();
        let source = b"Feature: Login\n  Scenario: Successful login\n    Given user has an account\n    When user submits valid credentials\n";
        let parse_result = parser
            .parse(source, Language::Gherkin, Path::new("sample.feature"))
            .unwrap();
        let mut scenarios = extract_gherkin_scenarios(&parse_result, "sample.feature");

        let report = ExecutionReport {
            scenarios: vec![ExecutionScenario {
                name: "Successful login".to_string(),
                line: scenarios[0].line,
                status: ExecutionStatus::Failed,
                steps: vec![
                    ExecutionStep {
                        text: "user has an account".to_string(),
                        keyword: "Given".to_string(),
                        line: scenarios[0].steps[0].line,
                        status: ExecutionStatus::Passed,
                    },
                    ExecutionStep {
                        text: "user submits valid credentials".to_string(),
                        keyword: "When".to_string(),
                        line: scenarios[0].steps[1].line,
                        status: ExecutionStatus::Failed,
                    },
                ],
            }],
        };

        Analyzer::default().apply_execution_report(&mut scenarios, &report);

        assert!(scenarios[0].executed);
        assert_eq!(scenarios[0].execution_status, Some(ExecutionStatus::Failed));
        assert!(scenarios[0].steps[0].executed);
        assert_eq!(
            scenarios[0].steps[0].execution_status,
            Some(ExecutionStatus::Passed)
        );
        assert!(scenarios[0].steps[1].executed);
        assert_eq!(
            scenarios[0].steps[1].execution_status,
            Some(ExecutionStatus::Failed)
        );
    }
}
