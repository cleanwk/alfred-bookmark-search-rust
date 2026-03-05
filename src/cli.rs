use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "alfred-chrome-bookmarks")]
pub struct Opt {
    #[command(subcommand)]
    pub cmd: SubCommand,
}

#[derive(Parser, Debug)]
pub enum SubCommand {
    /// Search bookmarks
    #[command(alias = "s")]
    Search {
        /// Search keywords
        query: Vec<String>,

        /// Filter by folder (comma-separated, supports hierarchical paths like work/project)
        #[arg(short = 'p', long = "folders")]
        folders: Option<String>,

        /// Use fuzzy search (slower)
        #[arg(short = 'f', long = "fuzzy")]
        fuzzy: bool,

        /// Limit number of results
        #[arg(short = 'l', long = "limit", default_value = "50")]
        limit: usize,
    },

    /// Refresh browser bookmark cache and index
    #[command(alias = "rf")]
    Refresh,

    /// Show statistics
    #[command(alias = "st")]
    Stats,

    /// Show workflow action list
    #[command(alias = "a")]
    Actions {
        /// Action filter keywords
        query: Vec<String>,
    },
}
