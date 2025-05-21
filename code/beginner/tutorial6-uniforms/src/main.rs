use app_surface::{AppSurface, SurfaceFrame};
use std::sync::Arc;
use utils::framework::{WgpuAppAction, run};
use wgpu::{BindingResource, util::DeviceExt};
use winit::{
    dpi::PhysicalSize,
    event::*,
    keyboard::{KeyCode, PhysicalKey},
};

mod texture;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
}

impl Vertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        use core::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

const VERTICES: &[Vertex] = &[
    Vertex {
        position: [-0.0868241, 0.49240386, 0.0],
        tex_coords: [0.4131759, 0.00759614],
    }, // A
    Vertex {
        position: [-0.49513406, 0.06958647, 0.0],
        tex_coords: [0.0048659444, 0.43041354],
    }, // B
    Vertex {
        position: [-0.21918549, -0.44939706, 0.0],
        tex_coords: [0.28081453, 0.949397],
    }, // C
    Vertex {
        position: [0.35966998, -0.3473291, 0.0],
        tex_coords: [0.85967, 0.84732914],
    }, // D
    Vertex {
        position: [0.44147372, 0.2347359, 0.0],
        tex_coords: [0.9414737, 0.2652641],
    }, // E
];

const INDICES: &[u16] = &[0, 1, 4, 1, 2, 4, 2, 3, 4];

struct Camera {
    eye: glam::Vec3,
    target: glam::Vec3,
    up: glam::Vec3,
    aspect: f32,
    fovy: f32,
    znear: f32,
    zfar: f32,
}

impl Camera {
    fn build_view_projection_matrix(&self) -> glam::Mat4 {
        //1. 视图矩阵移动并旋转世界坐标到摄像机所观察的位置
        let view = glam::Mat4::look_at_rh(self.eye, self.target, self.up);
        //2. 投影矩阵变换场景空间，以产生景深的效果
        let proj =
            glam::Mat4::perspective_rh(self.fovy.to_radians(), self.aspect, self.znear, self.zfar);
        //3. 在归一化设备坐标中，x 轴和 y 轴的范围是 [-1.0, 1.0]，而 z 轴是 [0.0, 1.0]
        // 移植 OpenGL 程序时需要注意：在 OpenGL 的归一化设备坐标中 z 轴的范围是 [-1.0, 1.0]
        proj * view
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    // glam 的数据类型不能直接用于 bytemuck
    // 需要先将 Matrix4 矩阵转为一个 4x4 的浮点数数组
    view_proj: [[f32; 4]; 4],
}

impl CameraUniform {
    fn new() -> Self {
        Self {
            view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
        }
    }

    fn update_view_proj(&mut self, camera: &Camera) {
        self.view_proj = camera.build_view_projection_matrix().to_cols_array_2d();
    }
}

struct CameraController {
    speed: f32,
    is_up_pressed: bool,
    is_down_pressed: bool,
    is_forward_pressed: bool,
    is_backward_pressed: bool,
    is_left_pressed: bool,
    is_right_pressed: bool,
}

impl CameraController {
    fn new(speed: f32) -> Self {
        Self {
            speed,
            is_up_pressed: false,
            is_down_pressed: false,
            is_forward_pressed: false,
            is_backward_pressed: false,
            is_left_pressed: false,
            is_right_pressed: false,
        }
    }

    fn process_events(&mut self, event: &KeyEvent) -> bool {
        // 直接检查 KeyEvent 的状态
        let is_pressed = event.state == ElementState::Pressed;

        if event.physical_key == PhysicalKey::Code(KeyCode::Space) {
            self.is_up_pressed = is_pressed;
            return true;
        }

        match event.physical_key {
            PhysicalKey::Code(KeyCode::ShiftLeft) => {
                self.is_down_pressed = is_pressed;
                true
            }
            PhysicalKey::Code(KeyCode::KeyW) | PhysicalKey::Code(KeyCode::ArrowUp) => {
                self.is_forward_pressed = is_pressed;
                true
            }
            PhysicalKey::Code(KeyCode::KeyA) | PhysicalKey::Code(KeyCode::ArrowLeft) => {
                self.is_left_pressed = is_pressed;
                true
            }
            PhysicalKey::Code(KeyCode::KeyS) | PhysicalKey::Code(KeyCode::ArrowDown) => {
                self.is_backward_pressed = is_pressed;
                true
            }
            PhysicalKey::Code(KeyCode::KeyD) | PhysicalKey::Code(KeyCode::ArrowRight) => {
                self.is_right_pressed = is_pressed;
                true
            }
            _ => false,
        }
    }

