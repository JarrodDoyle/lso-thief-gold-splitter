#![no_std]

#[macro_use]
extern crate alloc;
use alloc::format;
// use alloc::string::String;
use asr::{Address, Process};
use once_cell::sync::Lazy;
use spinning_top::{const_spinlock, Spinlock};

const MAIN_MODULE: &str = "THIEF.EXE";
const IDLE_TICK_RATE: f64 = 10.0;
const RUNNING_TICK_RATE: f64 = 100.0;

#[derive(asr::Settings)]
struct Settings {
    /// IL Mode
    #[default = true]
    il_mode: bool,
    /// Constantine Ritual Split
    #[default = true]
    constantine_ritual_split: bool,
}

#[derive(Default)]
struct RunProgress {
    constantine_ritual_split: bool,
}

#[derive(Default)]
struct MemoryAddresses {
    main_address: Option<asr::Address>,
    igt: Option<asr::Address>,
}

struct State {
    main_process: Option<Process>,
    addresses: Lazy<MemoryAddresses>,
    game: Option<Game>,
    settings: Option<Settings>,
}

impl State {
    fn init(&mut self) -> Result<(), &str> {
        asr::print_message("--------Attaching Process--------");
        self.main_process = Process::attach(MAIN_MODULE);
        if self.main_process.is_none() {
            return Err("Process not found or failed to attach.");
        }

        asr::print_message("--------Getting Module Address--------");
        self.addresses.main_address = match &self.main_process {
            Some(info) => match info.get_module_address(MAIN_MODULE) {
                Ok(address) => Some(address),
                Err(_) => {
                    return Err("Failed to get main module address.");
                }
            },
            None => return Err("Process info is not initialised."),
        };

        asr::print_message("WE CONNECTED LADS");

        // self.addresses.igt = Some(self.addresses.main_address.unwrap().add(0x4C6234));

        asr::set_tick_rate(RUNNING_TICK_RATE);
        Ok(())
    }

    fn update(&mut self) {
        // let settings = self.settings.get_or_insert_with(Settings::register);

        match &self.main_process {
            None => {
                // Need to try and attach to the game.
                // Regardless of whether we're successful, we return and only start
                // using the process next update.
                if let Err(msg) = self.init() {
                    asr::print_message(msg);
                    asr::set_tick_rate(IDLE_TICK_RATE);
                }
                return;
            }
            Some(process) => {
                // Games closed so we'll detach and look for it next update
                if !process.is_open() {
                    self.main_process = None;
                    // self.addresses = Default::default();
                    asr::set_tick_rate(IDLE_TICK_RATE);
                    return;
                }
            }
        }

        // !HACK: TEST IL SPLITTING
        let main_process = self.main_process.as_ref().unwrap();
        let main_address = self.addresses.main_address.unwrap();

        let igt = main_process
            .read::<i32>(main_address.add(0x4C6234))
            .unwrap();
        let menu_state = main_process
            .read::<i32>(main_address.add(0x3D8808))
            .unwrap();
        let loading = main_process
            .read::<i32>(main_address.add(0x3D89B0))
            .unwrap();

        if igt == 0 && menu_state == 10 {
            asr::timer::reset();
            asr::timer::start();
        }

        if menu_state == 13 {
            asr::timer::split();
        }

        if menu_state == 7 || menu_state == 9 {
            asr::timer::reset();
        }

        if (loading != 0 && menu_state != 9) || menu_state == 6 || menu_state == 12 {
            asr::timer::pause_game_time();
        } else {
            asr::timer::resume_game_time();
        }
    }
}

// struct MemoryValues {
//     miss_id: Watcher<i32>,
//     loading_flag: Watcher<i32>,
//     menu_state: Watcher<i32>,
//     game_time: Watcher<i32>,
//     cutscene_name: Watcher<&'static str>,
// }

struct Game {}

static STATE: Spinlock<State> = const_spinlock(State {
    main_process: None,
    addresses: Lazy::new(Default::default),
    game: None,
    settings: None,
});

#[no_mangle]
pub extern "C" fn update() {
    STATE.lock().update();
}
