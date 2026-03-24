use std::ffi::c_char;

pub const VIRGLRENDERER_USE_EGL: u32 = 1 << 0;
pub const VIRGLRENDERER_THREAD_SYNC: u32 = 1 << 1;
pub const VIRGLRENDERER_VENUS: u32 = 1 << 6;
pub const VIRGLRENDERER_NO_VIRGL: u32 = 1 << 7;
pub const VIRGLRENDERER_USE_ASYNC_FENCE_CB: u32 = 1 << 8;
pub const VIRGLRENDERER_RENDER_SERVER: u32 = 1 << 9;
pub const VIRGLRENDERER_DRM: u32 = 1 << 10;

#[link(name = "krun")]
unsafe extern "C" {
	pub fn krun_set_log_level(level: u32) -> i32;
	pub fn krun_create_ctx() -> i32;
	pub fn krun_set_vm_config(ctx_id: u32, num_vcpus: u8, ram_mib: u32) -> i32;
	pub fn krun_set_gpu_options2(ctx_id: u32, virgl_flags: u32, shm_size: u64) -> i32;
	pub fn krun_set_root(ctx_id: u32, root_path: *const c_char) -> i32;
	pub fn krun_add_virtiofs2(
		ctx_id: u32,
		c_tag: *const c_char,
		c_path: *const c_char,
		shm_size: u64,
	) -> i32;
	pub fn krun_add_disk(
		ctx_id: u32,
		block_id: *const c_char,
		disk_path: *const c_char,
		read_only: bool,
	) -> i32;
	pub fn krun_set_passt_fd(ctx_id: u32, fd: i32) -> i32;
	pub fn krun_add_vsock_port(ctx_id: u32, port: u32, c_filepath: *const c_char) -> i32;
	pub fn krun_add_vsock_port2(
		ctx_id: u32,
		port: u32,
		c_filepath: *const c_char,
		listen: bool,
	) -> i32;
	pub fn krun_set_workdir(ctx_id: u32, workdir_path: *const c_char) -> i32;
	pub fn krun_set_env(ctx_id: u32, envp: *const *const c_char) -> i32;
	pub fn krun_start_enter(ctx_id: u32) -> i32;
}