    fn update_camera(&self, camera: &mut Camera) {
        let forward = camera.target - camera.eye;
        let forward_norm = forward.normalize();
        let forward_mag = forward.length();

        // Prevents glitching when camera gets too close to the
        // center of the scene.
        // 防止摄像机离场景中心太近时出现问题
        if self.is_forward_pressed && forward_mag > self.speed {
            camera.eye += forward_norm * self.speed;
        }
        if self.is_backward_pressed {
            camera.eye -= forward_norm * self.speed;
        }

        let right = forward_norm.cross(camera.up);

        // Redo radius calc in case the up/ down is pressed.
        // 重新计算半径
        let forward = camera.target - camera.eye;
        let forward_mag = forward.length();

        if self.is_right_pressed {
            // Rescale the distance between the target and eye so
            // that it doesn't change. The eye therefore still
            // lies on the circle made by the target and eye.
            camera.eye = camera.target - (forward + right * self.speed).normalize() * forward_mag;
        }
        if self.is_left_pressed {
            camera.eye = camera.target - (forward - right * self.speed).normalize() * forward_mag;
        }
    }
}

struct WgpuApp {
    app: AppSurface,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    size: PhysicalSize<u32>,
    size_changed: bool,
    #[allow(dead_code)]
    diffuse_texture: texture::Texture,
    diffuse_bind_group: wgpu::BindGroup,
    // NEW!
    camera: Camera,
    camera_controller: CameraController,
    camera_uniform: CameraUniform,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
}

impl WgpuApp {
    /// 必要的时候调整 surface 大小
    fn resize_surface_if_needed(&mut self) {
        if self.size_changed {
            self.app
                .resize_surface_by_size((self.size.width, self.size.height));

            // 重新设置视口大小
            self.camera.aspect = self.app.config.width as f32 / self.app.config.height as f32;

            self.size_changed = false;
        }
    }
}

impl WgpuAppAction for WgpuApp {
    async fn new(window: Arc<winit::window::Window>) -> Self {
        // 创建 wgpu 应用
        let app = AppSurface::new(window).await;

        let diffuse_bytes = include_bytes!("happy-tree.png");
        let diffuse_texture =
            texture::Texture::from_bytes(&app.device, &app.queue, diffuse_bytes, "happy-tree.png")
                .unwrap();

        let texture_bind_group_layout =
            app.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                    label: Some("texture_bind_group_layout"),
                });

