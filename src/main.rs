#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
compile_error!("This application targets x86_64 Windows only.");

mod backend;
mod protocol;
use backend::{Adapter, Report};
use eframe::egui;
use std::{
    fs,
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

enum Event {
    Devices(protocol::Result<Vec<Adapter>>),
    Operation(Report),
}
struct App {
    adapters: Vec<Adapter>,
    selected: Option<usize>,
    receiver: Option<Receiver<Event>>,
    status: String,
    log: String,
    confirm: Option<Adapter>,
    uncertain: bool,
    demo: bool,
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>, demo: bool) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        for file in ["msyh.ttc", "simhei.ttf", "simsun.ttc"] {
            if let Ok(bytes) = fs::read(backend::windows_dir().join("Fonts").join(file)) {
                fonts
                    .font_data
                    .insert("Chinese".into(), egui::FontData::from_owned(bytes).into());
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Proportional)
                    .unwrap()
                    .insert(0, "Chinese".into());
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Monospace)
                    .unwrap()
                    .push("Chinese".into());
                break;
            }
        }
        cc.egui_ctx.set_fonts(fonts);
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        let mut app = Self {
            adapters: vec![],
            selected: None,
            receiver: None,
            status: "请选择适配器，将自动查询状态。".into(),
            log: String::new(),
            confirm: None,
            uncertain: false,
            demo,
        };
        app.record(&format!(
            "Steam Frame 6 GHz 设置工具 {} / Windows x64 / demo={demo}",
            env!("CARGO_PKG_VERSION")
        ));
        app.record("不扫描、不自动连接、不修改文件或注册表。查看状态会消耗共享诊断缓冲，请关闭其他原厂诊断程序。日志仅保存在内存，需点击保存日志导出。");
        app.refresh();
        app
    }
    fn record(&mut self, text: &str) {
        self.log
            .push_str(&format!("[{}] {text}\n", backend::timestamp()));
    }
    fn refresh(&mut self) {
        self.selected = None;
        self.adapters.clear();
        self.confirm = None;
        self.status = "正在读取适配器列表…".into();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        let demo = self.demo;
        thread::spawn(move || {
            let _ = tx.send(Event::Devices(if demo {
                Ok(vec![demo_adapter()])
            } else {
                backend::enumerate()
            }));
        });
    }
    fn start(&mut self, adapter: Adapter, set_us: bool) {
        if self.receiver.is_some() {
            return;
        }
        self.confirm = None;
        self.record(&format!(
            "{}：{} [{}]",
            if set_us {
                "用户确认设置 US，一次提交并自动复查"
            } else {
                "选中适配器，自动查询状态"
            },
            adapter.name,
            adapter.id
        ));
        self.status = if set_us {
            "正在设置并复查…"
        } else {
            "正在读取国家及 6 GHz 状态…"
        }
        .into();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        let demo = self.demo;
        thread::spawn(move || {
            let result = if demo {
                Report { status: Ok(protocol::Status { country: if set_us {"US"} else {"CN"}.into(), info: if set_us {"[6G Info]\n6G Support (domain:05), due to REGU_RSN_MANUAL"} else {"[6G Info]\n6G NOT Support\n[Hint] domain (00), country (CN), wwsku rsn REGU_RSN_11D"}.into() }), logs: vec!["演示数据：未调用任何设备 API。".into()], uncertain: false }
            } else {
                backend::operate(&adapter, set_us)
            };
            let _ = tx.send(Event::Operation(result));
        });
    }
    fn select(&mut self, selected: Option<usize>) {
        if self.receiver.is_some() || self.selected == selected {
            return;
        }
        self.selected = selected;
        self.confirm = None;
        self.status = "请选择适配器。".into();
        if let Some(adapter) = selected.and_then(|i| self.adapters.get(i)).cloned() {
            if adapter.supported() {
                self.start(adapter, false);
            } else {
                self.status = "驱动不受支持，无法查询，详情见日志。".into();
            }
        }
    }
    fn save(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("实验日志", &["txt"])
            .set_file_name("steam-frame-6ghz-tool-log.txt")
            .save_file()
        {
            let content = format!(
                "\u{feff}Steam Frame 6 GHz 设置工具 实验日志 (UTC timestamps)\n{}",
                self.log
            );
            match fs::write(&path, content) {
                Ok(()) => self.record(&format!(
                    "日志已保存到 {}（包含接口 GUID 和设备实例 ID，无 Wi-Fi 密码）",
                    path.display()
                )),
                Err(e) => {
                    self.record(&format!("保存失败：{e}"));
                    rfd::MessageDialog::new()
                        .set_title("保存失败")
                        .set_description(e.to_string())
                        .show();
                }
            }
        }
    }
    fn poll(&mut self) {
        let received = self.receiver.as_ref().map(|r| r.try_recv());
        match received {
            Some(Ok(event)) => {
                self.receiver = None;
                match event {
                    Event::Devices(result) => match result {
                        Ok(items) => {
                            self.status = if items.is_empty() {
                                "未发现 Steam Frame 适配器，请插入设备后刷新。"
                            } else {
                                "请选择适配器。"
                            }
                            .into();
                            for a in &items {
                                self.record(&format!(
                                    "发现 {} [{}] / {} / {:?}",
                                    a.name, a.id, a.pnp, a.compatibility
                                ));
                            }
                            self.adapters = items;
                            if self.adapters.len() == 1 {
                                self.select(Some(0));
                            }
                        }
                        Err(e) => {
                            self.record(&format!("枚举失败：{e}"));
                            self.status = "无法读取适配器，请查看日志。".into();
                        }
                    },
                    Event::Operation(report) => {
                        for line in report.logs {
                            self.record(&line);
                        }
                        self.uncertain = report.uncertain;
                        self.status = match report.status {
                            Ok(s) => {
                                self.record(&format!("原始状态：国家={}\n{}", s.country, s.info));
                                status_summary(&s)
                            }
                            Err(e) => {
                                self.record(&e);
                                "操作未成功，请查看日志。".into()
                            }
                        };
                        self.record(&self.status.clone());
                    }
                }
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.receiver = None;
                self.uncertain = true;
                self.status = "工作线程意外终止；设备状态未知，不自动重试。".into();
                self.record(&self.status.clone());
            }
            _ => {}
        }
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        let busy = self.receiver.is_some();
        if busy {
            ctx.request_repaint_after(Duration::from_millis(100));
            if ctx.input(|i| i.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.record("操作尚未结束，请等待后再关闭窗口。");
            }
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(20.0))
            .show(ctx, |ui| {
                ui.heading("Steam Frame 6 GHz 设置工具");
                if self.demo {
                    ui.weak("演示模式");
                }
                ui.add_space(18.0);
                ui.horizontal(|ui| {
                    ui.label("适配器");
                    if ui
                        .add_enabled(!busy && self.confirm.is_none(), egui::Button::new("刷新"))
                        .clicked()
                    {
                        self.refresh();
                    }
                    if busy {
                        ui.spinner();
                    }
                });
                let mut choice = self.selected;
                ui.add_enabled_ui(self.receiver.is_none() && self.confirm.is_none(), |ui| {
                    egui::ComboBox::from_id_salt("adapter")
                        .width(ui.available_width())
                        .selected_text(
                            self.selected
                                .and_then(|i| self.adapters.get(i))
                                .map(|a| adapter_label(a, &self.adapters))
                                .unwrap_or_else(|| "选择适配器".into()),
                        )
                        .show_ui(ui, |ui| {
                            for (i, a) in self.adapters.iter().enumerate() {
                                ui.selectable_value(
                                    &mut choice,
                                    Some(i),
                                    adapter_label(a, &self.adapters),
                                );
                            }
                        });
                });
                self.select(choice);
                let selected = self.selected.and_then(|i| self.adapters.get(i)).cloned();
                if selected
                    .as_ref()
                    .is_some_and(|a| a.supported() && a.unverified_driver)
                {
                    ui.colored_label(
                        egui::Color32::from_rgb(160, 95, 0),
                        "此驱动版本尚未验证，仍可操作。结果请以复查为准。",
                    );
                }
                if selected.as_ref().is_some_and(|a| a.compatibility.is_err()) {
                    ui.colored_label(
                        egui::Color32::DARK_RED,
                        "此适配器或驱动不受支持，详情见日志。",
                    );
                }
                let enabled = selected.as_ref().is_some_and(|a| a.supported())
                    && self.receiver.is_none()
                    && self.confirm.is_none();
                ui.add_space(16.0);
                egui::Frame::group(ui.style())
                    .inner_margin(14.0)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(egui::RichText::new(&self.status).size(17.0));
                    });
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(enabled && !self.uncertain, egui::Button::new("设置 US…"))
                        .clicked()
                    {
                        self.confirm = selected.clone();
                    }
                    if ui
                        .add_enabled(
                            self.receiver.is_none() && self.confirm.is_none(),
                            egui::Button::new("保存日志…"),
                        )
                        .clicked()
                    {
                        self.save();
                    }
                });
                if self.uncertain {
                    ui.colored_label(
                        egui::Color32::DARK_RED,
                        "结果不确定，请稍后刷新并重新选择适配器以查询。设置暂不可用。",
                    );
                }
                ui.add_space(12.0);
                ui.collapsing("详细日志", |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(ui.available_height().max(40.0))
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.log)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY)
                                    .interactive(false),
                            );
                        });
                });
            });
        if self.receiver.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if let Some(adapter) = self.confirm.clone() {
            egui::Modal::new(egui::Id::new("confirm-us")).show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.heading("设置为 US？");
                ui.label(adapter_label(&adapter,&self.adapters));
                if adapter.unverified_driver {
                    ui.label("此驱动版本尚未验证，可能不兼容。");
                }
                ui.add_space(8.0);
                ui.label("会改变适配器运行时策略，可能启用 6 GHz 或短暂影响连接。仅执行一次并复查，不自动回滚；重插后可能失效。");
                ui.horizontal(|ui| {
                    if ui.button("取消").clicked() { self.confirm = None; self.record("用户取消设置；没有提交命令。"); }
                    if ui.button("确认，仅执行一次").clicked() { self.start(adapter.clone(),true); }
                });
            });
        }
    }
}

