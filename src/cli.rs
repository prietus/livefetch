use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "livefetch", version, about = "Animated fastfetch-style system info")]
pub struct Args {
    /// Path to a gif / webp / png / jpg to display.
    #[arg(short, long)]
    pub image: Option<PathBuf>,

    /// Force a specific terminal image protocol.
    #[arg(long, value_enum)]
    pub protocol: Option<ProtocolArg>,

    /// Number of columns reserved for the image.
    #[arg(long)]
    pub image_cols: Option<u16>,

    /// Disable animation — render only the first frame.
    #[arg(long)]
    pub no_animate: bool,

    /// Snapshot mode: print info + first frame and exit (like fastfetch).
    /// Without this flag, livefetch stays open and refreshes live metrics.
    #[arg(long)]
    pub once: bool,

    /// Refresh interval for live metrics, in milliseconds (default 500).
    #[arg(long, value_name = "MS")]
    pub refresh: Option<u64>,

    /// Path to a config file (JSON).
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Override the module list, comma separated (e.g. os,kernel,cpu,memory).
    #[arg(long, value_delimiter = ',')]
    pub modules: Option<Vec<String>>,

    /// Remove a solid background color from the image.
    /// Accepts: `none`, `auto` (sample image corners), `white`, `black`, or `#RRGGBB`.
    #[arg(long)]
    pub chroma: Option<String>,

    /// Per-channel tolerance (0-255) when matching the chroma color.
    #[arg(long)]
    pub chroma_tolerance: Option<u8>,
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum ProtocolArg {
    Auto,
    Kitty,
    Iterm2,
    Ansi,
    None,
}
