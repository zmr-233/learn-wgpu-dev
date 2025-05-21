use parking_lot::Mutex;
use std::{rc::Rc, sync::Arc};
use winit::dpi::PhysicalSize;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

struct WgpuApp {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    _adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    size_changed: bool,
}

impl WgpuApp {
    async fn new(window: Arc<Window>) -> Self {
        if cfg!(not(target_arch = "wasm32")) {
            // 计算一个默认显示高度
            let height = 600 * window.scale_factor() as u32;
            let width = (height as f32 * 1.6) as u32;
            let _ = window.request_inner_size(PhysicalSize::new(width, height));
        }

        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowExtWebSys;
            let canvas = window.canvas().unwrap();

            // 将 canvas 添加到当前网页中
            web_sys::window()
                .and_then(|win| win.document())
                .map(|doc| {
                    let _ = canvas.set_attribute("id", "winit-canvas");
                    match doc.get_element_by_id("wgpu-app-container") {
                        Some(dst) => {
                            let _ = dst.append_child(canvas.as_ref());
                        }
                        None => {
                            let container = doc.create_element("div").unwrap();
                            let _ = container.set_attribute("id", "wgpu-app-container");
                            let _ = container.append_child(canvas.as_ref());

                            doc.body().map(|body| body.append_child(container.as_ref()));
                        }
                    };
                })
                .expect("无法将 canvas 添加到当前网页中");

            // 确保画布可以获得焦点
            // https://developer.mozilla.org/en-US/docs/Web/HTML/Global_attributes/tabindex
            canvas.set_tab_index(0);

            // 设置画布获得焦点时不显示高亮轮廓
            let style = canvas.style();
            style.set_property("outline", "none").unwrap();
            canvas.focus().expect("画布无法获取焦点");
        }

        // The instance is a handle to our GPU
        // BackendBit::PRIMARY => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();
        let _ = instance
            .enumerate_adapters(wgpu::Backends::all())
            .iter()
            .for_each(|adapter| {
                log::info!("Adapter: {:?}", adapter.get_info());
            });

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                // WebGL doesn't support all of wgpu's features, so if
                // we're building for the web we'll have to disable some.
                required_limits: if cfg!(target_arch = "wasm32") {
                    wgpu::Limits::downlevel_webgl2_defaults()
                } else {
                    wgpu::Limits::default()
                },
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .unwrap();

        let mut size = window.inner_size();
        size.width = size.width.max(1);
        size.height = size.height.max(1);
        let config = surface
            .get_default_config(&adapter, size.width, size.height)
            .unwrap();
        dbg!(&config);
        let modes = surface.get_capabilities(&adapter).present_modes;
        log::info!("Surface present modes: {:?}", modes);
        surface.configure(&device, &config);

        Self {
            window,
            surface,
            _adapter: adapter,
            device,
            queue,
            config,
            size,
            size_changed: false,
        }
    }

    /// 记录窗口大小已发生变化
    ///
    /// # NOTE:
    /// 当缩放浏览器窗口时, 窗口大小会以高于渲染帧率的频率发生变化，
    /// 如果窗口 size 发生变化就立即调整 surface 大小, 会导致缩放浏览器窗口大小时渲染画面闪烁。
    fn set_window_resized(&mut self, new_size: PhysicalSize<u32>) {
        if new_size == self.size {
            return;
        }
        self.size = new_size;
        self.size_changed = true;
    }

    /// 必要的时候调整 surface 大小
    fn resize_surface_if_needed(&mut self) {
        if self.size_changed {
            self.config.width = self.size.width;
            self.config.height = self.size.height;
            self.surface.configure(&self.device, &self.config);
            self.size_changed = false;
        }
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        if self.size.width == 0 || self.size.height == 0 {
            return Ok(());
        }
        self.resize_surface_if_needed();

        let output: wgpu::SurfaceTexture = self.surface.get_current_texture()?;
        let view: wgpu::TextureView = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder: wgpu::CommandEncoder =
            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                });

        {
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }

        self.queue.submit(Some(encoder.finish()));
        output.present();

        Ok(())
    }
}

