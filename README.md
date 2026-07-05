# RemoteDesk

Кроссплатформенное приложение удаленного доступа с функциями управления экраном/вводом и передачей файлов.

**Статус:** Фаза 0 — Подготовка и исследование

## Архитектура

- **Ядро:** RustDesk (AGPL-3.0) — захват экрана, кодирование, P2P-сеть, NAT traversal
- **UI:** Tauri v2 (MIT) + SolidJS + TypeScript
- **Сервер:** hbbs/hbbr (RustDesk-совместимый signaling/relay)

## Быстрый старт

### Требования

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) 22+
- [Tauri CLI](https://v2.tauri.app/start/prerequisites/) (platform-specific dependencies)

### Установка

```bash
# Клонировать с подмодулями
git clone --recurse-submodules https://github.com/RemoteDesk/RemoteDesk.git
cd RemoteDesk

# Установить frontend-зависимости
npm install

# Запустить в режиме разработки
npm run tauri:dev

# Собрать production build
npm run tauri:build
```

### Сборка отдельных крейтов

```bash
# Проверить весь workspace
cargo check --workspace

# Тесты
cargo test --workspace

# Линтинг
cargo clippy --workspace -- -D warnings
```

## Структура проекта

```
RemoteDesk/
├── crates/                 # Rust-крейты
│   ├── hbb-common/         # Общие типы, protobuf, конфигурация
│   ├── screen-capture/     # Захват экрана (обёртка над scrap)
│   ├── codec/              # Видеокодеки (hwcodec + OpenH264)
│   ├── input-sim/          # Инжекция ввода (enigo)
│   └── network/            # P2P, rendezvous, relay
├── src-tauri/              # Tauri desktop приложение
├── src/                    # Frontend (SolidJS + TypeScript)
├── vendor/rustdesk/        # Git submodule: оригинальный RustDesk
├── docs/                   # Документация
└── .github/workflows/      # CI/CD
```

## Документация

- [Архитектура](docs/architecture.md)
- [Лицензионный аудит](docs/licensing.md)
- [API захвата экрана](docs/capture-apis.md)
- [Видеокодеки](docs/codecs.md)

## Лицензия

AGPL-3.0 — унаследована от RustDesk. См. [docs/licensing.md](docs/licensing.md) для детального анализа лицензионных рисков.

## Роадмап

- [x] **Фаза 0**: Подготовка, структура проекта, CI/CD
- [ ] **Фаза 1**: Базовый стриминг (MVP) в локальной сети
- [ ] **Фаза 2**: NAT Traversal, передача файлов, управление вводом
- [ ] **Фаза 3**: UI, безопасность, полировка
- [ ] **Фаза 4**: Продакшен и масштабирование
