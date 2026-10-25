# Agent Instructions Bootstrap

This repository uses local instruction files by default.

## Instruction Load Order
1. If `.codex/AGENTS.remote.md` is already present (provisioned during fresh Codex web-agent environment setup), read it first.
2. Apply repository overrides from `REPO_AGENTS.md` when the file exists.
3. Apply additional scoped `AGENTS.md` files in subdirectories when present.

## Initialization Policy
- Do not run pre-task live fetch in normal local development.
- Use codex-tools initialization scripts only when bootstrapping a fresh Codex web-agent environment from scratch.
- Do not commit `.codex/AGENTS.remote.md`.

## Локальные результаты и графы

Сгенерированные графы `graphify-out/`, отчёты, captures и результаты прогонов
хранятся локально в игнорируемых каталогах. Не добавляйте их в Git, включая
`git add -f`. Исходники, необходимые тестовые данные и эталоны остаются в Git.
После нового клонирования создайте граф штатной командой проекта перед его
использованием; наличие сохранённого графа в истории Git не требуется.