fn adapter_label(adapter: &Adapter, all: &[Adapter]) -> String {
    if all.iter().filter(|a| a.name == adapter.name).count() > 1 {
        format!(
            "{} · {}",
            adapter.name,
            &adapter.id[adapter.id.len().saturating_sub(8)..]
        )
    } else {
        adapter.name.clone()
    }
}

fn status_summary(status: &protocol::Status) -> String {
    let six = if status
        .info
        .lines()
        .any(|l| l.trim().starts_with("6G NOT Support"))
    {
        "不可用"
    } else if status
        .info
        .lines()
        .any(|l| l.trim().starts_with("6G Support (domain:"))
    {
        "可用"
    } else {
        "未知（见日志）"
    };
    format!("国家：{}     6 GHz：{}", status.country, six)
}

fn demo_adapter() -> Adapter {
    Adapter {
        guid: windows_sys::core::GUID {
            data1: 0,
            data2: 0,
            data3: 0,
            data4: [0; 8],
        },
        id: "DEMO-ONLY-NOT-A-REAL-INTERFACE".into(),
        name: "Valve USB Adapter（模拟）".into(),
        pnp: "DEMO — 不访问硬件".into(),
        service: "模拟".into(),
        compatibility: Ok("演示设备；非真实兼容性检查".into()),
        unverified_driver: false,
    }
}
fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--list"] {
        match backend::enumerate() {
            Ok(items) => {
                for a in items {
                    println!("{} | {} | {} | {:?}", a.id, a.name, a.pnp, a.compatibility);
                }
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    if !args.is_empty() && args != ["--demo"] {
        rfd::MessageDialog::new()
            .set_title("参数错误")
            .set_description("仅支持无参数启动、--demo 演示、--list 只读枚举。没有命令行设置入口。")
            .show();
        return Ok(());
    }
    let demo = args == ["--demo"];
    eframe::run_native(
        "Steam Frame 6 GHz 设置工具",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                // Empty icon suppresses eframe's logo; leave the Windows default.
                .with_icon(egui::IconData::default())
                .with_inner_size([620.0, 410.0])
                .with_min_inner_size([500.0, 370.0]),
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(App::new(cc, demo)))),
    )
}

