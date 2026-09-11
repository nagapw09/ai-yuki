//! Обновление приложения (`docs/GAPS.md` §2).
//!
//! # Почему это обязательный слой, а не удобство
//!
//! Yuki ходит в чужие API, запускает чужие MCP-серверы и просит системные
//! разрешения. Любая из этих частей однажды потребует срочной правки — и без
//! канала обновлений единственный способ её доставить это попросить человека
//! самому найти и скачать новую сборку. На двух ОС сразу это не работает.
//!
//! # Обновление подписано, и без подписи его не будет
//!
//! Канал обновлений — это право запускать код на чужой машине. Поэтому Tauri
//! проверяет подпись пакета публичным ключом из конфигурации, и пока ключ не
//! задан, обновления **выключены целиком**, а не «работают без проверки».
//!
//! Ключ подписи не лежит в репозитории и не может там лежать: приватная
//! половина принадлежит тому, кто выпускает сборки. Как её создать и куда
//! положить — `docs/RELEASE.md`.

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Состояние канала обновлений.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    /// Настроен ли канал: есть ли публичный ключ и адрес.
    pub configured: bool,
    /// Почему канал не работает — текстом, а не молчанием.
    pub reason: Option<String>,
}

/// Найденное обновление.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    pub version: String,
    pub current_version: String,
    /// Список изменений из манифеста выпуска.
    pub notes: Option<String>,
    pub published_at: Option<String>,
}

/// Настроен ли канал обновлений.
///
/// Проверяется по конфигурации, а не по результату запроса: «ключ не задан» и
/// «сервер недоступен» — разные беды с разными действиями, и валить их в одну
/// ошибку значит отправить человека чинить не то.
fn channel(app: &AppHandle) -> Result<(), String> {
    let config = app.config();

    let updater = config
        .plugins
        .0
        .get("updater")
        .ok_or("канал обновлений не настроен в конфигурации сборки")?;

    let pubkey = updater
        .get("pubkey")
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    if pubkey.trim().is_empty() {
        return Err(
            "обновления выключены: не задан публичный ключ подписи. \
             Без него проверить, что пакет пришёл от автора, невозможно — \
             см. docs/RELEASE.md"
                .into(),
        );
    }

    let has_endpoint = updater
        .get("endpoints")
        .and_then(|value| value.as_array())
        .is_some_and(|list| !list.is_empty());

    if !has_endpoint {
        return Err("обновления выключены: не задан адрес канала".into());
    }

    Ok(())
}

#[tauri::command]
pub fn update_status(app: AppHandle) -> UpdateStatus {
    let outcome = channel(&app);

    UpdateStatus {
        current_version: app.package_info().version.to_string(),
        configured: outcome.is_ok(),
        reason: outcome.err(),
    }
}

/// Спрашивает канал, есть ли версия новее.
///
/// `Ok(None)` — обновлений нет; это нормальный ответ, а не ошибка, и путать их
/// нельзя: «всё актуально» и «канал недоступен» человек должен различать.
#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<Option<AvailableUpdate>, String> {
    channel(&app)?;

    let update = app
        .updater()
        .map_err(err)?
        .check()
        .await
        .map_err(|e| format!("не удалось проверить обновления: {e}"))?;

    Ok(update.map(|update| AvailableUpdate {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
        notes: update.body.clone(),
        published_at: update.date.map(|date| date.to_string()),
    }))
}

/// Скачивает и ставит обновление, затем перезапускает приложение.
///
/// Перезапуск здесь же, а не «когда-нибудь потом»: наполовину обновлённое
/// приложение — это старый процесс с новыми файлами на диске, и жить в таком
/// состоянии не должно ничего.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    channel(&app)?;

    let update = app
        .updater()
        .map_err(err)?
        .check()
        .await
        .map_err(|e| format!("не удалось проверить обновления: {e}"))?
        .ok_or("обновлений нет")?;

    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|e| format!("не удалось установить обновление: {e}"))?;

    // На Windows установщик просит закрыть приложение сам, на macOS замена
    // происходит на месте; перезапуск нужен обеим системам.
    app.restart();
}

#[cfg(test)]
mod tests {
    /// Конфигурация сборки должна упоминать канал — пусть и незаполненный.
    ///
    /// Тест сторожит сам факт: секция `updater`, тихо исчезнувшая из
    /// конфигурации, превращает обновления в кнопку, которая всегда отвечает
    /// «не настроено», и заметить это можно только руками.
    #[test]
    fn the_build_configuration_declares_an_update_channel() {
        let config = include_str!("../tauri.conf.json");
        assert!(config.contains("\"updater\""), "в конфигурации нет секции updater");
    }

    /// Артефакты обновления и ключ подписи включаются вместе.
    ///
    /// Ровно эта рассинхронизация стоила испорченной сборки: артефакты
    /// создавались, ключа не было, и `tauri build` доводил установщик до конца,
    /// а потом падал с «no private key». Обратная ошибка не лучше: ключ есть,
    /// артефактов нет — публиковать нечего, а канал выглядит настроенным.
    #[test]
    fn update_artifacts_and_the_signing_key_are_switched_on_together() {
        let config = include_str!("../tauri.conf.json");

        let has_key = !config.contains("\"pubkey\": \"\"");
        let makes_artifacts = config.contains("\"createUpdaterArtifacts\": true");

        assert_eq!(
            has_key, makes_artifacts,
            "ключ подписи и создание артефактов обновления разошлись"
        );
    }

    /// Приватного ключа в репозитории быть не может.
    ///
    /// Он даёт право выпустить обновление от имени автора, то есть запустить
    /// код на чужих машинах. Проверка дешёвая, а цена ошибки — весь канал.
    #[test]
    fn no_private_signing_key_is_committed() {
        let config = include_str!("../tauri.conf.json");
        assert!(
            !config.contains("untrusted comment"),
            "в конфигурации оказался приватный ключ подписи"
        );
    }
}
