# RemoteDesk — Handoff

**Дата:** 2026-07-06  
**Репозиторий:** https://github.com/ybatkovsky-jpg/RemoteDesk  
**Статус:** Фазы 0+1+2 завершены, `cargo check --workspace` проходит (0 ошибок)

---

## Быстрый старт

```bash
git clone --recurse-submodules https://github.com/ybatkovsky-jpg/RemoteDesk.git
cd RemoteDesk
npm install
```

### Окружение Windows (GNU toolchain)

```bash
export LIBCLANG_PATH="/c/Program Files/LLVM/bin"
export VCPKG_ROOT=/tmp/vcpkg-stub
export PATH="/tmp/mingw-full/mingw64/bin:$PATH"
```

**Важно:** Rust 1.96.1 GNU не включает `as.exe` — скопирован из MinGW в `self-contained/`.  
Полный MinGW лежит в `/tmp/mingw-full/`. Нужен `windres` для `tauri-winres`.

### Команды

```bash
cargo check --workspace          # Проверка (работает)
npm run tauri:dev                # Dev-режим (UI + Rust)
npm run tauri:build              # Релизный .msi (~8-10 мин)
# → src-tauri/target/release/bundle/msi/RemoteDesk_*.msi
# → src-tauri/target/release/bundle/nsis/RemoteDesk_*-setup.exe
```

---

## Что сделано

### Фаза 0 — Структура
- Монорепо: 6 крейтов + Tauri v2 + SolidJS
- CI/CD (GitHub Actions): формат, clippy, тесты, лицензии, сборка
- Документация: `docs/architecture.md`, `docs/licensing.md`, `docs/capture-apis.md`, `docs/codecs.md`
- RustDesk как git submodule в `vendor/rustdesk/`

### Фаза 1 — Код MVP

| Крейт | Назначение |
|-------|-----------|
| `rd-common` | Типы, protobuf, Config |
| `screen-capture` | Захват экрана (stub) |
| `codec` | zstd-компрессия |
| `input-sim` | enigo-обёртка + clipboard (Windows) |
| `network` | TCP + протокол |
| `remote-desk` | Tauri shell |

12 Tauri команд + Host/Client mode + Canvas-рендеринг.

### Фаза 2 — Продакшен-фичи (сегодня)

| Слайс | Что сделано | Файлы |
|-------|------------|-------|
| **1. Багфиксы** | Input forwarding работает (KeyEvent/MouseEvent → TCP → InputSimulator). HostSession в `Arc<Mutex<>>` → `stop_host` работает. Фреймы без base64 (`client_get_frame_raw` → ArrayBuffer). | `client.rs`, `host.rs`, `commands/mod.rs`, `state.rs`, `RemoteScreen.tsx`, `tauri.ts` |
| **2. E2E шифрование** | Новый крейт `crates/crypto/` — Curve25519 key exchange + XSalsa20-Poly1305. Все сообщения шифруются после handshake. | `crates/crypto/*`, `protocol.rs`, `client.rs`, `host.rs` |
| **3. NAT Traversal** | UDP транспорт с фрагментацией + STUN клиент. Сигналинг-протокол (RegisterPeer, RequestConnection, IceCandidate, PeerInfo). | `crates/network/src/udp.rs`, `protocol.rs` |
| **4. H.264 кодек** | Codec negotiation в Hello/Welcome. `FrameEncoder::with_codec()`. Фичи `openh264`/`hwcodec` для будущей интеграции нативных библиотек. | `codec/encoder.rs`, `codec/lib.rs`, `protocol.rs` |
| **5. Мульти-монитор + Clipboard** | `SwitchDisplay` в протоколе. `ClipboardText` с real Win32 реализацией (raw FFI, без конфликтов версий windows crate). | `input-sim/lib.rs`, `protocol.rs`, `host.rs` |
| **6. File transfer + Audio** | Протокольные сообщения: `FileRequest`, `FileStart`, `FileChunk`, `FileEnd`, `FileCancel`, `AudioFrame`, `AudioControl`. | `protocol.rs`, `host.rs` |

