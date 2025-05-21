// 顶点着色器

struct Camera {
    view_pos: vec4f,
    view_proj: mat4x4f,
}
@group(1) @binding(0)
var<uniform> camera: Camera;

struct Light {
    position: vec3f,
    color: vec3f,
}
@group(2) @binding(0)
var<uniform> light: Light;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) tex_coords: vec2f,
    @location(2) normal: vec3f,
    @location(3) tangent: vec3f,
    @location(4) bitangent: vec3f,
}
struct InstanceInput {
    @location(5) model_matrix_0: vec4f,
    @location(6) model_matrix_1: vec4f,
    @location(7) model_matrix_2: vec4f,
    @location(8) model_matrix_3: vec4f,
    @location(9) normal_matrix_0: vec3f,
    @location(10) normal_matrix_1: vec3f,
    @location(11) normal_matrix_2: vec3f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) tex_coords: vec2f,
    @location(1) tangent_position: vec3f,
    @location(2) tangent_light_position: vec3f,
    @location(3) tangent_view_position: vec3f,
}

@vertex
fn vs_main(
    model: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    let model_matrix = mat4x4f(
        instance.model_matrix_0,
        instance.model_matrix_1,
        instance.model_matrix_2,
        instance.model_matrix_3,
    );
    let normal_matrix = mat3x3f(
        instance.normal_matrix_0,
        instance.normal_matrix_1,
        instance.normal_matrix_2,
    );

    // 构建切向量矩阵
    let world_normal = normalize(normal_matrix * model.normal);
    let world_tangent = normalize(normal_matrix * model.tangent);
    let world_bitangent = normalize(normal_matrix * model.bitangent);
    let tangent_matrix = transpose(mat3x3f(
        world_tangent,
        world_bitangent,
        world_normal,
    ));

    let world_position = model_matrix * vec4f(model.position, 1.0);

    var out: VertexOutput;
    out.clip_position = camera.view_proj * world_position;
    out.tex_coords = model.tex_coords;
    
    // 将顶点，光源和视图坐标变换到切空间
    out.tangent_position = tangent_matrix * world_position.xyz;
    out.tangent_view_position = tangent_matrix * camera.view_pos.xyz;
    out.tangent_light_position = tangent_matrix * light.position;
    return out;
}

// 片元着色器

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0)@binding(1)
var s_diffuse: sampler;
@group(0)@binding(2)
var t_normal: texture_2d<f32>;
@group(0) @binding(3)
var s_normal: sampler;

// 这段代码之所以选 切线空间 (Tangent Space)，是为了 直接使用法线贴图里的 RGB 向量，
// 避免把它再变换到世界/视图空间，从而节省每片元一次 3×3 矩阵乘法的成本
// 注意：没有任何矩阵乘法——采样到的 tangent_normal 直接能和 light_dir / view_dir 做点积，因为三者同处切线空间
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    //1. 取纹理
    let object_color: vec4f = textureSample(t_diffuse, s_diffuse, in.tex_coords);
    // 在上一张用的是顶点法线 @location(1) world_normal: vec3f,
    let object_normal: vec4f = textureSample(t_normal, s_normal, in.tex_coords);
    
    // We don't need (or want) much ambient light, so 0.1 is fine
    let ambient_strength = 0.1;
    let ambient_color = light.color * ambient_strength;

    //2. 把法线贴图值映射到 [-1,1]
    // 光照计算需要的向量
    let tangent_normal = object_normal.xyz * 2.0 - 1.0;

    //3. 构造光照向量（都在切线空间）
    let light_dir = normalize(in.tangent_light_position - in.tangent_position);
    let view_dir = normalize(in.tangent_view_position - in.tangent_position);
    let half_dir = normalize(view_dir + light_dir);

    //4. Blinn-Phong（但可换成 PBR）
    let diffuse_strength = max(dot(tangent_normal, light_dir), 0.0);
    let diffuse_color = light.color * diffuse_strength;

    let specular_strength = pow(max(dot(tangent_normal, half_dir), 0.0), 32.0);
    let specular_color = specular_strength * light.color;

    //5. 最终颜色
    let result = (ambient_color + diffuse_color + specular_color) * object_color.xyz;

    return vec4f(result, object_color.a);
}