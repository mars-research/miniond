#![deny(
    private_in_public,
    unused_imports,
    unused_must_use,
    unreachable_patterns,
)]

mod applet;
mod account;
mod config;
mod error;
mod geni;
mod mount;
mod tmcc;

use std::env;
use std::error::Error;
use std::path::PathBuf;

use clap::Clap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    init_logging();

    log::info!("miniond {} starting", env!("CARGO_PKG_VERSION"));

    let opts = Opts::parse();

    if opts.config.is_none() {
        log::warn!("It's strongly recommended to explicitly set a configuration file with `--config`.");
        log::warn!("See <https://github.com/mars-research/miniond> for available options.");
    }

    let config = config::get_config(opts.config);
    applet::run(config).await.unwrap();

    Ok(())
}

fn init_logging() {
    if env::var("RUST_LOG").is_err() {
        // HACK
        env::set_var("RUST_LOG", "info");
    }

    env_logger::builder()
        .format_module_path(false)
        .format_target(false)
        .init();
}

/// Alternative implementation of Emulab Clientside.
#[derive(Debug, Clap)]
#[clap(version = "0.1.0", author = "Zhaofeng Li <hello@zhaofeng.li>")]
struct Opts {
    /// Path to the config file.
    #[clap(short = 'f', long, global = true)]
    config: Option<PathBuf>,
}
