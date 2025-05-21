// 顶点着色器

struct Camera {
    // 因为镜面反射是相对于视角而言的，所以我们需要将摄像机的位置传入顶点及片元着色器中：
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
}
struct InstanceInput {
    @location(5) model_matrix_0: vec4f,
    @location(6) model_matrix_1: vec4f,
    @location(7) model_matrix_2: vec4f,
    @location(8) model_matrix_3: vec4f,
    // 我们只需要用到矩阵的旋转分量，故法线矩阵的类型是 Matrix3 而不是 Matrix4。
    @location(9) normal_matrix_0: vec3f,
    @location(10) normal_matrix_1: vec3f,
    @location(11) normal_matrix_2: vec3f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) tex_coords: vec2f,
    @location(1) world_normal: vec3f,
    @location(2) world_position: vec3f,
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
    var out: VertexOutput;
    out.tex_coords = model.tex_coords;
    // 这是由于法向量并没有随对象一起旋转，因此无论对象转向哪个方向，法向量的方向始终没变:
    //  虽然可以在顶点着色器中计算法线矩阵，但这涉及到反转模型矩阵 model_matrix，而 WGSL 实际上没有矩阵求逆的函数
    //  替代方案是，向 InstanceRaw 结构体添加一个 normal 字段。不用去反转模型矩阵，
    //  而是使用模型实例的旋转来创建一个 Matrix3 类型的法线矩阵
    out.world_normal = normal_matrix * model.normal;
    var world_position: vec4f = model_matrix * vec4f(model.position, 1.0);
    out.world_position = world_position.xyz;
    out.clip_position = camera.view_proj * world_position;
    return out;
}

// 片元着色器

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0)@binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let object_color: vec4f = textureSample(t_diffuse, s_diffuse, in.tex_coords);
    
    // 我们不需要太强的环境光，强度设置为 0.1 就够了
    let ambient_strength = 0.1;
    let ambient_color = light.color * ambient_strength;

    let light_dir = normalize(light.position - in.world_position);
    let view_dir = normalize(camera.view_pos.xyz - in.world_position);
    // https://learnopengl.com/Advanced-Lighting/Advanced-Lighting
    let half_dir = normalize(view_dir + light_dir);

    let diffuse_strength = max(dot(in.world_normal, light_dir), 0.0);
    let diffuse_color = light.color * diffuse_strength;

    let specular_strength = pow(max(dot(in.world_normal, half_dir), 0.0), 32.0);
    let specular_color = specular_strength * light.color;

    let result = (ambient_color + diffuse_color + specular_color) * object_color.xyz;

    return vec4f(result, object_color.a);
}