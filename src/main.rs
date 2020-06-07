use log::{error, info};
use std::time::Duration;
use swyt::{find_swyt_filepath, load_config, load_rules, process_rules, SwytError};

macro_rules! fatal {
    ($($tt:tt)*) => {{
        error!($($tt)*);
        ::std::process::exit(1)
    }}
}

fn main() -> Result<(), SwytError> {
    env_logger::init();

    info!("Swyt is starting...");
    let swyt_filepath = match find_swyt_filepath() {
        Ok(filepath) => filepath,
        Err(err) => fatal!("{}", err),
    };

    if !swyt_filepath.exists() {
        info!(
            "Swyt configuration directory doesn't exist, creating: {}",
            swyt_filepath
                .to_str()
                .expect("Couldn't convert swyt filepath to str")
        );

        if let Err(err) = std::fs::create_dir(&swyt_filepath) {
            fatal!("{}", err);
        }
    }
    let configuration = match load_config(&swyt_filepath) {
        Ok(config) => config,
        Err(err) => fatal!("{}", err),
    };

    let rules = match load_rules(&swyt_filepath) {
        Ok(rules) => rules,
        Err(err) => fatal!("{}", err),
    };

    loop {
        if let Err(err) = process_rules(&rules) {
            fatal!("{}", err);
        }
        std::thread::sleep(Duration::from_secs(configuration.check_interval() as u64))
    }
}
