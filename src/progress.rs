use anstream::println;
use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use std::cell::RefCell;

/// 底部状态栏管理器 - 用 indicatif
pub struct StatusBar {
    bar: ProgressBar,
    quiet: bool,
    state: RefCell<State>,
}

struct State {
    total: usize,
    current: usize,
}

impl StatusBar {
    /// 创建新的状态栏
    pub fn new(total: usize, quiet: bool) -> Self {
        let bar = ProgressBar::new(total as u64);
        bar.set_style(
            ProgressStyle::default_spinner()
                .template("{wide_msg} {spinner:.green}")
                .unwrap(),
        );
        bar.enable_steady_tick(std::time::Duration::from_millis(100));

        Self {
            bar,
            quiet,
            state: RefCell::new(State { total, current: 0 }),
        }
    }

    /// 更新总任务数
    #[allow(dead_code)]
    pub fn set_total(&self, total: usize) {
        self.state.borrow_mut().total = total;
        self.bar.set_length(total as u64);
    }

    /// 打印内容，自动处理状态栏
    pub fn println(&self, content: &str) {
        if self.quiet {
            return;
        }
        if std::env::var("NO_COLOR").is_ok() {
            self.bar
                .println(anstream::adapter::strip_str(content).to_string());
        } else {
            self.bar.println(content);
        }
    }

    /// 开始一个任务，返回任务guard
    pub fn task(&self, name: &str, description: &str) -> TaskGuard<'_> {
        self.begin_task(name, description);
        TaskGuard { _bar: self }
    }

    /// 内部：开始任务
    fn begin_task(&self, name: &str, description: &str) {
        if self.quiet {
            return;
        }

        let mut state = self.state.borrow_mut();
        state.current += 1;

        // 统一宽度 12，右对齐（模仿 Cargo）
        let start_msg = format!("{:>12} {}", name.green().bold(), description);
        if std::env::var("NO_COLOR").is_ok() {
            self.bar
                .println(anstream::adapter::strip_str(&start_msg).to_string());
        } else {
            self.bar.println(start_msg);
        }

        // 设置状态栏消息，进度数字放在对齐文字后面
        let status_line = format!(
            "{:>12} {} [{}/{}]",
            name.green().bold(),
            description,
            state.current,
            state.total
        );

        if std::env::var("NO_COLOR").is_ok() {
            self.bar
                .set_message(anstream::adapter::strip_str(&status_line).to_string());
        } else {
            self.bar.set_message(status_line);
        }
        self.bar.set_position(state.current as u64);
    }

    /// 完成并显示最终信息（清除进度条后打印，避免模板前缀干扰对齐）
    pub fn finish_with_message(&self, msg: &str) {
        if !self.quiet {
            self.bar.finish_and_clear();
            println!("{}", msg);
        }
    }
}

/// 任务 guard
pub struct TaskGuard<'a> {
    _bar: &'a StatusBar,
}

impl<'a> Drop for TaskGuard<'a> {
    fn drop(&mut self) {}
}
