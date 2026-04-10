use clap::Parser;

#[derive(Parser, Debug)]
pub struct TestArgs {
    /// 模块名称，支持两种格式：
    /// - entry: 简单模块名
    /// - entry@target: 带目标配置的模块
    #[arg(short, long)]
    pub module: Option<String>,
    /// 静默模式，仅输出必要信息
    #[arg(short, long)]
    pub quiet: bool,
}

pub(crate) fn handle_test(args: TestArgs) {
    let quiet = args.quiet;

    if !quiet {
        match &args.module {
            Some(module) => {
                println!("测试模块：{}", module);
            }
            None => {
                println!("测试默认模块");
            }
        }
    }

    println!("测试完成");
}
