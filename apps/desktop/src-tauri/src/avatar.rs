//! Аватар: отдельное прозрачное окно поверх всех (ТЗ §12).
//!
//! # Почему отдельное окно
//!
//! ТЗ требует always-on-top, прозрачность и click-through. Всё это — свойства
//! окна, а не элемента внутри страницы: сделать «поверх всех» частью главного
//! окна нельзя, а прозрачность главного окна сломала бы весь остальной
//! интерфейс. Поэтому аватар живёт своим окном без рамки, а состояние получает
//! событиями из главного.
//!
//! # Модель приносит пользователь
//!
//! Готового VRM в поставке нет и не будет: у моделей свои лицензии, и класть
//! чужую в дистрибутив нельзя. Yuki принимает путь к `.vrm`, читает файл сама и
//! отдаёт байты в окно — так модель не проходит через файловый протокол
//! WebView и не требует ослаблять CSP.

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::state::AppState;

/// Метка окна аватара. По ней его находят и команды, и события.
pub const WINDOW_LABEL: &str = "avatar";

/// Ключи настроек.
const KEY_ENABLED: &str = "avatar.enabled";
const KEY_MODEL: &str = "avatar.model";
const KEY_CLICK_THROUGH: &str = "avatar.click_through";
const KEY_ON_TOP: &str = "avatar.always_on_top";
const KEY_PLACEMENT: &str = "avatar.placement";
const KEY_POSE: &str = "avatar.pose";
const KEY_ANIMATIONS: &str = "avatar.animations";
const KEY_ANCHOR: &str = "avatar.anchor";

/// Событие смены кадра: окно аватара пересчитывает камеру, получив его.
pub const POSE_EVENT: &str = "yuki://avatar-pose";

/// Что показывать в окне.
///
/// `portrait` — голова и торс: так аватар читается как собеседник, и лицо видно
/// даже в маленьком окне. `full` — во весь рост: так он становится существом,
/// которое стоит на краю экрана, и для этого его и держат на рабочем столе.
const POSE_PORTRAIT: &str = "portrait";
const POSE_FULL: &str = "full";

/// Где держать окно.
///
/// `taskbar` прижимает его к нижней границе рабочей области — той самой, выше
/// которой начинается панель задач. Аватар получается стоящим на панели, а не
/// висящим в случайном месте, и не закрывает её собой.
const ANCHOR_FREE: &str = "free";
const ANCHOR_TASKBAR: &str = "taskbar";

/// Размеры окна по умолчанию для каждого кадра.
///
/// У полного роста окно узкое и высокое: человек в полный рост занимает по
/// вертикали вчетверо больше, чем по горизонтали, и квадратное окно вокруг него
/// было бы прозрачным на три четверти — то есть перехватывало бы клики там, где
/// ничего не нарисовано.
const SIZE_PORTRAIT: (u32, u32) = (320, 480);
/// Полный рост: окно шире, чем кажется нужным.
///
/// Фигура со опущенными руками узкая, но анимации разводят руки в стороны, и
/// кадр приходится строить по самой размашистой позе. В окне 200×420 такая поза
/// влезает только целиком уменьшившись — фигура становится вдвое мельче окна и
/// половину времени висит в пустоте. При отношении сторон около двух третей
/// запас по ширине и по высоте выходит одинаковым, и фигура заполняет кадр.
const SIZE_FULL: (u32, u32) = (280, 440);

/// Расширение единственного поддерживаемого формата модели.
const MODEL_EXTENSION: &str = "vrm";

/// Расширение файлов анимации.
///
/// `.vrma` — формат анимаций VRM: те же кости гуманоида, что у модели, поэтому
/// один и тот же танец подходит любой модели. Unity-клипы `.anim` и FBX сюда не
/// годятся: первые — формат чужого движка, вторые несут свой скелет, который
/// надо переносить на гуманоида отдельной работой.
const ANIMATION_EXTENSION: &str = "vrma";

