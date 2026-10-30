use eframe::egui;
use egui::{Color32, RichText};

use std::sync::mpsc::{Receiver, Sender};

use crate::{config::Config, meta, net, util};

pub const APP_NAME: &str = "DFL";

const ACCENT: Color32 = Color32::from_rgb(64, 156, 214);
const ACCENT_SOFT: Color32 = Color32::from_rgb(36, 74, 104);
const BG: Color32 = Color32::from_rgb(20, 22, 26);
const PANEL: Color32 = Color32::from_rgb(28, 31, 37);
const TEXT: Color32 = Color32::from_rgb(228, 231, 236);
const SUBTLE: Color32 = Color32::from_rgb(140, 148, 160);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Versions,
    Instances,
    Mods,
    Accounts,
    Settings,
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
}

impl LauncherApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        install_style(&cc.egui_ctx);

        let mut config = Config::load();
        if config.theme.is_empty() {
            config.theme = "dark".into();
        }
        util::ensure_dirs();
        if !util::config_path().exists() {
            let _ = config.save();
        }

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
        }
    }

    fn notify(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), 0.0));
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("nav")
            .exact_width(188.0)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(PANEL))
            .show(ctx, |ui| {
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    if let Some(logo) = &self.logo {
                        let size = egui::vec2(34.0, 34.0);
                        ui.add(
                            egui::Image::new(egui::load::SizedTexture::new(logo.id(), size))
                                .corner_radius(8),
                        );
                    }
                    ui.add_space(2.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(APP_NAME).size(19.0).strong().color(TEXT));
                        ui.label(RichText::new("轻盈 · 单文件").size(10.0).color(SUBTLE));
                    });
                });
                ui.add_space(16.0);

                for page in [
                    Page::Home,
                    Page::Versions,
                    Page::Instances,
                    Page::Mods,
                    Page::Accounts,
                    Page::Settings,
                    Page::Sponsor,
                    Page::Github,
                    Page::About,
                ] {
                    let selected = self.page == page;
                    ui.horizontal(|ui| {
                        ui.add_space(10.0);
                        let w = ui.available_width() - 10.0;
                        let btn = egui::Button::new(
                            RichText::new(page.label()).size(14.0).color(if selected {
                                TEXT
                            } else {
                                SUBTLE
                            }),
                        )
                        .fill(if selected {
                            ACCENT_SOFT
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(egui::Stroke::NONE)
                        .corner_radius(6)
                        .min_size(egui::vec2(w, 32.0));
                        if ui.add(btn).clicked() {
                            self.page = page;
                        }
                    });
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        ui.label(
                            RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                .size(11.0)
                                .color(SUBTLE),
                        );
                    });
                    ui.add_space(4.0);
                });
            });
    }

    fn status_bar(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .frame(egui::Frame::NONE.fill(PANEL))
            .show(ctx, |ui| {
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(format!("数据目录: {}", util::base_dir().display()))
                            .size(11.0)
                            .color(SUBTLE),
                    );
                });
                ui.add_space(3.0);
            });
    }

    fn toast(&mut self, ctx: &egui::Context, dt: f64) {
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
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -56.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(46, 52, 62, alpha as u8))
                    .inner_margin(egui::Margin::symmetric(14, 9))
                    .corner_radius(6)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(msg.as_str())
                                .size(13.0)
                                .color(Color32::from_rgba_unmultiplied(230, 234, 240, alpha as u8)),
                        );
                    });
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dt = ctx.input(|i| i.stable_dt) as f64;
        self.toast(ctx, dt);
        self.poll_worker(ctx);
        self.sidebar(ctx);
        self.status_bar(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BG))
            .show(ctx, |ui| {
                let w = ui.available_width();
                ui.add_space(18.0);
                ui.horizontal_top(|ui| {
                    ui.add_space(22.0);
                    ui.vertical(|ui| {
                        ui.set_max_width((w - 44.0).max(320.0));
                        match self.page {
                            Page::Home => self.page_home(ui),
                            Page::Versions => self.page_versions(ui),
                            Page::Settings => self.page_settings(ui),
                            Page::Sponsor => self.page_sponsor(ui),
                            Page::Github => self.page_github(ui),
                            Page::About => self.page_about(ui),
                            other => self.page_placeholder(ui, other),
                        }
                    });
                });
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.config.save();
    }
}

