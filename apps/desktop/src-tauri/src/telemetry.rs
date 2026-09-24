//! On-demand sampled system counters. No polling while the dashboard is closed.
use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use sysinfo::{Disks, Networks, System};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    cpu: Option<f64>,
    gpu: Option<f64>,
    /// Имя самого занятого адаптера: без него одно число на две видеокарты не
    /// сходится с диспетчером задач, который показывает их раздельно.
    gpu_name: Option<String>,
    /// Каждый адаптер отдельно — как «GPU 0» и «GPU 1» в диспетчере задач.
    gpus: Vec<GpuLoad>,
    memory_used: u64,
    memory_total: u64,
    sample_seconds: f64,
    disks: Vec<Disk>,
    /// Байт в секунду за последний замер, все физические адаптеры вместе.
    net_down: f64,
    net_up: f64,
    /// Нет у настольного компьютера — тогда `None`, и панель её не рисует.
    battery: Option<Battery>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    percent: u8,
    charging: bool,
    /// Сколько осталось по оценке Windows; при зарядке не известно.
    seconds_left: Option<u32>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuLoad {
    name: String,
    usage: f64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Disk {
    name: String,
    total: u64,
    free: u64,
    read_bytes: u64,
    written_bytes: u64,
}
struct Sampler {
    system: System,
    disks: Disks,
    networks: Networks,
    last: Instant,
    cached: Option<Metrics>,
    #[cfg(windows)]
    counters: Counters,
}
impl Sampler {
    fn new() -> Self {
        Self {
            system: System::new(),
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            last: Instant::now(),
            cached: None,
            #[cfg(windows)]
            counters: Counters::new(),
        }
    }
    fn read(&mut self) -> Metrics {
        // Диспетчер задач обновляется раз в секунду, и панель рядом с ним не
        // должна показывать позавчерашнее число. Меньше 700 мс смысла нет:
        // счётчик считает разницу между замерами, и на коротком интервале она
        // превращается в дрожь.
        if self.last.elapsed() < Duration::from_millis(700) {
            if let Some(v) = &self.cached {
                return v.clone();
            }
        }
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.disks.refresh(true);
        self.networks.refresh(true);
        let seconds = self.last.elapsed().as_secs_f64().max(0.1);
        // Виртуальные адаптеры Hyper-V/WSL пересылают тот же трафик ещё раз —
        // с ними скорость удваивалась бы.
        let (down, up) = self
            .networks
            .iter()
            .filter(|(name, _)| {
                let name = name.to_ascii_lowercase();
                !name.contains("loopback") && !name.starts_with("vethernet") && !name.contains("wsl")
            })
            .fold((0u64, 0u64), |(d, u), (_, n)| (d + n.received(), u + n.transmitted()));

        // Пока счётчик Windows не прогрелся (первый замер), показываем расчёт
        // sysinfo, чтобы панель не висела с прочерком.
        let fallback_cpu = self
            .cached
            .as_ref()
            .map(|_| self.system.global_cpu_usage() as f64);

        #[cfg(windows)]
        let (cpu, gpus) = {
            let sample = self.counters.read();
            (sample.cpu.or(fallback_cpu), sample.gpus)
        };
        #[cfg(not(windows))]
        let (cpu, gpus): (Option<f64>, Vec<GpuLoad>) = (fallback_cpu, Vec::new());

        let busiest = gpus
            .iter()
            .max_by(|a, b| a.usage.total_cmp(&b.usage))
            .cloned();

        let value = Metrics {
            sample_seconds: seconds,
            net_down: down as f64 / seconds,
            net_up: up as f64 / seconds,
            battery: battery(),
            cpu,
            gpu: busiest.as_ref().map(|g| g.usage),
            gpu_name: busiest.map(|g| g.name),
            gpus,
            memory_used: self.system.used_memory(),
            memory_total: self.system.total_memory(),
            disks: self
                .disks
                .iter()
                .map(|d| Disk {
                    name: d.mount_point().to_string_lossy().into(),
                    total: d.total_space(),
                    free: d.available_space(),
                    read_bytes: d.usage().read_bytes,
                    written_bytes: d.usage().written_bytes,
                })
                .collect(),
        };
        self.last = Instant::now();
        self.cached = Some(value.clone());
        value
    }
}
#[cfg(windows)]
fn battery() -> Option<Battery> {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut status = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut status) }.ok()?;
    // 128 — батареи нет, 255 — неизвестно; процент 255 — тоже неизвестно.
    if status.BatteryFlag & 128 != 0 || status.BatteryFlag == 255 || status.BatteryLifePercent > 100 {
        return None;
    }
    Some(Battery {
        percent: status.BatteryLifePercent,
        charging: status.ACLineStatus == 1,
        seconds_left: (status.BatteryLifeTime != u32::MAX && status.ACLineStatus != 1)
            .then_some(status.BatteryLifeTime),
    })
}

