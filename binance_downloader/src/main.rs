use chrono::{Duration, NaiveDate};
use eframe::egui;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

fn main() -> eframe::Result<()> {
    // 设置窗口的大小和属性
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([550.0, 500.0])
            .with_title("🦀 币安数据下载器 (图形界面版)"),
        ..Default::default()
    };

    eframe::run_native(
        "Binance Downloader",
        options,
        Box::new(|cc| {
            // 解决 Windows 下中文显示为方块的问题 (加载微软雅黑字体)
            let mut fonts = egui::FontDefinitions::default();
            if let Ok(font_data) = std::fs::read("C:\\Windows\\Fonts\\msyh.ttc") {
                fonts.font_data.insert("msyh".to_owned(), egui::FontData::from_owned(font_data));
                fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "msyh".to_owned());
                fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(0, "msyh".to_owned());
            }
            cc.egui_ctx.set_fonts(fonts);

            // 初始化我们的应用，填入你要求的“默认好的条件”
            Box::new(DownloaderApp::new())
        }),
    )
}

// 定义应用程序的状态
struct DownloaderApp {
    symbol: String,
    interval: String,
    start_date: String,
    end_date: String,
    save_path: String,
    log_messages: String,
    is_downloading: bool,
    // 用于接收后台下载线程发来的消息
    log_receiver: Option<mpsc::Receiver<String>>, 
}

impl DownloaderApp {
    fn new() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            interval: "1m".to_string(),
            start_date: "2026-07-24".to_string(),
            end_date: "2026-07-29".to_string(),
            save_path: "请选择一个文件夹...".to_string(),
            log_messages: "欢迎使用！请确认参数后点击开始下载。\n".to_string(),
            is_downloading: false,
            log_receiver: None,
        }
    }
}

// 构建图形界面的核心逻辑
impl eframe::App for DownloaderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 如果正在下载，不断接收后台线程发来的日志信息
        if let Some(rx) = &self.log_receiver {
            while let Ok(msg) = rx.try_recv() {
                if msg == "DONE_SIGNAL" {
                    self.is_downloading = false;
                } else {
                    self.log_messages.push_str(&format!("{}\n", msg));
                }
            }
        }
        
        // 如果处于下载状态，让界面保持持续刷新，避免卡顿
        if self.is_downloading {
            ctx.request_repaint();
        }

        // 绘制界面布局
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("📊 币安历史数据批量下载器");
            ui.separator();

            // 表单输入区
            egui::Grid::new("my_grid")
                .num_columns(2)
                .spacing([40.0, 15.0])
                .show(ui, |ui| {
                    ui.label("1. 交易对 (大写):");
                    ui.add(egui::TextEdit::singleline(&mut self.symbol));
                    ui.end_row();

                    ui.label("2. K线周期:");
                    ui.add(egui::TextEdit::singleline(&mut self.interval));
                    ui.end_row();

                    ui.label("3. 开始日期 (YYYY-MM-DD):");
                    ui.add(egui::TextEdit::singleline(&mut self.start_date));
                    ui.end_row();

                    ui.label("4. 结束日期 (YYYY-MM-DD):");
                    ui.add(egui::TextEdit::singleline(&mut self.end_date));
                    ui.end_row();

                    ui.label("5. 保存路径:");
                    ui.horizontal(|ui| {
                        // 文件夹选择按钮
                        if ui.button("📂 选择文件夹").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                self.save_path = path.display().to_string();
                            }
                        }
                        ui.label(&self.save_path);
                    });
                    ui.end_row();
                });

            ui.add_space(20.0);

            // 开始下载按钮
            ui.horizontal(|ui| {
                // 如果正在下载，禁用按钮，防止重复点击
                let start_btn = ui.add_enabled(!self.is_downloading, egui::Button::new("🚀 开始下载"));
                
                if start_btn.clicked() {
                    if self.save_path == "请选择一个文件夹..." {
                        self.log_messages.push_str("❌ 错误：请先选择保存路径！\n");
                    } else {
                        self.start_download();
                    }
                }

                if self.is_downloading {
                    ui.spinner(); // 显示一个加载动画
                    ui.label("正在疯狂下载中，请耐心等待...");
                }
            });

            ui.add_space(20.0);
            ui.separator();
            ui.label("运行日志:");

            // 日志输出显示区 (滚动框)
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true) // 自动滚动到最底部
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.log_messages)
                            .desired_width(f32::INFINITY)
                            .interactive(false), // 设为只读
                    );
                });
        });
    }
}

