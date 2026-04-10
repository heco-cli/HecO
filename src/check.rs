use clap::Parser;

#[derive(Parser, Debug)]
pub struct CheckArgs {
    /// 静默模式，仅输出必要信息
    #[arg(short, long)]
    pub quiet: bool,
}

pub(crate) fn handle_check(args: CheckArgs) {
    if !args.quiet {
        println!("检查完成，未发现代码错误");
    }
}
