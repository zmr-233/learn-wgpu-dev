use app_surface::{AppSurface, SurfaceFrame};
use std::sync::Arc;
use utils::framework::{WgpuAppAction, run};
use winit::dpi::PhysicalSize;

struct WgpuApp {
    app: AppSurface,
    size: PhysicalSize<u32>,
    size_changed: bool,
    // NEW!
    render_pipeline: wgpu::RenderPipeline,
}

impl WgpuApp {
    /// 必要的时候调整 surface 大小
    fn resize_surface_if_needed(&mut self) {
        if self.size_changed {
            self.app
                .resize_surface_by_size((self.size.width, self.size.height));
            self.size_changed = false;
        }
    }
}

impl WgpuAppAction for WgpuApp {
    async fn new(window: Arc<winit::window::Window>) -> Self {
        // 创建 wgpu 应用
        let app: AppSurface = AppSurface::new(window).await;

        // 创建着色器
        let shader: wgpu::ShaderModule =
            app.device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("Shader"),
                    source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
                });

        // 创建渲染管线布局
        let render_pipeline_layout: wgpu::PipelineLayout =
            app.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Render Pipeline Layout"),
                    bind_group_layouts: &[],
                    push_constant_ranges: &[],
                });

        // 创建渲染管线
        let render_pipeline: wgpu::RenderPipeline =
            app.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Render Pipeline"),
                    layout: Some(&render_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        //1. 指定着色器中的哪个函数应该是入口点
                        entry_point: Some("vs_main"),
                        compilation_options: Default::default(),
                        //2. buffers 字段告诉 wgpu 要把什么类型的顶点数据传递给顶点着色器
                        buffers: &[],
                    },
                    //3. 如果想把颜色数据存储到 surface 就需要用到它
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        compilation_options: Default::default(),
                        //4. 告诉 wgpu 应该设置哪些颜色输出目标
                        // 指定为使用 surface 的格式，并且指定混合模式为仅用新的像素数据替换旧的
                        targets: &[Some(wgpu::ColorTargetState {
                            format: app.config.format.add_srgb_suffix(),
                            blend: Some(wgpu::BlendState {
                                color: wgpu::BlendComponent::REPLACE,
                                alpha: wgpu::BlendComponent::REPLACE,
                            }),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    //5. 图元（primitive）字段描述了将如何解释顶点来转换为三角形。
                    primitive: wgpu::PrimitiveState {
                        //6. PrimitiveTopology::TriangleList 意味着每三个顶点组成一个三角形
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        //7. 确定三角形的朝向
                        // 指定顶点的帧缓冲区坐标（framebuffer coordinates）按逆时针顺序给出的三角形为朝前
                        front_face: wgpu::FrontFace::Ccw,
                        //8. cull_mode 字段告诉 wgpu 如何做三角形剔除
                        // CullMode::Back 指定朝后（面向屏幕内）的三角形会被剔除（不被渲染）
                        cull_mode: Some(wgpu::Face::Back),
                        polygon_mode: wgpu::PolygonMode::Fill,
                        // Requires Features::DEPTH_CLIP_CONTROL
                        unclipped_depth: false,
                        // Requires Features::CONSERVATIVE_RASTERIZATION
                        conservative: false,
                    },
                    //9. 使用深度/模板缓冲区
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState {
                        //10. count 确定管线将使用多少个采样
                        count: 1,
                        //11. mask 指定哪些采样应处于活动状态。目前我们使用全部采样
                        mask: !0,
                        //12. 与抗锯齿有关
                        alpha_to_coverage_enabled: false,
                    },
                    // If the pipeline will be used with a multiview render pass, this
                    // indicates how many array layers the attachments will have.
                    //13. multiview 表示渲染附件可以有多少数组层。我们不会渲染到数组纹理
                    multiview: None,
                    cache: None,
                });

        let size = PhysicalSize {
            width: app.config.width,
            height: app.config.height,
        };

        Self {
            app,
            size,
            size_changed: false,
            render_pipeline,
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
            //1. 把 _render_pass 声明为可变变量并重命名为 render_pass
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
            //2. 在 render_pass 上设置刚刚创建的管线
            render_pass.set_pipeline(&self.render_pipeline);
            //3. 告诉 wgpu 用 3 个顶点和 1 个实例（实例的索引就是 @builtin(vertex_index) 的由来）
            render_pass.draw(0..3, 0..1);
        }

        self.app.queue.submit(Some(encoder.finish()));
        output.present();

        Ok(())
    }
}

pub fn main() -> Result<(), impl std::error::Error> {
    run::<WgpuApp>("tutorial3-pipeline")
}
