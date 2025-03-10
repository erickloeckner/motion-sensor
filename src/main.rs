use std::fs;                                                                                                                                                                                                       
use std::process::Command;                                                                                                                                                                                         
use std::thread;                                                                                                                                                                                                   
use std::time::{Duration, Instant};                                                                                                                                                                                
                                                                                                                                                                                                                   
use gpio_cdev::{Chip, LineRequestFlags};                                                                                                                                                                           
use serde::Deserialize;

#[derive(Debug, PartialEq, Deserialize)]                                                                                                                                                                           
struct Config {                                                                                                                                                                                                    
    debug: bool,                                                                                                                                                                                                   
    chip: String,                                                                                                                                                                                                  
    gpio_pin: u32,                                                                                                                                                                                                 
    active_state: u8,                                                                                                                                                                                              
    micros_per_loop: u32,                                                                                                                                                                                          
    hold_micros: u32,                                                                                                                                                                                              
    cooldown_micros: u32,                                                                                                                                                                                          
    retrigger: bool,                                                                                                                                                                                               
    on_action: String,
    off_action: String,
}

enum MainState {                                                                                                                                                                                          [59/1899]
    Off,
    On,
    Cooldown,
}

fn main() {
    let config_raw = fs::read_to_string("./config.yaml").expect("Unable to open configuration file");
    let config: Config = serde_yaml::from_str(&config_raw).expect("Unable to parse configuration file");

    let mut chip = Chip::new(&config.chip).expect("Unable to open GPIO device");

    let gpio = chip.get_line(config.gpio_pin).expect("Unable to get GPIO line")
        .request(LineRequestFlags::INPUT, 0, "motion-sensor").expect("Unable to make GPIO line request"); 

    let mut state = MainState::Off;
    let mut last_active_time = Instant::now();

    let mut on_action = Command::new("sh");
    on_action.arg("-c")
        .arg(&config.on_action);

    let mut off_action = Command::new("sh");
    off_action.arg("-c")
        .arg(&config.off_action);

    loop {
        let loop_start = Instant::now();

        match state {
            MainState::Off => {
                if gpio.get_value().expect("Unable to get GPIO value") == config.active_state {
                    state = MainState::On;
                    last_active_time = Instant::now();
                    match on_action.output() {
                        Ok(_) => (),
                        Err(e) => {
                            if config.debug { println!("{}", e) }
                        }
                    }

                    if config.debug { println!("state off -> on") }
                }
            }
            MainState::On => {                                                                                                                                                                            [15/1899]
                let last = last_active_time
                    .elapsed()
                    .as_micros();
                if gpio.get_value().expect("Unable to get GPIO value") == config.active_state && config.retrigger == true {
                    last_active_time = Instant::now();
                }
                let last_active_elapsed: u32 = u32::try_from(last)
                    .unwrap_or(u32::MAX);
                if last_active_elapsed >= config.hold_micros {
                    state = MainState::Cooldown;
                    match off_action.output() {
                        Ok(_) => (),
                        Err(e) => {
                            if config.debug { println!("{}", e) }
                        }
                    }

                    if config.debug { println!("state on -> cooldown") }
                }
            }
            MainState::Cooldown => {
                let delay_time = config.cooldown_micros.saturating_sub(loop_start.elapsed().as_micros() as u32);
                thread::sleep(Duration::from_micros(delay_time.into()));
                state = MainState::Off;
                if config.debug { println!("state cooldown -> off") }
                continue;
            }
        }

        let delay_time = config.micros_per_loop.saturating_sub(loop_start.elapsed().as_micros() as u32);
        thread::sleep(Duration::from_micros(delay_time.into()));
    }
