use anyhow::Result;
use erb_num2csv::{convert, Opt};
use structopt::StructOpt;

fn main() -> Result<()> {
    env_logger::init();
    convert(&Opt::from_args())
}
