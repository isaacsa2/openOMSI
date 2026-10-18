$input a_position, a_normal, a_texcoord0
$output v_color0

#include <bgfx_shader.sh>

void main()
{
    gl_Position = mul(u_modelViewProj, vec4(a_position, 1.0));
    vec3 n = normalize(a_normal);
    v_color0 = vec4(n * 0.5 + 0.5, 1.0);
}