impl LauncherApp {
    fn title(&self, ui: &mut egui::Ui, text: &str, sub: &str) {
        ui.label(RichText::new(text).size(22.0).strong().color(TEXT));
        ui.add_space(2.0);
        ui.label(RichText::new(sub).size(12.0).color(SUBTLE));
        ui.add_space(16.0);
    }

    fn page_home(&mut self, ui: &mut egui::Ui) {
        self.title(ui, "开始游戏", "选择版本后点击启动；首次使用请先下载游戏版本");

        egui::Frame::group(ui.style())
            .fill(PANEL)
            .corner_radius(8)
            .inner_margin(16)
            .show(ui, |ui| {
                ui.set_min_width(360.0);
                ui.label(RichText::new("当前版本").size(12.0).color(SUBTLE));
                let name = if self.config.last_version.is_empty() {
                    "未选择版本".to_string()
                } else {
                    self.config.last_version.clone()
                };
                ui.label(RichText::new(name).size(18.0).color(TEXT));
                ui.add_space(12.0);
                let has_version = !self.config.last_version.is_empty();
                let btn = egui::Button::new(RichText::new("启动游戏").size(15.0).color(TEXT))
                    .fill(if has_version { ACCENT } else { Color32::from_rgb(58, 66, 78) })
                    .corner_radius(6)
                    .min_size(egui::vec2(150.0, 38.0));
                if ui.add(btn).clicked() {
                    if has_version {
                        self.notify("启动逻辑将在下一个模块接入");
                    } else {
                        self.notify("请先在“版本管理”中下载并选择一个版本");
                    }
                }
            });

        ui.add_space(14.0);
        egui::Frame::group(ui.style())
            .fill(PANEL)
            .corner_radius(8)
            .inner_margin(16)
            .show(ui, |ui| {
                ui.set_min_width(360.0);
                ui.label(RichText::new("快速开始").size(13.0).strong().color(TEXT));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("下载新版本").clicked() {
                        self.page = Page::Versions;
                    }
                    if ui.button("新建实例").clicked() {
                        self.page = Page::Instances;
                    }
                    if ui.button("账号设置").clicked() {
                        self.page = Page::Accounts;
                    }
                });
            });

        ui.add_space(14.0);
        ui.label(
            RichText::new(format!(
                "内存: {} MB / 下载源: {}",
                self.config.max_memory_mb,
                match self.config.mirror.as_str() {
                    "bmclapi" => "BMCLAPI（国内）",
                    "official" => "官方",
                    _ => "自定义",
                }
            ))
            .size(12.0)
            .color(SUBTLE),
        );
    }

    fn page_placeholder(&mut self, ui: &mut egui::Ui, page: Page) {
        self.title(ui, page.label(), "该模块正在开发中");
        ui.label(RichText::new("功能规划：").size(13.0).color(TEXT));
        ui.add_space(6.0);
        let items: &[&str] = match page {
            Page::Versions => &[
                "拉取版本清单（BMCLAPI / 官方）",
                "下载客户端、依赖库、资源与 natives",
                "断点续传与 SHA1 校验",
                "安装 Fabric / Forge / NeoForge / Quilt",
            ],
            Page::Instances => &[
                "创建/复制/删除实例",
                "实例级版本隔离（独立 mods、存档、配置）",
                "内存与 JVM 参数覆盖",
            ],
            Page::Mods => &[
                "扫描 mods / resourcepacks / shaderpacks",
                "启用禁用与冲突提示",
                "Modrinth / CurseForge 搜索安装",
            ],
            Page::Accounts => &[
                "离线登录（UUID 生成）",
                "微软正版登录（设备码 OAuth2）",
                "Yggdrasil 外置登录",
            ],
            _ => &[],
        };
        for it in items {
            ui.label(RichText::new(format!("· {}", it)).size(13.0).color(SUBTLE));
            ui.add_space(3.0);
        }
    }

    fn page_sponsor(&mut self, ui: &mut egui::Ui) {
        self.title(ui, "赞助支持", "赞助完全自愿，不影响任何功能，也不解锁任何特权");
        ui.label(
            RichText::new("如果 DFL 帮你省下了折腾启动器的时间，可以请开发者喝杯咖啡。")
                .size(13.0)
                .color(TEXT),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new("扫码即可，金额随意；赞助时备注 DFL 可选。")
                .size(11.0)
                .color(SUBTLE),
        );
        ui.add_space(18.0);

        egui::Frame::group(ui.style())
            .fill(PANEL)
            .corner_radius(8)
            .inner_margin(18)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(tex) = &self.wechat {
                        qr_block(ui, tex, "微信支付", "WeChat Pay");
                    }
                    ui.add_space(36.0);
                    if let Some(tex) = &self.alipay {
                        qr_block(ui, tex, "支付宝", "Alipay");
                    }
                });
            });

        ui.add_space(14.0);
        ui.label(
            RichText::new("二维码仅用于个人收款，请勿用于其他用途；如二维码失效可在 GitHub 反馈。")
                .size(11.0)
                .color(SUBTLE),
        );
    }

    fn page_github(&mut self, ui: &mut egui::Ui) {
        self.title(ui, "GitHub", "开源 · 免费 · 无广告 · 无捆绑");

        ui.label(
            RichText::new("如果这个项目对你有用，欢迎在 GitHub 上点一个 ★ Star。")
                .size(14.0)
                .color(TEXT),
        );
        ui.add_space(6.0);
        ui.label(
            RichText::new("你的 Star 是持续更新版本下载、模组管理与启动兼容性的动力。")
                .size(12.0)
                .color(SUBTLE),
        );
        ui.add_space(16.0);

        egui::Frame::group(ui.style())
            .fill(PANEL)
            .corner_radius(8)
            .inner_margin(16)
            .show(ui, |ui| {
                ui.set_max_width(360.0);
                ui.label(RichText::new("仓库地址").size(12.0).color(SUBTLE));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(self.config.github_url.clone())
                        .size(14.0)
                        .color(ACCENT)
                        .monospace(),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let go = egui::Button::new(RichText::new("打开仓库").size(14.0).color(TEXT))
                        .fill(ACCENT)
                        .corner_radius(6)
                        .min_size(egui::vec2(120.0, 34.0));
                    if ui.add(go).clicked() {
                        util::open_url(&self.config.github_url);
                    }
                    if ui.button("复制地址").clicked() {
                        ui.ctx().copy_text(self.config.github_url.clone());
                        self.notify("仓库地址已复制");
                    }
                });
            });

        ui.add_space(14.0);
        ui.label(
            RichText::new("提示：仓库地址可在「设置」中修改，指向你自己的 fork。")
                .size(11.0)
                .color(SUBTLE),
        );
    }


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
                Ok(net::Msg::Failed(err)) => {
                    self.installing = false;
                    self.versions_loading = false;
                    self.version_error = Some(err.clone());
                    self.push_log(format!("失败: {err}"));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }
        }
        if self.installing || self.versions_loading {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
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

    fn page_versions(&mut self, ui: &mut egui::Ui) {
        self.title(
            ui,
            "版本管理",
            "从 BMCLAPI / 官方源获取清单，下载版本 JSON、client.jar 与资源索引",
        );

        ui.horizontal(|ui| {
            if ui.button("刷新版本列表").clicked() {
                self.start_manifest();
            }
            egui::ComboBox::from_id_salt("version_kind")
                .selected_text(match self.version_kind {
                    0 => "全部",
                    1 => "正式版",
                    _ => "快照",
                })
                .width(96.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.version_kind, 0, "全部");
                    ui.selectable_value(&mut self.version_kind, 1, "正式版");
                    ui.selectable_value(&mut self.version_kind, 2, "快照");
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.version_filter)
                    .hint_text("搜索版本号")
                    .desired_width(150.0),
            );
            if self.versions_loading {
                ui.spinner();
                ui.label(RichText::new("加载中…").size(12.0).color(SUBTLE));
            }
        });

        if let Some(err) = &self.version_error {
            ui.add_space(6.0);
            ui.add(egui::Label::new(RichText::new(err.clone()).size(12.0).color(Color32::from_rgb(220, 110, 110))).wrap());
        }

        if self.versions.is_empty() && !self.versions_loading && self.version_error.is_none() {
            self.start_manifest();
        }

        ui.add_space(8.0);
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

        if self.installing {
            ui.add_space(6.0);
            let frac = if self.install_total > 0 {
                self.install_done as f32 / self.install_total as f32
            } else {
                0.0
            };
            ui.add(
                egui::ProgressBar::new(frac)
                    .show_percentage()
                    .desired_width(380.0),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "{}  {}/{}",
                    self.install_stage,
                    fmt_size(self.install_done),
                    fmt_size(self.install_total)
                ))
                .size(11.0)
                .color(SUBTLE),
            );
        }

        let available_h = (ui.available_height() - 12.0).max(120.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height(available_h)
            .show(ui, |ui| {
                egui::Grid::new("versions_grid")
                    .num_columns(4)
                    .striped(true)
                    .spacing([14.0, 6.0])
                    .show(ui, |ui| {
                        for v in &list {
                            let installed = meta::installed(&v.id);
                            let color = if installed { ACCENT } else { TEXT };
                            ui.label(RichText::new(&v.id).size(13.0).color(color));
                            let (kind_text, kind_color) = match v.kind.as_str() {
                                "release" => ("正式版", ACCENT),
                                "snapshot" => ("快照", Color32::from_rgb(196, 156, 92)),
                                _ => ("旧版", SUBTLE),
                            };
                            ui.label(RichText::new(kind_text).size(11.0).color(kind_color));
                            let date = v.release_time.get(0..10).unwrap_or("").to_string();
                            ui.label(RichText::new(date).size(11.0).color(SUBTLE));
                            let text = if installed { "选择" } else { "下载" };
                            if ui
                                .add_sized(
                                    [64.0, 22.0],
                                    egui::Button::new(RichText::new(text).size(12.0)),
                                )
                                .clicked()
                            {
                                if installed {
                                    self.config.last_version = v.id.clone();
                                    let _ = self.config.save();
                                    self.selected_version = Some(v.id.clone());
                                    self.notify(format!("已选择 {}", v.id));
                                } else {
                                    self.start_install(v);
                                }
                            }
                            ui.end_row();
                        }
                    });
            });

    }

    fn page_settings(&mut self, ui: &mut egui::Ui) {
        self.title(ui, "设置", "修改后点击“保存设置”生效");

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("settings_grid")
                .num_columns(2)
                .spacing([14.0, 12.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Java 路径").size(13.0).color(TEXT));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.config.java_path)
                            .hint_text("留空则自动检测")
                            .desired_width(300.0),
                    );
                    ui.end_row();

                    ui.label(RichText::new("最大内存").size(13.0).color(TEXT));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.config.max_memory_mb)
                                .range(512..=65536)
                                .speed(64.0)
                                .suffix(" MB"),
                        );
                        ui.label(RichText::new("建议 2048~8192").size(11.0).color(SUBTLE));
                    });
                    ui.end_row();

                    ui.label(RichText::new("最小内存").size(13.0).color(TEXT));
                    ui.add(
                        egui::DragValue::new(&mut self.config.min_memory_mb)
                            .range(128..=8192)
                            .speed(64.0)
                            .suffix(" MB"),
                    );
                    ui.end_row();

                    ui.label(RichText::new("下载源").size(13.0).color(TEXT));
                    egui::ComboBox::from_id_salt("mirror")
                        .selected_text(match self.config.mirror.as_str() {
                            "bmclapi" => "BMCLAPI（国内镜像）",
                            "official" => "Mojang 官方",
                            _ => "自定义",
                        })
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.config.mirror,
                                "bmclapi".into(),
                                "BMCLAPI（国内镜像）",
                            );
                            ui.selectable_value(
                                &mut self.config.mirror,
                                "official".into(),
                                "Mojang 官方",
                            );
                            ui.selectable_value(&mut self.config.mirror, "custom".into(), "自定义");
                        });
                    ui.end_row();

                    if self.config.mirror == "custom" {
                        ui.label(RichText::new("自定义地址").size(13.0).color(TEXT));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.config.custom_mirror)
                                .hint_text("https://...")
                                .desired_width(300.0),
                        );
                        ui.end_row();
                    }

                    ui.label(RichText::new("JVM 参数").size(13.0).color(TEXT));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.config.jvm_args)
                            .hint_text("-XX:+UseG1GC 等，留空使用默认")
                            .desired_width(300.0),
                    );
                    ui.end_row();

                    ui.label(RichText::new("游戏参数").size(13.0).color(TEXT));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.config.game_args)
                            .hint_text("例如 -fullscreen")
                            .desired_width(300.0),
                    );
                    ui.end_row();

                    ui.label(RichText::new("离线用户名").size(13.0).color(TEXT));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.config.username)
                            .hint_text("Steve")
                            .desired_width(200.0),
                    );
                    ui.end_row();

                    ui.label(RichText::new("GitHub 仓库").size(13.0).color(TEXT));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.config.github_url)
                            .hint_text("https://github.com/...")
                            .desired_width(300.0),
                    );
                    ui.end_row();

                    ui.label(RichText::new("界面主题").size(13.0).color(TEXT));
                    egui::ComboBox::from_id_salt("theme")
                        .selected_text(match self.config.theme.as_str() {
                            "light" => "浅色",
                            _ => "深色",
                        })
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.config.theme, "dark".into(), "深色");
                            ui.selectable_value(&mut self.config.theme, "light".into(), "浅色");
                        });
                    ui.end_row();
                });

            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if ui.button("保存设置").clicked() {
                    match self.config.save() {
                        Ok(()) => self.notify("设置已保存"),
                        Err(e) => self.notify(format!("保存失败: {e}")),
                    }
                }
                if ui.button("恢复默认").clicked() {
                    self.config = Config::default();
                    self.notify("已恢复默认设置");
                }
            });
        });
    }

    fn page_about(&mut self, ui: &mut egui::Ui) {
        self.title(ui, "关于", "轻量、单文件、无第三方运行时的 Minecraft 启动器");

        let rows = [
            ("名称", APP_NAME),
            ("版本", env!("CARGO_PKG_VERSION")),
            ("构建", "Rust + egui (immediate mode)"),
            ("数据目录", &util::base_dir().display().to_string()),
            ("平台", "Windows 10 / 11 x64"),
        ];
        for (k, v) in rows {
            ui.horizontal(|ui| {
                ui.label(RichText::new(k).size(12.0).color(SUBTLE));
                ui.label(RichText::new(v.to_string()).size(12.0).color(TEXT));
            });
            ui.add_space(4.0);
        }
        ui.add_space(10.0);
        ui.label(
            RichText::new("本启动器不内置任何账号、密钥或 API Key；仅用于个人学习。")
                .size(11.0)
                .color(SUBTLE),
        );
    }
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

