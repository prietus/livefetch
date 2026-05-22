mod cli;
mod config;
mod image;
mod info;
mod layout;
mod terminal;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let args = cli::Args::parse();
    let cfg = config::load(args.config.as_deref())?.merge_cli(&args);

    let info_blocks = info::collect(&cfg);

    let image = match cfg.image_path.as_deref() {
        Some(path) => {
            let mut asset = image::load(path)?;
            image::chroma::apply(&mut asset, cfg.chroma, cfg.chroma_tolerance);
            Some(asset)
        }
        None => None,
    };

    let proto = terminal::detect_protocol(&cfg);
    layout::run(cfg, info_blocks, image, proto)
}