/// Событие «проиграй анимацию»: окно аватара получает имя клипа.
pub const PLAY_EVENT: &str = "yuki://avatar-play";

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarStatus {
    pub enabled: bool,
    /// Путь к модели; пустая строка — модель не выбрана.
    pub model: String,
    /// Существует ли файл модели прямо сейчас.
    pub model_present: bool,
    pub click_through: bool,
    pub always_on_top: bool,
    /// Открыто ли окно в данный момент.
    pub open: bool,
    /// `portrait` или `full`.
    pub pose: String,
    /// `free` или `taskbar`.
    pub anchor: String,
    /// Папка с файлами анимаций; пустая строка — папка не выбрана.
    pub animations: String,
}

/// Найденный файл анимации.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationClip {
    /// Имя без расширения — под ним анимацию просят проиграть.
    pub name: String,
}

fn setting(state: &AppState, key: &str) -> Option<String> {
    state
        .storage
        .with_conn(|conn| {
            conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get::<_, String>(0)
            })
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
        })
        .ok()
        .flatten()
}

fn set_setting(state: &AppState, key: &str, value: &str) -> Result<(), String> {
    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = unixepoch()",
                rusqlite::params![key, value],
            )
        })
        .map(|_| ())
        .map_err(err)
}

fn flag(state: &AppState, key: &str, default: bool) -> bool {
    match setting(state, key).as_deref() {
        Some("on") => true,
        Some("off") => false,
        _ => default,
    }
}

// ── Команды ─────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn avatar_status(app: tauri::AppHandle, state: State<'_, AppState>) -> AvatarStatus {
    let model = setting(&state, KEY_MODEL).unwrap_or_default();

    AvatarStatus {
        enabled: flag(&state, KEY_ENABLED, false),
        // Файл могли переместить между запусками; «модель выбрана» и «модель
        // есть» — разные утверждения, и путать их значит показать пустое окно
        // без объяснения.
        model_present: !model.is_empty() && std::path::Path::new(&model).is_file(),
        model,
        click_through: flag(&state, KEY_CLICK_THROUGH, false),
        always_on_top: flag(&state, KEY_ON_TOP, true),
        open: app.get_webview_window(WINDOW_LABEL).is_some(),
        pose: pose(&state),
        anchor: anchor(&state),
        animations: setting(&state, KEY_ANIMATIONS).unwrap_or_default(),
    }
}

/// Папка с анимациями.
///
/// Своих клипов в поставке нет по той же причине, по какой нет модели: у
/// анимаций свои лицензии, и класть чужие в дистрибутив нельзя. Человек даёт
/// папку, Yuki читает из неё файлы сама.
#[tauri::command]
pub fn avatar_set_animations(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<AvatarStatus, String> {
    let trimmed = path.trim();

    if !trimmed.is_empty() && !std::path::Path::new(trimmed).is_dir() {
        return Err(format!("{trimmed} — не папка"));
    }

    set_setting(&state, KEY_ANIMATIONS, trimmed)?;
    Ok(avatar_status(app, state))
}

/// Перечисляет анимации в выбранной папке.
///
/// Пустой список — нормальный ответ, а не ошибка: папку могли выбрать заранее,
/// а файлы положить потом. Вложенные папки не обходятся: аватару нужен плоский
/// набор клипов, а рекурсия по чужой папке — это чтение того, о чём не просили.
#[tauri::command]
pub fn avatar_animations(state: State<'_, AppState>) -> Result<Vec<AnimationClip>, String> {
    let folder = setting(&state, KEY_ANIMATIONS).unwrap_or_default();
    if folder.trim().is_empty() {
        return Ok(Vec::new());
    }

    let entries = std::fs::read_dir(&folder).map_err(|e| format!("{folder}: {e}"))?;
    let mut clips: Vec<AnimationClip> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case(ANIMATION_EXTENSION))
        })
        .filter_map(|path| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .map(|name| AnimationClip { name: name.to_string() })
        })
        .collect();

    // Порядок файловой системы произволен, а список показывается человеку.
    clips.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(clips)
}