fn qr_block(ui: &mut egui::Ui, tex: &egui::TextureHandle, label: &str, sub: &str) {
    ui.vertical(|ui| {
        let src = tex.size_vec2();
        let h = 160.0;
        let size = egui::vec2(src.x * (h / src.y), h);
        ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), size)).corner_radius(6));
        ui.add_space(6.0);
        ui.label(RichText::new(label).size(13.0).strong().color(TEXT));
        ui.label(RichText::new(sub).size(10.0).color(SUBTLE));
    });
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
    let candidates = [
        "C:/Windows/Fonts/Deng.ttf",
        "C:/Windows/Fonts/simhei.ttf",
        "C:/Windows/Fonts/NotoSansSC-VF.ttf",
        "C:/Windows/Fonts/simkai.ttf",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("cjk".to_owned(), egui::FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "cjk".to_owned());
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

fn install_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = Color32::from_rgb(16, 18, 22);
    visuals.override_text_color = Some(TEXT);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(38, 42, 50);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(48, 54, 64);
    visuals.widgets.active.bg_fill = Color32::from_rgb(56, 62, 74);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(38, 42, 50);
    visuals.selection.bg_fill = Color32::from_rgb(44, 96, 136);
    visuals.selection.stroke.color = Color32::WHITE;
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.interact_size.y = 26.0;
    ctx.set_style(style);
}




