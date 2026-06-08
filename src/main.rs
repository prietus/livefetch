mod cli;
mod config;
mod image;
mod info;
mod layout;
mod logo;
mod terminal;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let args = cli::Args::parse();

    if args.list_modules {
        let width = info::ALL_MODULES.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
        for (name, desc) in info::ALL_MODULES {
            println!("  {name:width$}  {desc}");
        }
        return Ok(());
    }

    if args.list_logos {
        for l in logo::ALL {
            println!("  {}", l.name);
        }
        return Ok(());
    }

    let cfg = config::load(args.config.as_deref())?.merge_cli(&args);

    let image = match cfg.image_path.as_deref() {
        Some(path) => {
            let mut asset = image::load(path)?;
            image::chroma::apply(&mut asset, cfg.chroma, cfg.chroma_tolerance);
            Some(asset)
        }
        None => None,
    };

    let ascii_logo = if image.is_none() {
        logo::lookup(&cfg.logo)
    } else {
        None
    };

    let proto = terminal::detect_protocol(&cfg);
    layout::run(cfg, image, ascii_logo, proto)
}
