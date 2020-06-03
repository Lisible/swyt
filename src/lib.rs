use chrono::prelude::*;
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
    let processes = match psutil::process::processes() {
        Ok(processes) => processes,
        Err(_) => return Err(SwytError::ProcessFetchError),
    };

    for process_results in processes.iter() {
        if let Ok(process) = process_results {
            let process_name = match process.name() {
                Ok(name) => name,
                Err(_) => return Err(SwytError::ProcessFetchError),
            };

            if let Some(periods) = rules.get(&process_name) {
                if !periods.iter().any(|p| {
                    p.days_of_week.contains(&current_date_time.date().weekday())
                        && current_date_time.time() >= p.begin_time
                        && current_date_time.time() <= p.end_time
                }) {
                    trace!("Killed process {}", process_name);
                    let _ = process.kill();
                }
            }
        }
    }

    Ok(())
}

pub fn load_rules() -> Result<Rules, SwytError> {
    let rules_filepath = find_rules_filepath()?;
    parse_rules_file(rules_filepath)
}

pub fn load_config() -> Result<Configuration, SwytError> {
    let config_filepath = find_config_filepath()?;
    parse_config_file(config_filepath)
}

fn find_config_filepath() -> Result<PathBuf, SwytError> {
    let mut config_directory = find_swyt_filepath()?;
    config_directory.push(CONFIG_FILE_NAME);
    Ok(config_directory)
}

fn find_rules_filepath() -> Result<PathBuf, SwytError> {
    let mut rules_filepath = find_swyt_filepath()?;
    rules_filepath.push(RULES_FILE_NAME);
    Ok(rules_filepath)
}

fn find_swyt_filepath() -> Result<PathBuf, SwytError> {
    let mut config_directory = match dirs::config_dir() {
        Some(directory) => directory,
        None => return Err(SwytError::ConfigFileNotFound),
    };

    config_directory.push(SWYT_DIRECTORY_NAME);
    Ok(config_directory)
}

fn parse_rules_file(rules_filepath: PathBuf) -> Result<Rules, SwytError> {
    let mut rules = Rules::new();
    let rules_file = File::open(rules_filepath)?;
    let reader = BufReader::new(rules_file);
    for line in reader.lines() {
        let rule = parse_rule(line?)?;
        rules.insert(rule.process_name, rule.allowed_periods);
    }

    Ok(rules)
}

fn parse_rule(rule: String) -> Result<Rule, SwytError> {
    let split_rule: Vec<&str> = rule.split("=").collect();
    let process_name = match split_rule.get(0) {
        Some(name) => name.to_string(),
        None => return Err(SwytError::RuleParseError),
    };
    let periods_string = match split_rule.get(1) {
        Some(periods) => periods.to_string(),
        None => return Err(SwytError::RuleParseError),
    };

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
    let split_period: Vec<&str> = period.split(";").collect();
    let period_time = match split_period.get(0) {
        Some(time) => time.to_string(),
        None => return Err(SwytError::RuleParseError),
    };
    let period_days_of_week = match split_period.get(1) {
        Some(days_of_week) => days_of_week.to_string(),
        None => return Err(SwytError::RuleParseError),
    };

    let (begin_time, end_time) = parse_period_time(period_time)?;
    let days_of_week = parse_days_of_week(period_days_of_week)?;

    Ok(Period {
        days_of_week,
        begin_time,
        end_time,
    })
}

fn parse_period_time(period_time: String) -> Result<(NaiveTime, NaiveTime), SwytError> {
    match period_time.as_str() {
        "*" => Ok((
            NaiveTime::from_hms(0, 0, 0),
            NaiveTime::from_hms(23, 59, 59),
        )),
        _ => {
            let split_time: Vec<&str> = period_time.split("~").collect();
            let begin_time = match split_time.get(0) {
                Some(time) => parse_time(time)?,
                None => return Err(SwytError::RuleParseError),
            };
            let end_time = match split_time.get(0) {
                Some(time) => parse_time(time)?,
                None => return Err(SwytError::RuleParseError),
            };

            Ok((begin_time, end_time))
        }
    }
}

fn parse_time(time: &str) -> Result<NaiveTime, SwytError> {
    let split_time: Vec<&str> = time.split(":").collect();
    let hours = match split_time.get(0) {
        Some(hours) => match u32::from_str(hours) {
            Ok(hours) => hours,
            Err(_) => return Err(SwytError::RuleParseError),
        },
        None => return Err(SwytError::RuleParseError),
    };
    let minutes = match split_time.get(1) {
        Some(minutes) => match u32::from_str(minutes) {
            Ok(minutes) => minutes,
            Err(_) => return Err(SwytError::RuleParseError),
        },
        None => return Err(SwytError::RuleParseError),
    };

    Ok(NaiveTime::from_hms(hours, minutes, 0))
}

fn parse_days_of_week(days_of_week: String) -> Result<HashSet<Weekday>, SwytError> {
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
    let mut config = Configuration::default();
    let config_file = File::open(config_filepath)?;
    let reader = BufReader::new(config_file);
    for line in reader.lines() {
        parse_config_line(line?, &mut config)?;
    }

    Ok(config)
}

fn parse_config_line(line: String, config: &mut Configuration) -> Result<(), SwytError> {
    let split_line: Vec<&str> = line.split("=").collect();
    let config_identifier = match split_line.get(0) {
        Some(identifier) => identifier.trim(),
        None => return Err(SwytError::ConfigParseError),
    };

    let config_value = match split_line.get(1) {
        Some(value) => value.trim(),
        None => return Err(SwytError::ConfigParseError),
    };

    match config_identifier {
        "check_interval" => {
            let value = u32::from_str(&config_value).unwrap_or(DEFAULT_CHECK_INTERVAL);
            config.check_interval = value
        }
        _ => (),
    }

    Ok(())
}