/// Отдаёт файл анимации окну аватара.
///
/// Имя, а не путь: путь из окна означал бы, что страница может попросить любой
/// файл на диске. Имя склеивается с выбранной папкой здесь, и всё, что вышло за
/// её пределы, отвергается.
#[tauri::command]
pub fn avatar_animation_bytes(
    state: State<'_, AppState>,
    name: String,
) -> Result<tauri::ipc::Response, String> {
    let folder = setting(&state, KEY_ANIMATIONS).unwrap_or_default();
    if folder.trim().is_empty() {
        return Err("папка с анимациями не выбрана".into());
    }

    let path = animation_path(std::path::Path::new(folder.trim()), &name)?;
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Собирает путь к файлу анимации и проверяет, что он не вышел из папки.
///
/// Отдельной функцией, потому что это граница доверия: имя приходит из окна, а
/// `..` в нём означало бы чтение любого файла на диске. Проверка идёт по
/// составу имени, а не по получившемуся пути: канонизация требует, чтобы файл
/// уже существовал, и на несуществующем имени молча пропускала бы проверку.
fn animation_path(folder: &std::path::Path, name: &str) -> Result<std::path::PathBuf, String> {
    let trimmed = name.trim();

    if trimmed.is_empty() {
        return Err("не указано имя анимации".into());
    }

    let looks_like_a_path = trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains("..")
        || std::path::Path::new(trimmed).components().count() != 1;

    if looks_like_a_path {
        return Err(format!("«{trimmed}» не имя анимации"));
    }

    Ok(folder.join(format!("{trimmed}.{ANIMATION_EXTENSION}")))
}

/// Названные места экрана, куда можно попросить аватар встать.
///
/// Названия, а не координаты: «встань справа» — это то, что человек говорит, а
/// «встань в 1712, 972» — то, что он не скажет никогда. Координаты остаются в
/// перетаскивании мышью.
const SPOTS: &[&str] = &[
    "left",
    "right",
    "center",
    "top-left",
    "top-right",
    "bottom-left",
    "bottom-right",
];

/// Куда встанет окно в названном месте рабочей области.
///
/// Отдельной функцией, потому что вся содержательная часть — арифметика, и её
/// можно проверить тестом, не спрашивая систему про мониторы.
///
/// Рабочая область, а не весь экран: её границы — это то, что не закрыто
/// панелью задач. Аватар, поставленный «внизу», должен стоять на панели, а не
/// прятаться за ней.
fn spot_position(
    area_origin: (i32, i32),
    area_size: (u32, u32),
    window_size: (u32, u32),
    spot: &str,
) -> Option<(i32, i32)> {
    let clamp = |value: u32| i32::try_from(value).unwrap_or(i32::MAX);

    let (origin_x, origin_y) = area_origin;
    let free_x = (clamp(area_size.0) - clamp(window_size.0)).max(0);
    let free_y = (clamp(area_size.1) - clamp(window_size.1)).max(0);

    let left = origin_x;
    let right = origin_x + free_x;
    let middle = origin_x + free_x / 2;
    let top = origin_y;
    let bottom = origin_y + free_y;

    match spot {
        // Без уточнения по вертикали — низ: аватар стоит на панели задач, а не
        // висит в середине экрана.
        "left" => Some((left, bottom)),
        "right" => Some((right, bottom)),
        "center" => Some((middle, bottom)),
        "top-left" => Some((left, top)),
        "top-right" => Some((right, top)),
        "bottom-left" => Some((left, bottom)),
        "bottom-right" => Some((right, bottom)),
        _ => None,
    }
}

/// Ставит аватар в названное место экрана.
#[tauri::command]
pub fn avatar_move(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    spot: String,
) -> Result<(), String> {
    let window = app
        .get_webview_window(WINDOW_LABEL)
        .ok_or("окно аватара закрыто")?;

    let monitor = window
        .current_monitor()
        .map_err(err)?
        .ok_or("не удалось спросить монитор")?;

    let area = monitor.work_area();
    let size = window.outer_size().map_err(err)?;

    let (x, y) = spot_position(
        (area.position.x, area.position.y),
        (area.size.width, area.size.height),
        (size.width, size.height),
        spot.trim(),
    )
    .ok_or_else(|| format!("не знаю места «{spot}»; есть: {}", SPOTS.join(", ")))?;

    window
        .set_position(tauri::PhysicalPosition { x, y })
        .map_err(err)?;

    // Место запоминается сразу: попросив встать справа, человек ждёт, что там
    // она и окажется после перезапуска.
    let _ = remember_placement(&window, &state);
    Ok(())
}

/// Просит окно аватара проиграть анимацию.
///
/// Пустое имя означает «вернись к покою»: у анимации есть конец, а у покоя нет,
/// и отдельная команда «останови» заставляла бы вызывающего помнить, что
/// играло.
#[tauri::command]
pub fn avatar_play(app: tauri::AppHandle, name: String) -> Result<(), String> {
    let window = app
        .get_webview_window(WINDOW_LABEL)
        .ok_or("окно аватара закрыто")?;

    window.emit(PLAY_EVENT, name).map_err(err)
}

/// Текущий кадр, с приведением незнакомого значения к умолчанию.
///
/// Настройку можно поправить руками в базе, и «portret» с опечаткой не должен
/// оставлять окно без камеры вообще.
fn pose(state: &AppState) -> String {
    match setting(state, KEY_POSE).as_deref() {
        Some(POSE_FULL) => POSE_FULL.into(),
        _ => POSE_PORTRAIT.into(),
    }
}

fn anchor(state: &AppState) -> String {
    match setting(state, KEY_ANCHOR).as_deref() {
        Some(ANCHOR_TASKBAR) => ANCHOR_TASKBAR.into(),
        _ => ANCHOR_FREE.into(),
    }
}

/// Прижимает окно к нижней границе рабочей области монитора, на котором оно стоит.
///
/// Рабочая область, а не весь экран: её нижняя граница — это верх панели задач,
/// и аватар встаёт на панель, а не поверх неё. Если монитор спросить не удалось
/// (окно только что создано, монитор отключили), окно остаётся там, где было:
/// поставить его наугад хуже, чем не двигать.
fn snap_to_taskbar(window: &tauri::WebviewWindow) -> Result<(), String> {
    let Some(monitor) = window.current_monitor().map_err(err)? else {
        return Ok(());
    };

    let area = monitor.work_area();
    let size = window.outer_size().map_err(err)?;
    let position = window.outer_position().map_err(err)?;

    let (x, y) = anchored_position(
        (area.position.x, area.position.y),
        (area.size.width, area.size.height),
        (size.width, size.height),
        position.x,
    );

    window
        .set_position(tauri::PhysicalPosition { x, y })
        .map_err(err)
}

/// Куда встанет окно, прижатое к нижней границе рабочей области.
///
/// Отдельной функцией, потому что вся содержательная часть здесь — арифметика,
/// а её можно проверить тестом; спрашивать у системы монитор ради этого не надо.
///
/// По горизонтали окно остаётся там, куда его поставил человек, но не уезжает
/// за край: аватар, наполовину вышедший за экран, выглядит поломкой, а не
/// задумкой. Окно шире экрана прижимается к левому краю — показать его целиком
/// всё равно нельзя, а уехавший влево левый край хуже уехавшего вправо правого,
/// потому что слева у фигуры лицо.
fn anchored_position(
    area_origin: (i32, i32),
    area_size: (u32, u32),
    window_size: (u32, u32),
    x: i32,
) -> (i32, i32) {
    let clamp = |value: u32| i32::try_from(value).unwrap_or(i32::MAX);

    let (origin_x, origin_y) = area_origin;
    let free_x = (clamp(area_size.0) - clamp(window_size.0)).max(0);
    let free_y = (clamp(area_size.1) - clamp(window_size.1)).max(0);

    (
        x.clamp(origin_x, origin_x + free_x),
        origin_y + free_y,
    )
}

/// Открывает окно аватара, восстанавливая прежние размер и место.
#[tauri::command]
pub async fn avatar_open(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<AvatarStatus, String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window.show().map_err(err)?;
        window.set_focus().map_err(err)?;
        return Ok(avatar_status(app, state));
    }

    let (default_width, default_height) = if pose(&state) == POSE_FULL {
        SIZE_FULL
    } else {
        SIZE_PORTRAIT
    };

    let placement: Placement = setting(&state, KEY_PLACEMENT)
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or(Placement {
            x: 80,
            y: 80,
            width: default_width,
            height: default_height,
        });

    let mut builder = WebviewWindowBuilder::new(
        &app,
        WINDOW_LABEL,
        WebviewUrl::App("index.html".into()),
    )
    .title("Yuki")
    .inner_size(placement.width as f64, placement.height as f64)
    .position(placement.x as f64, placement.y as f64)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .resizable(true)
    // В панели задач ему делать нечего: это компаньон на экране, а не второе
    // приложение.
    .skip_taskbar(true);

    if flag(&state, KEY_ON_TOP, true) {
        builder = builder.always_on_top(true);
    }

    let window = builder.build().map_err(err)?;

    if flag(&state, KEY_CLICK_THROUGH, false) {
        window.set_ignore_cursor_events(true).map_err(err)?;
    }

    // Прижатие после создания, а не вместо запомненного места: монитор мог
    // отключиться или сменить разрешение, и запомненные координаты оказались бы
    // за краем экрана.
    if anchor(&state) == ANCHOR_TASKBAR {
        snap_to_taskbar(&window)?;
    }

    set_setting(&state, KEY_ENABLED, "on")?;
    Ok(avatar_status(app, state))
}

