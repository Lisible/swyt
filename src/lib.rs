use chrono::prelude::*;
use futures::StreamExt;
use log::{info, trace};
use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Display, Formatter};
use std::fs::File;
use std::io::{BufRead, BufReader, Error};
use std::path::PathBuf;
use std::str::FromStr;

const SWYT_DIRECTORY_NAME: &'static str = "swyt";
const CONFIG_FILE_NAME: &'static str = "config.jbb";
const RULES_FILE_NAME: &'static str = "rules.jbb";

const DEFAULT_CHECK_INTERVAL: u32 = 60;

type Rules = HashMap<String, Vec<Period>>;

pub struct Rule {
    process_name: String,
    allowed_periods: Vec<Period>,
}

#[derive(Debug, Clone)]
pub struct Period {
    days_of_week: HashSet<Weekday>,
    begin_time: NaiveTime,
    end_time: NaiveTime,
}

pub struct Configuration {
    check_interval: u32,
}

impl Configuration {
    pub fn check_interval(&self) -> u32 {
        self.check_interval
    }
}

impl Default for Configuration {
    fn default() -> Self {
        Configuration {
            check_interval: DEFAULT_CHECK_INTERVAL,
        }
    }
}

#[derive(Debug)]
pub enum SwytError {
    ConfigFileNotFound,
    ConfigParseError,
    RuleParseError,
    ProcessFetchError,
    ProcessKillError,
    IoError(std::io::Error),
}

impl Display for SwytError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match *self {
            SwytError::ConfigFileNotFound => write!(f, "Couldn't find config file"),
            SwytError::ConfigParseError => write!(f, "Couldn't parse config file"),
            SwytError::RuleParseError => write!(f, "Couldn't parse rule"),
            SwytError::ProcessFetchError => write!(f, "Couldn't fetch process"),
            SwytError::ProcessKillError => write!(f, "Couldn't kill process"),
            SwytError::IoError(ref err) => std::fmt::Display::fmt(err, f),
        }
    }
}

impl From<std::io::Error> for SwytError {
    fn from(io_error: Error) -> Self {
        SwytError::IoError(io_error)
    }
}

pub fn process_rules(rules: &Rules) -> Result<(), SwytError> {
    trace!("Process rules...");
    let current_date_time = Local::now();
    let mut processes = heim::process::processes();
    while let Ok(process_result) =
        futures::executor::block_on(processes.next()).ok_or(SwytError::ProcessFetchError)
    {
        if let Ok(process) = process_result {
            let process_name = futures::executor::block_on(process.name())
                .map_err(|_| SwytError::ProcessFetchError)?;
            if let Some(periods) = rules.get(&process_name) {
                if !periods.iter().any(|p| {
                    p.days_of_week.contains(&current_date_time.date().weekday())
                        && current_date_time.time() >= p.begin_time
                        && current_date_time.time() <= p.end_time
                }) {
                    trace!("Killed process {}", process_name);
                    let _ = futures::executor::block_on(process.kill())
                        .map_err(|_| SwytError::ProcessKillError);
                }
            }
        }
    }

    Ok(())
}

pub fn load_rules(swyt_filepath: &PathBuf) -> Result<Rules, SwytError> {
    let rules_filepath = get_rules_filepath(swyt_filepath)?;
    parse_rules_file(rules_filepath)
}

pub fn load_config(swyt_filepath: &PathBuf) -> Result<Configuration, SwytError> {
    let config_filepath = get_config_filepath(swyt_filepath)?;
    parse_config_file(config_filepath)
}

fn get_config_filepath(swyt_filepath: &PathBuf) -> Result<PathBuf, SwytError> {
    let mut config_directory = swyt_filepath.clone();
    config_directory.push(CONFIG_FILE_NAME);
    Ok(config_directory)
}

fn get_rules_filepath(swyt_filepath: &PathBuf) -> Result<PathBuf, SwytError> {
    let mut rules_filepath = swyt_filepath.clone();
    rules_filepath.push(RULES_FILE_NAME);
    Ok(rules_filepath)
}

pub fn find_swyt_filepath() -> Result<PathBuf, SwytError> {
    let mut config_directory = dirs::config_dir().ok_or(SwytError::ConfigFileNotFound)?;
    config_directory.push(SWYT_DIRECTORY_NAME);
    Ok(config_directory)
}

