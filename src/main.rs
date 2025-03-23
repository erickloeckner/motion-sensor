use std::fs;
use std::process::Command;
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use gpio_cdev::{Chip, EventRequestFlags, LineRequestFlags};
use gpio_cdev::EventType::{FallingEdge, RisingEdge};
use serde::Deserialize;

#[derive(Debug, PartialEq, Deserialize)]
struct Config {
    debug: bool,
    chip: String,
    gpio_pin: u32,
    active_state: u8,
    micros_per_loop: u32,
    debounce: bool,
    debounce_micros: u32,
    hold_micros: u32,
    cooldown_micros: u32,
    retrigger: bool,
    on_action: String,
    off_action: String,
}

enum MainState {
    Off,
    Debounce,
    On,
    Cooldown,
}

fn main() -> std::result::Result<(), gpio_cdev::Error> {
    let config_raw = fs::read_to_string("./config.yaml").expect("Unable to open configuration file");
    let config: Config = serde_yaml::from_str(&config_raw).expect("Unable to parse configuration file");

    let mut chip = Chip::new(&config.chip).expect("Unable to open GPIO device");
    let gpio = chip.get_line(config.gpio_pin).expect("Unable to get GPIO line");

    let mut state = MainState::Off;
    let mut last_active_time = Instant::now();
    let mut cooldown_start = Instant::now();

    let mut on_action = Command::new("sh");
    on_action.arg("-c")
        .arg(&config.on_action);

    let mut off_action = Command::new("sh");
    off_action.arg("-c")
        .arg(&config.off_action);

    let active_low = match config.active_state {
        0 => true,
        _ => false,
    };
    
    let (tx, rx): (Sender<bool>, Receiver<bool>) = channel();

    thread::spawn(move || {
        loop {
            match state {
                MainState::Off => {
                    if let Ok(gpio) = rx.try_recv() {
                        //println!("message: {}", gpio);
                        if gpio == true {
                            last_active_time = Instant::now();
                            if config.debounce == true {
                                state = MainState::Debounce;
                                if config.debug { println!("state off -> debounce") }
                            } else {
                                state = MainState::On;
                                match on_action.output() {
                                    Ok(_) => (),
                                    Err(e) => {
                                        if config.debug { println!("{}", e) }
                                    }
                                }
                                if config.debug { println!("state off -> on") }
                            }
                        }
                    }
                }
                MainState::Debounce => {
                    let last = last_active_time.elapsed().as_micros();
                    if let Ok(gpio) = rx.try_recv() {
                        //println!("message: {}", gpio);
                        if gpio == true {
                            state = MainState::On;
                            last_active_time = Instant::now();
                            match on_action.output() {
                                Ok(_) => (),
                                Err(e) => {
                                    if config.debug { println!("{}", e) }
                                }
                            }
                            if config.debug { println!("state debounce -> on") }
                        }
                    } else {
                        if last >= config.debounce_micros as u128 {
                            state = MainState::Off;
                            if config.debug { println!("state debounce -> off") }
                        }
                    }

                }
                MainState::On => {
                    let last = last_active_time.elapsed().as_micros();
                    if let Ok(gpio) = rx.try_recv() {
                        if gpio == true && config.retrigger == true {
                            last_active_time = Instant::now();
                        }
                    } else {
                        if last >= config.hold_micros as u128 {
                            state = MainState::Cooldown;
                            cooldown_start = Instant::now();
                            match off_action.output() {
                                Ok(_) => (),
                                Err(e) => {
                                    if config.debug { println!("{}", e) }
                                }
                            }
                            if config.debug { println!("state on -> cooldown") }
                        }
                    }
                }
                MainState::Cooldown => {
                    let start = cooldown_start.elapsed().as_micros();
                    if let Ok(_) = rx.try_recv() { }
                    if start >= config.cooldown_micros as u128 {
                        state = MainState::Off;
                        if config.debug { println!("state cooldown -> off") }
                    }
                }
            }
            
            thread::sleep(Duration::from_micros(config.micros_per_loop as u64));
        }
    });

    for e in gpio.events(
        LineRequestFlags::INPUT,
        EventRequestFlags::BOTH_EDGES,
        "rust-gpio"
    )? {
        let event = e?;
        match event.event_type() {
            RisingEdge => {
                if active_low == false {
                    let _ = tx.send(true);
                } else {
                    let _ = tx.send(false);
                }
            }
            FallingEdge => {
                if active_low == false {
                    let _ = tx.send(false);
                } else {
                    let _ = tx.send(true);
                }
            }
        }
    }

    Ok(())
}
