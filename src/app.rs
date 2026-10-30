use std::sync::mpsc::{Receiver, Sender};

use eframe::egui;
use egui::{Color32, RichText, Stroke};

use crate::{config::Config, java, meta, net, util};

pub const APP_NAME: &str = "DFL";

#[derive(Clone, Copy)]
struct Palette {
    bg: Color32,
    sidebar: Color32,
    card: Color32,
    border: Color32,
    text: Color32,
    sub: Color32,
    accent: Color32,
    pill: Color32,
    hover: Color32,
    on_accent: Color32,
}

impl Palette {
    fn light() -> Self {
        Self {
            bg: Color32::from_rgb(245, 245, 247),
            sidebar: Color32::from_rgb(236, 236, 239),
            card: Color32::from_rgb(255, 255, 255),
            border: Color32::from_rgb(227, 227, 232),
            text: Color32::from_rgb(29, 29, 31),
            sub: Color32::from_rgb(110, 110, 115),
            accent: Color32::from_rgb(10, 132, 255),
            pill: Color32::from_rgb(233, 233, 238),
            hover: Color32::from_rgb(224, 224, 229),
            on_accent: Color32::WHITE,
        }
    }

    fn dark() -> Self {
        Self {
            bg: Color32::from_rgb(28, 28, 30),
            sidebar: Color32::from_rgb(35, 35, 38),
            card: Color32::from_rgb(44, 44, 46),
            border: Color32::from_rgb(58, 58, 60),
            text: Color32::from_rgb(245, 245, 247),
            sub: Color32::from_rgb(152, 152, 157),
            accent: Color32::from_rgb(10, 132, 255),
            pill: Color32::from_rgb(58, 58, 60),
            hover: Color32::from_rgb(70, 70, 72),
            on_accent: Color32::WHITE,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Versions,
    Instances,
    Mods,
    Accounts,
    Settings,
    Logs,
    Sponsor,
    Github,
    About,
}

impl Page {
    fn label(self) -> &'static str {
        match self {
            Page::Home => "首页",
            Page::Versions => "版本管理",
            Page::Instances => "实例管理",
            Page::Mods => "模组与资源",
            Page::Accounts => "账号",
            Page::Settings => "设置",
            Page::Logs => "日志",
            Page::Sponsor => "赞助支持",
            Page::Github => "GitHub",
            Page::About => "关于",
        }
    }

    fn from_env() -> Option<Page> {
        match std::env::var("DFL_PAGE").ok()?.as_str() {
            "versions" => Some(Page::Versions),
            "instances" => Some(Page::Instances),
            "mods" => Some(Page::Mods),
            "accounts" => Some(Page::Accounts),
            "settings" => Some(Page::Settings),
            "logs" => Some(Page::Logs),
            "sponsor" => Some(Page::Sponsor),
            "github" => Some(Page::Github),
            "about" => Some(Page::About),
            _ => None,
        }
    }
}

pub struct LauncherApp {
    config: Config,
    page: Page,
    toast: Option<(String, f64)>,
    logo: Option<egui::TextureHandle>,
    wechat: Option<egui::TextureHandle>,
    alipay: Option<egui::TextureHandle>,
    versions: Vec<net::VersionEntry>,
    versions_loading: bool,
    version_error: Option<String>,
    version_filter: String,
    version_kind: usize,
    selected_version: Option<String>,
    installing: bool,
    install_stage: String,
    install_done: u64,
    install_total: u64,
    logs: Vec<String>,
    tx: Sender<net::Msg>,
    rx: Receiver<net::Msg>,
    running: bool,
    log_cache: String,
    applied_dark: bool,
    java_probe: Option<String>,
}

impl LauncherApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);

        let mut config = Config::load();
        if config.theme.is_empty() {
            config.theme = "light".into();
        }
        util::ensure_dirs();
        if !util::config_path().exists() {
            let _ = config.save();
        }

        let dark = config.theme == "dark";
        apply_theme(&cc.egui_ctx, dark);

        let logo = texture_from_image(
            &cc.egui_ctx,
            "logo",
            include_bytes!("../assets/icon.jpg"),
            96,
        );
        let wechat = texture_from_image(
            &cc.egui_ctx,
            "wechat_qr",
            include_bytes!("../assets/wechat_qr.png"),
            560,
        );
        let alipay = texture_from_image(
            &cc.egui_ctx,
            "alipay_qr",
            include_bytes!("../assets/alipay_qr.png"),
            560,
        );

        let (tx, rx) = std::sync::mpsc::channel();

        Self {
            config,
            page: Page::from_env().unwrap_or(Page::Home),
            toast: None,
            logo,
            wechat,
            alipay,
            versions: Vec::new(),
            versions_loading: false,
            version_error: None,
            version_filter: String::new(),
            version_kind: 0,
            selected_version: None,
            installing: false,
            install_stage: String::new(),
            install_done: 0,
            install_total: 0,
            logs: Vec::new(),
            tx,
            rx,
            running: false,
            log_cache: String::new(),
            applied_dark: dark,
            java_probe: None,
        }
    }

    fn p(&self) -> Palette {
        if self.config.theme == "dark" {
            Palette::dark()
        } else {
            Palette::light()
        }
    }

    fn notify(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), 0.0));
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        let p = self.p();
        egui::SidePanel::left("nav")
            .exact_width(200.0)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(p.sidebar).inner_margin(egui::Margin {
                left: 14,
                right: 14,
                top: 18,
                bottom: 14,
            }))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(logo) = &self.logo {
                        ui.add(
                            egui::Image::new(egui::load::SizedTexture::new(
                                logo.id(),
                                egui::vec2(34.0, 34.0),
                            ))
                            .corner_radius(9),
                        );
                    }
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(APP_NAME).size(18.0).strong().color(p.text));
                        ui.label(RichText::new("轻量启动器").size(10.0).color(p.sub));
                    });
                });
                ui.add_space(18.0);
                ui.spacing_mut().item_spacing.y = 2.0;

                for page in [
                    Page::Home,
                    Page::Versions,
                    Page::Instances,
                    Page::Mods,
                    Page::Accounts,
                    Page::Settings,
                    Page::Logs,
                    Page::Sponsor,
                    Page::Github,
                    Page::About,
                ] {
                    let selected = self.page == page;
                    let text = RichText::new(page.label()).size(13.5).color(if selected {
                        p.accent
                    } else {
                        p.text
                    });
                    let btn = egui::Button::new(text)
                        .fill(if selected { p.card } else { Color32::TRANSPARENT })
                        .stroke(Stroke::NONE)
                        .corner_radius(9)
                        .min_size(egui::vec2(ui.available_width(), 30.0));
                    if ui.add(btn).clicked() {
                        self.page = page;
                    }
                }

            });
    }

    fn status_bar(&self, ctx: &egui::Context) {
        let p = self.p();
        egui::TopBottomPanel::bottom("status")
            .frame(egui::Frame::NONE.fill(p.bg).inner_margin(egui::Margin::symmetric(16, 6)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("数据目录  {}", util::base_dir().display()))
                            .size(10.5)
                            .color(p.sub),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("DFL v{}", env!("CARGO_PKG_VERSION")))
                                .size(10.5)
                                .color(p.sub),
                        );
                        if self.running {
                            ui.add_space(10.0);
                            ui.label(RichText::new("● 游戏运行中").size(10.5).color(p.accent));
                        }
                    });
                });
            });
    }

    fn toast(&mut self, ctx: &egui::Context, dt: f64) {
        let p = self.p();
        let Some((msg, t)) = self.toast.as_mut() else {
            return;
        };
        *t += dt;
        let elapsed = *t;
        if elapsed > 2.6 {
            self.toast = None;
            return;
        }
        let alpha = if elapsed < 2.0 {
            255.0
        } else {
            (1.0 - (elapsed - 2.0) / 0.6) * 255.0
        };
        egui::Area::new(egui::Id::new("toast"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -64.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(
                        p.card.r(),
                        p.card.g(),
                        p.card.b(),
                        alpha as u8,
                    ))
                    .stroke(Stroke::new(1.0, p.border))
                    .inner_margin(egui::Margin::symmetric(16, 10))
                    .corner_radius(12)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(msg.as_str())
                                .size(12.5)
                                .color(Color32::from_rgba_unmultiplied(
                                    p.text.r(),
                                    p.text.g(),
                                    p.text.b(),
                                    alpha as u8,
                                )),
                        );
                    });
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dark = self.config.theme == "dark";
        if dark != self.applied_dark {
            apply_theme(ctx, dark);
            self.applied_dark = dark;
        }

        let dt = ctx.input(|i| i.stable_dt) as f64;
        self.toast(ctx, dt);
        self.poll_worker(ctx);
        self.sidebar(ctx);
        self.status_bar(ctx);

        let p = self.p();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(p.bg).inner_margin(egui::Margin {
                left: 28,
                right: 28,
                top: 24,
                bottom: 16,
            }))
            .show(ctx, |ui| match self.page {
                Page::Home => self.page_home(ui),
                Page::Versions => self.page_versions(ui),
                Page::Settings => self.page_settings(ui),
                Page::Logs => self.page_logs(ui),
                Page::Sponsor => self.page_sponsor(ui),
                Page::Github => self.page_github(ui),
                Page::About => self.page_about(ui),
                other => self.page_placeholder(ui, other),
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.config.save();
    }
}

