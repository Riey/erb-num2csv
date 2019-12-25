use anyhow::Result;
use erb_num2csv::convert;
use std::env::args;
use std::path::Path;

fn main() -> Result<()> {
    env_logger::init();
    convert(Path::new(&args().skip(1).next().unwrap()))
}
