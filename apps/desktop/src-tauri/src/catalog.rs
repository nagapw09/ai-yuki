//! Каталог готовых интеграций (ТЗ §17, раздел «Available Integrations»).
//!
//! ТЗ требует раздел с доступными интеграциями, но не задаёт их список.
//! Каталог живёт в коде, а не в базе: он поставляется вместе с приложением
//! и обновляется вместе с ним, а не отдельной миграцией на каждую новую строчку.
//!
//! # Что каталог обещает, а что нет
//!
//! Запись — это **заготовка**, а не гарантия. Команда запуска может устареть,
//! пакет — переехать, сервер — потребовать другой ключ. Поэтому установка
//! всегда заканчивается настоящим подключением: Yuki здоровается с сервером и
//! забирает у него список инструментов. Если этого не вышло, возможность
//! помечается сломанной, а не «установленной» — инвариант ТЗ §5 действует и здесь.

use serde::Serialize;

/// Готовая интеграция.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Integration {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// `stdio` или `http`.
    pub transport: &'static str,
    pub command: &'static str,
    pub args: &'static [&'static str],
    /// Имя переменной окружения с ключом, если он нужен.
    pub secret_env: Option<&'static str>,
    /// Где пользователю взять ключ.
    pub secret_hint: Option<&'static str>,
    /// Категории разрешений Yuki, которые затрагивает интеграция (ТЗ §21).
    pub permissions: &'static [&'static str],
    /// Поддерживается сообществом, а не авторами протокола.
    pub community: bool,
}

/// Заготовки интеграций.
///
/// Аргумент `-y` у `npx` обязателен: без него команда останавливается на вопросе
/// «установить пакет?», которого в stdio-канале никто не увидит, и подключение
/// молча зависает до таймаута.
pub const INTEGRATIONS: &[Integration] = &[
    Integration {
        id: "filesystem",
        label: "Файловая система",
        description:
            "Чтение и запись файлов в разрешённых каталогах. Полезно, когда нужен \
             доступ к папке за пределами того, что умеют встроенные инструменты.",
        transport: "stdio",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-filesystem"],
        secret_env: None,
        secret_hint: None,
        permissions: &["files"],
        community: false,
    },
    Integration {
        id: "github",
        label: "GitHub",
        description: "Репозитории, issue, pull request и поиск по коду.",
        transport: "stdio",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-github"],
        secret_env: Some("GITHUB_PERSONAL_ACCESS_TOKEN"),
        secret_hint: Some("github.com → Settings → Developer settings → Personal access tokens"),
        permissions: &["network", "external_services"],
        community: false,
    },
    Integration {
        id: "memory",
        label: "Граф знаний",
        description:
            "Долговременная память в виде графа сущностей и связей. Дополняет \
             собственную память Yuki там, где важны связи между фактами.",
        transport: "stdio",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-memory"],
        secret_env: None,
        secret_hint: None,
        permissions: &[],
        community: false,
    },
    Integration {
        id: "postgres",
        label: "PostgreSQL",
        description: "Запросы к базе данных только на чтение.",
        transport: "stdio",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-postgres"],
        secret_env: Some("DATABASE_URL"),
        secret_hint: Some("Строка подключения вида postgres://пользователь:пароль@хост/база"),
        permissions: &["network", "external_services"],
        community: false,
    },
    Integration {
        id: "slack",
        label: "Slack",
        description: "Чтение каналов и отправка сообщений.",
        transport: "stdio",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-slack"],
        secret_env: Some("SLACK_BOT_TOKEN"),
        secret_hint: Some("api.slack.com → ваше приложение → OAuth & Permissions"),
        permissions: &["network", "external_services"],
        community: false,
    },
    Integration {
        id: "spotify",
        label: "Spotify",
        description:
            "Управление воспроизведением, поиск треков и плейлисты. \
             Сервер поддерживается сообществом — при установке Yuki проверит, \
             что он действительно поднимается.",
        transport: "stdio",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-spotify"],
        secret_env: Some("SPOTIFY_ACCESS_TOKEN"),
        secret_hint: Some("developer.spotify.com → Dashboard → ваше приложение"),
        permissions: &["network", "external_services"],
        community: true,
    },
];

/// Ищет интеграцию по идентификатору.
pub fn find(id: &str) -> Option<&'static Integration> {
    INTEGRATIONS.iter().find(|i| i.id == id)
}

/// Подбирает интеграции по свободному запросу пользователя.
///
/// Поиск по подстроке в идентификаторе, названии и описании: пользователь просит
/// «управлять Spotify», а не «установи сервер с идентификатором spotify», и
/// точное совпадение здесь почти никогда не сработает.
pub fn search(query: &str) -> Vec<&'static Integration> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return INTEGRATIONS.iter().collect();
    }

    INTEGRATIONS
        .iter()
        .filter(|i| {
            i.id.contains(&needle)
                || i.label.to_lowercase().contains(&needle)
                || i.description.to_lowercase().contains(&needle)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_an_integration_by_a_human_phrase() {
        let found = search("spotify");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "spotify");
    }

    #[test]
    fn search_is_case_insensitive_and_looks_into_descriptions() {
        assert!(!search("GitHub").is_empty());
        assert!(!search("плейлист").is_empty());
    }

    #[test]
    fn empty_query_returns_the_whole_catalogue() {
        assert_eq!(search("  ").len(), INTEGRATIONS.len());
    }

    #[test]
    fn unknown_request_returns_nothing_rather_than_a_guess() {
        // ТЗ §42: отсутствие возможности — это честный ответ, а не ближайшее похожее.
        assert!(search("телепатия").is_empty());
    }

    #[test]
    fn every_npx_entry_passes_the_flag_that_prevents_an_install_prompt() {
        for integration in INTEGRATIONS {
            if integration.command == "npx" {
                assert!(
                    integration.args.contains(&"-y"),
                    "{} зависнет на вопросе об установке пакета",
                    integration.id
                );
            }
        }
    }

    #[test]
    fn integrations_that_need_a_key_explain_where_to_get_it() {
        for integration in INTEGRATIONS {
            if integration.secret_env.is_some() {
                assert!(
                    integration.secret_hint.is_some(),
                    "{} требует ключ, но не говорит откуда его взять",
                    integration.id
                );
            }
        }
    }
}
