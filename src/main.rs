use log::info;
use std::time::Duration;
use swyt::{load_config, load_rules, process_rules, SwytError};

fn main() -> Result<(), SwytError> {
    env_logger::init();

    info!("Swyt is starting...");
    let configuration = load_config()?;
    let rules = load_rules()?;

    loop {
        process_rules(&rules)?;
        std::thread::sleep(Duration::from_secs(configuration.check_interval() as u64))
    }
}
