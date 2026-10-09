//! The system tray icon (StatusNotifierItem): driver status plus a menu to open settings.

use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ksni::blocking::TrayMethods;
use ksni::menu::{MenuItem, StandardItem};

use crate::client::{Client, Status};

struct Tray {
    status: Status,
    /// Our icon, drawn in the panel's text colour.
    icons: Vec<ksni::Icon>,
}

impl Tray {
    fn summary(&self) -> String {
        match &self.status.running {
            None => "Driver not running".into(),
            Some(s) => match s.connected.iter().filter(|c| **c).count() {
                0 => "No controllers connected".into(),
                1 => "1 controller connected".into(),
                n => format!("{n} controllers connected"),
            },
        }
    }
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "x360chatpad".into()
    }

    fn title(&self) -> String {
        "X360 Chatpad".into()
    }

    fn icon_name(&self) -> String {
        // Empty: hosts then use `icon_pixmap`, which always works.
        String::new()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icons.clone()
    }

    fn status(&self) -> ksni::Status {
        // Passive tucks the icon into the overflow area on desktops that support it.
        match &self.status.running {
            Some(s) if s.connected.iter().any(|c| *c) => ksni::Status::Active,
            _ => ksni::Status::Passive,
        }
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "X360 Chatpad".into(),
            description: self.summary(),
            ..Default::default()
        }
    }

    fn activate(&mut self, x: i32, y: i32) {
        open_settings(Some((x, y)));
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items: Vec<MenuItem<Self>> = vec![label(self.summary())];
        if let Some(s) = &self.status.running {
            for (i, connected) in s.connected.iter().enumerate() {
                if *connected {
                    items.push(label(format!("    Player {}", i + 1)));
                }
            }
            items.push(label(format!("Keyboard layout: {}", s.layout)));
        }
        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: "Settings…".into(),
                icon_name: "configure".into(),
                activate: Box::new(|_| open_settings(None)),
                ..Default::default()
            }
            .into(),
        );
        if self.status.running.is_some() {
            items.push(
                StandardItem {
                    label: "Reload settings and keyboard layout".into(),
                    icon_name: "view-refresh".into(),
                    activate: Box::new(|_| {
                        if let Err(e) = Client::new().reload() {
                            eprintln!("reload: {e}");
                        }
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }
        items.push(
            StandardItem {
                label: if self.status.running.is_some() {
                    "Restart driver"
                } else {
                    "Start driver"
                }
                .into(),
                icon_name: "system-reboot".into(),
                activate: Box::new(|_| restart_driver()),
                ..Default::default()
            }
            .into(),
        );
        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: "Hide tray icon".into(),
                icon_name: "window-close".into(),
                activate: Box::new(|_| std::process::exit(0)),
                ..Default::default()
            }
            .into(),
        );
        items
    }
}

fn label(text: String) -> MenuItem<Tray> {
    StandardItem {
        label: text,
        enabled: false,
        ..Default::default()
    }
    .into()
}

/// The open settings popup, if any, and when the last one closed.
static POPUP: Mutex<(Option<u32>, Option<Instant>)> = Mutex::new((None, None));

/// Toggle the settings popup. Clicking the tray icon while the popup is open closes it.
/// (Clicking the icon also takes focus from the popup, which then closes itself; a click
/// right after that is the same click, so it mustn't reopen the popup.)
pub fn open_settings(at: Option<(i32, i32)>) {
    let mut popup = POPUP.lock().unwrap();
    if let Some(pid) = popup.0 {
        let _ = Command::new("kill").arg(pid.to_string()).status();
        return;
    }
    if popup
        .1
        .is_some_and(|t| t.elapsed() < Duration::from_millis(400))
    {
        return;
    }
    let exe = std::env::current_exe().unwrap_or_else(|_| "x360chatpad-tray".into());
    let mut command = Command::new(exe);
    command.arg("--settings");
    if let Some((x, y)) = at {
        command.arg(format!("{x},{y}"));
    }
    match command.spawn() {
        Ok(mut child) => {
            popup.0 = Some(child.id());
            std::thread::spawn(move || {
                // Reap it, and note when it closed.
                let _ = child.wait();
                *POPUP.lock().unwrap() = (None, Some(Instant::now()));
            });
        }
        Err(e) => eprintln!("opening settings: {e}"),
    }
}

pub fn restart_driver() {
    let _ = Command::new("systemctl")
        .args(["--user", "restart", "x360chatpad.service"])
        .status();
}

/// Run the tray until killed. Returns an error if the desktop has no tray (e.g. GNOME without
/// the AppIndicator extension).
pub fn run() -> anyhow::Result<()> {
    let text = crate::theme::Palette::detect().text;
    let icons = crate::icon::tray_pixmaps([text.r(), text.g(), text.b()]);
    let handle = Tray {
        status: Status::default(),
        icons,
    }
    .spawn()
    .map_err(|e| anyhow::anyhow!("no system tray available ({e})"))?;
    crate::client::watch_status(Duration::from_secs(2), move |status| {
        let status = status.clone();
        handle.update(move |tray: &mut Tray| tray.status = status);
    });
    loop {
        std::thread::park();
    }
}