impl LauncherApp {
    fn page_home(&mut self, ui: &mut egui::Ui) {
        let p = self.p();
        big_title(ui, &p, "开始游戏", "选择一个版本，一键进入游戏");

        let version = if self.config.last_version.is_empty() {
            "未选择版本".to_string()
        } else {
            self.config.last_version.clone()
        };
        let running = self.running;
        let mut start = false;
        card(ui, &p, |ui| {
            ui.set_max_width(350.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("当前版本").size(11.5).color(p.sub));
                    ui.add_space(2.0);
                    ui.label(RichText::new(version.clone()).size(21.0).strong().color(p.text));
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!(
                            "Java 自动检测 · 内存 {} MB · {}",
                            self.config.max_memory_mb,
                            match self.config.mirror.as_str() {
                                "bmclapi" => "BMCLAPI",
                                "official" => "官方源",
                                _ => "自定义源",
                            }
                        ))
                        .size(11.0)
                        .color(p.sub),
                    );
                });
                let label = if running { "游戏运行中" } else { "启动游戏" };
                let remain = ui.available_width();
                ui.add_space((remain - 152.0).max(8.0));
                if ui.add(primary_button(&p, label, 152.0, 42.0)).clicked() {
                    start = true;
                }
            });
        });

        ui.add_space(14.0);
        let stats = [
            (
                "账户",
                if self.config.username.is_empty() {
                    "Steve".to_string()
                } else {
                    self.config.username.clone()
                },
            ),
            ("版本数", format!("{}", self.versions.len())),
            ("内存", format!("{} MB", self.config.max_memory_mb)),
        ];
        card(ui, &p, |ui| {
            ui.set_width(350.0);
            ui.horizontal(|ui| {
                for (i, (k, v)) in stats.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(26.0);
                    }
                    ui.vertical(|ui| {
                        ui.label(RichText::new(*k).size(10.5).color(p.sub));
                        ui.add_space(2.0);
                        ui.label(RichText::new(v.clone()).size(14.0).strong().color(p.text));
                    });
                }
            });
        });

        ui.add_space(14.0);
        let mut go_versions = false;
        let mut go_instances = false;
        let mut go_accounts = false;
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                if ui.add(ghost_button(&p, "下载新版本", 108.0)).clicked() {
                    go_versions = true;
                }
                ui.add_space(6.0);
                if ui.add(ghost_button(&p, "实例管理", 92.0)).clicked() {
                    go_instances = true;
                }
                ui.add_space(6.0);
                if ui.add(ghost_button(&p, "账号", 70.0)).clicked() {
                    go_accounts = true;
                }
            });
        });
        if start {
            self.start_game();
        }
        if go_versions {
            self.page = Page::Versions;
        }
        if go_instances {
            self.page = Page::Instances;
        }
        if go_accounts {
            self.page = Page::Accounts;
        }
    }

    fn page_placeholder(&mut self, ui: &mut egui::Ui, page: Page) {
        let p = self.p();
        big_title(ui, &p, page.label(), "该模块正在开发中");
        let items: &[&str] = match page {
            Page::Instances => &[
                "创建 / 复制 / 删除实例",
                "实例级版本隔离（mods、存档、配置独立）",
                "实例级内存与 JVM 参数",
            ],
            Page::Mods => &[
                "扫描 mods / resourcepacks / shaderpacks",
                "启用禁用与依赖提示",
                "Modrinth / CurseForge 搜索安装",
            ],
            Page::Accounts => &[
                "离线登录（UUID 生成）",
                "微软正版登录（设备码 OAuth2）",
                "Yggdrasil 外置登录",
            ],
            _ => &[],
        };
        card(ui, &p, |ui| {
            ui.set_min_width(320.0);
            for it in items {
                ui.label(RichText::new(format!("·  {}", it)).size(12.5).color(p.text));
                ui.add_space(5.0);
            }
        });
    }

    fn page_versions(&mut self, ui: &mut egui::Ui) {
        let p = self.p();
        big_title(ui, &p, "版本管理", "BMCLAPI / 官方源，下载后即可启动");

        let mut clicked_kind: Option<usize> = None;
        let mut refresh = false;
        ui.horizontal(|ui| {
            for (idx, name) in [(0usize, "全部"), (1, "正式版"), (2, "快照")] {
                let selected = self.version_kind == idx;
                let btn = egui::Button::new(
                    RichText::new(name)
                        .size(12.0)
                        .color(if selected { p.on_accent } else { p.text }),
                )
                .fill(if selected { p.accent } else { p.pill })
                .stroke(Stroke::NONE)
                .corner_radius(8)
                .min_size(egui::vec2(52.0, 28.0));
                if ui.add(btn).clicked() {
                    clicked_kind = Some(idx);
                }
            }
            ui.add_space(10.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.version_filter)
                    .hint_text("搜索版本号")
                    .desired_width(112.0),
            );
            ui.add_space(8.0);
            if ui.add(ghost_button(&p, "刷新", 54.0)).clicked() {
                refresh = true;
            }
            if self.versions_loading {
                ui.spinner();
            }
        });
        if let Some(k) = clicked_kind {
            self.version_kind = k;
        }
        if refresh {
            self.start_manifest();
        }

        if let Some(err) = &self.version_error {
            ui.add_space(6.0);
            ui.add(
                egui::Label::new(
                    RichText::new(err.clone())
                        .size(11.5)
                        .color(Color32::from_rgb(255, 59, 48)),
                )
                .wrap(),
            );
        }

        if self.versions.is_empty() && !self.versions_loading && self.version_error.is_none() {
            self.start_manifest();
        }

        if self.installing {
            ui.add_space(8.0);
            let frac = if self.install_total > 0 {
                self.install_done as f32 / self.install_total as f32
            } else {
                0.0
            };
            ui.add(egui::ProgressBar::new(frac).desired_width(360.0));
            ui.label(
                RichText::new(format!(
                    "{}   {} / {}",
                    self.install_stage,
                    fmt_size(self.install_done),
                    fmt_size(self.install_total)
                ))
                .size(10.5)
                .color(p.sub),
            );
        }

        ui.add_space(10.0);
        let filter = self.version_filter.trim().to_lowercase();
        let list: Vec<net::VersionEntry> = self
            .versions
            .iter()
            .filter(|v| {
                let kind_ok = match self.version_kind {
                    0 => true,
                    1 => v.kind == "release",
                    _ => v.kind == "snapshot",
                };
                kind_ok && (filter.is_empty() || v.id.to_lowercase().contains(&filter))
            })
            .take(400)
            .cloned()
            .collect();

        let mut action: Option<net::VersionEntry> = None;
        let mut choose: Option<String> = None;
        let h = (ui.available_height() - 8.0).max(120.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height(h)
            .show(ui, |ui| {
                egui::Grid::new("versions_grid")
                    .num_columns(4)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        for v in &list {
                            let installed = meta::installed(&v.id);
                            ui.label(
                                RichText::new(&v.id)
                                    .size(12.5)
                                    .color(if installed { p.accent } else { p.text }),
                            );
                            let (kind, color) = match v.kind.as_str() {
                                "release" => ("正式版", p.accent),
                                "snapshot" => ("快照", Color32::from_rgb(191, 144, 0)),
                                _ => ("旧版", p.sub),
                            };
                            ui.label(RichText::new(kind).size(10.5).color(color));
                            ui.label(
                                RichText::new(v.release_time.get(0..10).unwrap_or(""))
                                    .size(10.5)
                                    .color(p.sub),
                            );
                            let text = if installed { "选择" } else { "下载" };
                            let btn = egui::Button::new(
                                RichText::new(text).size(11.5).color(if installed {
                                    p.text
                                } else {
                                    p.on_accent
                                }),
                            )
                            .fill(if installed { p.pill } else { p.accent })
                            .stroke(Stroke::NONE)
                            .corner_radius(8)
                            .min_size(egui::vec2(58.0, 24.0));
                            if ui.add(btn).clicked() {
                                if installed {
                                    choose = Some(v.id.clone());
                                } else {
                                    action = Some(v.clone());
                                }
                            }
                            ui.end_row();
                        }
                    });
            });

        if let Some(id) = choose {
            self.config.last_version = id.clone();
            let _ = self.config.save();
            self.selected_version = Some(id.clone());
            self.notify(format!("已选择 {id}"));
        }
        if let Some(v) = action {
            self.start_install(&v);
        }
    }

    fn page_settings(&mut self, ui: &mut egui::Ui) {
        let p = self.p();
        big_title(ui, &p, "设置", "修改后点击保存生效");

        let mut detect = false;
        let mut save = false;
        let mut reset = false;

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(380.0);

            card(ui, &p, |ui| {
                ui.set_min_width(340.0);
                section(ui, &p, "Java");
                ui.add(
                    egui::TextEdit::singleline(&mut self.config.java_path)
                        .hint_text("留空自动检测")
                        .desired_width(ui.available_width()),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(ghost_button(&p, "自动检测", 84.0)).clicked() {
                        detect = true;
                    }
                    if let Some(info) = &self.java_probe {
                        ui.label(RichText::new(info).size(10.0).color(p.sub));
                    }
                });
            });

            ui.add_space(12.0);
            card(ui, &p, |ui| {
                ui.set_min_width(340.0);
                section(ui, &p, "内存");
                ui.horizontal(|ui| {
                    ui.label(RichText::new("最大").size(11.5).color(p.sub));
                    ui.add(
                        egui::DragValue::new(&mut self.config.max_memory_mb)
                            .range(512..=65536)
                            .speed(64.0)
                            .suffix(" MB"),
                    );
                    ui.add_space(18.0);
                    ui.label(RichText::new("最小").size(11.5).color(p.sub));
                    ui.add(
                        egui::DragValue::new(&mut self.config.min_memory_mb)
                            .range(128..=8192)
                            .speed(64.0)
                            .suffix(" MB"),
                    );
                });
            });

            ui.add_space(12.0);
            card(ui, &p, |ui| {
                ui.set_min_width(340.0);
                section(ui, &p, "网络与参数");
                egui::ComboBox::from_id_salt("mirror")
                    .selected_text(match self.config.mirror.as_str() {
                        "bmclapi" => "BMCLAPI（国内）",
                        "official" => "Mojang 官方",
                        _ => "自定义",
                    })
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.config.mirror, "bmclapi".into(), "BMCLAPI（国内）");
                        ui.selectable_value(&mut self.config.mirror, "official".into(), "Mojang 官方");
                        ui.selectable_value(&mut self.config.mirror, "custom".into(), "自定义");
                    });
                if self.config.mirror == "custom" {
                    ui.add_space(4.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.config.custom_mirror)
                            .hint_text("https://...")
                            .desired_width(ui.available_width()),
                    );
                }
                ui.add_space(8.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.config.jvm_args)
                        .hint_text("额外 JVM 参数（可留空）")
                        .desired_width(ui.available_width()),
                );
            });

            ui.add_space(12.0);
            card(ui, &p, |ui| {
                ui.set_min_width(340.0);
                section(ui, &p, "路径");
                path_line(ui, &p, "数据目录", &util::base_dir());
                path_line(ui, &p, "版本", &util::versions_dir());
                path_line(ui, &p, "依赖库", &util::libraries_dir());
                path_line(ui, &p, "资源", &util::assets_dir());
                path_line(ui, &p, "实例", &util::instances_dir());
                path_line(ui, &p, "日志", &util::logs_dir());
                ui.add_space(6.0);
                ui.label(
                    RichText::new("自定义数据目录（留空用 exe 同级 data/，重启生效）")
                        .size(10.0)
                        .color(p.sub),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.config.data_dir)
                        .hint_text("D:\\MCData")
                        .desired_width(ui.available_width()),
                );
            });

            ui.add_space(12.0);
            card(ui, &p, |ui| {
                ui.set_min_width(340.0);
                section(ui, &p, "外观与账号");
                let mut theme = self.config.theme.clone();
                ui.horizontal(|ui| {
                    for (value, name) in [("light", "浅色"), ("dark", "深色")] {
                        let selected = theme == value;
                        let btn = egui::Button::new(
                            RichText::new(name).size(12.0).color(if selected {
                                p.on_accent
                            } else {
                                p.text
                            }),
                        )
                        .fill(if selected { p.accent } else { p.pill })
                        .stroke(Stroke::NONE)
                        .corner_radius(8)
                        .min_size(egui::vec2(68.0, 28.0));
                        if ui.add(btn).clicked() {
                            theme = value.to_string();
                        }
                    }
                    ui.add_space(18.0);
                    ui.label(RichText::new("离线用户名").size(11.5).color(p.sub));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.config.username)
                            .hint_text("Steve")
                            .desired_width(120.0),
                    );
                });
                self.config.theme = theme;
                ui.add_space(6.0);
                ui.label(
                    RichText::new("微软正版 / Yggdrasil 登录将在下一模块加入")
                        .size(10.0)
                        .color(p.sub),
                );
            });

            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui.add(primary_button(&p, "保存设置", 110.0, 34.0)).clicked() {
                    save = true;
                }
                ui.add_space(8.0);
                if ui.add(ghost_button(&p, "恢复默认", 94.0)).clicked() {
                    reset = true;
                }
            });
        });

        if detect {
            self.java_probe = java::detect(&self.config.java_path)
                .map(|j| format!("{} · {}", j.full, j.path.display()));
            if self.java_probe.is_none() {
                self.notify("未找到 Java");
            }
        }
        if save {
            match self.config.save() {
                Ok(()) => self.notify("设置已保存"),
                Err(e) => self.notify(format!("保存失败: {e}")),
            }
        }
        if reset {
            let data_dir = self.config.data_dir.clone();
            self.config = Config::default();
            self.config.data_dir = data_dir;
            self.notify("已恢复默认设置");
        }
    }

    fn page_logs(&mut self, ui: &mut egui::Ui) {
        let p = self.p();
        big_title(ui, &p, "日志", "logs/latest.log");
        let mut refresh = false;
        let mut open_dir = false;
        ui.horizontal(|ui| {
            if ui.add(ghost_button(&p, "刷新", 54.0)).clicked() {
                refresh = true;
            }
            ui.add_space(6.0);
            if ui.add(ghost_button(&p, "打开目录", 82.0)).clicked() {
                open_dir = true;
            }
        });
        if refresh {
            self.log_cache.clear();
        }
        if open_dir {
            util::open_url(&util::logs_dir().to_string_lossy());
        }
        if self.log_cache.is_empty() {
            let path = util::logs_dir().join("latest.log");
            self.log_cache = match std::fs::read_to_string(&path) {
                Ok(text) => {
                    let limit = 150_000usize;
                    let mut start = text.len().saturating_sub(limit);
                    while start < text.len() && !text.is_char_boundary(start) {
                        start += 1;
                    }
                    text[start..].to_string()
                }
                Err(_) => "（暂无日志，启动一次游戏后生成）".to_string(),
            };
        }
        ui.add_space(10.0);
        egui::Frame::NONE
            .fill(Color32::from_rgb(28, 28, 30))
            .corner_radius(12)
            .inner_margin(14)
            .show(ui, |ui| {
                let text = self.log_cache.clone();
                let h = (ui.available_height() - 4.0).max(120.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(h)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(text)
                                    .size(11.0)
                                    .monospace()
                                    .color(Color32::from_rgb(210, 214, 220)),
                            )
                            .wrap(),
                        );
                    });
            });
    }

    fn page_sponsor(&mut self, ui: &mut egui::Ui) {
        let p = self.p();
        big_title(ui, &p, "赞助支持", "完全自愿，不影响任何功能");

        card(ui, &p, |ui| {
            ui.label(
                RichText::new("如果 DFL 帮你省下了折腾启动器的时间，可以请开发者喝杯咖啡。")
                    .size(12.5)
                    .color(p.text),
            );
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if let Some(tex) = &self.wechat {
                    qr_block(ui, &p, tex, "微信支付");
                }
                ui.add_space(40.0);
                if let Some(tex) = &self.alipay {
                    qr_block(ui, &p, tex, "支付宝");
                }
            });
        });
        ui.add_space(10.0);
        ui.label(
            RichText::new("二维码仅用于个人收款；如失效可在 GitHub 反馈。")
                .size(10.5)
                .color(p.sub),
        );
    }

    fn page_github(&mut self, ui: &mut egui::Ui) {
        let p = self.p();
        big_title(ui, &p, "GitHub", "开源 · 免费 · 无广告 · 无捆绑");

        let mut open = false;
        let mut copy = false;
        card(ui, &p, |ui| {
            ui.label(
                RichText::new("如果这个项目对你有用，欢迎点一个 Star。")
                    .size(13.0)
                    .color(p.text),
            );
            ui.add_space(12.0);
            ui.label(
                RichText::new(self.config.github_url.clone())
                    .size(12.5)
                    .monospace()
                    .color(p.accent),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.add(primary_button(&p, "打开仓库", 108.0, 32.0)).clicked() {
                    open = true;
                }
                ui.add_space(8.0);
                if ui.add(ghost_button(&p, "复制地址", 86.0)).clicked() {
                    copy = true;
                }
            });
        });
        if open {
            util::open_url(&self.config.github_url);
        }
        if copy {
            ui.ctx().copy_text(self.config.github_url.clone());
            self.notify("已复制");
        }
    }

    fn page_about(&mut self, ui: &mut egui::Ui) {
        let p = self.p();
        big_title(ui, &p, "关于", "轻量、单文件、无第三方运行时");
        card(ui, &p, |ui| {
            for (k, v) in [
                ("名称", APP_NAME),
                ("版本", env!("CARGO_PKG_VERSION")),
                ("构建", "Rust + egui"),
                ("平台", "Windows 10 / 11 x64"),
            ] {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(k).size(11.5).color(p.sub));
                    ui.add_space(8.0);
                    ui.label(RichText::new(v).size(11.5).color(p.text));
                });
                ui.add_space(4.0);
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new("不内置任何账号、密钥或 API Key，仅用于个人学习。")
                    .size(10.5)
                    .color(p.sub),
            );
        });
    }

    // ---------- 后台任务 ----------
    fn push_log(&mut self, line: impl Into<String>) {
        self.logs.push(line.into());
        if self.logs.len() > 200 {
            self.logs.remove(0);
        }
    }

    fn poll_worker(&mut self, ctx: &egui::Context) {
        loop {
            match self.rx.try_recv() {
                Ok(net::Msg::Manifest(m)) => {
                    self.versions = m.versions.clone();
                    self.versions_loading = false;
                    let n = self.versions.len();
                    self.push_log(format!("版本清单加载完成，共 {n} 个版本"));
                }
                Ok(net::Msg::Stage(stage)) => {
                    self.install_stage = stage.clone();
                    self.push_log(stage);
                }
                Ok(net::Msg::Progress(done, total)) => {
                    self.install_done = done;
                    self.install_total = total;
                }
                Ok(net::Msg::Log(line)) => self.push_log(line),
                Ok(net::Msg::Done(id)) => {
                    self.installing = false;
                    self.install_stage = format!("安装完成: {id}");
                    self.config.last_version = id.clone();
                    let _ = self.config.save();
                    self.push_log(format!("安装完成: {id}"));
                }
                Ok(net::Msg::GameStarted(pid)) => {
                    self.running = true;
                    self.push_log(format!("游戏进程已启动，PID {pid}"));
                }
                Ok(net::Msg::GameExited(code)) => {
                    self.running = false;
                    self.install_stage = format!("游戏已退出（退出码 {code}）");
                    self.push_log(format!("游戏已退出，退出码 {code}"));
                }
                Ok(net::Msg::Failed(err)) => {
                    self.installing = false;
                    self.running = false;
                    self.version_error = Some(err.clone());
                    self.push_log(format!("失败: {err}"));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }
        }
        if self.installing || self.versions_loading || self.running {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }
    }

    fn start_manifest(&mut self) {
        if self.versions_loading {
            return;
        }
        self.versions_loading = true;
        self.version_error = None;
        let base = self.config.mirror_base().to_string();
        let tx = self.tx.clone();
        std::thread::spawn(move || match net::fetch_manifest(&base) {
            Ok(m) => {
                let _ = tx.send(net::Msg::Manifest(Box::new(m)));
            }
            Err(e) => {
                let _ = tx.send(net::Msg::Failed(format!("{e:#}")));
            }
        });
    }

    fn start_install(&mut self, entry: &net::VersionEntry) {
        if self.installing {
            self.notify("已有下载任务在进行");
            return;
        }
        self.installing = true;
        self.install_stage = format!("准备安装 {}", entry.id);
        self.install_done = 0;
        self.install_total = 0;
        let cfg = self.config.clone();
        let entry = entry.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || crate::install::install_version(&cfg, &entry, tx));
    }

    fn start_game(&mut self) {
        if self.running {
            self.notify("游戏已经在运行");
            return;
        }
        let id = self.config.last_version.clone();
        if id.is_empty() {
            self.notify("请先在“版本管理”中选择版本");
            return;
        }
        let cfg = self.config.clone();
        let tx = self.tx.clone();
        self.running = true;
        self.push_log(format!("准备启动 {id}"));
        std::thread::spawn(move || match crate::launch::launch(&id, &cfg, &tx) {
            Ok(mut child) => {
                let _ = tx.send(net::Msg::GameStarted(child.id()));
                let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
                let _ = tx.send(net::Msg::GameExited(code));
            }
            Err(e) => {
                let _ = tx.send(net::Msg::Failed(format!("启动失败: {e:#}")));
            }
        });
    }
}

