use clap::Parser;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    /// Point to a specific namespace ('default' otherwise)
    #[clap(short, long)]
    pub namespace: Option<String>
}

impl Args {
    pub fn collect() -> Args {
        Args::parse()
    }
}