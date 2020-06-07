use chrono::prelude::*;
use futures::StreamExt;
use log::{info, trace};
use std::collections::{HashMap, HashSet};
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
        .map(parse_period)
        .collect::<Result<_, SwytError>>()?;

    Ok(Rule {
        process_name,
        allowed_periods,
    })
}

fn parse_period(period: &str) -> Result<Period, SwytError> {
    let mut split_period = period.split(";");
    let period_time = split_period.next().ok_or(SwytError::RuleParseError)?;
    let period_days_of_week = split_period.next().ok_or(SwytError::RuleParseError)?;

    let (begin_time, end_time) = parse_period_time(period_time)?;
    let days_of_week = parse_days_of_week(period_days_of_week)?;

    Ok(Period {
        days_of_week,
        begin_time,
        end_time,
    })
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
