use crate::*;

static CONFIG_UPDATES_DISABLED: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_config_updates_disabled(disabled: bool) {
    CONFIG_UPDATES_DISABLED.store(disabled, Ordering::Release);
}

pub(crate) fn config_updates_disabled() -> bool {
    CONFIG_UPDATES_DISABLED.load(Ordering::Acquire)
}

#[cfg(feature = "rtt")]
pub(crate) fn list_jlinks(explicit_lib: Option<PathBuf>, sn: Option<u32>) -> Result<()> {
    let candidates = default_library_candidates(explicit_lib.or_else(env_jlink_lib));
    let (mut jlink, loaded_from) = map_jlink(JLink::from_candidates(&candidates))?;
    println!("Loaded J-Link library: {}", loaded_from.display());
    map_jlink(jlink.open(OpenOptions::default()))?;
    let mut list = map_jlink(jlink.list_connected_emulators(JLinkHost::Usb))?;
    if let Some(sn) = sn {
        list.retain(|emu| emu.serial_number == sn);
    }
    if list.is_empty() {
        println!("No J-Links found");
    } else {
        for emu in list {
            println!(
                "SN={} Product={} FW={}",
                emu.serial_number,
                emu.product_string(),
                emu.fw_string()
            );
        }
    }
    jlink.close();
    Ok(())
}

#[cfg(feature = "rtt")]
pub(crate) fn list_jlink_devices(explicit_lib: Option<PathBuf>, filter: &str) -> Result<()> {
    let candidates = default_library_candidates(explicit_lib.or_else(env_jlink_lib));
    let (jlink, loaded_from) = map_jlink(JLink::from_candidates(&candidates))?;
    println!("Loaded J-Link library: {}", loaded_from.display());
    let devices = if filter.trim().is_empty() {
        map_jlink(jlink.list_devices())?
    } else {
        map_jlink(jlink.find_devices(filter))?
    };
    if devices.is_empty() {
        println!("No devices found");
    } else {
        for device in devices {
            println!(
                "{}\t{}\tflash=0x{:08x}/{}\tram=0x{:08x}/{}",
                device.name,
                device.manufacturer.as_deref().unwrap_or("-"),
                device.flash_addr,
                device.flash_size,
                device.ram_addr,
                device.ram_size
            );
        }
    }
    Ok(())
}

#[cfg(feature = "rtt")]
pub(crate) fn pick_jlink_device(explicit_lib: Option<PathBuf>) -> Result<String> {
    let candidates = default_library_candidates(explicit_lib.or_else(env_jlink_lib));
    let (jlink, loaded_from) = map_jlink(JLink::from_candidates(&candidates))?;
    eprintln!("[rttio] loaded J-Link library: {}", loaded_from.display());
    let device = map_jlink(jlink.select_device_dialog())?;
    eprintln!("[rttio] selected device: {}", device.name);
    Ok(device.name)
}

#[cfg(feature = "rtt")]
pub(crate) fn pick_default_jlink_sn(explicit_lib: Option<PathBuf>) -> Result<Option<u32>> {
    let candidates = default_library_candidates(explicit_lib.or_else(env_jlink_lib));
    let (mut jlink, _) = map_jlink(JLink::from_candidates(&candidates))?;
    map_jlink(jlink.open(OpenOptions::default()))?;
    let emulators = map_jlink(jlink.list_connected_emulators(JLinkHost::Usb))?;
    jlink.close();
    Ok(emulators
        .into_iter()
        .find(|emulator| emulator.serial_number != 0)
        .map(|emulator| emulator.serial_number))
}

pub(crate) fn load_config() -> Result<RttioConfig> {
    load_config_from_path(Path::new(CONFIG_FILE))
}

pub(crate) fn load_config_from_path(path: &Path) -> Result<RttioConfig> {
    match fs::read_to_string(path) {
        Ok(data) => load_config_data(path, &data),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(RttioConfig::default()),
        Err(e) => Err(e).with_context(|| format!("failed to read {}", path.display())),
    }
}

pub(crate) fn save_config(config: &RttioConfig) -> Result<()> {
    if config_updates_disabled() {
        return Ok(());
    }
    save_config_to_path(Path::new(CONFIG_FILE), config)
}