// ---------- 辅助 ----------
fn big_title(ui: &mut egui::Ui, p: &Palette, title: &str, sub: &str) {
    ui.label(RichText::new(title).size(26.0).strong().color(p.text));
    ui.add_space(2.0);
    ui.label(RichText::new(sub).size(12.0).color(p.sub));
    ui.add_space(18.0);
}

fn card(ui: &mut egui::Ui, p: &Palette, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(14)
        .inner_margin(18)
        .show(ui, add);
}

fn section(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).size(11.5).strong().color(p.sub));
    ui.add_space(8.0);
}

fn primary_button(p: &Palette, text: &str, w: f32, h: f32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).size(14.0).color(p.on_accent))
        .fill(p.accent)
        .stroke(Stroke::NONE)
        .corner_radius(11)
        .min_size(egui::vec2(w, h))
}

fn ghost_button(p: &Palette, text: &str, w: f32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).size(12.5).color(p.text))
        .fill(p.pill)
        .stroke(Stroke::NONE)
        .corner_radius(9)
        .min_size(egui::vec2(w, 30.0))
}

fn path_line(ui: &mut egui::Ui, p: &Palette, label: &str, path: &std::path::Path) {
    let mut open = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.0).color(p.sub));
        ui.add_space(6.0);
        let full = path.display().to_string();
        ui.label(
            RichText::new(elide(&full, 34))
                .size(11.0)
                .monospace()
                .color(p.text),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(RichText::new("打开").size(10.5).color(p.accent))
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::NONE),
                )
                .clicked()
            {
                open = true;
            }
        });
        if open {
            util::open_url(&full);
        }
    });
    ui.add_space(4.0);
}