#[tauri::command]
pub fn avatar_close(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        // Место запоминаем до закрытия: после него окна уже нет, и спросить
        // его координаты будет не у кого.
        let _ = remember_placement(&window, &state);
        window.close().map_err(err)?;
    }
    set_setting(&state, KEY_ENABLED, "off")
}

/// Пропускать ли клики сквозь окно (ТЗ §12: click-through).
#[tauri::command]
pub fn avatar_set_click_through(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window.set_ignore_cursor_events(enabled).map_err(err)?;
    }
    set_setting(&state, KEY_CLICK_THROUGH, if enabled { "on" } else { "off" })
}

/// Меняет кадр: по пояс или во весь рост.
///
/// Размер окна меняется вместе с кадром, потому что это одно решение, а не
/// два: в окне 320×480 фигура в полный рост занимает узкую полоску посередине,
/// а остальное — прозрачная область, которая всё равно висит поверх окон.
#[tauri::command]
pub fn avatar_set_pose(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    pose: String,
) -> Result<AvatarStatus, String> {
    let pose = if pose == POSE_FULL { POSE_FULL } else { POSE_PORTRAIT };
    set_setting(&state, KEY_POSE, pose)?;

    let (width, height) = if pose == POSE_FULL { SIZE_FULL } else { SIZE_PORTRAIT };

    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window
            .set_size(tauri::LogicalSize {
                width: f64::from(width),
                height: f64::from(height),
            })
            .map_err(err)?;

        if anchor(&state) == ANCHOR_TASKBAR {
            snap_to_taskbar(&window)?;
        }

        let _ = remember_placement(&window, &state);
        // Камера в окне пересчитывается сама: размеры фигуры в кадре считает
        // сцена, и знать о них Rust не должен.
        let _ = window.emit(POSE_EVENT, pose);
    }

    Ok(avatar_status(app, state))
}

