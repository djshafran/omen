# BDD Coverage (`bdd-coverage`)

`bdd-coverage` анализирует связь `feature`-сценариев, шагов и Python step definitions.

Поддерживается:
- `Given`/`When`/`Then`/`And`/`But` из `.feature`
- `@given`/`@when`/`@then`/`@step` декораторы в Python
- `parsers.parse("{...}")` и `parsers.re(...)`
- опциональная корреляция с execution-report (JSON или XML)

```bash
omen -p /path/to/project bdd-coverage -f json
```

## Параметры CLI

- `-r, --execution-report <path>` — путь к execution report (поведение behave/pytest-bdd).
- `--report-format <json|xml>` — подсказывает формат отчета, если имя файла не очевидно.

```bash
omen -p /path/to/project bdd-coverage \
  -r reports/execution.json \
  -f json

omen -p /path/to/project bdd-coverage \
  -r reports/junit.xml \
  --report-format xml \
  -f json
```

## MCP метод `bdd_coverage`

Инструмент доступен через MCP как `tools/call`:

```bash
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
  "name":"bdd_coverage",
  "arguments":{
    "path":"/path/to/project",
    "execution_report":"reports/execution.json"
  }
}}
```

## Формат результата

Корневой объект:
- `summary` — агрегированные метрики
- `scenarios` — список сценариев с шагами и их статусами
- `steps` — плоский список шагов
- `step_definitions` — найденные декораторы из Python
- `missing_steps` — шаги без определения
- `ambiguous_steps` — шаги с >1 матчем
- `orphan_step_definitions` — декораты без привязки к сценариям

### Сводка `summary`

- `steps_total`, `steps_implemented`, `steps_missing`, `steps_ambiguous`
- `scenarios_total`, `scenarios_fully_implemented`
- `scenarios_executed`, `scenarios_passed`, `scenarios_failed`
- `steps_executed`, `steps_failed`
- `scenarios_with_missing_steps`, `scenarios_with_ambiguous_steps`

### Подход к статусам исполнения

Когда `execution_report` передан, по имени+линии сценария (и при возможности по шагам)
устанавливаются поля:
- `executed` — `true/false`
- `execution_status` — `passed|failed|error|skipped|undefined|unknown|notrun`
- для шагов дополнительно `execution_status` и `executed`

Это удобно использовать как "мини-альтернатива Allure-like" — для каждого шага видно:
- есть ли реализация в коде
- срабатывает ли шаг в отчете запуска
- кто виноват в падении (`ambiguous`/`missing`/`failed`/`orphan`)

## Примеры чтения

- Если `scenarios_fully_implemented == scenarios_total` и `missing_steps == []`  
  → покрытие реализацией полное.

- Если много `orphan_step_definitions`  
  → старый/мертвый тестовый код, который можно удалять или актуализировать.

- Если много `ambiguous_steps`  
  → одинаковые step patterns перекрывают друг друга, нужны более точные/конкретные паттерны.

## Пример шаблона parse-паттерна

```python
@then(parsers.parse("the result is {status}"))
def assert_result(context, status):
    ...
```

В `bdd-coverage` такие шаблоны распознаются как `pattern_type: parse` и матчатся как
параметрические выражения.

