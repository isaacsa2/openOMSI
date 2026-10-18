$input v_color0, v_texcoord0

#include <bgfx_shader.sh>
SAMPLER2D(s_texColor, 0);

void main()
{
    vec4 texel = texture2D(s_texColor, v_texcoord0);
    gl_FragColor = texel * vec4(v_color0.rgb * 0.65 + vec3(0.35), v_color0.a);
}