/// Прижимать ли окно к панели задач.
#[tauri::command]
pub fn avatar_set_anchor(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    anchor: String,
) -> Result<AvatarStatus, String> {
    let value = if anchor == ANCHOR_TASKBAR { ANCHOR_TASKBAR } else { ANCHOR_FREE };
    set_setting(&state, KEY_ANCHOR, value)?;

    if value == ANCHOR_TASKBAR {
        if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
            snap_to_taskbar(&window)?;
            let _ = remember_placement(&window, &state);
        }
    }

    Ok(avatar_status(app, state))
}

#[tauri::command]
pub fn avatar_set_always_on_top(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window.set_always_on_top(enabled).map_err(err)?;
    }
    set_setting(&state, KEY_ON_TOP, if enabled { "on" } else { "off" })
}

/// Запоминает путь к модели.
#[tauri::command]
pub fn avatar_set_model(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<AvatarStatus, String> {
    let trimmed = path.trim();

    if !trimmed.is_empty() {
        let file = std::path::Path::new(trimmed);
        if !file.is_file() {
            return Err(format!("файла нет: {trimmed}"));
        }
        // Проверяем расширение здесь, а не при загрузке: сообщение «выберите
        // .vrm» полезнее, чем ошибка разбора glTF в консоли окна аватара.
        let extension = file
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if extension != MODEL_EXTENSION {
            return Err("нужна модель в формате .vrm".into());
        }
    }

    set_setting(&state, KEY_MODEL, trimmed)?;
    Ok(avatar_status(app, state))
}

/// Отдаёт файл модели окну аватара.
///
/// Байтами через IPC, а не ссылкой на файл: так не нужен файловый протокол в
/// WebView и не нужно ослаблять CSP ради одной картинки. Модель читается один
/// раз при открытии окна.
#[tauri::command]
pub fn avatar_model_bytes(state: State<'_, AppState>) -> Result<tauri::ipc::Response, String> {
    let path = setting(&state, KEY_MODEL).unwrap_or_default();
    if path.trim().is_empty() {
        return Err("модель не выбрана".into());
    }

    let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Сохраняет текущее положение окна.
#[tauri::command]
pub fn avatar_remember_placement(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let window = app
        .get_webview_window(WINDOW_LABEL)
        .ok_or("окно аватара закрыто")?;
    remember_placement(&window, &state)
}

fn remember_placement(window: &tauri::WebviewWindow, state: &AppState) -> Result<(), String> {
    let position = window.outer_position().map_err(err)?;
    let size = window.inner_size().map_err(err)?;

    let placement = Placement {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };

    set_setting(
        state,
        KEY_PLACEMENT,
        &serde_json::to_string(&placement).map_err(err)?,
    )
}

/// Открывает окно на старте, если в прошлый раз оно было открыто.
pub fn restore(app: tauri::AppHandle) {
    let state = app.state::<AppState>();
    if !flag(&state, KEY_ENABLED, false) {
        return;
    }

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = handle.state::<AppState>();
        if let Err(error) = avatar_open(handle.clone(), state).await {
            tracing::warn!(%error, "не удалось восстановить окно аватара");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Названные места считаются от рабочей области.
    #[test]
    fn spots_are_measured_from_the_work_area() {
        // Экран 1920×1080 с панелью задач 48: рабочая область 1032.
        let area = ((0, 0), (1920u32, 1032u32));
        let window = (200u32, 420u32);

        assert_eq!(
            spot_position(area.0, area.1, window, "left"),
            Some((0, 612)),
            "слева и на панели задач"
        );
        assert_eq!(spot_position(area.0, area.1, window, "right"), Some((1720, 612)));
        assert_eq!(spot_position(area.0, area.1, window, "center"), Some((860, 612)));
        assert_eq!(spot_position(area.0, area.1, window, "top-left"), Some((0, 0)));
        assert_eq!(spot_position(area.0, area.1, window, "top-right"), Some((1720, 0)));
    }

    /// «Слева» без уточнения — это низ, а не середина.
    ///
    /// Аватар — существо, стоящее на рабочем столе: «встань слева» означает
    /// «слева на полу», а не «слева в воздухе».
    #[test]
    fn a_side_without_a_height_means_the_floor() {
        let same = |a: &str, b: &str| {
            assert_eq!(
                spot_position((0, 0), (1920, 1032), (200, 420), a),
                spot_position((0, 0), (1920, 1032), (200, 420), b),
                "{a} и {b} должны совпадать"
            );
        };

        same("left", "bottom-left");
        same("right", "bottom-right");
    }

    /// Второй монитор со сдвинутым началом координат считается так же.
    #[test]
    fn spots_respect_a_second_monitor() {
        assert_eq!(
            spot_position((1920, 0), (2560, 1392), (200, 420), "left"),
            Some((1920, 972))
        );
        assert_eq!(
            spot_position((1920, 0), (2560, 1392), (200, 420), "right"),
            Some((4280, 972))
        );
    }

    /// Окно больше экрана прижимается к началу, а не уезжает за оба края.
    #[test]
    fn a_window_larger_than_the_screen_stays_put() {
        assert_eq!(
            spot_position((0, 0), (800, 600), (1200, 900), "right"),
            Some((0, 0))
        );
    }

    /// Незнакомое место — отказ, а не движение наугад.
    #[test]
    fn an_unknown_spot_is_refused() {
        for spot in ["", "куда-нибудь", "middle", "LEFT"] {
            assert_eq!(
                spot_position((0, 0), (1920, 1032), (200, 420), spot),
                None,
                "«{spot}» не должно приниматься"
            );
        }
    }

    /// Список мест и разбор не расходятся.
    ///
    /// Список показывается в ошибке и в описании инструмента: место, которое
    /// он обещает, а разбор не понимает, — это обещание, которое не работает.
    #[test]
    fn every_listed_spot_is_understood() {
        for spot in SPOTS {
            assert!(
                spot_position((0, 0), (1920, 1032), (200, 420), spot).is_some(),
                "место «{spot}» обещано, но не понято"
            );
        }
    }

    /// Имя анимации складывается с папкой и получает нужное расширение.
    #[test]
    fn an_animation_name_becomes_a_file_in_the_chosen_folder() {
        let folder = std::path::Path::new("D:/clips");
        let path = animation_path(folder, "dance").expect("имя должно приняться");
        assert_eq!(path, folder.join("dance.vrma"));

        // Пробелы по краям — опечатка человека, а не часть имени.
        let trimmed = animation_path(folder, "  wave  ").expect("имя должно приняться");
        assert_eq!(trimmed, folder.join("wave.vrma"));
    }

    /// Из папки с анимациями выйти нельзя.
    ///
    /// Имя приходит из окна, то есть со страницы. Если бы `..` или разделитель
    /// пути в нём проходили, страница могла бы попросить любой файл на диске —
    /// и получить его байтами через тот же канал, которым забирает анимацию.
    #[test]
    fn an_animation_name_cannot_escape_the_folder() {
        let folder = std::path::Path::new("D:/clips");

        for evil in [
            "../secrets",
            r"..\secrets",
            "sub/dance",
            r"sub\dance",
            "..",
            "C:/Windows/win",
            "/etc/passwd",
            "",
            "   ",
        ] {
            assert!(
                animation_path(folder, evil).is_err(),
                "имя «{evil}» не должно приниматься"
            );
        }
    }

    /// Прижатое окно стоит ногами на верхней границе панели задач.
    #[test]
    fn the_anchored_window_stands_on_the_taskbar() {
        // Экран 1920×1080, панель задач 48 пикселей: рабочая область 1032.
        let (x, y) = anchored_position((0, 0), (1920, 1032), (200, 420), 600);
        assert_eq!(x, 600, "по горизонтали окно не должно двигаться");
        assert_eq!(y, 612, "низ окна должен совпасть с низом рабочей области");
        assert_eq!(y + 420, 1032, "ноги стоят ровно на панели задач");
    }

    /// За край экрана окно не уезжает.
    #[test]
    fn the_anchored_window_stays_on_screen() {
        let (right, _) = anchored_position((0, 0), (1920, 1032), (200, 420), 5000);
        assert_eq!(right, 1720, "правый край окна упирается в правый край экрана");

        let (left, _) = anchored_position((0, 0), (1920, 1032), (200, 420), -300);
        assert_eq!(left, 0, "левый край окна упирается в левый край экрана");
    }

    /// Второй монитор со сдвинутым началом координат считается так же.
    #[test]
    fn the_anchored_window_respects_a_second_monitor() {
        // Монитор справа от основного: начало координат сдвинуто на 1920.
        let (x, y) = anchored_position((1920, 0), (2560, 1392), (200, 420), 1000);
        assert_eq!(x, 1920, "окно с чужого монитора притягивается к этому");
        assert_eq!(y, 972);
    }

    /// Окно шире экрана прижимается к левому краю, а не уезжает за оба.
    #[test]
    fn a_window_wider_than_the_screen_hugs_the_left_edge() {
        let (x, y) = anchored_position((0, 0), (800, 600), (1200, 900), 400);
        assert_eq!(x, 0);
        assert_eq!(y, 0);
    }

    /// Политика окна должна разрешать `blob:` в `connect-src`.
    ///
    /// Текстуры VRM лежат внутри файла модели, и three.js достаёт их через
    /// `fetch` по `blob:`-адресу — а `fetch` подчиняется `connect-src`, а не
    /// `img-src`. Без этого разрешения модель грузится целиком, но приходит
    /// без текстур: белое лицо и плоские цвета вместо глаз.
    ///
    /// Заметить это можно только в настоящей сборке: в режиме разработки
    /// политика не применяется, и там всё выглядит правильно. Поэтому проверка
    /// живёт в тесте, а не в чьей-то памяти.
    #[test]
    fn the_window_policy_lets_the_page_read_its_own_blobs() {
        let config = include_str!("../tauri.conf.json");

        let connect = config
            .split("connect-src")
            .nth(1)
            .and_then(|rest| rest.split(';').next())
            .expect("в политике нет connect-src");

        assert!(
            connect.contains("blob:"),
            "connect-src запрещает blob: — текстуры аватара не загрузятся: {connect}"
        );
    }

    /// А вот сетевой доступ странице по-прежнему закрыт.
    ///
    /// Весь смысл политики в том, что запросы к провайдерам идут через Rust,
    /// где лежат ключи (ТЗ §29). Разрешение `blob:` этого не меняет — но
    /// соседняя правка могла бы, и заметить это стоит здесь.
    #[test]
    fn the_page_still_cannot_reach_the_network() {
        let config = include_str!("../tauri.conf.json");

        let connect = config
            .split("connect-src")
            .nth(1)
            .and_then(|rest| rest.split(';').next())
            .expect("в политике нет connect-src");

        assert!(!connect.contains("https:"), "странице открыли сеть: {connect}");
        assert!(!connect.contains('*'), "странице открыли сеть: {connect}");
    }

    #[test]
    fn placement_survives_a_round_trip_through_settings() {
        let placement = Placement {
            x: -1200,
            y: 40,
            width: 320,
            height: 480,
        };
        let text = serde_json::to_string(&placement).expect("должно сериализоваться");
        let back: Placement = serde_json::from_str(&text).expect("и разобраться обратно");

        // Отрицательная координата — это второй монитор слева, а не ошибка.
        assert_eq!(back.x, -1200);
        assert_eq!(back.width, 320);
    }

    #[test]
    fn an_unreadable_placement_falls_back_instead_of_failing() {
        assert!(serde_json::from_str::<Placement>("не json").is_err());
        // Вызов в avatar_open использует ok(), поэтому испорченная настройка
        // означает «открыть на месте по умолчанию», а не отказ открыть окно.
    }
}