fn elide(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(10).collect();
        let tail: String = s.chars().skip(count - (max - 12)).collect();
        format!("{head}…{tail}")
    }
}

fn qr_block(ui: &mut egui::Ui, p: &Palette, tex: &egui::TextureHandle, label: &str) {
    ui.vertical(|ui| {
        let src = tex.size_vec2();
        let h = 170.0;
        let size = egui::vec2(src.x * (h / src.y), h);
        ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), size)).corner_radius(10));
        ui.add_space(8.0);
        ui.label(RichText::new(label).size(12.0).strong().color(p.text));
    });
}

fn fmt_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let b = bytes as f64;
    if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{b} B")
    }
}

fn texture_from_image(
    ctx: &egui::Context,
    name: &str,
    bytes: &[u8],
    max_h: u32,
) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = (img.width(), img.height());
    let img = if h > max_h {
        let scale = max_h as f32 / h as f32;
        image::imageops::resize(
            &img,
            ((w as f32 * scale) as u32).max(1),
            max_h,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img
    };
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture(name, color, egui::TextureOptions::LINEAR))
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Ok(bytes) = std::fs::read("C:/Windows/Fonts/segoeui.ttf") {
        fonts.font_data.insert(
            "latin".to_owned(),
            egui::FontData::from_owned(bytes).into(),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "latin".to_owned());
    }
    for path in [
        "C:/Windows/Fonts/Deng.ttf",
        "C:/Windows/Fonts/simhei.ttf",
        "C:/Windows/Fonts/NotoSansSC-VF.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "cjk".to_owned(),
                egui::FontData::from_owned(bytes).into(),
            );
            let list = fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default();
            if !list.contains(&"cjk".to_owned()) {
                list.push("cjk".to_owned());
            }
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("cjk".to_owned());
            break;
        }
    }
    ctx.set_fonts(fonts);
}

fn apply_theme(ctx: &egui::Context, dark: bool) {
    let p = if dark { Palette::dark() } else { Palette::light() };
    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = p.bg;
    visuals.window_fill = p.card;
    visuals.extreme_bg_color = if dark {
        Color32::from_rgb(22, 22, 24)
    } else {
        Color32::WHITE
    };
    visuals.faint_bg_color = p.pill;
    visuals.override_text_color = Some(p.text);
    visuals.widgets.inactive.bg_fill = p.pill;
    visuals.widgets.inactive.weak_bg_fill = p.pill;
    visuals.widgets.hovered.bg_fill = p.hover;
    visuals.widgets.active.bg_fill = p.hover;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    visuals.selection.bg_fill = p.accent;
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.interact_size.y = 26.0;
    ctx.set_style(style);
}
