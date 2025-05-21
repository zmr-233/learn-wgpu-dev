pub mod framework;
pub use framework::{WgpuAppAction, run};

pub mod load_texture;
pub use load_texture::{
    AnyTexture, bilinear_sampler, default_sampler, mirror_repeate_sampler, repeate_sampler,
};
pub mod node;

mod plane;
pub use plane::Plane;

mod buffer;
pub use buffer::BufferObj;

pub mod matrix_helper;
pub mod vertex;

mod color;
pub use color::*;

use bytemuck::{Pod, Zeroable};

pub static DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MVPMatUniform {
    pub mvp: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
/// 场景统一数据结构，用于传递给着色器的常量数据
pub struct SceneUniform {
    /// 模型-视图-投影矩阵（Model-View-Projection Matrix）
    /// 用于将模型坐标转换为裁剪空间坐标
    pub mvp: [[f32; 4]; 4],

    /// 视口尺寸（宽度和高度），以像素为单位
    /// 通常用于在着色器中进行坐标转换或计算
    pub viewport_pixels: [f32; 2],

    /// 填充字段，用于确保数据对齐
    /// 在WebGPU中，统一缓冲区通常需要特定的内存对齐
    pub padding: [f32; 2],
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn application_root_dir() -> String {
    let location = web_sys::window().unwrap().location();
    let host = location.host().unwrap();
    let href = location.href().unwrap();
    if host.contains("localhost") || host.contains("127.0.0.1") {
        String::from("http://")
            + &host
            + if href.contains("learn-wgpu-zh") {
                "/learn-wgpu-zh/"
            } else {
                "/"
            }
    } else if host.contains("jinleili.github.io") {
        String::from("https://jinleili.github.io/learn-wgpu-zh/")
    } else {
        String::from("https://cannot.access/")
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn application_root_dir() -> String {
    use std::env;
    use std::fs;

    match env::var("PROFILE") {
        Ok(_) => String::from(env!("CARGO_MANIFEST_DIR")),
        Err(_) => {
            let mut path = env::current_exe().expect("Failed to find executable path.");
            while let Ok(target) = fs::read_link(path.clone()) {
                path = target;
            }
            if cfg!(any(
                target_os = "macos",
                target_os = "windows",
                target_os = "linux"
            )) {
                path = path.join("../../../assets/").canonicalize().unwrap();
            }

            String::from(path.to_str().unwrap())
        }
    }
}

use std::path::PathBuf;
#[allow(unused)]
pub(crate) fn get_texture_file_path(name: &str) -> PathBuf {
    PathBuf::from(application_root_dir()).join(name)
}

// 根据不同平台初始化日志。
pub fn init_logger() {
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            // 使用查询字符串来获取日志级别。
            let query_string = web_sys::window().unwrap().location().search().unwrap();
            let query_level: Option<log::LevelFilter> = parse_url_query_string(&query_string, "RUST_LOG")
                .and_then(|x| x.parse().ok());

            // 我们将 wgpu 日志级别保持在错误级别，因为 Info 级别的日志输出非常多。
            let base_level = query_level.unwrap_or(log::LevelFilter::Info);
            let wgpu_level = query_level.unwrap_or(log::LevelFilter::Error);

            // 在 web 上，我们使用 fern，因为 console_log 没有按模块级别过滤功能。
            fern::Dispatch::new()
                .level(base_level)
                .level_for("wgpu_core", wgpu_level)
                .level_for("wgpu_hal", wgpu_level)
                .level_for("naga", wgpu_level)
                .chain(fern::Output::call(console_log::log))
                .apply()
                .unwrap();
            std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        } else if #[cfg(target_os = "android")] {
            // 添加 Android 平台的日志初始化
            android_logger::init_once(
                android_logger::Config::default()
                    .with_max_level(log::LevelFilter::Info)
            );
            log_panics::init();
        } else {
            // parse_default_env 会读取 RUST_LOG 环境变量，并在这些默认过滤器之上应用它。
            env_logger::builder()
                .filter_level(log::LevelFilter::Info)
                .filter_module("wgpu_core", log::LevelFilter::Info)
                .filter_module("wgpu_hal", log::LevelFilter::Error)
                .filter_module("naga", log::LevelFilter::Error)
                .parse_default_env()
                .init();
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn parse_url_query_string<'a>(query: &'a str, search_key: &str) -> Option<&'a str> {
    let query_string = query.strip_prefix('?')?;

    for pair in query_string.split('&') {
        let mut pair = pair.split('=');
        let key = pair.next()?;
        let value = pair.next()?;

        if key == search_key {
            return Some(value);
        }
    }

    None
}
