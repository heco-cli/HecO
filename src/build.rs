use clap::Parser;

#[derive(Parser, Debug)]
pub struct BuildArgs {
    /// 模块名称，支持两种格式：
    /// - entry: 简单模块名
    /// - entry@target: 带目标配置的模块
    #[arg(short, long)]
    pub module: Option<String>,
}

pub(crate) fn handle_build(args: BuildArgs) {
    match &args.module {
        Some(module) => {
            println!("构建模块: {}", module);
        }
        None => {
            println!("构建默认模块");
        }
    }
}