fn parse_rules_file(rules_filepath: PathBuf) -> Result<Rules, SwytError> {
    if !rules_filepath.exists() {
        info!(
            "Rules file doesn't exist, creating: {}",
            &rules_filepath
                .to_str()
                .expect("Couldn't convert rules filepath to str")
        );
        File::create(&rules_filepath)?;
        return Ok(Rules::new());
    }

    let mut rules = Rules::new();
    let rules_file = File::open(&rules_filepath)?;
    let reader = BufReader::new(rules_file);
    for line in reader.lines() {
        let rule = parse_rule(&line?)?;
        rules.insert(rule.process_name, rule.allowed_periods);
    }

    Ok(rules)
}

fn parse_rule(rule: &str) -> Result<Rule, SwytError> {
    let mut split_rule = rule.split("=");
    let process_name = split_rule
        .next()
        .ok_or(SwytError::RuleParseError)?
        .to_string();
    let periods_string = split_rule.next().ok_or(SwytError::RuleParseError)?;

    let allowed_periods: Vec<Period> = periods_string
        .split("|")
        .map(parse_periods)
        .collect::<Result<Vec<Vec<Period>>, SwytError>>()?
        .iter()
        .flatten()
        .map(|p| p.clone())
        .collect();
    Ok(Rule {
        process_name,
        allowed_periods,
    })
}

fn parse_periods(period: &str) -> Result<Vec<Period>, SwytError> {
    let mut split_period = period.split(";");
    let period_time = split_period.next().ok_or(SwytError::RuleParseError)?;
    let period_days_of_week = split_period.next().ok_or(SwytError::RuleParseError)?;
    let start_ends = parse_period_times(period_time)?;
    let days_of_week = parse_days_of_week(period_days_of_week)?;

    Ok(start_ends
        .iter()
        .map(|&(begin_time, end_time)| Period {
            days_of_week: days_of_week.clone(),
            begin_time,
            end_time,
        })
        .collect())
}

fn parse_period_times(period_times: &str) -> Result<Vec<(NaiveTime, NaiveTime)>, SwytError> {
    Ok(period_times
        .split(",")
        .map(parse_period_time)
        .collect::<Result<_, SwytError>>()?)
}

fn parse_period_time(period_time: &str) -> Result<(NaiveTime, NaiveTime), SwytError> {
    match period_time {
        "*" => Ok((
            NaiveTime::from_hms(0, 0, 0),
            NaiveTime::from_hms(23, 59, 59),
        )),
        _ => {
            let mut split_time = period_time.split("~");
            let begin_time = parse_time(split_time.next().ok_or(SwytError::RuleParseError)?)?;
            let end_time = parse_time(split_time.next().ok_or(SwytError::RuleParseError)?)?;
            Ok((begin_time, end_time))
        }
    }
}

fn parse_time(time: &str) -> Result<NaiveTime, SwytError> {
    let mut split_time = time.split(":");
    let hours = u32::from_str(split_time.next().ok_or(SwytError::RuleParseError)?)
        .map_err(|_| SwytError::RuleParseError)?;
    let minutes = u32::from_str(split_time.next().ok_or(SwytError::RuleParseError)?)
        .map_err(|_| SwytError::RuleParseError)?;

    Ok(NaiveTime::from_hms(hours, minutes, 0))
}

fn parse_days_of_week(days_of_week: &str) -> Result<HashSet<Weekday>, SwytError> {
    Ok(days_of_week
        .split(",")
        .map(parse_day_of_week)
        .collect::<Result<HashSet<Weekday>, SwytError>>()?)
}

fn parse_day_of_week(day_of_week: &str) -> Result<Weekday, SwytError> {
    Ok(match day_of_week {
        "MO" => Weekday::Mon,
        "TU" => Weekday::Tue,
        "WE" => Weekday::Wed,
        "TH" => Weekday::Thu,
        "FR" => Weekday::Fri,
        "SA" => Weekday::Sat,
        "SU" => Weekday::Sun,
        _ => return Err(SwytError::RuleParseError),
    })
}

fn parse_config_file(config_filepath: PathBuf) -> Result<Configuration, SwytError> {
    if !config_filepath.exists() {
        info!(
            "Configuration file doesn't exist, creating: {}",
            &config_filepath
                .to_str()
                .expect("Couldn't convert swyt config filepath to str")
        );
        File::create(&config_filepath)?;
        return Ok(Configuration::default());
    }

    let mut config = Configuration::default();
    let config_file = File::open(&config_filepath)?;
    let reader = BufReader::new(config_file);
    for line in reader.lines() {
        parse_config_line(line?, &mut config)?;
    }

    Ok(config)
}