pub(crate) async fn save_config_blocking(config: RttioConfig) -> Result<()> {
    tokio::task::spawn_blocking(move || save_config(&config))
        .await
        .context("config save task failed")?
}

pub(crate) fn save_config_to_path(path: &Path, config: &RttioConfig) -> Result<()> {
    let mut config = RttioConfig {
        version: CONFIG_VERSION,
        ..config.clone()
    };
    config.normalize();
    let data = serde_json::to_string_pretty(&config).context("failed to serialize config")?;
    let tmp = path.with_file_name(format!(
        "{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(CONFIG_FILE),
        std::process::id()
    ));
    {
        let mut file = FsOpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)
            .with_context(|| format!("failed to open temporary config {}", tmp.display()))?;
        file.write_all(data.as_bytes())
            .with_context(|| format!("failed to write temporary config {}", tmp.display()))?;
        file.write_all(b"\n")
            .with_context(|| format!("failed to finish temporary config {}", tmp.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync temporary config {}", tmp.display()))?;
    }
    fs::rename(&tmp, path).with_context(|| {
        format!(
            "failed to replace {} with {}",
            path.display(),
            tmp.display()
        )
    })?;
    Ok(())
}

pub(crate) fn load_config_data(path: &Path, data: &str) -> Result<RttioConfig> {
    match serde_json::from_str::<RttioConfig>(data) {
        Ok(mut config) => {
            config.normalize();
            Ok(config)
        }
        Err(e) => {
            backup_invalid_config(path, data)?;
            eprintln!(
                "[rttio] ignored invalid config {}; backup saved: {e}",
                path.display()
            );
            Ok(RttioConfig::default())
        }
    }
}

pub(crate) fn backup_invalid_config(path: &Path, data: &str) -> Result<PathBuf> {
    let backup = next_invalid_config_backup_path(path);
    fs::write(&backup, data)
        .with_context(|| format!("failed to back up invalid config to {}", backup.display()))?;
    Ok(backup)
}

pub(crate) fn next_invalid_config_backup_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(CONFIG_FILE);
    let mut index = 0usize;
    loop {
        let name = if index == 0 {
            format!("{file_name}.invalid")
        } else {
            format!("{file_name}.invalid.{index}")
        };
        let candidate = path.with_file_name(name);
        if !candidate.exists() {
            return candidate;
        }
        index += 1;
    }
}

pub(crate) fn load_config_or_default_for_ui(
    terminal_tx: &mpsc::Sender<TerminalEvent>,
) -> RttioConfig {
    match load_config() {
        Ok(config) => config,
        Err(e) => {
            terminal_status_blocking(terminal_tx, &format!("failed to load {CONFIG_FILE}: {e}"));
            RttioConfig::default()
        }
    }
}

#[cfg(any(feature = "rtt", feature = "espflash"))]
pub(crate) fn remember_flash_file(path: &Path, addr: Option<u32>) -> Result<()> {
    if config_updates_disabled() {
        return Ok(());
    }
    let mut config = load_config()?;
    config.recent_flash.retain(|existing| existing != path);
    config.recent_flash.insert(0, path.to_path_buf());
    config.recent_flash.truncate(10);
    if let Some(addr) = addr {
        config.recent_flash_addr.retain(|entry| entry.path != path);
        config.recent_flash_addr.insert(
            0,
            RecentFlashAddress {
                path: path.to_path_buf(),
                addr,
            },
        );
    }
    save_config(&config)
}

#[cfg(any(feature = "rtt", feature = "espflash"))]
pub(crate) async fn remember_flash_file_blocking(path: PathBuf, addr: Option<u32>) -> Result<()> {
    tokio::task::spawn_blocking(move || remember_flash_file(&path, addr))
        .await
        .context("flash history save task failed")?
}

pub(crate) fn recent_flash_addr(config: &RttioConfig, path: &Path) -> Option<u32> {
    config
        .recent_flash_addr
        .iter()
        .find(|entry| entry.path == path)
        .map(|entry| entry.addr)
}

pub(crate) fn is_serial_connected_status(text: &str) -> bool {
    text == "connected" || text.starts_with("TCP serial connected ")
}

pub(crate) fn is_rtt_connected_status(text: &str) -> bool {
    text.starts_with("connected up=") || text.starts_with("RTT stream connected ")
}

pub(crate) fn is_disconnected_status(text: &str) -> bool {
    text == "disconnected"
}