---

## Архитектура после Фазы 2

```
crates/
  rd-common/     — типы, Config, Error, proto (KeyEvent, MouseEvent, DisplayInfo, ClipboardData)
  crypto/        — KeyExchange (Curve25519), SessionCipher (XSalsa20-Poly1305)
  screen-capture/ — Capturer (stub/vcpkg-native)
  codec/         — FrameEncoder/FrameDecoder (zstd + H264/H265 стобы)
  input-sim/     — InputSimulator (enigo) + clipboard (Win32 raw FFI)
  network/
    protocol.rs  — NetworkMessage: Hello/Welcome (c codec negotiation), CryptoHandshake*,
                   VideoFrame, KeyEvent, MouseEvent, SwitchDisplay, ClipboardText,
                   FileRequest/Start/Chunk/End/Cancel, AudioFrame/Control,
                   UpdateSettings, Ping/Pong, Disconnect,
                   RegisterPeer, RequestConnection, IceCandidate, PeerInfo
    host.rs      — HostSession: захват → компрессия → шифрование → TCP/UDP
    client.rs    — ClientSession: TCP → дешифровка → декомпрессия → Canvas
    udp.rs       — UdpTransport: фрейминг, фрагментация, STUN
  src-tauri/     — Tauri shell: 13 команд (12 старых + client_get_frame_raw)
  src/            — SolidJS фронтенд: Host/Client mode, Canvas (ArrayBuffer)
```

---

## Ключевые протокольные изменения (Фаза 2)

```
Client                          Host
  |                               |
  |--- Hello {codecs} ---------->>|  (plaintext: версия + список кодеков)
  |<<-- Welcome {codec} ---------|  (plaintext: negotiated codec + размеры)
  |--- CryptoHandshake {pk} --->>|  (plaintext: Curve25519 pubkey)
  |<<-- CryptoHandshakeAck {pk} -|  (plaintext: Curve25519 pubkey)
  |                               |
  |<===== ENCRYPTED ============>|  (XSalsa20-Poly1305, nonce=счётчик)
  |   VideoFrame / KeyEvent /    |
  |   MouseEvent / Ping / Pong / |
  |   ClipboardText / File* /    |
  |   Audio* / SwitchDisplay     |
  |                               |
```

---

## Известные проблемы

1. **scrap не компилируется** без vcpkg (libyuv + libvpx). `screen-capture` работает в stub-режиме.
2. **dlltool/as.exe** — баг Rust 1.96.1 GNU. Фикс: скопирован `as.exe` из MinGW.
3. **Tauri build** требует валидный `icon.ico` — исправлено в `tauri.conf.json` (`"icon": ["icons/icon.ico"]`).
4. **OpenH264/hwcodec** — фичи объявлены, но native-библиотеки не установлены. Падают на zstd.
5. **NAT traversal** — протокол готов, но требуется rendezvous сервер (hbbs) для hole-punching.
6. **Clipboard** — только Windows (raw FFI). Linux/macOS — TODO.
7. **Audio** — протокол есть, реализация захвата/кодирования — TODO.

---

## Что дальше (Фаза 3)

- [ ] Включить реальный захват экрана: vcpkg + libyuv + libvpx → `screen-capture` с `feature=native`
- [ ] OpenH264: установить `openh264.dll`, включить feature `openh264` в `codec`
- [ ] Запустить rendezvous/relay сервер (hbbs/hbbr) для NAT traversal
- [ ] E2E-тестирование: два экземпляра, живой стриминг + ввод
- [ ] Clipboard: Linux (xclip) + macOS (NSPasteboard)
- [ ] Audio: захват через cpal, кодирование Opus, UDP-стриминг
- [ ] File transfer: серверная часть (валидация путей, chunking)
- [ ] Android клиент (Flutter или Tauri Mobile)
