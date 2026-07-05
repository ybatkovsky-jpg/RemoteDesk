# RemoteDesk — Handoff

**Дата:** 2026-07-06  
**Репозиторий:** https://github.com/ybatkovsky-jpg/RemoteDesk  
**Статус:** Фазы 0+1 завершены, `cargo check --workspace` проходит (0 ошибок)

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
export PATH="/c/Users/Yura/.cargo/bin:/tmp/mingw-full/mingw64/bin:.../self-contained:$PATH"
```

**Важно:** Rust 1.96.1 GNU не включает `as.exe` — скопирован из MinGW в `self-contained/`.  
Полный MinGW лежит в `/tmp/mingw-full/`.

### Команды

```bash
cargo check --workspace          # Проверка (работает)
cargo check -p rd-common -p ...  # Проверка отдельных крейтов
npm run tauri:dev                # Dev-режим (UI + Rust)
npm run tauri:build              # Релизный .msi (долгая сборка)
```

---

## Что сделано

### Фаза 0 — Структура
- Монорепо: 5 крейтов + Tauri v2 + SolidJS
- CI/CD (GitHub Actions): формат, clippy, тесты, лицензии, сборка
- Документация: `docs/architecture.md`, `docs/licensing.md`, `docs/capture-apis.md`, `docs/codecs.md`
- RustDesk как git submodule в `vendor/rustdesk/`

### Фаза 1 — Код MVP

| Крейт | Назначение | Статус |
|-------|-----------|--------|
| `rd-common` | Типы, protobuf, Config | ✅ Компилируется |
| `screen-capture` | Захват экрана | ⚠️ Stub (scrap отключён) |
| `codec` | zstd-компрессия | ✅ Компилируется |
| `input-sim` | enigo-обёртка | ✅ Компилируется |
| `network` | TCP + протокол | ✅ Компилируется |
| `remote-desk` | Tauri shell | ✅ Компилируется |

12 Tauri команд: `start_host`, `stop_host`, `client_connect`, `client_disconnect`, `client_get_frame`, `client_get_frame_size`, `send_key_event`, `send_mouse_event`, `list_displays`, `get_version`, `get_app_status`, `client_get_state`

Фронтенд: Host mode (захват) + Client mode (Canvas)

---

## Что дальше

### Сборка установщика (desktop)
```bash
npm run tauri:build    # ~10-15 мин первой сборки
# Выход: src-tauri/target/release/bundle/msi/*.msi
```

### Android
Текущая архитектура (Tauri) **не поддерживает Android как target** (только как host для webview). Для Android нужен:
- Отдельный Flutter-клиент (как в оригинальном RustDesk)
- Или Tauri Mobile (экспериментально)
- Или нативный Android клиент на Kotlin

### Включить настоящий захват экрана
```bash
# Установить vcpkg + libyuv + libvpx
# Затем: cargo check --features native -p screen-capture
```

### Фаза 2 — Продакшен-фичи
- [ ] H.264 через OpenH264 (BSD) или аппаратные кодеки
- [ ] NAT traversal: ICE/STUN/TURN (libjuice)
- [ ] E2E шифрование: NaCl/libsodium
- [ ] Передача файлов: tus-протокол
- [ ] Несколько мониторов
- [ ] Буфер обмена
- [ ] Аудио

---

## Известные проблемы

1. **scrap не компилируется** без vcpkg (libyuv + libvpx). Решение: `screen-capture` работает в stub-режиме.
2. **dlltool/as.exe** — баг Rust 1.96.1 GNU. Фикс: скопирован `as.exe` из MinGW.
3. **Tauri build** требует валидный `icon.ico` — взят из RustDesk `res/icon.ico`.
4. **Tauri bundle identifier** не должен заканчиваться на `.app`.