#[cfg(not(windows))]
fn battery() -> Option<Battery> {
    None
}

#[tauri::command]
pub async fn system_metrics() -> Result<Metrics, String> {
    tauri::async_runtime::spawn_blocking(|| {
        static SAMPLER: OnceLock<Mutex<Sampler>> = OnceLock::new();
        SAMPLER
            .get_or_init(|| Mutex::new(Sampler::new()))
            .lock()
            .map_err(|e| e.to_string())
            .map(|mut s| s.read())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Разбирает имя экземпляра счётчика `\GPU Engine(*)`.
///
/// Экземпляр выглядит как `pid_1234_luid_0x00000000_0x0000FC59_phys_0_eng_0_engtype_3D`.
/// Нужны две части: какой это адаптер и какой его движок. Процессы, делящие
/// один движок, складываются; адаптеры не складываются никогда — у ноутбука их
/// два, и их сумма не значит ничего.
fn split_engine_instance(instance: &str) -> Option<(String, String)> {
    let at = instance.find("luid_")?;
    let mut parts = instance[at..].splitn(4, '_');
    let luid = format!(
        "{}_{}_{}",
        parts.next()?,
        parts.next()?,
        parts.next()?.to_ascii_lowercase()
    );
    let engine = parts.next().unwrap_or_default().to_ascii_lowercase();
    Some((luid, engine))
}

#[cfg(windows)]
struct Sample {
    cpu: Option<f64>,
    gpus: Vec<GpuLoad>,
}

#[cfg(windows)]
struct Counters {
    query: isize,
    /// `% Processor Utility` — ровно то, что показывает диспетчер задач.
    ///
    /// Не `% Processor Time`: та считает долю незанятого простоем времени и не
    /// учитывает частоту. На этой машине (i5-12500H, базовые 2500 МГц, турбо до
    /// 3760) отношение устойчиво около 1,6 — как и отношение частот:
    ///
    /// ```text
    /// Processor Time    : 12 / 14 / 13 / 13
    /// Processor Utility : 22 / 21 / 19 / 26
    /// ```
    ///
    /// Диспетчер задач в тот же момент показывал 26 % при наших 16 % на `Time`.
    ///
    /// Сверять только по нескольким выборкам подряд: загрузка скачет на
    /// десятки процентов между соседними секундами, и одиночное сравнение
    /// двух окон ничего не доказывает — один раз мы на этом уже ошиблись.
    cpu: isize,
    gpu: isize,
    primed: bool,
    /// LUID адаптера → название видеокарты.
    adapters: std::collections::HashMap<String, String>,
}
#[cfg(windows)]
impl Counters {
    fn new() -> Self {
        use windows::{core::w, Win32::System::Performance::*};
        let mut result = Self {
            query: 0,
            cpu: 0,
            gpu: 0,
            primed: false,
            adapters: adapter_names(),
        };
        unsafe {
            if PdhOpenQueryW(None, 0, &mut result.query) == 0 {
                PdhAddEnglishCounterW(
                    result.query,
                    w!("\\Processor Information(_Total)\\% Processor Utility"),
                    0,
                    &mut result.cpu,
                );
                PdhAddEnglishCounterW(
                    result.query,
                    w!("\\GPU Engine(*)\\Utilization Percentage"),
                    0,
                    &mut result.gpu,
                );
            }
        }
        result
    }

    fn read(&mut self) -> Sample {
        use windows::Win32::System::Performance::*;
        let empty = Sample {
            cpu: None,
            gpus: Vec::new(),
        };
        if self.query == 0 {
            return empty;
        }
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return empty;
            }
            // Счётчик PDH — разность двух замеров: у первого разности нет.
            if !self.primed {
                self.primed = true;
                return empty;
            }
            Sample {
                cpu: self.cpu_value(),
                gpus: self.gpu_values(),
            }
        }
    }

    unsafe fn cpu_value(&self) -> Option<f64> {
        use windows::Win32::System::Performance::*;
        if self.cpu == 0 {
            return None;
        }
        let mut value = PDH_FMT_COUNTERVALUE::default();
        if PdhGetFormattedCounterValue(self.cpu, PDH_FMT_DOUBLE, None, &mut value) != 0 {
            return None;
        }
        let percent = value.Anonymous.doubleValue;
        // Utility уходит выше 100 на турбо-частотах; диспетчер задач тоже обрезает.
        percent.is_finite().then(|| percent.clamp(0.0, 100.0))
    }

    unsafe fn gpu_values(&self) -> Vec<GpuLoad> {
        use windows::Win32::System::Performance::*;
        if self.gpu == 0 {
            return Vec::new();
        }
        let (mut size, mut count) = (0, 0);
        PdhGetFormattedCounterArrayW(self.gpu, PDH_FMT_DOUBLE, &mut size, &mut count, None);
        if size == 0 || size > 16_000_000 {
            return Vec::new();
        }
        // u64 allocation guarantees alignment for PDH structures and trailing strings.
        let mut buffer = vec![0u64; (size as usize + 7) / 8];
        let items = buffer.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
        if PdhGetFormattedCounterArrayW(
            self.gpu,
            PDH_FMT_DOUBLE,
            &mut size,
            &mut count,
            Some(items),
        ) != 0
        {
            return Vec::new();
        }

        let mut engines = std::collections::HashMap::<(String, String), f64>::new();
        for item in std::slice::from_raw_parts(items, count as usize) {
            if item.FmtValue.CStatus > 1 {
                continue;
            }
            let Some(key) = split_engine_instance(&item.szName.to_string().unwrap_or_default())
            else {
                continue;
            };
            let value = item.FmtValue.Anonymous.doubleValue;
            if value.is_finite() {
                *engines.entry(key).or_default() += value.max(0.0);
            }
        }

        // Внутри адаптера берём самый занятый движок — так же считает диспетчер задач.
        let mut adapters = std::collections::HashMap::<String, f64>::new();
        for ((luid, _), value) in engines {
            let slot = adapters.entry(luid).or_default();
            *slot = slot.max(value);
        }

        let mut result: Vec<GpuLoad> = adapters
            .into_iter()
            .filter_map(|(luid, usage)| {
                // Нет среди настоящих адаптеров — значит программный слой вроде
                // `Microsoft Basic Render Driver`. В списке видеокарт он только
                // занимал бы строку вечным нулём.
                let name = self.adapters.get(&luid)?.clone();
                Some(GpuLoad {
                    name,
                    usage: usage.min(100.0),
                })
            })
            .collect();
        result.sort_by(|a, b| {
            b.usage
                .total_cmp(&a.usage)
                .then_with(|| a.name.cmp(&b.name))
        });
        result
    }
}

/// Названия видеокарт по их LUID.
///
/// На ноутбуке с встроенной и дискретной картой «GPU 60%» без имени не значит
/// ничего: диспетчер задач показывает их отдельно, и человек сравнивает наше
/// одно число с чужим другим.
#[cfg(windows)]
fn adapter_names() -> std::collections::HashMap<String, String> {
    use windows::Win32::Graphics::Dxgi::*;
    let mut names = std::collections::HashMap::new();
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
            return names;
        };
        for index in 0..16 {
            let Ok(adapter) = factory.EnumAdapters1(index) else {
                break;
            };
            let Ok(desc) = adapter.GetDesc1() else {
                continue;
            };
            // Программный адаптер — это отрисовка на процессоре, а не железо.
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let name = String::from_utf16_lossy(&desc.Description);
            let name = name.trim_end_matches('\0').trim().to_string();
            let luid = format!(
                "luid_0x{:08x}_0x{:08x}",
                desc.AdapterLuid.HighPart as u32, desc.AdapterLuid.LowPart
            );
            if !name.is_empty() {
                names.insert(luid, name);
            }
        }
    }
    names
}
#[cfg(windows)]
impl Drop for Counters {
    fn drop(&mut self) {
        unsafe {
            if self.query != 0 {
                windows::Win32::System::Performance::PdhCloseQuery(self.query);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::split_engine_instance;

    #[test]
    fn engine_instance_splits_into_adapter_and_engine() {
        let (luid, engine) =
            split_engine_instance("pid_31276_luid_0x00000000_0x0000FC59_phys_0_eng_0_engtype_3D")
                .expect("имя экземпляра должно разобраться");
        assert_eq!(luid, "luid_0x00000000_0x0000fc59");
        assert_eq!(engine, "phys_0_eng_0_engtype_3d");
    }

    #[test]
    fn two_processes_on_one_engine_land_on_one_adapter() {
        let first =
            split_engine_instance("pid_1_luid_0x00000000_0x0000FC59_phys_0_eng_0_engtype_3D");
        let second =
            split_engine_instance("pid_2_luid_0x00000000_0x0000fc59_phys_0_eng_0_engtype_3d");
        assert_eq!(first, second);
    }

    #[test]
    fn different_adapters_do_not_collide() {
        let integrated =
            split_engine_instance("pid_1_luid_0x00000000_0x0000FC59_phys_0_eng_0_engtype_3D")
                .expect("разбор")
                .0;
        let discrete =
            split_engine_instance("pid_1_luid_0x00000001_0x1B7E7E8B_phys_0_eng_0_engtype_3D")
                .expect("разбор")
                .0;
        assert_ne!(integrated, discrete);
    }

    #[test]
    fn an_instance_without_a_luid_is_skipped() {
        assert_eq!(split_engine_instance("total"), None);
    }
}
