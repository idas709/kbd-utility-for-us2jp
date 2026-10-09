use hid_remap_core::config::{AppConfig, LayoutMode};
use hid_remap_core::engine::{EngineAction, RemapEngine};
use hid_remap_core::keyboard::scancodes::*;
use hid_remap_core::keyboard::{KeyEvent, KeyFlags};
use hid_remap_device::get_keyboard_devices;
use hid_remap_ime::switch_ime;
use std::env;
use std::path::Path;

fn print_usage() {
    println!("HID Keyboard Layout Remapper Diagnostic CLI");
    println!();
    println!("USAGE:");
    println!("    hid-remap-cli <COMMAND> [OPTIONS]");
    println!();
    println!("COMMANDS:");
    println!("    list               Enumerate connected and registered keyboards");
    println!("    simulate           Simulate US -> JIS and JIS -> US key remapping");
    println!("    test-ime <on|off>  Test switching IME directly via IMM32/API");
    println!("    config [PATH]      Load and validate TOML configuration");
    println!();
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return;
    }

    match args[1].as_str() {
        "list" => cmd_list(),
        "simulate" => cmd_simulate(),
        "test-ime" => {
            if args.len() < 3 {
                println!("Error: specify 'on' or 'off'");
                return;
            }
            let on = match args[2].as_str() {
                "on" | "1" => true,
                "off" | "0" => false,
                _ => {
                    println!("Invalid argument: specify 'on' or 'off'");
                    return;
                }
            };
            let config = AppConfig::default();
            println!("Switching IME to {}...", if on { "ON" } else { "OFF" });
            switch_ime(on, config.ime.method);
            println!("Done.");
        }
        "config" => {
            let path = if args.len() >= 3 {
                &args[2]
            } else {
                "config.toml"
            };
            cmd_config(path);
        }
        _ => {
            print_usage();
        }
    }
}

fn cmd_list() {
    println!("Scanning connected keyboard devices...");
    let config = AppConfig::load_from_file("config.toml").unwrap_or_default();
    let devices = get_keyboard_devices(&config);

    if devices.is_empty() {
        println!("No keyboards found.");
        return;
    }

    println!("{:-<100}", "");
    println!(
        "{:<35} | {:<8} | {:<12} | {:<8} | {}",
        "Device Name", "Conn", "Mode", "Ctrl IME", "Instance ID"
    );
    println!("{:-<100}", "");

    for dev in devices {
        let conn_str = if dev.is_connected { "Online" } else { "Offline" };
        let ime_str = if dev.ctrl_ime_enabled { "ON" } else { "OFF" };
        println!(
            "{:<35} | {:<8} | {:<12} | {:<8} | {}",
            dev.name,
            conn_str,
            dev.layout_mode.as_str(),
            ime_str,
            dev.instance_id
        );
    }
    println!("{:-<100}", "");
}

fn cmd_simulate() {
    println!("=== US -> JIS Layout Conversion Simulation ===");
    let mut engine = RemapEngine::new(LayoutMode::UsToJis);

    // Test 1: US '@' (Shift + 2)
    println!("\n[Scenario 1] Typing US '@' (Press Shift, Press '2', Release '2', Release Shift):");
    let steps = vec![
        ("Shift Down", KeyEvent::down(LSHIFT, KeyFlags::NORMAL)),
        ("Key '2' Down", KeyEvent::down(KEY_2, KeyFlags::NORMAL)),
        ("Key '2' Up", KeyEvent::up(KEY_2, KeyFlags::NORMAL)),
        ("Shift Up", KeyEvent::up(LSHIFT, KeyFlags::NORMAL)),
    ];

    for (desc, evt) in steps {
        print!("  Input:  {:<15} -> ", desc);
        let action = engine.process(evt);
        match action {
            EngineAction::Emit(evts) => {
                let out_strs: Vec<String> = evts.iter().map(|e| e.to_string()).collect();
                println!("Output: [{}]", out_strs.join(", "));
            }
            EngineAction::Suppress => println!("Output: [SUPPRESSED]"),
        }
    }

    // Test 2: US ''' (Apostrophe)
    println!("\n[Scenario 2] Typing US ''' (Press ''', Release '''):");
    let steps = vec![
        ("Apostrophe Down", KeyEvent::down(APOSTROPHE, KeyFlags::NORMAL)),
        ("Apostrophe Up", KeyEvent::up(APOSTROPHE, KeyFlags::NORMAL)),
    ];

    for (desc, evt) in steps {
        print!("  Input:  {:<18} -> ", desc);
        let action = engine.process(evt);
        match action {
            EngineAction::Emit(evts) => {
                let out_strs: Vec<String> = evts.iter().map(|e| e.to_string()).collect();
                println!("Output: [{}]", out_strs.join(", "));
            }
            EngineAction::Suppress => println!("Output: [SUPPRESSED]"),
        }
    }

    // Test 3: Ctrl + C shortcut
    println!("\n[Scenario 3] Shortcut Ctrl + C (Press Ctrl, Press 'C', Release 'C', Release Ctrl):");
    let steps = vec![
        ("Ctrl Down", KeyEvent::down(LCTRL, KeyFlags::NORMAL)),
        ("Key 'C' Down", KeyEvent::down(KEY_C, KeyFlags::NORMAL)),
        ("Key 'C' Up", KeyEvent::up(KEY_C, KeyFlags::NORMAL)),
        ("Ctrl Up", KeyEvent::up(LCTRL, KeyFlags::NORMAL)),
    ];

    for (desc, evt) in steps {
        print!("  Input:  {:<15} -> ", desc);
        let action = engine.process(evt);
        match action {
            EngineAction::Emit(evts) => {
                let out_strs: Vec<String> = evts.iter().map(|e| e.to_string()).collect();
                println!("Output: [{}]", out_strs.join(", "));
            }
            EngineAction::Suppress => println!("Output: [SUPPRESSED]"),
        }
    }
}

fn cmd_config(path: &str) {
    println!("Loading config from: {}", path);
    if !Path::new(path).exists() {
        println!("File does not exist. Creating default sample configuration...");
        let mut config = AppConfig::default();
        config.devices.push(hid_remap_core::config::DeviceConfig {
            instance_id: r"HID\VID_046D&PID_C31C\EXAMPLE".to_string(),
            name: "My US Mechanical Keyboard".to_string(),
            layout_mode: LayoutMode::UsToJis,
            ctrl_ime_enabled: true,
        });
        if let Err(e) = config.save_to_file(path) {
            println!("Failed to save default config: {}", e);
            return;
        }
        println!("Default configuration written to {}", path);
    }

    match AppConfig::load_from_file(path) {
        Ok(conf) => {
            println!("Configuration loaded successfully!");
            println!("  General Enabled: {}", conf.general.enabled);
            println!("  Windows Layout: {}", conf.general.windows_layout);
            println!("  IME Method: {:?}", conf.ime.method);
            println!("  Tap Timeout: {} ms", conf.ime.tap_timeout_ms);
            println!("  Configured Devices: {}", conf.devices.len());
            for d in conf.devices {
                println!(
                    "    - {} [{}]: Mode={:?}, IME={}",
                    d.name, d.instance_id, d.layout_mode, d.ctrl_ime_enabled
                );
            }
        }
        Err(e) => {
            println!("Error loading configuration: {}", e);
        }
    }
}
