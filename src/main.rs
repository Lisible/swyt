use log::info;
use std::time::Duration;
use swyt::{find_swyt_filepath, load_config, load_rules, process_rules, SwytError};

fn main() -> Result<(), SwytError> {
    env_logger::init();

    info!("Swyt is starting...");
    let swyt_filepath = find_swyt_filepath()?;
    if !swyt_filepath.exists() {
        info!(
            "Swyt configuration directory doesn't exist, creating: {}",
            swyt_filepath
                .to_str()
                .expect("Couldn't convert swyt filepath to str")
        );
        std::fs::create_dir(&swyt_filepath)?;
    }
    let configuration = load_config(&swyt_filepath)?;
    let rules = load_rules(&swyt_filepath)?;

    loop {
        process_rules(&rules)?;
        std::thread::sleep(Duration::from_secs(configuration.check_interval() as u64))
    }
}
