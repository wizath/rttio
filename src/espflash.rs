use crate::*;

#[cfg(feature = "espflash")]
use ::espflash::{
    connection::{Connection, ResetAfterOperation, ResetBeforeOperation},
    flasher::Flasher,
    target::{Chip, ProgressCallbacks},
};
#[cfg(feature = "espflash")]
use serialport::{FlowControl as EspFlowControl, SerialPortType, UsbPortInfo};
#[cfg(feature = "espflash")]
use std::str::FromStr;

#[cfg(feature = "espflash")]
#[derive(Clone, Debug)]
pub(crate) struct EspFlashConfig {
    pub(crate) chip: Option<Chip>,
    pub(crate) baud: u32,
}

#[cfg(feature = "espflash")]
impl Default for EspFlashConfig {
    fn default() -> Self {
        Self {
            chip: None,
            baud: 921_600,
        }
    }
}

#[cfg(feature = "espflash")]
pub(crate) fn parse_esp_chip(input: &str) -> std::result::Result<Chip, String> {
    Chip::from_str(&input.to_ascii_lowercase())
        .map_err(|_| format!("unsupported ESP chip {input:?}"))
}

#[cfg(feature = "espflash")]
pub(crate) async fn esp_erase(path: PathBuf, config: EspFlashConfig) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        let mut flasher = connect_esp(
            &path,
            &config,
            true,
            true,
            true,
            ResetAfterOperation::HardReset,
        )?;
        flasher.erase_flash().context("failed to erase ESP flash")?;
        flasher
            .connection()
            .reset()
            .context("failed to reset ESP target after erase")
    })
    .await
    .context("ESP erase task failed")?
}

#[cfg(feature = "espflash")]
pub(crate) async fn esp_flash_bin(
    port_path: PathBuf,
    file_path: PathBuf,
    addr: u32,
    config: EspFlashConfig,
    progress_tx: Option<mpsc::UnboundedSender<TerminalFlashProgress>>,
) -> Result<usize> {
    tokio::task::spawn_blocking(move || {
        ensure_esp_raw_bin(&file_path)?;
        let data = fs::read(&file_path)
            .with_context(|| format!("failed to read {}", file_path.display()))?;
        let mut flasher = connect_esp(
            &port_path,
            &config,
            true,
            false,
            false,
            ResetAfterOperation::NoReset,
        )?;
        let mut progress = EspFlashProgressCallback::new(progress_tx);
        flasher
            .write_bin_to_flash(addr, &data, &mut progress)
            .context("failed to flash ESP binary")?;
        Ok(data.len())
    })
    .await
    .context("ESP flash task failed")?
}

#[cfg(feature = "espflash")]
struct EspFlashProgressCallback {
    tx: Option<mpsc::UnboundedSender<TerminalFlashProgress>>,
    action: String,
    total: usize,
    last_percent: i32,
}

#[cfg(feature = "espflash")]
impl EspFlashProgressCallback {
    fn new(tx: Option<mpsc::UnboundedSender<TerminalFlashProgress>>) -> Self {
        Self {
            tx,
            action: "flash".to_string(),
            total: 0,
            last_percent: -1,
        }
    }

    fn send(&mut self, percent: i32) {
        if percent == self.last_percent {
            return;
        }
        self.last_percent = percent;
        if let Some(tx) = &self.tx {
            let _ = tx.send(TerminalFlashProgress {
                action: self.action.clone(),
                percent,
            });
        }
    }
}

#[cfg(feature = "espflash")]
impl ProgressCallbacks for EspFlashProgressCallback {
    fn init(&mut self, addr: u32, total: usize) {
        self.total = total.max(1);
        self.action = format!("flash 0x{addr:08x}");
        self.send(0);
    }

    fn update(&mut self, current: usize) {
        let percent = current.saturating_mul(100) / self.total;
        self.send(percent.min(100) as i32);
    }

    fn verifying(&mut self) {
        self.action = "verify".to_string();
        self.last_percent = -1;
        self.send(0);
    }

    fn finish(&mut self, _skipped: bool) {
        self.send(100);
    }
}

#[cfg(feature = "espflash")]
fn connect_esp(
    path: &Path,
    config: &EspFlashConfig,
    use_stub: bool,
    verify: bool,
    skip: bool,
    after: ResetAfterOperation,
) -> Result<Flasher> {
    let path_text = path.display().to_string();
    let serial = serialport::new(&path_text, 115_200)
        .flow_control(EspFlowControl::None)
        .open_native()
        .with_context(|| format!("failed to open serial port {}", path.display()))?;
    let port_info = usb_port_info(&path_text).unwrap_or_else(default_usb_port_info);
    let connection = Connection::new(
        serial,
        port_info,
        after,
        ResetBeforeOperation::DefaultReset,
        config.baud,
    );
    Flasher::connect(
        connection,
        use_stub,
        verify,
        skip,
        config.chip,
        Some(config.baud),
    )
    .context("failed to connect ESP bootloader")
}

#[cfg(feature = "espflash")]
fn ensure_esp_raw_bin(path: &Path) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase);
    if ext.as_deref() == Some("bin") {
        Ok(())
    } else {
        Err(anyhow!(
            "ESP serial flash currently supports raw .bin files only; got {}",
            path.display()
        ))
    }
}

#[cfg(feature = "espflash")]
fn usb_port_info(port: &str) -> Option<UsbPortInfo> {
    serialport::available_ports()
        .ok()?
        .into_iter()
        .find(|info| info.port_name == port)
        .and_then(|info| match info.port_type {
            SerialPortType::UsbPort(usb) => Some(usb),
            _ => None,
        })
}

#[cfg(feature = "espflash")]
fn default_usb_port_info() -> UsbPortInfo {
    UsbPortInfo {
        vid: 0,
        pid: 0,
        serial_number: None,
        manufacturer: None,
        product: None,
    }
}
