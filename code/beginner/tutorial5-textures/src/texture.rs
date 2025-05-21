use anyhow::*;
use image::GenericImageView;

#[allow(dead_code)]
pub struct Texture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl Texture {
    pub fn from_bytes(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bytes: &[u8],
        label: &str,
    ) -> Result<Self> {
        let img = image::load_from_memory(bytes)?;
        Self::from_image(device, queue, &img, Some(label))
    }

    pub fn from_image(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        img: &image::DynamicImage,
        label: Option<&str>,
    ) -> Result<Self> {
        //1. 图像数据准备
        // 注意: 使用的是 to_rgba8() 而不是 as_rgba8()
        // PNG 使用 as_rgba8() 没问题，因为它们有一个 alpha 通道
        // 但是 JPEG 没有 alpha 通道，如果我们试图在 JPEG 纹理图像上调用 as_rgba8()，代码就会陷入恐慌
        // 相反，我们可以使用 to_rgba8() 来处理没有 alpha 通道的图像，它会生成一个新的图像缓冲区
        let rgba: image::ImageBuffer<image::Rgba<u8>, Vec<u8>> = img.to_rgba8();
        let dimensions = img.dimensions();

        //2. 纹理尺寸描述
        // WebGPU使用统一的3D表示法来描述所有纹理
        // 使是2D图像，也被视为"深度为1"的3D纹理
        let size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };

        //3. 创建GPU纹理对象
        // 这一步在GPU内存中实际分配空间，但尚未填充数据
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label,
            size,
            // 禁用mipmap（多级分辨率纹理），节省内存但可能影响远处渲染质量
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // 大多数图像都是使用 sRGB 来存储的，我们需要在这里指定。
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            //定义纹理用途，影响内存布局和访问模式
            // TEXTURE_BINDING 表示我们要在着色器中使用这个纹理。
            // COPY_DST 表示我们能将数据复制到这个纹理上。
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        //4. 上传图像数据
        // 命令队列是向GPU异步提交操作的管道
        // write_texture提供从CPU内存到GPU纹理的直接传输路径
        queue.write_texture(
            // 告诉 wgpu 将像素数据复制到何处
            wgpu::TexelCopyTextureInfo {
                aspect: wgpu::TextureAspect::All,
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            // 实际像素数据
            &rgba,
            // 纹理的内存布局
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                // bytes_per_row: Some(4 * dimensions.0): 指定内存布局，每像素4字节(RGBA)
                bytes_per_row: Some(4 * dimensions.0),
                // 值得注意的是 bytes_per_row 字段，这个值需要是 256 的倍数
                rows_per_image: Some(dimensions.1),
            },
            size,
        );
        // 填充纹理数据的经典方式是将像素数据先复制到一个缓冲区，然后再从缓冲区复制到纹理中。使用 write_texture 更有效率，因为它少用了一个缓冲区
        // {
        //     let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        //         label: Some("Temp Buffer"),
        //         contents: &diffuse_rgba,
        //         usage: wgpu::BufferUsages::COPY_SRC,
        //     });

        //     let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        //         label: Some("texture_buffer_copy_encoder"),
        //     });

        //     encoder.copy_buffer_to_texture(
        //         wgpu::TexelCopyBufferInfo {
        //             buffer: &buffer,
        //             offset: 0,
        //             bytes_per_row: 4 * dimensions.0,
        //             rows_per_image: dimensions.1,
        //         },
        //         wgpu::TexelCopyTextureInfo {
        //             texture: &diffuse_texture,
        //             mip_level: 0,
        //             array_layer: 0,
        //             origin: wgpu::Origin3d::ZERO,
        //         },
        //         size,
        //     );

        //     queue.submit(Some(encoder.finish()));
        // }

        //5. 创建纹理视图
        // 纹理视图是着色器访问纹理的媒介 改变纹理的解释方式(如格式、维度、mipmap范围等)
        // 同一纹理可以创建多个不同视图，实现高效资源复用
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        //6. 创建采样器
        // 采样器包含纹理采样时的所有规则，完全独立于纹理本身

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            //6.1 address_mode: 处理超出[0,1]范围的纹理坐标
            // ClampToEdge：任何在纹理外的纹理坐标将返回离纹理边缘最近的像素的颜色
            // Repeat 当纹理坐标超过纹理的尺寸时，纹理将重复
            // MirrorRepeat 类似于Repeat，但图像在越过边界时将翻转。
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,

            //6.2 当采样足迹小于或大于一个纹素（Texel）时该如何处理
            //Linear：在每个维度中选择两个纹素，并在它们的值之间返回线性插值。
            //Nearest：返回离纹理坐标最近的纹素的值 纹理被设计成像素化的应该使用
            //mag_filter: Linear: 放大时混合相邻像素，创造平滑过渡
            mag_filter: wgpu::FilterMode::Linear,
            //min_filter: Nearest: 缩小时使用最近像素，保持清晰边缘
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Ok(Self {
            texture,
            view,
            sampler,
        })
    }
}
