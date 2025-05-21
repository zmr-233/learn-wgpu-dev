use app_surface::{AppSurface, SurfaceFrame};
use std::sync::Arc;
use utils::framework::{WgpuAppAction, run};
use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

// 顶点缓冲区布局（VertexBufferLayout）对象
// 定义了缓冲区在内存中的表示方式，render_pipeline 需要它来在着色器中映射缓冲区
impl Vertex {
    // 但 Rust 认为 vertex_attr_array 的结果是一个临时值，所以需要进行调整才能从一个函数中返回
    // 我们可以将wgpu::VertexBufferLayout 的生命周期改为 'static，或者使其成为 const
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            //1. array_stride 定义了一个顶点所占的字节数
            // 下一个顶点时，它将跳过 array_stride 的字节数
            array_stride: core::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            //2. 每个元素代表的是每个顶点还是每个实例的数据
            step_mode: wgpu::VertexStepMode::Vertex,
            //3. 描述顶点的各个属性（Attribute）的布局
            // 一般来说，这与结构体的字段是 1:1 映射的
            attributes: &Self::ATTRIBS,
            // 可以使用 wgpu 提供的 vertex_attr_array 宏来清理一下:
            // &[
            //     wgpu::VertexAttribute {
            //         offset: 0,
            //         shader_location: 0,
            //         format: wgpu::VertexFormat::Float32x3,
            //     },
            //     wgpu::VertexAttribute {
            //         //3.1 定义了属性在一个顶点元素中的字节偏移量
            //         offset: core::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
            //         //3.2 告诉着色器要在什么位置存储这个属性
            //         // @location(1) x: vec3f 对应 color 字段
            //         shader_location: 1,
            //         //3.3 告诉着色器该属性的数据格式
            //         // Float32x3对应于着色器代码中的 vec3f
            //         format: wgpu::VertexFormat::Float32x3,
            //     },
            // ],
        }
    }
}

const VERTICES: &[Vertex] = &[
    Vertex {
        position: [0.0, 0.5, 0.0],
        color: [1.0, 0.0, 0.0],
    },
    Vertex {
        position: [-0.5, -0.5, 0.0],
        color: [0.0, 1.0, 0.0],
    },
    Vertex {
        position: [0.5, -0.5, 0.0],
        color: [0.0, 0.0, 1.0],
    },
];

const INDICES: &[u16] = &[0, 1, 2];

struct WgpuApp {
    app: AppSurface,
    render_pipeline: wgpu::RenderPipeline,
    size: PhysicalSize<u32>,
    size_changed: bool,
    // NEW!
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
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
        let app = AppSurface::new(window).await;

        let shader = app
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
            });

        let render_pipeline_layout =
            app.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Render Pipeline Layout"),
                    bind_group_layouts: &[],
                    push_constant_ranges: &[],
                });

        let render_pipeline = app
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Render Pipeline"),
                layout: Some(&render_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    // 在创建 render_pipeline 时使用它了：
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
                //使用了 bytemuck 来将 VERTICES 转换为 &[u8]
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

        // 需要修改 render_pass.draw() 的调用来使用 VERTICES 所指定的顶点数量:
        let num_indices = INDICES.len() as u32;

        let size = PhysicalSize {
            width: app.config.width,
            height: app.config.height,
        };

        Self {
            app,
            size,
            size_changed: false,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            num_indices,
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
            //还需要在渲染函数中实际设置顶点缓冲区，否则程序会崩溃:
            //a. 第一个参数是顶点缓冲区要使用的缓冲槽索引。你可以连续设置多个顶点缓冲区
            //b. 第二个参数是要使用的缓冲区的数据片断
            // 可以在硬件允许的情况下在一个缓冲区中存储尽可能多的对象
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            // 命令名称是 set_index_buffer 而不是 set_index_buffers, 一次绘制（draw_XXX()）只能设置一个索引缓冲区。
            // 但是，你可以在一个渲染通道内调用多次绘制，每次都设置不同的索引缓冲区。
            // ^^^^^^^^^^^^^ 意思是说，可以用set_index_buffer & draw_indexed 多次调用来实现 "多索引缓冲区"
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            // 当使用索引缓冲区时，需使用 draw_indexed 来绘制，draw 命令会忽略索引缓冲区：
            // 保你使用的是索引数（num_indices）而非顶点数，否则你的模型要么画错，要么因为没有足够的索引数而导致程序恐慌（panic）
            render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }

        self.app.queue.submit(Some(encoder.finish()));
        output.present();

        Ok(())
    }
}

pub fn main() -> Result<(), impl std::error::Error> {
    run::<WgpuApp>("tutorial4-buffer")
}

/// Additional mutation methods for `Option`.
pub trait OptionMutExt<T> {
    /// Replace the existing `Some` value with a new one.
    ///
    /// Returns the previous value if it was present, or `None` if no replacement was made.
    fn replace_sp(&mut self, val: T) -> Option<T>;

    /// Replace the existing `Some` value with the result of given closure.
    ///
    /// Returns the previous value if it was present, or `None` if no replacement was made.
    fn replace_with_sp<F: FnOnce() -> T>(&mut self, f: F) -> Option<T>;
}
impl<T> OptionMutExt<T> for Option<T> {
    fn replace_sp(&mut self, val: T) -> Option<T> {
        std::mem::replace(self, Some(val))
    }

    fn replace_with_sp<F: FnOnce() -> T>(&mut self, f: F) -> Option<T> {
        std::mem::replace(self, Some(f()))
    }
}

#[cfg(test)]
mod tests {
    use super::OptionMutExt;
    #[test]
    fn test_option_replace() {
        let mut opt = Some(42);
        let old_value = opt.replace_sp(100);
        assert_eq!(old_value, Some(42));
        assert_eq!(opt, Some(100));

        let mut empty_opt: Option<i32> = None;
        let old_value = empty_opt.replace_sp(200);
        assert_eq!(old_value, None);
        assert_eq!(empty_opt, Some(200));
    }

    #[test]
    fn test_option_replace_with() {
        let mut opt = Some(42);
        let old_value = opt.replace_with_sp(|| 100);
        assert_eq!(old_value, Some(42));
        assert_eq!(opt, Some(100));

        let mut empty_opt: Option<i32> = None;
        let old_value = empty_opt.replace_with_sp(|| 200);
        assert_eq!(old_value, None);
        assert_eq!(empty_opt, Some(200));
    }
}
