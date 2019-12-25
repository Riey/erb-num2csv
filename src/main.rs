use erb_num2csv::{CsvInfo, convert_erb, convert};
use glob::MatchOptions;
use anyhow::Result;
use std::path::Path;

fn main() -> Result<()> {
    env_logger::init();
    convert(Path::new("."))
}
