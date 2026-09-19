//! Minimal no-content libretro diagnostic core. Used by real_retroarch_pointer.py.
//! Records the actual frontend input callbacks, without a ROM or external game.
use std::{ffi::c_void, fs::File, io::Write};
type Env = unsafe extern "C" fn(u32, *mut c_void) -> bool;
type Input = unsafe extern "C" fn(u32, u32, u32, u32) -> i16;
type Video = unsafe extern "C" fn(*const c_void, u32, u32, usize);
static mut ENV: Option<Env> = None;
static mut INPUT: Option<Input> = None;
static mut POLL: Option<unsafe extern "C" fn()> = None;
static mut VIDEO: Option<Video> = None;
static mut LOG: Option<File> = None;
static mut FRAME: u64 = 0;
#[repr(C)]
pub struct Info { name: *const u8, version: *const u8, extensions: *const u8, fullpath: bool, block_extract: bool }
#[repr(C)]
pub struct Geometry { width: u32, height: u32, max_width: u32, max_height: u32, aspect: f32 }
#[repr(C)]
pub struct Timing { fps: f64, sample_rate: f64 }
#[repr(C)]
pub struct Av { geometry: Geometry, timing: Timing }
#[no_mangle] pub unsafe extern "C" fn retro_set_environment(cb: Env) {
    ENV = Some(cb);
    cb(18, &mut true as *mut bool as *mut c_void); // SET_SUPPORT_NO_GAME
}
#[no_mangle] pub unsafe extern "C" fn retro_set_input_state(cb: Input) { INPUT = Some(cb); }
#[no_mangle] pub unsafe extern "C" fn retro_set_input_poll(cb: unsafe extern "C" fn()) { POLL = Some(cb); }
#[no_mangle] pub unsafe extern "C" fn retro_set_video_refresh(cb: Video) { VIDEO = Some(cb); }
#[no_mangle] pub extern "C" fn retro_set_audio_sample(_: unsafe extern "C" fn(i16, i16)) {}
#[no_mangle] pub extern "C" fn retro_set_audio_sample_batch(_: unsafe extern "C" fn(*const i16, usize) -> usize) {}
#[no_mangle] pub unsafe extern "C" fn retro_init() {
    ENV.unwrap()(10, &mut 1u32 as *mut u32 as *mut c_void); // XRGB8888
    LOG = Some(File::create(std::env::var("PEPOMOTE_POINTER_LOG").unwrap()).unwrap());
}
#[no_mangle] pub unsafe extern "C" fn retro_deinit() { LOG = None; }
#[no_mangle] pub extern "C" fn retro_api_version() -> u32 { 1 }
#[no_mangle] pub unsafe extern "C" fn retro_get_system_info(out: *mut Info) {
    *out = Info { name: b"PepoMote Input Test\0".as_ptr(), version: b"1\0".as_ptr(),
        extensions: b"\0".as_ptr(), fullpath: false, block_extract: false };
}
#[no_mangle] pub unsafe extern "C" fn retro_get_system_av_info(out: *mut Av) {
    *out = Av { geometry: Geometry { width: 320, height: 240, max_width: 320, max_height: 240, aspect: 4.0 / 3.0 },
        timing: Timing { fps: 60.0, sample_rate: 44100.0 } };
}
#[no_mangle] pub unsafe extern "C" fn retro_run() {
    POLL.unwrap()();
    let read = INPUT.unwrap();
    let mut pad = 0u16;
    for id in 0..16 { if read(0, 1, 0, id) != 0 { pad |= 1 << id; } }
    let (mx, my, left, right) = (read(0, 2, 0, 0), read(0, 2, 0, 1), read(0, 2, 0, 2), read(0, 2, 0, 3));
    let (gx, gy, trigger) = (read(0, 4, 0, 13), read(0, 4, 0, 14), read(0, 4, 0, 2));
    FRAME += 1;
    let frame = FRAME;
    if let Some(ref mut file) = LOG { writeln!(file, "{},{mx},{my},{left},{right},{gx},{gy},{trigger},{pad}", frame).unwrap(); }
    let mut pixels = [0x182838u32; 320 * 240];
    let x = ((gx as i32 + 32768) * 319 / 65535).clamp(0, 319) as usize;
    let y = ((gy as i32 + 32768) * 239 / 65535).clamp(0, 239) as usize;
    for d in 0..320 { pixels[y * 320 + d] = 0x00ff88; }
    for d in 0..240 { pixels[d * 320 + x] = 0x00ff88; }
    VIDEO.unwrap()(pixels.as_ptr().cast(), 320, 240, 320 * 4);
}
#[no_mangle] pub extern "C" fn retro_load_game(_: *const c_void) -> bool { true }
#[no_mangle] pub extern "C" fn retro_unload_game() {}
#[no_mangle] pub extern "C" fn retro_reset() {}
#[no_mangle] pub extern "C" fn retro_set_controller_port_device(_: u32, _: u32) {}
#[no_mangle] pub extern "C" fn retro_get_region() -> u32 { 0 }
#[no_mangle] pub extern "C" fn retro_serialize_size() -> usize { 0 }
#[no_mangle] pub extern "C" fn retro_serialize(_: *mut c_void, _: usize) -> bool { false }
#[no_mangle] pub extern "C" fn retro_unserialize(_: *const c_void, _: usize) -> bool { false }
#[no_mangle] pub extern "C" fn retro_cheat_reset() {}
#[no_mangle] pub extern "C" fn retro_cheat_set(_: u32, _: bool, _: *const u8) {}
#[no_mangle] pub extern "C" fn retro_load_game_special(_: u32, _: *const c_void, _: usize) -> bool { false }
#[no_mangle] pub extern "C" fn retro_get_memory_data(_: u32) -> *mut c_void { std::ptr::null_mut() }
#[no_mangle] pub extern "C" fn retro_get_memory_size(_: u32) -> usize { 0 }
