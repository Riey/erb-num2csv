use anyhow::Result;
use conquer_once::Lazy;
use glob::MatchOptions;
use rayon::prelude::*;
use regex::{Captures, Regex, Replacer};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(
    name = "erb-num2csv",
    about = "Convert erb variable number to csv name"
)]
pub struct Opt {
    #[structopt(short)]
    includes: Vec<String>,
    #[structopt(short)]
    excludes: Vec<String>,
    //#[structopt(long)]
    //regex_path: Option<PathBuf>,
    #[structopt(short)]
    target: PathBuf,
    //#[structopt(long)]
    //normalize: bool,
}

fn is_need_csv(name: &str, opt: &Opt) -> bool {
    if opt.includes.iter().any(|n| n == name) {
        return true;
    }

    if opt.excludes.iter().any(|n| n == name) {
        return false;
    }

    match name {
        "ABL" | "BASE" | "EX" | "EXP" | "JUEL" | "MARK" | "SOURCE" | "STAIN" | "TALENT"
        | "TCVAR" | "STR" => true,
        _ => false,
    }
}

const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

fn check_bom(r: &mut impl Read) -> Result<bool> {
    let mut buf = [0u8; BOM.len()];
    r.read_exact(&mut buf)?;
    Ok(buf == BOM)
}

fn list_files(path: &Path) -> Vec<PathBuf> {
    glob::glob_with(
        path.to_str().unwrap(),
        MatchOptions {
            case_sensitive: false,
            ..Default::default()
        },
    )
    .unwrap()
    .filter_map(Result::ok)
    .collect()
}

fn parse_csv(path: &PathBuf) -> Result<HashMap<u32, String>> {
    let mut ret = HashMap::new();

    let file = std::fs::File::open(path)?;

    let mut file = BufReader::with_capacity(8196, file);
    if !check_bom(&mut file)? {
        log::warn!("Can't find BOM in {} skip it", path.display());
        return Ok(ret);
    }
    let mut buf = String::with_capacity(1024);

    loop {
        let len = file.read_line(&mut buf)?;
        if len == 0 {
            buf.clear();
            break;
        }
        let mut line = buf.trim();
        log::debug!("line: [{}]", line);

        match line.find(';') {
            Some(comment) => line = line.split_at(comment).0,
            _ => {}
        }

        if line.is_empty() {
            buf.clear();
            continue;
        }

        let at = line.find(',').unwrap();
        log::debug!("line: [{}]", line);
        let (num, name) = line.split_at(at);
        log::debug!("num: [{}]", num);

        let mut name = &name[1..];

        match name.find(",") {
            Some(pos) => name = &name[..pos],
            _ => {}
        }

        ret.insert(num.parse()?, name.into());
        buf.clear();
    }

    Ok(ret)
}

pub struct CsvInfo {
    dic: HashMap<String, HashMap<u32, String>>,
}

impl CsvInfo {
    pub fn new(opt: &Opt) -> Result<Self> {
        let mut path = opt.target.join("CSV");
        path.push("*.CSV");
        let files = list_files(&path);

        let dic = files
            .into_par_iter()
            .filter_map(|csv| {
                let name = csv.file_stem().unwrap().to_str().unwrap().to_uppercase();

                if !is_need_csv(&name, opt) {
                    None
                } else {
                    parse_csv(&csv).ok().map(|info| (name, info))
                }
            })
            .collect();

        Ok(Self { dic })
    }
}

impl<'a> Replacer for &'a CsvInfo {
    fn replace_append(&mut self, caps: &Captures, dst: &mut String) {
        let all = caps.get(0).unwrap();
        let start = all.start();
        let all = all.as_str();
        let var = caps.get(1).unwrap();
        match self.dic.get(match var.as_str() {
            "PALAM" | "UP" | "DOWN" => "JUEL",
            "NOWEX" => "EX",
            "UPBASE" | "DOWNBASE" => "BASE",
            var if var.ends_with("NAME") => var.split_at(var.len() - 4).0,
            var => var,
        }) {
            Some(dic) => {
                let idx = caps.get(3).unwrap();
                dst.push_str(&all[..idx.start() - start]);
                let idx = idx.as_str();
                dst.push_str(
                    dic.get(&idx.parse().unwrap())
                        .map(|v| v.as_str())
                        .unwrap_or(idx),
                );
                log::debug!("all: {}, var: [{}]", all, var.as_str());
            }
            None => {
                dst.push_str(all);
            }
        }
    }
}

static VAR_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new("([^(){\\[%: \\n]+)(:[^ (){\\n:]+)?:(\\d+)").unwrap());
static USELESS_NAME: Lazy<Regex> =
    Lazy::new(|| Regex::new("%(ABL|TALENT|EXP|MARK|PALAM)NAME:([^\\d]+?)%").unwrap());

pub fn convert_erb(path: &Path, csv: &CsvInfo) -> Result<()> {
    log::debug!("Start convert erb path: {}", path.display());
    let mut file = BufReader::with_capacity(8196, File::open(path)?);

    if !check_bom(&mut file)? {
        log::warn!("Can't find BOM in {} skip it", path.display());
        return Ok(());
    }
    let erb = std::fs::read_to_string(path)?;

    let ret = VAR_REGEX.replace_all(&erb, csv);
    let ret = USELESS_NAME.replace_all(ret.as_ref(), "$2");

    let mut file = BufWriter::with_capacity(8196, File::create(path)?);
    file.write_all(&BOM)?;
    file.write_all(ret.as_bytes())?;
    Ok(())
}

pub fn convert(opt: &Opt) -> Result<()> {
    log::debug!("Start in {:?}", opt.target);
    let csv = CsvInfo::new(opt)?;
    let mut erb_path = opt.target.join("ERB");
    erb_path.push("**");
    erb_path.push("*.ERB");

    let erb_files = list_files(&erb_path);

    erb_files.into_par_iter().for_each_with(&csv, |csv, erb| {
        if let Err(err) = convert_erb(&erb, &csv) {
            log::error!("convert erb {} failed: {:?}", erb.display(), err);
        }
    });

    Ok(())
}

#[test]
fn replace() {
    let csv = CsvInfo {
        dic: vec![
            (
                "ABL".into(),
                vec![(0, "C감각".into()), (1, "V감각".into())]
                    .into_iter()
                    .collect(),
            ),
            (
                "BASE".into(),
                vec![(0, "체력".into()), (1, "기력".into())]
                    .into_iter()
                    .collect(),
            ),
            (
                "TALENT".into(),
                vec![(0, "처녀".into())].into_iter().collect(),
            ),
        ]
        .into_iter()
        .collect(),
    };

    assert_eq!(
        VAR_REGEX.replace_all("ABL:TARGET:0", &csv),
        "ABL:TARGET:C감각"
    );
    assert_eq!(
        VAR_REGEX.replace_all("@BASERATIO(ARG, ARG:1, ARG:2)", &csv),
        "@BASERATIO(ARG, ARG:1, ARG:2)"
    );
    assert_eq!(
        VAR_REGEX.replace_all("ABL:TARGET:01", &csv),
        "ABL:TARGET:V감각"
    );
    assert_eq!(VAR_REGEX.replace_all("TALENT:2:0", &csv), "TALENT:2:처녀");
}
