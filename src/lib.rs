#![no_std]
#[macro_use]
extern crate alloc;
use alloc::{borrow::ToOwned, format, string::String, vec::Vec};
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
struct MemoryValues {
    miss_idx: Watcher<i32>,
    menu_state: Watcher<i32>,
    is_loading: Watcher<i32>,
    level_time: Watcher<i32>,
    cutscene_name: Watcher<asr::string::ArrayCString<255>>,
}

#[derive(Clone)]
struct Watcher<T> {
    pair: Option<asr::watcher::Pair<T>>,
    path: Vec<u64>,
}

impl<T> Default for Watcher<T> {
    fn default() -> Self {
        Self::new(&[])
    }
}

impl<T> Watcher<T> {
    fn new(path: &[u64]) -> Self {
        Self {
            pair: None,
            path: path.to_owned(),
        }
    }
}

impl<T: bytemuck::CheckedBitPattern> Watcher<T> {
    fn update(
        &mut self,
        process: &asr::Process,
        base: asr::Address,
    ) -> Result<asr::watcher::Pair<T>, &str> {
        let new_val = match process.read_pointer_path64(base, &self.path) {
            Ok(val) => val,
            Err(_) => return Err("Failed to update value from memory."),
        };

        let pair = self.pair.get_or_insert(asr::watcher::Pair {
            old: new_val,
            current: new_val,
        });
        pair.old = pair.current;
        pair.current = new_val;

        Ok(*pair)
    }
}

struct State {
    main_process: Option<Process>,
    base_address: Option<asr::Address>,
    values: Lazy<MemoryValues>,
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
        self.base_address = match &self.main_process {
            Some(info) => match info.get_module_address(MAIN_MODULE) {
                Ok(address) => Some(address),
                Err(_) => {
                    return Err("Failed to get main module address.");
                }
            },
            None => return Err("Process info is not initialised."),
        };

        self.values.miss_idx.path = vec![0x3D8800];
        self.values.menu_state.path = vec![0x3D8808];
        self.values.is_loading.path = vec![0x3D89B0];
        self.values.level_time.path = vec![0x4C6234];
        self.values.cutscene_name.path = vec![0x5CF9DE];

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
                    self.detach_process();
                    return;
                }
            }
        }

        if let Err(msg) = self.update_mem_values() {
            // Uh oh something fucky happened with the memory. Let's just try reattaching
            // next update?
            asr::print_message(&msg);
            self.detach_process();
            return;
        }

        let igt = self.values.level_time.pair.unwrap().current;
        let menu_state = self.values.menu_state.pair.unwrap().current;
        let is_loading = self.values.is_loading.pair.unwrap().current;

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

    fn update_mem_values(&mut self) -> Result<(), String> {
        let process = match &self.main_process {
            Some(process) => process,
            None => return Err("Could not load main process.".to_owned()),
        };

        let base = match self.base_address {
            Some(address) => address,
            None => return Err("Could not load base address.".to_owned()),
        };

        self.values.miss_idx.update(process, base)?;
        self.values.menu_state.update(process, base)?;
        self.values.is_loading.update(process, base)?;
        self.values.level_time.update(process, base)?;
        self.values.cutscene_name.update(process, base)?;
        Ok(())
    }

    fn detach_process(&mut self) {
        self.main_process = None;
        self.base_address = None;
        self.values = Default::default();
        asr::set_tick_rate(IDLE_TICK_RATE);
    }
}

static STATE: Spinlock<State> = const_spinlock(State {
    main_process: None,
    base_address: None,
    values: Lazy::new(Default::default),
    settings: None,
});

#[no_mangle]
pub extern "C" fn update() {
    STATE.lock().update();
}