#[cfg(test)]
mod ui_tests {
    use super::*;
    #[test]
    fn selection_queries_once_without_setting_us() {
        for count in 0..=2 {
            let (tx, rx) = mpsc::channel();
            let mut app = App {
                adapters: vec![],
                selected: None,
                receiver: Some(rx),
                status: String::new(),
                log: String::new(),
                confirm: None,
                uncertain: false,
                demo: true,
            };
            let items = (0..count)
                .map(|i| {
                    let mut a = demo_adapter();
                    a.id = format!("DEMO-{i}");
                    a.unverified_driver = true; // A warning must not block automatic status queries.
                    a
                })
                .collect();
            assert!(tx.send(Event::Devices(Ok(items))).is_ok());
            app.poll();
            assert_eq!(app.selected, (count == 1).then_some(0));
            assert_eq!(app.receiver.is_some(), count == 1);
            if count == 0 {
                continue;
            }
            if count == 2 {
                app.select(Some(1));
            }
            let chosen = app.selected;
            app.select(Some(0)); // In-flight reads prevent a selection race.
            assert_eq!(app.selected, chosen);
            let event = app
                .receiver
                .take()
                .unwrap()
                .recv_timeout(Duration::from_secs(2))
                .unwrap();
            let Event::Operation(report) = event else {
                panic!("expected status query")
            };
            assert_eq!(report.status.unwrap().country, "CN"); // Demo SET would produce US.
            app.select(chosen); // Repaint / unchanged selection must not query again.
            assert!(app.receiver.is_none());
            if count == 2 {
                app.select(Some(0));
                assert!(
                    app.receiver
                        .take()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(2))
                        .is_ok()
                );
                app.adapters[1].compatibility = Err("unsupported driver".into());
                app.select(Some(1));
                assert_eq!(app.selected, Some(1));
                assert!(app.receiver.is_none());
                assert!(app.status.contains("不受支持"));
            }
        }
    }
    #[test]
    fn short_status_keeps_unknown_distinct() {
        for (info, expected) in [
            ("6G NOT Support", "不可用"),
            ("6G Support (domain:05)", "可用"),
            ("Platform not support 6G", "未知（见日志）"),
        ] {
            assert_eq!(
                status_summary(&protocol::Status {
                    country: "US".into(),
                    info: info.into()
                }),
                format!("国家：US     6 GHz：{expected}")
            );
        }
    }
}
