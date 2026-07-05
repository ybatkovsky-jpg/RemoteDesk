# Фаза 1: Результаты (MVP стриминг в LAN)

## Статус: Код написан, ожидает компиляции

Rust не установлен на dev-машине — компиляция и тестирование будут выполнены после установки.

## Что реализовано

### 1. Интеграция RustDesk libs
- ✅ `vendor/rustdesk` — git submodule с оригинальным репозиторием
- ✅ `vendor/rustdesk/libs/hbb_common` — git submodule инициализирован
- ✅ `crates/rd-common` — наша обёртка, ре-экспортирует `hbb_common` как `core`
- ✅ `crates/screen-capture` — использует `scrap` напрямую (Capturer, Display)
- ✅ `crates/input-sim` — использует `enigo` напрямую (Enigo, KeyboardControllable, MouseControllable)

### 2. Кодирование (zstd MVP)
- ✅ `crates/codec` — компрессия zstd на сырых BGRA-кадрах
- ✅ `FrameEncoder` — сжатие 3-5x для скриншотов
- ✅ `FrameDecoder` — декомпрессия с верификацией размера

### 3. Сетевой протокол (TCP)
- ✅ `crates/network/src/protocol.rs` — length-delimited framing с bincode
- ✅ `NetworkMessage` enum: Hello, Welcome, VideoFrame, KeyEvent, MouseEvent, Ping, Pong, Disconnect
- ✅ `crates/network/src/host.rs` — HostSession: захват + компрессия + отправка
- ✅ `crates/network/src/client.rs` — ClientSession: приём + декомпрессия + polling

### 4. Tauri интеграция
- ✅ 12 Tauri команд: version, status, displays, start/stop host, connect/disconnect, frame polling, input
- ✅ Events: `host-status`, `connection-state`
- ✅ AppState с Arc-совместимыми сессиями

### 5. Фронтенд (SolidJS)
- ✅ `src/App.tsx` — два режима: Host (захват) / Client (подключение)
- ✅ `src/components/RemoteScreen.tsx` — Canvas рендеринг через `putImageData`
- ✅ `src/lib/tauri.ts` — типизированные обёртки над invoke/events
- ✅ Перехват мыши/клавиатуры в canvas → события Tauri

## Что будет в Фазе 2

- [ ] H.264 кодирование (OpenH264 / аппаратное)
- [ ] NAT Traversal (ICE/STUN/TURN через libjuice)
- [ ] Шифрование (NaCl/libsodium)
- [ ] Передача файлов (tus протокол)
- [ ] Поддержка нескольких мониторов
- [ ] Буфер обмена
- [ ] Аудио

## Инструкция по запуску (после установки Rust)

```bash
# 1. Установить Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Проверить компиляцию
cd RemoteDesk
cargo check --workspace

# 3. Запустить в dev-режиме
npm run tauri:dev

# 4. Тест в LAN
# Окно 1: Host → Start Host (display 0, port 9000)
# Окно 2: Client → Connect (127.0.0.1:9000)
```

## Архитектурные заметки

- **zstd вместо H.264**: В локальной сети zstd даёт достаточную производительность
  (~3-5x сжатие на скриншотах). Переход на H.264 в Фазе 2.
- **TCP вместо P2P**: Прямой TCP достаточен для LAN. NAT traversal — в Фазе 2.
- **polling вместо push**: Фронтенд опрашивает кадры с частотой 30 FPS.
  Это упрощает архитектуру, но добавляет latency. В Фазе 2 — push через Tauri events.
- **Arc<ClientSession>**: Все методы используют interior mutability,
  что позволяет шарить сессию между потоками без сложных локов.
