use crate::adapters::hdc::list_targets;
use crate::config::Config;
use crate::output;
use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct DeviceArgs {
    #[command(subcommand)]
    pub command: DeviceCommands,
}

#[derive(Subcommand, Debug)]
pub enum DeviceCommands {
    /// List all active devices
    List(ListArgs),
}

#[derive(Args, Debug)]
pub struct ListArgs {}

pub fn handle_device(args: DeviceArgs) -> Result<()> {
    match args.command {
        DeviceCommands::List(list_args) => handle_list(list_args),
    }
}

fn handle_list(_args: ListArgs) -> Result<()> {
    let config = Config::load(None)?;
    let devices = list_targets(&config)?;

    if devices.is_empty() {
        output::stdout_line("No active devices found.");
    } else {
        for (name, target) in devices {
            output::stdout_line(format!("{} ({})", name, target));
        }
    }

    Ok(())
}
