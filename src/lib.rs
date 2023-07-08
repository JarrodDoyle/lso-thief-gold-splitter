#![no_std]
#[macro_use]
extern crate alloc;
use alloc::{
    borrow::ToOwned,
    format,
    string::{String, ToString},
    vec::Vec,
};
use asr::{timer::TimerState, Process};
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
struct MemoryWatchers {
    miss_idx: Watcher<i32>,
    menu_state: Watcher<i32>,
    is_loading: Watcher<i32>,
    level_time: Watcher<i32>,
    difficulty: Watcher<i32>,
    cutscene_name: Watcher<asr::string::ArrayCString<255>>,
}

struct Vars {
    miss_idx: asr::watcher::Pair<i32>,
    menu_state: asr::watcher::Pair<i32>,
    is_loading: asr::watcher::Pair<i32>,
    level_time: asr::watcher::Pair<i32>,
    difficulty: asr::watcher::Pair<i32>,
    cutscene_name: asr::watcher::Pair<String>,
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
    values: Lazy<MemoryWatchers>,
    settings: Option<Settings>,
    miss_idx_order: Vec<i32>,
    split_idx: usize,
    is_gold: bool,
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
        self.values.difficulty.path = vec![0x5C1280];
        self.values.cutscene_name.path = vec![0x5CF9DE];

        self.miss_idx_order = vec![1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 14];
        self.split_idx = 0;
        self.is_gold = false;

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

        let vars = match self.update_mem_values() {
            Ok(vals) => vals,
            Err(msg) => {
                // Uh oh something fucky happened with the memory. Let's just try reattaching
                // next update?
                asr::print_message(&msg);
                self.detach_process();
                return;
            }
        };

        // There's no good way to know if we're on gold until we hit a gold level
        if vars.miss_idx.current == 15 && !self.is_gold {
            self.miss_idx_order = vec![1, 2, 3, 4, 5, 15, 6, 7, 16, 9, 17, 10, 11, 12, 13, 14];
            self.is_gold = true;
        }

        let timer_state = asr::timer::state();

        if timer_state == TimerState::NotRunning && vars.difficulty.current != 0 {
            self.split_idx = 1;
        }

        // Handle game timer
        if timer_state == TimerState::Running {
            if (vars.is_loading.current != 0 && vars.menu_state.current != 9)
                || vars.menu_state.current == 6
                || vars.menu_state.current == 12
            {
                asr::timer::pause_game_time();
            } else {
                asr::timer::resume_game_time();
            }
        }

        if self.should_start(timer_state, &vars) {
            asr::timer::start();
        } else if self.should_split(timer_state, &vars) {
            asr::timer::split();
            self.split_idx += 1;
        } else if self.should_reset(timer_state, &vars) {
            asr::timer::reset();
            self.split_idx = 0;
        }
    }

    fn should_start(&self, timer_state: TimerState, vars: &Vars) -> bool {
        let valid_timer = timer_state == TimerState::NotRunning;
        valid_timer
            && vars.miss_idx.current == self.miss_idx_order[self.split_idx]
            && vars.menu_state.current == 10
            && vars.is_loading.current != 0
    }

    fn should_split(&self, timer_state: TimerState, vars: &Vars) -> bool {
        let valid_timer = timer_state == TimerState::Running;
        valid_timer
            && vars.menu_state.current == 12
            && vars.miss_idx.current == self.miss_idx_order[self.split_idx]
            && vars.cutscene_name.current.contains("success")
    }

    fn should_reset(&self, timer_state: TimerState, vars: &Vars) -> bool {
        let valid_timer = timer_state == TimerState::Running || timer_state == TimerState::Ended;
        let start_split = if vars.difficulty.current == 0 { 0 } else { 1 };
        valid_timer
            && vars.miss_idx.current == self.miss_idx_order[start_split]
            && vars.menu_state.current == 7
    }

    fn update_mem_values(&mut self) -> Result<Vars, String> {
        let process = match &self.main_process {
            Some(process) => process,
            None => return Err("Could not load main process.".to_owned()),
        };

        let base = match self.base_address {
            Some(address) => address,
            None => return Err("Could not load base address.".to_owned()),
        };

        // Have to do some fuckery here due to cstrings :)
        let cutscene_name = self.values.cutscene_name.update(process, base)?;
        let cutscene_name = asr::watcher::Pair::<String> {
            old: Self::convert_cstring(cutscene_name.old)?,
            current: Self::convert_cstring(cutscene_name.current)?,
        };
        Ok(Vars {
            miss_idx: self.values.miss_idx.update(process, base)?,
            menu_state: self.values.menu_state.update(process, base)?,
            is_loading: self.values.is_loading.update(process, base)?,
            level_time: self.values.level_time.update(process, base)?,
            difficulty: self.values.difficulty.update(process, base)?,
            cutscene_name,
        })
    }

    fn detach_process(&mut self) {
        self.main_process = None;
        self.base_address = None;
        self.values = Default::default();
        asr::set_tick_rate(IDLE_TICK_RATE);
    }

    fn convert_cstring(cstring: asr::string::ArrayCString<255>) -> Result<String, String> {
        match cstring.validate_utf8() {
            Ok(val) => Ok(val.to_string()),
            Err(_) => Err("Failed to convert cstring to string.".to_owned()),
        }
    }
}

static STATE: Spinlock<State> = const_spinlock(State {
    main_process: None,
    base_address: None,
    values: Lazy::new(Default::default),
    settings: None,
    miss_idx_order: vec![],
    split_idx: 0,
    is_gold: false,
});

#[no_mangle]
pub extern "C" fn update() {
    STATE.lock().update();
}
