#![no_std]

extern crate alloc;
use alloc::{borrow::ToOwned, format, string::String};
use asr::Process;
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
    base: Option<asr::Address>,
    miss_idx: u64,
    menu_state: u64,
    is_loading: u64,
    level_time: u64,
    cutscene_name: u64,
}

#[derive(Default)]
struct MemoryValues {
    miss_idx: asr::watcher::Pair<i32>,
    menu_state: asr::watcher::Pair<i32>,
    is_loading: asr::watcher::Pair<i32>,
    level_time: asr::watcher::Pair<i32>,
    cutscene_name: asr::watcher::Pair<String>,
}

struct State {
    main_process: Option<Process>,
    addresses: Lazy<MemoryAddresses>,
    values: Lazy<MemoryValues>,
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
        self.addresses.base = match &self.main_process {
            Some(info) => match info.get_module_address(MAIN_MODULE) {
                Ok(address) => Some(address),
                Err(_) => {
                    return Err("Failed to get main module address.");
                }
            },
            None => return Err("Process info is not initialised."),
        };

        self.addresses.miss_idx = 0x3D8800;
        self.addresses.menu_state = 0x3D8808;
        self.addresses.is_loading = 0x3D89B0;
        self.addresses.level_time = 0x4C6234;
        self.addresses.cutscene_name = 0x5CF9DE;

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
                    self.addresses = Default::default();
                    asr::set_tick_rate(IDLE_TICK_RATE);
                    return;
                }
            }
        }

        if let Err(msg) = self.update_mem_values() {
            // Uh oh something fucky happened with the memory. Let's just try reattaching
            // next update?
            asr::print_message(msg);
            self.main_process = None;
            self.addresses = Default::default();
            asr::set_tick_rate(IDLE_TICK_RATE);
            return;
        }

        let igt = self.values.level_time.current;
        let menu_state = self.values.menu_state.current;
        let is_loading = self.values.is_loading.current;

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

        if (is_loading != 0 && menu_state != 9) || menu_state == 6 || menu_state == 12 {
            asr::timer::pause_game_time();
        } else {
            asr::timer::resume_game_time();
        }
    }

    fn update_mem_values(&mut self) -> Result<(), &str> {
        let main_process = match &self.main_process {
            Some(process) => process,
            None => return Err("Could not load main process."),
        };

        let main_address = match self.addresses.base {
            Some(address) => address,
            None => return Err("Could not load main address."),
        };

        let address = main_address.add(self.addresses.miss_idx);
        let miss_idx = match main_process.read::<i32>(address) {
            Ok(val) => val,
            Err(_) => return Err("Failed to update mission index value from memory."),
        };
        Self::update_pair_copy(&mut self.values.miss_idx, miss_idx);

        let address = main_address.add(self.addresses.menu_state);
        let menu_state = match main_process.read::<i32>(address) {
            Ok(val) => val,
            Err(_) => return Err("Failed to update menu state value from memory."),
        };
        Self::update_pair_copy(&mut self.values.menu_state, menu_state);

        let address = main_address.add(self.addresses.is_loading);
        let is_loading = match main_process.read::<i32>(address) {
            Ok(val) => val,
            Err(_) => return Err("Failed to update loading flag value from memory."),
        };
        Self::update_pair_copy(&mut self.values.is_loading, is_loading);

        let address = main_address.add(self.addresses.level_time);
        let level_time = match main_process.read::<i32>(address) {
            Ok(val) => val,
            Err(_) => return Err("Failed to update level IGT value from memory."),
        };
        Self::update_pair_copy(&mut self.values.level_time, level_time);

        let address = main_address.add(self.addresses.cutscene_name);
        let cutscene_name = match main_process.read::<asr::string::ArrayCString<255>>(address) {
            Ok(cstring) => match cstring.validate_utf8() {
                Ok(val) => val.to_owned(),
                Err(_) => return Err("Cutscene name isn't valid UTF8"),
            },
            Err(_) => return Err("Failed to update cutscene name value from memory."),
        };
        Self::update_pair_clone(&mut self.values.cutscene_name, cutscene_name);

        Ok(())
    }

    fn update_pair_copy<T: Copy>(pair: &mut asr::watcher::Pair<T>, val: T) {
        pair.old = pair.current;
        pair.current = val;
    }

    fn update_pair_clone<T: Clone>(pair: &mut asr::watcher::Pair<T>, val: T) {
        pair.old = pair.current.clone();
        pair.current = val;
    }
}

struct Game {}

static STATE: Spinlock<State> = const_spinlock(State {
    main_process: None,
    addresses: Lazy::new(Default::default),
    values: Lazy::new(Default::default),
    game: None,
    settings: None,
});

#[no_mangle]
pub extern "C" fn update() {
    STATE.lock().update();
}
