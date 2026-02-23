# Полный план внедрения Gherkin + BDD-контур в Omen

## Кратко
- Добавляем полноценную поддержку `.feature` как полноценного языка (`Language::Gherkin`) для детекта файлов и tree-sitter анализа.
- Экстрактим сценарии/шаги как “функции” для semantic search, чтобы `omen search` находил требования и шаги.
- Внедряем новый анализатор `bdd_coverage` с гибридным извлечением step definitions (tree-sitter Python + regex fallback).
- Добавляем CLI-команду и MCP-инструмент для запуска покрытия и отчёта “реальные/ожидаемые” связи.

## Изменения API / типов
- `src/core/language.rs`
  - Добавить `Language::Gherkin`.
  - `from_extension("feature") -> Some(Language::Gherkin)`.
  - `display_name()` добавить `"Gherkin"`.
  - `glob_patterns()` добавить `["**/*.feature"]`.
  - Обновить тесты обнаружения и списки языков где они явно перечислены.
- `src/core/source_file.rs`
  - Добавить `Language::Gherkin` в `is_comment_line` (линию комментариев в Gherkin считать `#`).
- `Cargo.toml`
  - Добавить зависимость `tree-sitter-gherkin` с фиксированным `rev` (пин).
- `src/parser/mod.rs`
  - `get_tree_sitter_language(Language::Gherkin)` -> `tree_sitter_gherkin::LANGUAGE`.
  - `get_function_node_types(Language::Gherkin)` -> `["scenario", "scenario_outline"]`.
  - `extract_function_info` для Gherkin: специальный путь `scenario_line -> context` (или цепочка `context` токенов).
- `src/analyzers/mod.rs`
  - Экспорт нового `bdd_coverage` модуля и `Analyzer`-типа.

## Фаза 1 — Core + индексирование сценариев
- В `src/core/language.rs`, `src/parser/mod.rs`, `src/core/file_set.rs`/`SourceFile`: сделать `.feature` полноценным входным типом.
- Проверить и обновить все exhaustive matches/перечисления языков в:
  - `src/parser/queries.rs` (тесты `test_nesting_node_types_per_language` и `test_flat_node_types_per_language`).
  - `src/parser/mod.rs` тест `test_get_tree_sitter_language_all`.
  - `src/semantic/chunking.rs`, `src/parser/queries.rs`, `src/cli`/`mcp`/`main` при необходимости.
- Принцип: там, где Gherkin не имеет смысла (например, арифметические операторы и т.п.), добавить `_ =>` с корректным дефолтом, чтобы минимизировать побочный эффект.

## Фаза 2 — Поиск по требованиям через semantic search
- В `src/semantic/sync.rs` и `extract_chunks` ничего не менять по типу символа (`symbol_type = "function"`), но сценарии будут индексироваться как символы за счёт `extract_functions`.
- Подтвердить, что `find`/`query` возвращает сценарии по фразам из `Scenario` и тексту шагов.
- Обновить `docs/semantic-search.md` и `README.md` + `src/lib.rs` про поддержку Gherkin в поддерживаемых языках.

## Фаза 3 — Извлечение step definitions (гибрид)
- Создать `src/analyzers/bdd_coverage/step_defs.rs`:
  - `tree-sitter` путь (приоритет):
    - парсить `decorated_definition` в Python,
    - извлекать декораторы `given/when/then/step`,
    - поддерживать `parsers.parse(...)`, `parsers.re(...)`,
    - извлекать сигнатуру/паттерн + имя функции + файл + позицию.
  - `regex` fallback:
    - распознавание `@given(...)` / `@when(...)` / `@then(...)` в строке `SourceFile`.
- Сформировать единый `StepDefinition` модель с полями: `name`, `keyword`, `pattern`, `pattern_type`, `line`, `file`, `signature`, `raw_decorator`.
- Добавить модуль `src/analyzers/bdd_coverage/gherkin.rs`:
  - разбор `scenario`/`scenario_outline`,
  - сбор тегов, шагов (Given/When/Then/And/But),
  - привязка строк/номеров.

## Фаза 4 — Анализатор покрытия `bdd_coverage`
- Создать `src/analyzers/bdd_coverage.rs` (или папку `src/analyzers/bdd_coverage/mod.rs`):
  - `analyze(ctx)`:
    - собрать `Language::Gherkin` файлы,
    - собрать шаги и scenario,
    - собрать step definitions из `.py` через гибридный extractor,
    - матчить шаги к definitions:
      - сначала exact match,
      - потом regex/parse-совместимость,
      - при нескольких матчах — ambiguous,
      - при нулевых — missing.
  - метрики:
    - `steps_total`, `steps_implemented`, `steps_missing`, `steps_ambiguous`,
    - `scenarios_total`, `scenarios_fully_implemented`, `scenarios_with_missing_steps`, `scenarios_with_ambiguous_steps`,
    - списки `orphan_step_definitions`, `ambiguous_steps`.
  - опционально модуль выполнения:
    - если передан execution report (json/xml), пометить `steps_executed`, `scenarios_executed`, failing step/scenario.
- Добавить модульные тесты для:
  - парсинга scenario/steps,
  - tree-sitter + regex fallback для defs,
  - ambiguity/missing/success матчинга.

## Фаза 5 — CLI + MCP интеграция
- `src/cli/mod.rs`
  - добавить команду `BddCoverage(AnalyzerArgs)` с коротким alias.
- `src/main.rs`
  - добавить ветку в `match` и `dispatch_analyzer`.
  - добавить в `all` агрегатор (после внедрения, если нужно, в отдельной группе).
- `src/mcp/mod.rs`
  - добавить инструмент в `tools/list`: `bdd_coverage`.
  - добавить обработчик в `handle_tool_call`: `run_analyzer::<crate::analyzers::bdd_coverage::Analyzer>`.
- Добавить путь для execution report в MCP-аргументах (path, report_path, report_format, strictness flags).

## Тест-план и приемка
- `cargo test` (включая unit + integration), с дополнительными новыми тестами для `.feature`.
- Добавить тестовые фикстуры:
  - `tests/fixtures/sample.feature`,
  - `tests/fixtures/bdd_steps.py` для behave/pytest-bdd декораторов.
- Интеграционные проверки:
  - `omen -p tests/fixtures -f json bdd-coverage` возвращает корректный JSON и summary.
  - `omen -p ... bdd-coverage` + `--execution-report` обрабатывает failing сценарий.
  - `cargo run -- mcp` tool list содержит `bdd_coverage`, `tools/call` валиден.
  - `grep` по semantic search: `omen search query "Scenario ..."`, сценарий/шаги индексируются.

## Предположения по умолчанию
- Используем вариант `Language::Gherkin` (а не псевдо-режим под существующий язык).
- Название команды: CLI `bdd-coverage`, MCP tool: `bdd_coverage`.
- Step definition matcher: сначала exact, затем regex/parse-морфинк, затем ambiguous/missing.
- Режеиспользуемый `Language`-ветвевой код в остальных модулях закрываем через `_`/no-op fallback.
