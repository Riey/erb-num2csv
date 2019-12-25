use anyhow::Result;
use conquer_once::Lazy;
use glob::MatchOptions;
use regex::{Captures, Regex, Replacer};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{PathBuf, Path};

fn is_need_csv(name: &str) -> bool {
    match name {
        "ABL" | "BASE" | "EX" | "EXP" | "JUEL" | "MARK" | "NOWEX" | "PALAM" | "SOURCE"
        | "STAIN" | "STR" | "TALENT" | "TCVAR" => true,
        _ => false,
    }
}

fn parse_csv(path: &PathBuf) -> Result<HashMap<u32, String>> {
    let mut ret = HashMap::new();

    let file = std::fs::File::open(path)?;

    let mut file = BufReader::with_capacity(8196, file);
    let mut buf = String::with_capacity(1024);

    loop {
        let len = file.read_line(&mut buf)?;
        if len == 0 {
            break;
        }
        let line = &buf[..len];
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        let at = line.find(',').unwrap();
        let (num, left) = line.split_at(at);
        let (name, _) = match left.find(';') {
            Some(semi) => left.split_at(semi),
            None => (left, ""),
        };

        ret.insert(num.parse()?, name.into());
        buf.clear();
    }

    Ok(ret)
}

pub struct CsvInfo {
    dic: HashMap<String, HashMap<u32, String>>,
}

impl CsvInfo {
    pub fn new(path: &Path) -> Result<Self> {
        let mut dic = HashMap::new();
        for csv in glob::glob_with(
            &(path.to_str().unwrap().to_string() + "csv/*.csv"),
            MatchOptions {
                case_sensitive: false,
                ..Default::default()
            },
        )? {
            let csv = csv?;
            let name = csv.file_stem().unwrap().to_str().unwrap().to_uppercase();
            log::debug!("Read csv {}, name: {}", csv.display(), name);

            if is_need_csv(&name) {
                log::debug!("{} Is needed csv", name);
                dic.insert(name, parse_csv(&csv)?);
            }
        }

        Ok(Self { dic })
    }
}

impl<'a> Replacer for &'a CsvInfo {
    fn replace_append(&mut self, caps: &Captures, dst: &mut String) {
        let all = caps.get(0).unwrap().as_str();
        let var = caps.get(1).unwrap();
        match self.dic.get(var.as_str()) {
            Some(dic) => {
                let idx = caps.get(2).unwrap();
                dst.push_str(&all[..idx.start()]);
                let idx = idx.as_str();
                dst.push_str(
                    dic.get(&idx.parse().unwrap())
                        .map(|v| v.as_str())
                        .unwrap_or(idx),
                );
            }
            None => {
                dst.push_str(all);
            }
        }
    }
}

static VAR_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new("([^:]+):[^:]+:(\\d+)").unwrap());

pub fn convert_erb(path: &Path, csv: &CsvInfo) -> Result<()> {
    let erb = std::fs::read_to_string(path)?;

    let ret = VAR_REGEX.replace_all(&erb, csv);

    std::fs::write(path, ret.as_ref())?;

    Ok(())
}

pub fn convert(path: &Path) -> Result<()> {
    let csv = CsvInfo::new(path)?;

    for erb in glob::glob_with(
        "erb/*.erb",
        MatchOptions {
            case_sensitive: false,
            ..Default::default()
        },
    )? {
        let erb = erb?;
        convert_erb(&erb, &csv)?;
    }

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
                "TALENT".into(),
                vec![(0, "처녀".into())].into_iter().collect(),
            ),
        ]
            .into_iter()
            .collect(),
    };

    assert_eq!(VAR_REGEX.replace_all("ABL:TARGET:0", &csv), "ABL:TARGET:C감각");
    assert_eq!(VAR_REGEX.replace_all("ABL:TARGET:01", &csv), "ABL:TARGET:V감각");
    assert_eq!(VAR_REGEX.replace_all("TALENT:2:0", &csv), "TALENT:2:처녀");
}