// 提取出来的后台下载逻辑
impl DownloaderApp {
    fn start_download(&mut self) {
        self.is_downloading = true;
        self.log_messages.push_str("\n--- 启动新任务 ---\n");

        // 创建一个通道，用于后台线程给界面发送文字日志
        let (tx, rx) = mpsc::channel();
        self.log_receiver = Some(rx);

        // 克隆参数，准备传给后台线程
        let symbol = self.symbol.clone();
        let interval = self.interval.clone();
        let start_str = self.start_date.clone();
        let end_str = self.end_date.clone();
        let save_path = self.save_path.clone();

        // 开启后台线程，避免阻塞界面
        thread::spawn(move || {
            let start_date = match NaiveDate::parse_from_str(&start_str, "%Y-%m-%d") {
                Ok(d) => d,
                Err(_) => {
                    let _ = tx.send("❌ 开始日期格式错误，必须为 YYYY-MM-DD".to_string());
                    let _ = tx.send("DONE_SIGNAL".to_string());
                    return;
                }
            };
            
            let end_date = match NaiveDate::parse_from_str(&end_str, "%Y-%m-%d") {
                Ok(d) => d,
                Err(_) => {
                    let _ = tx.send("❌ 结束日期格式错误，必须为 YYYY-MM-DD".to_string());
                    let _ = tx.send("DONE_SIGNAL".to_string());
                    return;
                }
            };

            if start_date > end_date {
                let _ = tx.send("❌ 错误：开始日期不能大于结束日期！".to_string());
                let _ = tx.send("DONE_SIGNAL".to_string());
                return;
            }

            let target_dir = PathBuf::from(&save_path).join(&symbol).join(&interval);
            if let Err(e) = fs::create_dir_all(&target_dir) {
                let _ = tx.send(format!("❌ 创建文件夹失败: {}", e));
                let _ = tx.send("DONE_SIGNAL".to_string());
                return;
            }

            let _ = tx.send(format!("🚀 任务就绪，保存路径: {:?}", target_dir));

            let client = match reqwest::blocking::Client::builder().user_agent("Mozilla/5.0").build() {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(format!("❌ 初始化网络请求失败: {}", e));
                    let _ = tx.send("DONE_SIGNAL".to_string());
                    return;
                }
            };

            let mut current_date = start_date;
            while current_date <= end_date {
                let date_str = current_date.format("%Y-%m-%d").to_string();
                let filename = format!("{}-{}-{}.zip", symbol, interval, date_str);
                let url = format!(
                    "https://data.binance.vision/data/spot/daily/klines/{}/{}/{}",
                    symbol, interval, filename
                );

                let _ = tx.send(format!("\n[{}] 正在请求...", date_str));

                match client.get(&url).send() {
                    Ok(response) => {
                        if response.status().is_success() {
                            if let Ok(bytes) = response.bytes() {
                                let zip_path = target_dir.join(&filename);
                                if let Ok(mut zip_file) = File::create(&zip_path) {
                                    let _ = zip_file.write_all(&bytes);
                                    let _ = tx.send(format!("  └─ 📥 下载完成: {}", filename));

                                    // 解压文件
                                    if let Ok(file) = File::open(&zip_path) {
                                        if let Ok(mut archive) = zip::ZipArchive::new(file) {
                                            for i in 0..archive.len() {
                                                if let Ok(mut zip_entry) = archive.by_index(i) {
                                                    if let Some(path) = zip_entry.enclosed_name() {
                                                        let outpath = target_dir.join(path);
                                                        if !zip_entry.is_dir() {
                                                            if let Ok(mut outfile) = File::create(&outpath) {
                                                                let _ = io::copy(&mut zip_entry, &mut outfile);
                                                                let _ = tx.send(format!("  └─ 📂 解压成功: {:?}", outpath.file_name().unwrap_or_default()));
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    let _ = fs::remove_file(&zip_path);
                                    let _ = tx.send("  └─ 🗑️ 清理压缩包完成".to_string());
                                }
                            }
                        } else if response.status() == reqwest::StatusCode::NOT_FOUND {
                            let _ = tx.send("  └─ ⚠️ 跳过: 404 (该日期无数据)".to_string());
                        } else {
                            let _ = tx.send(format!("  └─ ❌ 失败，状态码: {}", response.status()));
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(format!("  └─ ❌ 网络请求出错: {}", e));
                    }
                }
                current_date += Duration::days(1);
            }
            let _ = tx.send("\n✅ 所有任务已执行完毕！".to_string());
            let _ = tx.send("DONE_SIGNAL".to_string()); // 通知界面任务结束
        });
    }
}