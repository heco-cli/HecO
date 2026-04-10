use clap::Parser;

#[derive(Parser, Debug)]
pub struct RunArgs {
    /// 模块名称，支持两种格式：
    /// - entry: 简单模块名
    /// - entry@target: 带目标配置的模块
    #[arg(short, long)]
    pub module: Option<String>,
    /// 设备名称
    #[arg(short, long)]
    pub device: Option<String>,
    /// 静默模式，仅输出必要信息
    #[arg(short, long)]
    pub quiet: bool,
}

pub(crate) fn handle_run(args: RunArgs) {
    let quiet = args.quiet;

    if !quiet {
        match &args.module {
            Some(module) => {
                println!("运行模块：{}", module);
            }
            None => {
                println!("运行默认模块");
            }
        }
    }

    if !quiet {
        match &args.device {
            Some(device) => {
                println!("在设备 '{}' 上运行", device);
            }
            None => {
                println!("未指定设备，请在项目中连接设备后运行");
            }
        }
    }
}