fn parse_config_line(line: String, config: &mut Configuration) -> Result<(), SwytError> {
    let mut split_line = line.split("=");
    let config_identifier = split_line.next().ok_or(SwytError::ConfigParseError)?.trim();
    let config_value = split_line.next().ok_or(SwytError::ConfigParseError)?.trim();

    match config_identifier {
        "check_interval" => {
            let value = u32::from_str(&config_value).unwrap_or(DEFAULT_CHECK_INTERVAL);
            config.check_interval = value
        }
        _ => (),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_CONFIG_SWYT_PATH: &'static str = "./test_data/valid_config";
    const MISSING_VALUE_CONFIG_SWYT_PATH: &'static str = "./test_data/missing_value_config";
    const INVALID_CONFIG_SWYT_PATH: &'static str = "./test_data/invalid_config";
    const VALID_RULES_SWYT_PATH: &'static str = "./test_data/valid_rules";
    const NO_RULE_SWYT_PATH: &'static str = "./test_data/no_rule";
    const INVALID_RULES_SWYT_PATH: &'static str = "./test_data/invalid_rules";

    #[test]
    pub fn load_config_valid() {
        let config = load_config(&VALID_CONFIG_SWYT_PATH.into()).unwrap();
        assert_eq!(config.check_interval(), 120);
    }

    #[test]
    pub fn load_config_missing_value() {
        let config = load_config(&MISSING_VALUE_CONFIG_SWYT_PATH.into()).unwrap();
        assert_eq!(config.check_interval(), 60);
    }

    #[test]
    pub fn load_config_bad_value() {
        let config = load_config(&INVALID_CONFIG_SWYT_PATH.into()).unwrap();
        assert_eq!(config.check_interval(), 60);
    }

    #[test]
    pub fn load_rules_valid() {
        let rules = load_rules(&VALID_RULES_SWYT_PATH.into()).unwrap();

        assert_eq!(rules.len(), 3);

        let process0_rules = rules.get("process0").unwrap();
        let process0_rule0 = process0_rules.get(0).unwrap();
        assert_eq!(process0_rule0.begin_time, NaiveTime::from_hms(18, 00, 00));
        assert_eq!(process0_rule0.end_time, NaiveTime::from_hms(20, 00, 00));
        assert!(process0_rule0.days_of_week.contains(&Weekday::Mon));
        assert!(process0_rule0.days_of_week.contains(&Weekday::Tue));
        assert!(process0_rule0.days_of_week.contains(&Weekday::Wed));

        let process0_rule1 = process0_rules.get(1).unwrap();
        assert_eq!(process0_rule1.begin_time, NaiveTime::from_hms(12, 00, 00));
        assert_eq!(process0_rule1.end_time, NaiveTime::from_hms(14, 00, 00));
        assert!(process0_rule1.days_of_week.contains(&Weekday::Thu));
        assert!(process0_rule1.days_of_week.contains(&Weekday::Fri));

        let process0_rule2 = process0_rules.get(2).unwrap();
        assert_eq!(process0_rule2.begin_time, NaiveTime::from_hms(00, 00, 00));
        assert_eq!(process0_rule2.end_time, NaiveTime::from_hms(23, 59, 59));
        assert!(process0_rule2.days_of_week.contains(&Weekday::Sat));
        assert!(process0_rule2.days_of_week.contains(&Weekday::Sun));

        let process1_rules = rules.get("process1").unwrap();
        let process1_rule0 = process1_rules.get(0).unwrap();
        assert_eq!(process1_rule0.begin_time, NaiveTime::from_hms(10, 00, 00));
        assert_eq!(process1_rule0.end_time, NaiveTime::from_hms(11, 00, 00));
        assert!(process1_rule0.days_of_week.contains(&Weekday::Mon));
        assert!(process1_rule0.days_of_week.contains(&Weekday::Tue));
        assert!(process1_rule0.days_of_week.contains(&Weekday::Wed));

        let process2_rules = rules.get("process2").unwrap();
        let process2_rule0 = process2_rules.get(0).unwrap();
        assert_eq!(process2_rule0.begin_time, NaiveTime::from_hms(12, 00, 00));
        assert_eq!(process2_rule0.end_time, NaiveTime::from_hms(15, 00, 00));
        assert!(process2_rule0.days_of_week.contains(&Weekday::Mon));
        assert!(process2_rule0.days_of_week.contains(&Weekday::Thu));
        assert!(process2_rule0.days_of_week.contains(&Weekday::Fri));
    }

    #[test]
    fn load_invalid_rules() {
        match load_rules(&INVALID_RULES_SWYT_PATH.into()) {
            Err(SwytError::RuleParseError) => assert!(true),
            _ => assert!(false),
        }
    }

    #[test]
    fn load_no_rule() {
        let rules = load_rules(&NO_RULE_SWYT_PATH.into()).unwrap();
        assert_eq!(rules.len(), 0);
    }
}