        let diffuse_bind_group = app.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&diffuse_texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&diffuse_texture.sampler),
                },
            ],
            label: Some("diffuse_bind_group"),
        });

        let camera = Camera {
            // 将摄像机向上移动 1 个单位，向后移动 2 个单位
            // +z 朝向屏幕外
            eye: (0.0, 1.0, 2.0).into(),
            // 摄像机看向原点
            target: (0.0, 0.0, 0.0).into(),
            // 定义哪个方向朝上
            up: glam::Vec3::Y,
            aspect: app.config.width as f32 / app.config.height as f32,
            fovy: 45.0,
            znear: 0.1,
            zfar: 100.0,
        };
        let camera_controller = CameraController::new(0.2);

        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_proj(&camera);

        // Uniform = GPU 版的“全局常量内存”：
        //Uniform（统一变量）= 在一次 draw/dispatch 期间对所有着色器线程都保持相同值的、只读全局数据
        //它由 CPU（或另一个 GPU pass）在调用绘制/计算命令前写入并绑定，着色器里只能读取，不能修改
        // 从技术的角度来看，我们已经为纹理和采样器使用了 Uniform 缓冲区
        let camera_buffer = app
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Camera Buffer"),
                contents: bytemuck::cast_slice(&[camera_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        // 先创建绑定组的布局
        let camera_bind_group_layout =
            app.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        //1. 只在顶点着色器中需要虚拟摄像机信息，因为要用它来操作顶点
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            //2. has_dynamic_offset 字段表示这个缓冲区是否会动态改变偏移量
                            //想一次性在 Uniform 中存储多组数据，
                            //并实时修改偏移量来告诉着色器当前使用哪组数据时，这就很有用
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                    label: Some("camera_bind_group_layout"),
                });
        // 创建实际的绑定组
        let camera_bind_group = app.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                //Return the binding view of the entire buffer.
                resource: BindingResource::Buffer(camera_buffer.as_entire_buffer_binding()),
                //camera_buffer.as_entire_binding(),
            }],
            label: Some("camera_bind_group"),
        });

        let shader = app
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
            });

        // 渲染管线的两层绑定架构
        // WebGPU使用了一种分离的资源绑定架构：
        // a. 定义阶段（Pipeline创建时）：只声明格式和布局，不关联具体数据
        // b. 执行阶段（实际渲染时）：绑定实际的资源实例

        // 步骤1：创建管线布局，引入之前定义的绑定组布局(包括摄像机布局)
        let render_pipeline_layout =
            app.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Render Pipeline Layout"),
                    // @group(N) 这个数字由我们的 render_pipeline_layout 决定
                    bind_group_layouts: &[
                        &texture_bind_group_layout,
                        //在管线布局描述符中注册 camera_bind_group_layout
                        &camera_bind_group_layout,
                    ],
                    push_constant_ranges: &[],
                });

        // 步骤2：创建渲染管线时只需引用这个布局
        //  渲染执行时（实际绑定）
        let render_pipeline = app
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Render Pipeline"),
                layout: Some(&render_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Vertex::desc()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: app.config.format.add_srgb_suffix(),
                        blend: Some(wgpu::BlendState {
                            color: wgpu::BlendComponent::REPLACE,
                            alpha: wgpu::BlendComponent::REPLACE,
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    polygon_mode: wgpu::PolygonMode::Fill,
                    // Requires Features::DEPTH_CLIP_CONTROL
                    unclipped_depth: false,
                    // Requires Features::CONSERVATIVE_RASTERIZATION
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                // If the pipeline will be used with a multiview render pass, this
                // indicates how many array layers the attachments will have.
                multiview: None,
                cache: None,
            });

        let vertex_buffer = app
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: bytemuck::cast_slice(VERTICES),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buffer = app
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: bytemuck::cast_slice(INDICES),
                usage: wgpu::BufferUsages::INDEX,
            });
        let num_indices = INDICES.len() as u32;

        let size = PhysicalSize {
            width: app.config.width,
            height: app.config.height,
        };

        Self {
            app,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            num_indices,
            size,
            size_changed: false,
            diffuse_texture,
            diffuse_bind_group,
            camera,
            camera_controller,
            camera_buffer,
            camera_bind_group,
            camera_uniform,
        }
    }

    fn set_window_resized(&mut self, new_size: PhysicalSize<u32>) {
        if self.app.config.width == new_size.width && self.app.config.height == new_size.height {
            return;
        }
        self.size = new_size;
        self.size_changed = true;
    }

    fn get_size(&self) -> PhysicalSize<u32> {
        PhysicalSize::new(self.app.config.width, self.app.config.height)
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        self.camera_controller.process_events(event)
    }

    // uniform 缓冲区中的值需要被更新。有几种方式可以做到这一点：

    // 1. 可以创建一个单独的缓冲区，并将其数据复制到 camera_buffer。
    // 这个新的缓冲区被称为中继缓冲区（Staging Buffer）。
    // 这种方法允许主缓冲区（在这里是指 camera_buffer）的数据只被 GPU 访问，从而令 GPU 能做一些速度上的优化。
    // 如果缓冲区能被 CPU 访问，就无法实现此类优化。
    // 中继缓冲区是 GPU 编程中一种优化模式，其工作流程是：

    // a. 创建两个缓冲区：一个是 CPU 可写的"中继缓冲区"，另一个是 GPU 优化的"目标缓冲区"
    // b. CPU 将数据写入中继缓冲区
    // c. 然后通过命令将数据从中继缓冲区复制到目标缓冲区
    // d. GPU 从目标缓冲区读取数据
    // 这种方式的优势在于目标缓冲区可以完全放在 GPU 内存中（如显存），使 GPU 访问更高效。
    fn update(&mut self, _dt: instant::Duration) {
        // 更新相机数据
        self.camera_controller.update_camera(&mut self.camera);
        self.camera_uniform.update_view_proj(&self.camera);

        // 创建中继缓冲区
        let staging_buffer =
            self.app
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Camera Staging Buffer"),
                    contents: bytemuck::cast_slice(&[self.camera_uniform]),
                    usage: wgpu::BufferUsages::COPY_SRC,
                });

        // 创建命令编码器
        let mut encoder = self
            .app
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Camera Update Encoder"),
            });

        // 从中继缓冲区复制到目标缓冲区
        encoder.copy_buffer_to_buffer(
            &staging_buffer,
            0,
            &self.camera_buffer,
            0,
            std::mem::size_of::<CameraUniform>() as wgpu::BufferAddress,
        );

        // 提交命令
        self.app.queue.submit(Some(encoder.finish()));
    }

    // 2. 可以在缓冲区本身调用内存映射函数 map_read_async 和 map_write_async。
    // 此方式允许我们直接访问缓冲区的数据，但是需要处理异步代码，
    // 也需要缓冲区使用 BufferUsages::MAP_READ 和/或 BufferUsages::MAP_WRITE。
    // a. 调用 map_write_async 请求访问权限
    // b. 等待 GPU 准备好缓冲区
    // c. 获取内存映射视图并写入数据
    // d. 解除映射，释放缓冲区
    // fn update(&mut self, _dt: instant::Duration) {
    //     // 更新相机数据
    //     self.camera_controller.update_camera(&mut self.camera);
    //     self.camera_uniform.update_view_proj(&self.camera);

    //     // 使用 pollster 同步等待异步映射操作
    //     pollster::block_on(async {
    //         // 请求映射缓冲区
    //         let buffer_slice = self.camera_buffer.slice(..);
    //         let mapping = buffer_slice.map_async(wgpu::MapMode::Write, ..);

    //         // 等待 GPU 完成当前操作
    //         self.app.device.poll(wgpu::Maintain::Wait);
    //         mapping.await.unwrap();

    //         // 获取映射视图并写入数据
    //         let mut view = buffer_slice.get_mapped_range_mut();
    //         bytemuck::cast_slice_mut(&mut view)[0] = self.camera_uniform;

    //         // 解除映射
    //         drop(view);
    //         self.camera_buffer.unmap();
    //     });
    // }

    // 3. 可以在 queue 上使用 write_buffer 函数。
    //write_buffer 是 WebGPU 提供的简化方法，它抽象了底层的内存传输细节。
    //内部可能使用了临时缓冲区或其他机制，但对开发者隐藏了这些复杂性。
    // 事件只记录是否按下，而实际控制相机则依赖于渲染的update
    // fn update(&mut self, _dt: instant::Duration) {
    //     self.camera_controller.update_camera(&mut self.camera);
    //     self.camera_uniform.update_view_proj(&self.camera);
    //     self.app.queue.write_buffer(
    //         &self.camera_buffer,
    //         0,
    //         bytemuck::cast_slice(&[self.camera_uniform]),
    //     );
    // }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        self.resize_surface_if_needed();

        let (output, view) = self.app.get_current_frame_view(None);
        let mut encoder = self
            .app
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.diffuse_bind_group, &[]);
            // 在 render() 函数中使用绑定组：
            render_pass.set_bind_group(1, &self.camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }

        self.app.queue.submit(Some(encoder.finish()));
        output.present();

        Ok(())
    }
}

pub fn main() -> Result<(), impl std::error::Error> {
    run::<WgpuApp>("tutorial6-uniforms")
}
