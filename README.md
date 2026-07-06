# RemoteDesk

Кроссплатформенное приложение удаленного доступа с шифрованием, передачей файлов, чатом и мульти-мониторной поддержкой.

**Статус:** Фазы 0-5 завершены ✅ · `cargo check` 0 ошибок · `cargo test` 10/10

<p align="center">
  <img src="https://img.shields.io/badge/rust-stable-orange?logo=rust" />
  <img src="https://img.shields.io/badge/tauri-v2-blue?logo=tauri" />
  <img src="https://img.shields.io/badge/solidjs-1.9-4fc08d?logo=solid" />
  <img src="https://img.shields.io/badge/license-AGPL--3.0-red" />
</p>

---

## Возможности

| Фича | Статус |
|---|---|
| 🖥️ Захват и стриминг экрана (DXGI / scrap) | ✅ |
| 🎥 Аппаратное кодирование H.264/H.265 (NVENC, AMF, QSV, VT) | ✅ |
| 🔒 E2E шифрование (Curve25519 + XSalsa20-Poly1305) | ✅ |
| 🔑 Аутентификация по паролю | ✅ |
| 🖥️🖥️ Мульти-монитор — переключение на лету | ✅ |
| 📋 Буфер обмена | ✅ |
| ⌨️🖱️ Проброс клавиатуры и мыши | ✅ |
| 📁 Передача файлов (download/upload с прогрессом) | ✅ |
| 💬 Чат между host и client | ✅ |
| 🔊 Аудио-стриминг (захват → Opus → воспроизведение) | ✅ |
| ⚙️ Конфигурация (TOML, сохраняется локально) | ✅ |
| 🌐 NAT traversal / UDP hole punching | ✅ |
| 🔄 Relay-сервер (TCP fallback) | ✅ |
| 📦 Инсталляторы (NSIS/WiX/DMG/DEB) + CI release | ✅ |
| 📱 Android (Tauri mobile, MediaProjection, JNI) | ✅ |

## Архитектура

- **Ядро:** RustDesk (AGPL-3.0) — захват экрана, кодирование, P2P-сеть
- **UI:** Tauri v2 (MIT) + SolidJS + TypeScript
- **Шифрование:** NaCl/libsodium (sodiumoxide)
- **Сервер:** hbbs/hbbr (RustDesk-совместимый signaling/relay)

## Быстрый старт

### Требования

- [Rust](https://rustup.rs/) (stable, **GNU toolchain** — `stable-x86_64-pc-windows-gnu`)
- [Node.js](https://nodejs.org/) 22+
- GCC (mingw-w64) — для Windows GNU
- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

### Переменные окружения (Windows GNU)

```bash
export LIBCLANG_PATH="/c/Program Files/LLVM/bin"
export PATH="/path/to/mingw64/bin:$PATH"
```

### Установка и запуск

```bash
git clone --recurse-submodules https://github.com/ybatkovsky-jpg/RemoteDesk.git
cd RemoteDesk
npm install
npm run tauri:dev      # dev-режим
npm run tauri:build    # production build
```

### Сборка и тесты (Rust)

```bash
cargo check --workspace     # проверка компиляции
cargo test --workspace      # тесты (10/10)
cargo clippy --workspace    # линтинг
```

## Структура проекта

```
RemoteDesk/
├── crates/
│   ├── rd-common/          # Общие типы, конфигурация, proto
│   ├── screen-capture/     # Захват экрана (scrap/DXGI)
│   ├── codec/              # Видеокодеки (Zstd, H.264 HW)
│   ├── input-sim/          # Инжекция ввода (enigo)
│   ├── network/            # TCP/UDP транспорт, протокол, host/client/rendezvous
│   ├── crypto/             # E2E шифрование (NaCl)
│   ├── audio/              # Аудио: захват (cpal), Opus (magnum-opus), вывод
│   └── relay-server/       # Лёгкий TCP relay сервер для P2P fallback
├── src-tauri/              # Tauri desktop приложение (Rust)
│   └── src/
│       ├── commands/       # Tauri IPC команды
│       ├── state.rs        # Глобальное состояние приложения
│       └── lib.rs          # Точка входа Tauri
├── src/                    # Frontend (SolidJS + TypeScript)
│   ├── App.tsx             # Главный компонент (host/client/idle режимы)
│   ├── App.css             # Тёмная тема
│   ├── lib/tauri.ts        # Типизированные биндинги Tauri API
│   └── components/
│       ├── RemoteScreen.tsx       # Canvas для удалённого экрана + ввод
│       ├── SettingsPanel.tsx      # Настройки видео/безопасности/сервера
│       ├── ChatPanel.tsx          # Чат host ↔ client
│       ├── FileTransferPanel.tsx  # Браузер файлов + download/upload
│       └── AuthDialog.tsx         # Диалог ввода пароля
├── vendor/rustdesk/        # Git submodule: оригинальный RustDesk (hbb_common, enigo)
├── docs/                   # Документация (архитектура, кодеки, лицензии)
└── .github/workflows/      # CI/CD
```

## Документация

- [Архитектура](docs/architecture.md)
- [API захвата экрана](docs/capture-apis.md)
- [Видеокодеки](docs/codecs.md)
- [Лицензионный аудит](docs/licensing.md)
- [Результаты фазы 1](docs/phase1-results.md)

## Роадмап

- [x] **Фаза 0**: Структура проекта, CI/CD, исследование API
- [x] **Фаза 1**: Базовый стриминг (MVP) в локальной сети — захват экрана, кодирование, TCP
- [x] **Фаза 2**: E2E шифрование, NAT traversal, H.264, мульти-монитор, clipboard, файлы, аудио-протокол
- [x] **Фаза 3**: UI — аутентификация, настройки, чат, файловый трансфер, аудио-контролы
- [x] **Фаза 4**: Продакшен — аудио-стриминг (cpal + Opus), relay-сервер, P2P (UDP/STUN/rendezvous), инсталляторы, CI release

## Лицензия

AGPL-3.0 — унаследована от RustDesk. См. [docs/licensing.md](docs/licensing.md) для детального анализа.

---

**Автор:** [Yura Batkovsky](https://github.com/ybatkovsky-jpg)
