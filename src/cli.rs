use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "alfred-chrome-bookmarks")]
pub struct Opt {
    #[structopt(subcommand)]
    pub cmd: SubCommand,
}

#[derive(StructOpt, Debug)]
pub enum SubCommand {
    /// 搜索书签
    #[structopt(name = "search", alias = "s")]
    Search {
        /// 搜索关键词
        query: Vec<String>,

        /// 按目录过滤（逗号分隔，支持多级目录，如 work/project）
        #[structopt(short = "p", long = "folders")]
        folders: Option<String>,

        /// 使用模糊搜索（更慢）
        #[structopt(short = "f", long = "fuzzy")]
        fuzzy: bool,

        /// 限制结果数量
        #[structopt(short = "l", long = "limit", default_value = "50")]
        limit: usize,
    },

    /// 刷新浏览器书签缓存与索引
    #[structopt(name = "refresh", alias = "rf")]
    Refresh,

    /// 显示统计信息
    #[structopt(name = "stats", alias = "st")]
    Stats,
}