#[derive(Default)]
struct WgpuAppHandler {
    app: Rc<Mutex<Option<WgpuApp>>>,
    /// 错失的窗口大小变化
    ///
    /// # NOTE：
    /// 在 web 端，app 的初始化是异步的，当收到 resized 事件时，初始化可能还没有完成从而错过窗口 resized 事件，
    /// 当 app 初始化完成后会调用 `set_window_resized` 方法来补上错失的窗口大小变化事件。
    #[allow(dead_code)]
    missed_resize: Rc<Mutex<Option<PhysicalSize<u32>>>>,

    /// 错失的请求重绘事件
    ///
    /// # NOTE：
    /// 在 web 端，app 的初始化是异步的，当收到 redraw 事件时，初始化可能还没有完成从而错过请求重绘事件，
    /// 当 app 初始化完成后会调用 `request_redraw` 方法来补上错失的请求重绘事件。
    #[allow(dead_code)]
    missed_request_redraw: Rc<Mutex<bool>>,
}

impl ApplicationHandler for WgpuAppHandler {
    /// 恢复事件
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // 如果 app 已经初始化完成，则直接返回
        if self.app.as_ref().lock().is_some() {
            return;
        }

        let window_attributes = Window::default_attributes().with_title("tutorial2-surface");

        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
        cfg_if::cfg_if! {
            if #[cfg(target_arch = "wasm32")] {
                let app = self.app.clone();
                let missed_resize = self.missed_resize.clone();
                let missed_request_redraw = self.missed_request_redraw.clone();

                wasm_bindgen_futures::spawn_local(async move {
                    let window_cloned = window.clone();

                    let wgpu_app = WgpuApp::new(window).await;
                    let mut app = app.lock();
                    *app = Some(wgpu_app);

                    // 如果错失了窗口大小变化事件，则补上
                    if let Some(resize) = *missed_resize.lock() {
                        app.as_mut().unwrap().set_window_resized(resize);
                    }

                    // 如果错失了请求重绘事件，则补上
                    if *missed_request_redraw.lock() {
                        window_cloned.request_redraw();
                    }
                });
            } else {
                // 使用 pollster 提供的 `block_on` 函数来等待异步任务执行完成
                let wgpu_app = pollster::block_on(WgpuApp::new(window));
                self.app.lock().replace(wgpu_app);
                // NOTE: 在非 web 端，不会错失窗口大小变化事件和请求重绘事件
            }
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // 暂停事件
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let mut app = self.app.lock();
        if app.as_ref().is_none() {
            // 如果 app 还没有初始化完成，则记录错失的窗口事件
            match event {
                WindowEvent::Resized(physical_size) => {
                    if physical_size.width > 0 && physical_size.height > 0 {
                        let mut missed_resize = self.missed_resize.lock();
                        *missed_resize = Some(physical_size);
                    }
                }
                WindowEvent::RedrawRequested => {
                    let mut missed_request_redraw = self.missed_request_redraw.lock();
                    *missed_request_redraw = true;
                }
                _ => (),
            }
            return;
        }

        let app = app.as_mut().unwrap();

        // 窗口事件
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                if physical_size.width == 0 || physical_size.height == 0 {
                    // 处理最小化窗口的事件
                    log::info!("Window minimized!");
                } else {
                    log::info!("Window resized: {:?}", physical_size);

                    app.set_window_resized(physical_size);
                }
            }
            WindowEvent::KeyboardInput { .. } => {
                // 键盘事件
            }
            WindowEvent::RedrawRequested => {
                // surface 重绘事件
                app.window.pre_present_notify();

                match app.render() {
                    Ok(_) => {}
                    // 当展示平面的上下文丢失，就需重新配置
                    Err(wgpu::SurfaceError::Lost) => eprintln!("Surface is lost"),
                    // 所有其他错误（过期、超时等）应在下一帧解决
                    Err(e) => eprintln!("{e:?}"),
                }
                // 除非我们手动请求，RedrawRequested 将只会触发一次。
                app.window.request_redraw();
            }
            _ => (),
        }
    }
}

fn main() -> Result<(), impl std::error::Error> {
    utils::init_logger();

    let events_loop = EventLoop::new().unwrap();
    let mut app = WgpuAppHandler::default();
    events_loop.run_app(&mut app)
}
