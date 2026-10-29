//! SDK-P10: wgpu turns an indirect dispatch above
//! `max_compute_workgroups_per_dimension` (65,535) groups into no work. The SDK
//! dispatches its indirect passes as one-dimensional grids that can exceed it
//! (79,136 denoiser groups at 3024×1898). A conversion pass reads the SDK's
//! arguments and writes a bounded 2D grid; the pass entry point is wrapped to
//! rebuild the SDK's logical group ID from the physical one. Surplus groups
//! return uniformly before entering any SDK workgroup barrier. The SDK's
//! arguments, host jobs and pass WGSL are unchanged.
use wgpu::util::DeviceExt;

pub(crate) struct DispatchGrid {
    pipeline: wgpu::ComputePipeline,
    pub layout: wgpu::BindGroupLayout,
    pub arguments: wgpu::Buffer,
    limit: u32,
}
impl DispatchGrid {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("AMD logical dispatch to wgpu grid"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
@group(0) @binding(0) var<storage,read> original:array<u32>;
@group(0) @binding(1) var<storage,read_write> grid:array<u32,8>;
@group(0) @binding(2) var<uniform> parameters:vec4<u32>;
@compute @workgroup_size(1) fn main() {
 let count=original[parameters.x];
 let width=min(count,parameters.y);
 grid[0]=width;grid[1]=(count+max(width,1u)-1u)/max(width,1u);grid[2]=1u;
 grid[3]=count;grid[4]=width;grid[5]=0u;grid[6]=0u;grid[7]=0u;
}
"#
                .into(),
            ),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("AMD logical dispatch to wgpu grid"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("AMD logical dispatch dimensions"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let arguments = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("AMD bounded dispatch arguments"),
            size: 32,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::UNIFORM
                | wgpu::BufferUsages::INDIRECT,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            layout,
            arguments,
            limit: device.limits().max_compute_workgroups_per_dimension,
        }
    }
    pub fn prepare(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        original: &wgpu::Buffer,
        offset: u32,
    ) {
        let parameters = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("AMD original dispatch offset and grid limit"),
            contents: bytemuck::cast_slice(&[offset / 4, self.limit, 0, 0]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("AMD dispatch conversion inputs"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: original.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.arguments.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: parameters.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
}

pub(crate) fn wgsl_entry(source: &str) -> String {
    // The entry point's workgroup_id parameter becomes the physical group; the
    // original name is rebound to the logical group ID inside the entry body.
    let marker = "@builtin(workgroup_id) ";
    let name_start = source.find(marker).expect("retained shader workgroup ID") + marker.len();
    let name_end = name_start + source[name_start..].find(':').expect("typed parameter");
    let name = source[name_start..name_end].trim();
    let source = format!(
        "{}sdk_physical_group{}",
        &source[..name_start],
        &source[name_end..]
    );
    let body = name_start
        + source[name_start..]
            .find('{')
            .expect("retained shader entry body")
        + 1;
    format!(
        "{}\nlet sdk_group=sdk_physical_group.x+sdk_physical_group.y*sdk_dispatch.grid.x;\nif sdk_group>=sdk_dispatch.args.w {{return;}}\nlet {name}=vec3<u32>(sdk_group,0u,0u);\n{}\nstruct SdkDispatchGrid {{args:vec4<u32>,grid:vec4<u32>}}\n@group(1) @binding(0) var<uniform> sdk_dispatch:SdkDispatchGrid;",
        &source[..body],
        &source[body..]
    )
}

// The upstream GPU stress test uses test-only helpers from sp-fidelity-wgpu,
// which are not part of the vendored runtime adapter. Keep portable unit
// coverage here; indirect GPU dispatch gets its own integration test once
// a compatible software or physical adapter is provisioned in CI.
#[cfg(test)]
mod tests {
    use super::wgsl_entry;

    #[test]
    fn rewritten_shader_restores_logical_workgroup_id() {
        let shader = "@compute @workgroup_size(8, 8, 1) fn CS(@builtin(workgroup_id) WorkGroupId: vec3<u32>) { let x = WorkGroupId.x; }";
        let adapted = wgsl_entry(shader);
        assert!(adapted.contains("sdk_physical_group"));
        assert!(adapted.contains("let WorkGroupId=vec3<u32>(sdk_group,0u,0u);"));
        assert!(adapted.contains("if sdk_group>=sdk_dispatch.args.w {return;}"));
        assert!(adapted.contains("let x = WorkGroupId.x;"));
        assert!(adapted.contains("struct SdkDispatchGrid"));
    }

    #[test]
    fn rewrite_preserves_original_entry_point() {
        let shader = "@compute @workgroup_size(1) fn CS(@builtin(workgroup_id) id: vec3<u32>) { let idx = id.x; }";
        let adapted = wgsl_entry(shader);
        assert!(adapted.contains("fn CS("));
        assert!(adapted.contains("let id=vec3<u32>(sdk_group,0u,0u);"));
    }
}
