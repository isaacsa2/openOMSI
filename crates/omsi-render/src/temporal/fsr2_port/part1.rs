// wgpu backend for the [`sp_fidelity`] port: an implementation of the SDK's
// backend function table, [`FfxInterface`], as AMD's DX12 and Vulkan backends
// (`sdk/src/backends/{dx12,vk}`) implement `FfxInterface` for their APIs.
//
// Resources and pipelines come from the SDK's resource and pipeline
// descriptions and shader blobs: pipeline layouts follow each blob's binding
// tables (registers `tN`, `uN`, `bN`, `sN` at the port's WGSL binding
// offsets), with each binding's type reflected from the port's WGSL for that
// blob. GPU jobs are recorded into the caller's `wgpu::CommandEncoder`.
//
// Where the SDK's DX12 backend takes pointers, this backend takes ownership
// for the duration of a scope:
//
// - `FfxCommandList`: [`FfxWgpuBackend::ffx_get_command_list_wgpu`] moves the
//   caller's encoder into the backend and returns its handle (the analogue of
//   `ffxGetCommandListDX12`); [`FfxWgpuBackend::ffx_take_command_list_wgpu`]
//   returns the encoder with the executed jobs recorded.
// - `FfxResource::resource`: [`FfxWgpuBackend::ffx_get_resource_wgpu`] (the
//   analogue of `ffxGetResourceDX12`) returns a handle to the caller's texture
//   that stays valid until the next `ffx_take_command_list_wgpu`.
//
// Platform workarounds live here and only here: the bounded indirect dispatch
// grid (SDK-P10, `dispatch_grid.rs`), the missing forced-wave64 mode
// (SDK-P6: the reported shader model stays below 6.6, so the SDK never
// selects a wave64 permutation), `R32G32_FLOAT` UAVs allocated as
// `Rgba32Float` (Metal has no read-write `Rg32Float` storage texture) and a
// storage buffer backing a texture that a pipeline binds as a buffer (WGSL
// texture atomics return nothing; see `FfxWgpuBackend::back_buffer_bindings`)
// and a job that fails before recording its new views or bind group when
// wgpu rejects one, stopping the jobs after it (SDK-P28: wgpu would
// invalidate the caller's whole encoder; see
// [`FfxWgpuBackend::take_job_error`]). Nothing else differs from the SDK.
use sp_fidelity::blob_accessors::ffx_get_permutation_blob_by_index;
use sp_fidelity::error::*;
use sp_fidelity::interface::*;
use sp_fidelity::shaders::*;
use sp_fidelity::types::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;
use wgpu::util::DeviceExt;

mod dispatch_grid;
// Upstream readback helper omitted: not used by the runtime backend.

/// The wgpu features every SDK pass needs; request them when creating the
/// device. [`optional_features`] adds the FP16 permutations.
///
/// Derived from the FSR2 WGSL (`fsr2/ffx_fsr2_callbacks.wgsl`) and the
/// resources `ffx_fsr2.cpp` creates:
///
/// - `SUBGROUP`: SPD's wave operations in the luminance pyramid.
/// - `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`: UAVs in formats outside
///   WebGPU's core storage set (`R16G16_FLOAT`, `R16_FLOAT`, `R8_UNORM`,
///   `R8G8_UNORM`, `R11G11B10_FLOAT`) and read-write UAVs (`R8_UNORM` new
///   locks, `R16_FLOAT` exposure mips, the `R32G32_FLOAT` auto exposure
///   allocated as `Rgba32Float`).
/// - `TEXTURE_ATOMIC`: `InterlockedMin`/`InterlockedMax` on the `R32_UINT`
///   reconstructed previous depth.
/// - `TEXTURE_FORMAT_16BIT_NORM`: the `R16_SNORM` Lanczos and maximum-bias
///   lookup textures.
/// - `FLOAT32_FILTERABLE`: linear sampling of 32-bit float application inputs
///   (`SampleInputColor`).
pub fn required_features() -> wgpu::Features {
    wgpu::Features::SUBGROUP
        | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
        | wgpu::Features::TEXTURE_ATOMIC
        | wgpu::Features::TEXTURE_FORMAT_16BIT_NORM
        | wgpu::Features::FLOAT32_FILTERABLE
}

/// Optional features: with `SHADER_F16` the backend reports `fp16Supported`,
/// and the SDK selects its FP16 permutations.
pub fn optional_features() -> wgpu::Features {
    wgpu::Features::SHADER_F16
}

/// A resource bound by a job, as seen by a [`FfxWgpuJobObserver`].
#[derive(Clone, Copy, Debug)]
pub enum FfxWgpuObject<'a> {
    Texture(&'a wgpu::Texture),
    Buffer(&'a wgpu::Buffer),
}

/// One resource of a job.
#[derive(Clone, Copy, Debug)]
pub struct FfxWgpuObservedResource<'a> {
    /// The SDK binding name (`"r_input_color"`), or `"cmdArgument"`, `"src"`,
    /// `"dst"` or `"target"` for indirect arguments, copies and clears.
    pub binding: &'a str,
    /// The name the resource was created or registered with.
    pub name: &'a str,
    /// Its SDK description, whose type and format the object's can differ
    /// from: an `R32G32_FLOAT` UAV is an `Rgba32Float` texture, and a
    /// texture a pipeline binds as a buffer is a buffer of its texels.
    pub description: FfxResourceDescription,
    pub object: FfxWgpuObject<'a>,
    /// The bound mip of a UAV texture; 0 otherwise.
    pub mip: u32,
    /// Whether the job may write the resource.
    pub writable: bool,
}

/// A job about to be recorded (`after == false`) or just recorded.
pub struct FfxWgpuJobObservation<'a> {
    pub device: &'a wgpu::Device,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub job: &'a FfxGpuJobDescription,
    /// The effect and pass of a compute job's pipeline.
    pub pass: Option<(FfxEffect, FfxPass)>,
    pub after: bool,
    pub resources: Vec<FfxWgpuObservedResource<'a>>,
}

/// Called around every GPU job in `execute_gpu_jobs`, for captures and
/// fidelity tests. It may record into the encoder.
pub type FfxWgpuJobObserver = Box<dyn FnMut(&mut FfxWgpuJobObservation<'_>)>;

#[derive(Clone, Debug, PartialEq)]
enum Object {
    Texture(wgpu::Texture),
    Buffer(wgpu::Buffer),
}

impl Object {
    fn observed(&self) -> FfxWgpuObject<'_> {
        match self {
            Self::Texture(texture) => FfxWgpuObject::Texture(texture),
            Self::Buffer(buffer) => FfxWgpuObject::Buffer(buffer),
        }
    }

    /// The mip a UAV binding of `mip` views: a mip the texture lacks is its
    /// last, as AMD's Vulkan backend binds it (`executeGpuJobCompute`,
    /// `sdk/src/backends/vk/ffx_vk.cpp:3769-3771`). FSR2's luminance pyramid
    /// binds mips 4 and 5 of a texture that has them only from a 64-pixel
    /// maximum render size (`ffx_fsr2.cpp:669-678, 998-1004`).
    fn uav_mip(&self, mip: u32) -> u32 {
        match self {
            Self::Texture(texture) => mip.min(texture.mip_level_count().saturating_sub(1)),
            Self::Buffer(_) => mip,
        }
    }
}

/// What an `FfxResource::resource` handle refers to.
#[derive(Clone, Debug)]
struct Handle {
    object: Object,
    description: FfxResourceDescription,
    name: String,
}

struct Resource {
    handle: Handle,
    state: FfxResourceStates,
    effect_context_id: u32,
    /// Registered by `fpRegisterResource`; released by `fpUnregisterResources`.
    dynamic: bool,
    /// The `FfxResource::resource` that `fpGetResource` returns.
    handle_id: usize,
    /// Row pitch of an upload buffer that initializes a texture.
    upload_row_pitch: Option<u32>,
    bytes: u64,
}

struct Pipeline {
    effect: FfxEffect,
    pass: FfxPass,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    /// Layout entries by WGSL binding.
    entries: HashMap<u32, wgpu::BindGroupLayoutEntry>,
    /// Constant buffer sizes by WGSL binding, from the WGSL declaration.
    constant_sizes: HashMap<u32, u64>,
    /// One uniform buffer per constant buffer binding, which each job's
    /// constants are copied into before it runs.
    constants: Vec<(u32, wgpu::Buffer)>,
    samplers: Vec<(u32, wgpu::Sampler)>,
    grid_group: Option<wgpu::BindGroup>,
    /// The groups of the latest jobs with what each binds, most recent
    /// first. FSR2's passes bind one of two resource sets, alternating by
    /// frame (`ffx_fsr2.cpp:1056-1069`), so two are kept.
    groups: RefCell<Vec<(Vec<JobBinding>, wgpu::BindGroup)>>,
}

/// Groups each pipeline keeps (`Pipeline::groups`).
const KEPT_GROUPS: usize = 2;

/// What a compute job binds at one WGSL binding and array index: a texture,
/// whole for an SRV or one mip for a UAV, or a buffer range.
#[derive(Debug, PartialEq)]
struct JobBinding {
    binding: u32,
    array_index: u32,
    object: Object,
    /// The UAV mip of a texture.
    mip: Option<u32>,
    offset: u64,
    size: Option<u64>,
}

/// A job's constant buffer in the upload buffer of `stage_constants`.
struct StagedConstants {
    /// Its WGSL binding.
    binding: u32,
    offset: u64,
    size: u64,
}

#[derive(Default)]
struct EffectContext {
    dynamic: Vec<usize>,
}

/// The wgpu `FfxInterface`. Share it with the SDK components as an
/// [`FfxInterfaceRef`] (see [`ffx_get_interface_wgpu`]).
pub struct FfxWgpuBackend {
    device: wgpu::Device,
    fp16: bool,
    /// By `FfxResourceInternal::internalIndex`; index 0 is the null resource.
    resources: Vec<Option<Resource>>,
    free_dynamic: Vec<usize>,
    handles: HashMap<usize, Handle>,
    next_handle: usize,
    external_handles: Vec<usize>,
    contexts: Vec<Option<EffectContext>>,
    /// By `FfxPipelineState::pipeline - 1`.
    pipelines: Vec<Option<Pipeline>>,
    jobs: Vec<FfxGpuJobDescription>,
    /// By `FfxCommandList - 1`.
    command_lists: Vec<Option<wgpu::CommandEncoder>>,
    grid: dispatch_grid::DispatchGrid,
    clears: HashMap<wgpu::TextureFormat, wgpu::ComputePipeline>,
    observer: Option<FfxWgpuJobObserver>,
    pass_timestamps: Option<FfxWgpuPassTimestamps>,
    /// The first job that stopped `execute_gpu_jobs` since
    /// `take_job_error`, whose result the SDK's dispatch ignores.
    job_error: Option<FfxWgpuJobError>,
}

/// Timestamp query pairs for the compute passes `execute_gpu_jobs` begins, in
/// order from `first`; passes beyond `count` pairs are untimed. The SDK's
/// backends have no pass timing; this lets the application time its dispatch.
pub struct FfxWgpuPassTimestamps {
    pub query_set: wgpu::QuerySet,
    /// Query index of the first pass's beginning.
    pub first: u32,
    /// Pairs available.
    pub count: u32,
    /// Pairs written so far.
    pub used: std::cell::Cell<u32>,
}

/// A GPU job that stopped `execute_gpu_jobs`, which
/// [`FfxWgpuBackend::take_job_error`] reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfxWgpuJobError {
    /// `FFX_ERROR_BACKEND_API_ERROR` where wgpu rejected one of the job's
    /// objects (SDK-P28); otherwise the code of the backend's own refusal,
    /// such as `FFX_ERROR_INVALID_ARGUMENT`.
    pub code: FfxErrorCode,
    /// wgpu's message for the object it rejected, or which job the backend
    /// refused.
    pub message: String,
}

impl std::fmt::Display for FfxWgpuJobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({:#x})", self.message, self.code)
    }
}

impl std::error::Error for FfxWgpuJobError {}

/// Why a job stopped: its code, and wgpu's message where wgpu rejected one
/// of its objects.
struct JobFailure {
    code: FfxErrorCode,
    rejection: Option<String>,
}

impl From<FfxErrorCode> for JobFailure {
    fn from(code: FfxErrorCode) -> Self {
        Self {
            code,
            rejection: None,
        }
    }
}

/// wgpu's validation and out-of-memory scopes around a job's object creation
/// (SDK-P28). wgpu reports an object it rejects when it is created, but the
/// error of recording it only at the encoder's `finish`, where it invalidates
/// the caller's whole encoder, so a job checks its new objects before
/// recording them. The fields drop in order, popping the inner scope first,
/// as wgpu requires.
struct CreationScopes {
    out_of_memory: wgpu::ErrorScopeGuard,
    validation: wgpu::ErrorScopeGuard,
}

impl CreationScopes {
    fn push(device: &wgpu::Device) -> Self {
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let out_of_memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        Self {
            out_of_memory,
            validation,
        }
    }

    /// Fails with `FFX_ERROR_BACKEND_API_ERROR` and wgpu's message if wgpu
    /// rejected an object created inside the scopes. Native wgpu resolves a
    /// pop at once; WebGPU resolves it later, so there this sees nothing and
    /// the error stays deferred to `finish`.
    fn check(self) -> Result<(), JobFailure> {
        let Self {
            out_of_memory,
            validation,
        } = self;
        let out_of_memory = resolved(out_of_memory.pop());
        let validation = resolved(validation.pop());
        match validation.or(out_of_memory) {
            Some(error) => Err(JobFailure {
                code: FFX_ERROR_BACKEND_API_ERROR,
                rejection: Some(error.to_string()),
            }),
            None => Ok(()),
        }
    }
}

/// A popped scope's error, if its future is ready.
fn resolved(future: impl Future<Output = Option<wgpu::Error>>) -> Option<wgpu::Error> {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
    {
        std::task::Poll::Ready(error) => error,
        std::task::Poll::Pending => None,
    }
}

/// `ffxGetInterfaceDX12` for wgpu: a backend for `device`, and the same
/// backend as the components' `backendInterface`. Keep the concrete reference
/// for the command-list and resource functions.
pub fn ffx_get_interface_wgpu(
    device: &wgpu::Device,
) -> (Rc<RefCell<FfxWgpuBackend>>, FfxInterfaceRef) {
    let backend = Rc::new(RefCell::new(FfxWgpuBackend::new(device)));
    let interface: FfxInterfaceRef = backend.clone();
    (backend, interface)
}

impl FfxWgpuBackend {
    /// A backend for `device`, which must have [`required_features`].
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            device: device.clone(),
            fp16: device.features().contains(wgpu::Features::SHADER_F16),
            resources: vec![None],
            free_dynamic: Vec::new(),
            handles: HashMap::new(),
            next_handle: 1,
            external_handles: Vec::new(),
            contexts: Vec::new(),
            pipelines: Vec::new(),
            jobs: Vec::new(),
            command_lists: Vec::new(),
            grid: dispatch_grid::DispatchGrid::new(device),
            clears: HashMap::new(),
            observer: None,
            pass_timestamps: None,
            job_error: None,
        }
    }

    /// Report `fp16Supported` as `enabled` (and the device has `SHADER_F16`),
    /// as a device without 16-bit shader support would. Contexts created
    /// afterwards select FP32 permutations when `false`.
    pub fn set_fp16_supported(&mut self, enabled: bool) {
        self.fp16 = enabled && self.device.features().contains(wgpu::Features::SHADER_F16);
    }

    /// `ffxGetCommandListDX12`: moves `encoder` into the backend and returns
    /// the `FfxCommandList` that dispatch descriptions carry.
    pub fn ffx_get_command_list_wgpu(&mut self, encoder: wgpu::CommandEncoder) -> FfxCommandList {
        if let Some(index) = self.command_lists.iter().position(Option::is_none) {
            self.command_lists[index] = Some(encoder);
            index + 1
        } else {
            self.command_lists.push(Some(encoder));
            self.command_lists.len()
        }
    }

    /// Returns the encoder of `command_list`, with every executed job recorded,
    /// and releases the handles of [`Self::ffx_get_resource_wgpu`].
    pub fn ffx_take_command_list_wgpu(
        &mut self,
        command_list: FfxCommandList,
    ) -> Option<wgpu::CommandEncoder> {
        for handle in self.external_handles.drain(..) {
            self.handles.remove(&handle);
        }
        self.command_lists
            .get_mut(command_list.checked_sub(1)?)
            .and_then(Option::take)
    }

    /// `ffxGetResourceDX12`: an `FfxResource` for a caller texture, described
    /// by [`ffx_get_resource_description_wgpu`]. Valid until the next
    /// [`Self::ffx_take_command_list_wgpu`]. Each pass views the texture as its
    /// binding declares, e.g. as a cube for the environment map. Captures and
    /// copy sources need `COPY_SRC`; the output needs `COPY_DST`.
    pub fn ffx_get_resource_wgpu(
        &mut self,
        texture: &wgpu::Texture,
        name: &str,
        state: FfxResourceStates,
    ) -> FfxResource {
        let description = ffx_get_resource_description_wgpu(texture);
        let handle = self.insert_handle(Handle {
            object: Object::Texture(texture.clone()),
            description,
            name: name.to_owned(),
        });
        self.external_handles.push(handle);
        FfxResource {
            resource: handle,
            description,
            state,
            name: name.to_owned(),
        }
    }

    /// The first job that stopped `execute_gpu_jobs` since the last call, if
    /// any. A failed job records nothing invalid and the jobs after it are not
    /// executed (SDK-P28): a job whose new views or bind group wgpu rejects
    /// fails with `FFX_ERROR_BACKEND_API_ERROR` and wgpu's message before
    /// recording them. The SDK's dispatch (for example
    /// `ffxFsr2ContextDispatch`) ignores `fpExecuteGpuJobs`'s result, so check
    /// this after taking the command list back: the effect's output is
    /// undefined for the frame.
    pub fn take_job_error(&mut self) -> Option<FfxWgpuJobError> {
        self.job_error.take()
    }

    /// Observe every job that `execute_gpu_jobs` records, or stop with `None`.
    pub fn set_job_observer(&mut self, observer: Option<FfxWgpuJobObserver>) {
        self.observer = observer;
    }

    /// Time the compute passes that later dispatches record, or stop with
    /// `None`; returns the previous pool with its `used` count.
    pub fn set_pass_timestamps(
        &mut self,
        timestamps: Option<FfxWgpuPassTimestamps>,
    ) -> Option<FfxWgpuPassTimestamps> {
        std::mem::replace(&mut self.pass_timestamps, timestamps)
    }

    fn timestamp_writes(&self) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        let pool = self.pass_timestamps.as_ref()?;
        let used = pool.used.get();
        (used < pool.count).then(|| {
            pool.used.set(used + 1);
            let index = pool.first + used * 2;
            wgpu::ComputePassTimestampWrites {
                query_set: &pool.query_set,
                beginning_of_pass_write_index: Some(index),
                end_of_pass_write_index: Some(index + 1),
            }
        })
    }

    fn insert_handle(&mut self, handle: Handle) -> usize {
        let id = self.next_handle;
        self.next_handle += 1;
        self.handles.insert(id, handle);
        id
    }

    fn resource(&self, resource: FfxResourceInternal) -> Result<&Resource, FfxErrorCode> {
        usize::try_from(resource.internal_index)
            .ok()
            .and_then(|index| self.resources.get(index))
            .and_then(Option::as_ref)
            .ok_or(FFX_ERROR_INVALID_ARGUMENT)
    }

    fn insert_static(&mut self, resource: Resource) -> FfxResourceInternal {
        self.resources.push(Some(resource));
        FfxResourceInternal {
            internal_index: i32::try_from(self.resources.len() - 1).unwrap(),
        }
    }

    fn texture(&self, resource: FfxResourceInternal) -> Result<&wgpu::Texture, FfxErrorCode> {
        match &self.resource(resource)?.handle.object {
            Object::Texture(texture) => Ok(texture),
            Object::Buffer(_) => Err(FFX_ERROR_INVALID_ARGUMENT),
        }
    }

    fn pipeline(&self, state: &FfxPipelineState) -> Result<&Pipeline, FfxErrorCode> {
        state
            .pipeline
            .checked_sub(1)
            .and_then(|index| self.pipelines.get(index))
            .and_then(Option::as_ref)
            .ok_or(FFX_ERROR_INVALID_ARGUMENT)
    }

    fn clear_pipeline(
        &mut self,
        format: wgpu::TextureFormat,
    ) -> Result<wgpu::ComputePipeline, FfxErrorCode> {
        if let Some(pipeline) = self.clears.get(&format) {
            return Ok(pipeline.clone());
        }
        let storage = storage_format_name(format).ok_or(FFX_ERROR_INVALID_ARGUMENT)?;
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("FFX_GPU_JOB_CLEAR_FLOAT"),
                source: wgpu::ShaderSource::Wgsl(
                    format!(
                        "@group(0) @binding(0) var cleared: texture_storage_2d<{storage}, write>;
@group(0) @binding(1) var<uniform> color: vec4<f32>;
@compute @workgroup_size(8, 8) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
    let size = textureDimensions(cleared);
    if (id.x < size.x && id.y < size.y) {{ textureStore(cleared, id.xy, color); }}
}}"
                    )
                    .into(),
                ),
            });
        let pipeline = self
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("FFX_GPU_JOB_CLEAR_FLOAT"),
                layout: None,
                module: &module,
                entry_point: Some("main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });
        self.clears.insert(format, pipeline.clone());
        Ok(pipeline)
    }

    /// `ClearUnorderedAccessViewFloat` on the target's UAV (mip 0), as a
    /// storage write of `color`.
    fn execute_clear(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        clear: &FfxClearFloatJobDescription,
    ) -> Result<(), JobFailure> {
        if let Object::Buffer(buffer) = &self.resource(clear.target)?.handle.object {
            // A texture backed by a buffer (`back_buffer_bindings`): zero is
            // zero in every format; other colours would need the format's
            // conversion, which no SDK recipe clears such a texture with.
            if clear.color != [0.0; 4] {
                return Err(FFX_ERROR_INVALID_ARGUMENT.into());
            }
            encoder.clear_buffer(buffer, 0, None);
            return Ok(());
        }
        let texture = self.texture(clear.target)?.clone();
        let pipeline = self.clear_pipeline(texture.format())?;
        let scopes = CreationScopes::push(&self.device);
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            mip_level_count: Some(1),
            array_layer_count: Some(1),
            ..Default::default()
        });
        let color = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("FfxClearFloatJobDescription::color"),
                contents: bytemuck::cast_slice(&clear.color),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("FFX_GPU_JOB_CLEAR_FLOAT"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: color.as_entire_binding(),
                },
            ],
        });
        scopes.check()?;
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("FFX_GPU_JOB_CLEAR_FLOAT"),
            timestamp_writes: self.timestamp_writes(),
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(texture.width().div_ceil(8), texture.height().div_ceil(8), 1);
        Ok(())
    }

    fn execute_copy(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        copy: &FfxCopyJobDescription,
    ) -> Result<(), FfxErrorCode> {
        let src = self.resource(copy.src)?;
        let dst = self.resource(copy.dst)?;
        match (&src.handle.object, &dst.handle.object) {
            (Object::Buffer(from), Object::Buffer(to)) => {
                let size = if copy.size == 0 {
                    from.size() - u64::from(copy.src_offset)
                } else {
                    u64::from(copy.size)
                };
                encoder.copy_buffer_to_buffer(
                    from,
                    u64::from(copy.src_offset),
                    to,
                    u64::from(copy.dst_offset),
                    size,
                );
            }
            (Object::Buffer(from), Object::Texture(to)) => {
                let row_pitch = src.upload_row_pitch.ok_or(FFX_ERROR_INVALID_ARGUMENT)?;
                encoder.copy_buffer_to_texture(
                    wgpu::TexelCopyBufferInfo {
                        buffer: from,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: u64::from(copy.src_offset),
                            bytes_per_row: Some(row_pitch),
                            rows_per_image: Some(to.height()),
                        },
                    },
                    to.as_image_copy(),
                    to.size(),
                );
            }
            (Object::Texture(from), Object::Texture(to)) => {
                // The whole source subresource, which may be smaller than the
                // destination (SDK-P8).
                encoder.copy_texture_to_texture(
                    from.as_image_copy(),
                    to.as_image_copy(),
                    wgpu::Extent3d {
                        width: from.width(),
                        height: from.height(),
                        depth_or_array_layers: from.depth_or_array_layers(),
                    },
                );
            }
            (Object::Texture(_), Object::Buffer(_)) => return Err(FFX_ERROR_INVALID_ARGUMENT),
        }
        Ok(())
    }

    /// What `job` binds, in its pipeline's binding order.
    fn job_bindings(
        &self,
        pipeline: &Pipeline,
        job: &FfxComputeJobDescription,
    ) -> Result<Vec<JobBinding>, FfxErrorCode> {
        let state = &job.pipeline;
        let mut bindings = Vec::new();
        let mut bind = |binding: u32, array_index, object, mip, offset, size| {
            bindings.push(JobBinding {
                binding,
                array_index,
                object,
                mip,
                offset,
                size,
            });
        };
        for (binding, srv) in state.srv_texture_bindings.iter().zip(&job.srv_textures) {
            let texture = Object::Texture(self.texture(srv.resource)?.clone());
            let wgsl = FFX_WGSL_BINDING_OFFSET_SRV + binding.slot_index;
            bind(wgsl, binding.array_index, texture, None, 0, None);
        }
        for (binding, uav) in state.uav_texture_bindings.iter().zip(&job.uav_textures) {
            let wgsl = FFX_WGSL_BINDING_OFFSET_UAV + binding.slot_index;
            let entry = pipeline
                .entries
                .get(&wgsl)
                .ok_or(FFX_ERROR_INVALID_ARGUMENT)?;
            let object = self.resource(uav.resource)?.handle.object.clone();
            // A texture the pipeline binds as a buffer was converted by
            // `back_buffer_bindings` before the job.
            let as_buffer = matches!(entry.ty, wgpu::BindingType::Buffer { .. });
            let mip = match (&object, as_buffer) {
                (Object::Buffer(_), true) => None,
                (Object::Texture(_), false) => Some(object.uav_mip(uav.mip)),
                _ => return Err(FFX_ERROR_INVALID_ARGUMENT),
            };
            bind(wgsl, binding.array_index, object, mip, 0, None);
        }
        let srv_buffers = state.srv_buffer_bindings.iter().zip(
            job.srv_buffers
                .iter()
                .map(|b| (b.resource, b.offset, b.size)),
        );
        let uav_buffers = state.uav_buffer_bindings.iter().zip(
            job.uav_buffers
                .iter()
                .map(|b| (b.resource, b.offset, b.size)),
        );
        for (offset, (binding, (resource, start, size))) in srv_buffers
            .map(|b| (FFX_WGSL_BINDING_OFFSET_SRV, b))
            .chain(uav_buffers.map(|b| (FFX_WGSL_BINDING_OFFSET_UAV, b)))
        {
            let object = self.resource(resource)?.handle.object.clone();
            if !matches!(object, Object::Buffer(_)) {
                return Err(FFX_ERROR_INVALID_ARGUMENT);
            }
            bind(
                offset + binding.slot_index,
                binding.array_index,
                object,
                None,
                u64::from(start),
                (size != 0).then_some(u64::from(size)),
            );
        }
        Ok(bindings)
    }

    /// A group of `pipeline` binding `bindings`, its constant buffers and its
    /// samplers.
    fn bind_group(
        &self,
        pipeline: &Pipeline,
        label: &str,
        bindings: &[JobBinding],
    ) -> Result<wgpu::BindGroup, FfxErrorCode> {
        let entry = |binding: u32| {
            pipeline
                .entries
                .get(&binding)
                .ok_or(FFX_ERROR_INVALID_ARGUMENT)
        };
        // Texture views by WGSL binding and array index, and buffers by WGSL
        // binding: SRV/UAV buffer ranges, textures backed by buffers and the
        // constants.
        let mut views: Vec<(u32, Vec<Option<wgpu::TextureView>>)> = Vec::new();
        let mut buffers: Vec<(u32, &wgpu::Buffer, u64, Option<u64>)> = Vec::new();
        for b in bindings {
            let texture = match &b.object {
                Object::Buffer(buffer) => {
                    buffers.push((b.binding, buffer, b.offset, b.size));
                    continue;
                }
                Object::Texture(texture) => texture,
            };
            let view = if let Some(mip) = b.mip {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            } else {
                let wgpu::BindingType::Texture { view_dimension, .. } = entry(b.binding)?.ty else {
                    return Err(FFX_ERROR_INVALID_ARGUMENT);
                };
                texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(view_dimension),
                    aspect: if texture.format().is_depth_stencil_format() {
                        wgpu::TextureAspect::DepthOnly
                    } else {
                        wgpu::TextureAspect::All
                    },
                    ..Default::default()
                })
            };
            let count = entry(b.binding)?.count.map_or(1, NonZeroU32::get) as usize;
            let index = if let Some(index) = views.iter().position(|(w, _)| *w == b.binding) {
                index
            } else {
                views.push((b.binding, vec![None; count]));
                views.len() - 1
            };
            *views[index]
                .1
                .get_mut(b.array_index as usize)
                .ok_or(FFX_ERROR_INVALID_ARGUMENT)? = Some(view);
        }
        let views = views
            .into_iter()
            .map(|(binding, views)| {
                let views: Option<Vec<_>> = views.into_iter().collect();
                views
                    .map(|views| (binding, views))
                    .ok_or(FFX_ERROR_INVALID_ARGUMENT)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let view_refs: Vec<(u32, Vec<&wgpu::TextureView>)> = views
            .iter()
            .map(|(binding, views)| (*binding, views.iter().collect()))
            .collect();
        for (binding, buffer) in &pipeline.constants {
            buffers.push((*binding, buffer, 0, None));
        }

        let mut entries: Vec<wgpu::BindGroupEntry> = Vec::new();
        for (binding, views) in &view_refs {
            entries.push(wgpu::BindGroupEntry {
                binding: *binding,
                resource: if entry(*binding)?.count.is_some() {
                    wgpu::BindingResource::TextureViewArray(views)
                } else {
                    wgpu::BindingResource::TextureView(views[0])
                },
            });
        }
        for (binding, buffer, offset, size) in buffers {
            entries.push(wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer,
                    offset,
                    size: size.and_then(std::num::NonZeroU64::new),
                }),
            });
        }
        for (binding, sampler) in &pipeline.samplers {
            entries.push(wgpu::BindGroupEntry {
                binding: *binding,
                resource: wgpu::BindingResource::Sampler(sampler),
            });
        }
        Ok(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &pipeline.layout,
            entries: &entries,
        }))
    }

    /// Every compute job's constant buffers in one upload buffer, as the
    /// SDK's DX12 backend copies them into its upload ring
    /// (`FallbackConstantAllocator` in `ffx_dx12.cpp`): each zero-padded or
    /// cut to its WGSL struct, which the shader reads no further than. Per
    /// job, each constant buffer's WGSL binding, offset and size; a job whose
    /// pipeline is invalid stages nothing and fails when it executes.
    fn stage_constants(
        &self,
        jobs: &[FfxGpuJobDescription],
    ) -> (Option<wgpu::Buffer>, Vec<Vec<StagedConstants>>) {
        let mut words: Vec<u32> = Vec::new();
        let mut ranges = Vec::with_capacity(jobs.len());
        for job in jobs {
            let mut job_ranges = Vec::new();
            if let FfxGpuJobDescriptor::Compute(compute) = &job.descriptor
                && let Ok(pipeline) = self.pipeline(&compute.pipeline)
            {
                let cbs = compute.pipeline.constant_buffer_bindings.iter();
                for (binding, cb) in cbs.zip(&compute.cbs) {
                    let wgsl = FFX_WGSL_BINDING_OFFSET_CBV + binding.slot_index;
                    let Some(size) = pipeline.constant_sizes.get(&wgsl) else {
                        continue;
                    };
                    let start = words.len();
                    words.extend_from_slice(&cb.data[..cb.num_32bit_entries as usize]);
                    words.resize(start + size.div_ceil(4) as usize, 0);
                    job_ranges.push(StagedConstants {
                        binding: wgsl,
                        offset: start as u64 * 4,
                        size: (words.len() - start) as u64 * 4,
                    });
                }
            }
            ranges.push(job_ranges);
        }
        let upload = (!words.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("FfxConstantBuffer upload"),
                    contents: bytemuck::cast_slice(&words),
                    usage: wgpu::BufferUsages::COPY_SRC,
                })
        });
        (upload, ranges)
    }

    /// Records `job` with the constants `stage_constants` staged for it in
    /// `upload`. Its pipeline keeps the group it binds while later jobs bind
    /// the same resources (`Pipeline::groups`).
    fn execute_compute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        job: &FfxComputeJobDescription,
        upload: Option<&wgpu::Buffer>,
        constants: &[StagedConstants],
    ) -> Result<(), JobFailure> {
        let pipeline = self.pipeline(&job.pipeline)?;
        let state = &job.pipeline;
        let bindings = self.job_bindings(pipeline, job)?;
        let group = {
            let mut groups = pipeline.groups.borrow_mut();
            if let Some(index) = groups.iter().position(|(bound, _)| *bound == bindings) {
                let kept = groups.remove(index);
                groups.insert(0, kept);
            } else {
                let scopes = CreationScopes::push(&self.device);
                let group = self.bind_group(pipeline, &state.name, &bindings)?;
                scopes.check()?;
                groups.insert(0, (bindings, group));
                groups.truncate(KEPT_GROUPS);
            }
            groups[0].1.clone()
        };
        for binding in &state.constant_buffer_bindings {
            let wgsl = FFX_WGSL_BINDING_OFFSET_CBV + binding.slot_index;
            let staged = constants
                .iter()
                .find(|staged| staged.binding == wgsl)
                .ok_or(FFX_ERROR_INVALID_ARGUMENT)?;
            let (_, buffer) = pipeline
                .constants
                .iter()
                .find(|(constant, _)| *constant == wgsl)
                .ok_or(FFX_ERROR_INVALID_ARGUMENT)?;
            let upload = upload.ok_or(FFX_ERROR_INVALID_ARGUMENT)?;
            encoder.copy_buffer_to_buffer(upload, staged.offset, buffer, 0, staged.size);
        }

        if let Some(grid_group) = &pipeline.grid_group {
            let Object::Buffer(arguments) = &self.resource(job.cmd_argument)?.handle.object else {
                return Err(FFX_ERROR_INVALID_ARGUMENT.into());
            };
            self.grid
                .prepare(&self.device, encoder, arguments, job.cmd_argument_offset);
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(&state.name),
                timestamp_writes: self.timestamp_writes(),
            });
            pass.set_pipeline(&pipeline.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.set_bind_group(1, grid_group, &[]);
            pass.dispatch_workgroups_indirect(&self.grid.arguments, 0);
        } else {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(&state.name),
                timestamp_writes: self.timestamp_writes(),
            });
            pass.set_pipeline(&pipeline.pipeline);
            pass.set_bind_group(0, &group, &[]);
            let [x, y, z] = job.dimensions;
            pass.dispatch_workgroups(x, y, z);
        }
        Ok(())
    }

    /// Back each texture that `job`'s pipeline binds as a storage buffer
    /// with a buffer of its texels, from the first such job on.
    ///
    /// FSR2 creates `FSR2_SpdAtomicCounter` as a 1x1 `R32_UINT` UAV texture
    /// (`ffx_fsr2.cpp:702-711`), whose `InterlockedAdd` returns the previous
    /// value (`ffx_fsr2_callbacks_hlsl.h:949`); WGSL texture atomics return
    /// nothing, so the port binds it as a storage buffer. The texture's
    /// contents so far (its initial data, clears) are copied into the buffer,
    /// which then replaces it. Only a single-row texture the effect created
    /// can be converted: its texels are the buffer's layout.
    fn back_buffer_bindings(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        job: &FfxComputeJobDescription,
    ) -> Result<(), FfxErrorCode> {
        let pipeline = self.pipeline(&job.pipeline)?;
        let resources: Vec<FfxResourceInternal> = job
            .pipeline
            .uav_texture_bindings
            .iter()
            .zip(&job.uav_textures)
            .filter(|(binding, _)| {
                pipeline
                    .entries
                    .get(&(FFX_WGSL_BINDING_OFFSET_UAV + binding.slot_index))
                    .is_some_and(|e| matches!(e.ty, wgpu::BindingType::Buffer { .. }))
            })
            .map(|(_, uav)| uav.resource)
            .collect();
        for resource in resources {
            let device = self.device.clone();
            let r = usize::try_from(resource.internal_index)
                .ok()
                .and_then(|index| self.resources.get_mut(index))
                .and_then(Option::as_mut)
                .ok_or(FFX_ERROR_INVALID_ARGUMENT)?;
            let Object::Texture(texture) = &r.handle.object else {
                continue;
            };
            if r.dynamic
                || texture.height() != 1
                || texture.mip_level_count() != 1
                || texture.depth_or_array_layers() != 1
            {
                return Err(FFX_ERROR_INVALID_ARGUMENT);
            }
            let block = texture
                .format()
                .block_copy_size(None)
                .ok_or(FFX_ERROR_INVALID_ARGUMENT)?;
            let size = u64::from(texture.width() * block).next_multiple_of(4);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&r.handle.name),
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout::default(),
                },
                texture.size(),
            );
            r.handle.object = Object::Buffer(buffer);
            r.bytes = size;
            let (handle_id, object) = (r.handle_id, r.handle.object.clone());
            if let Some(handle) = self.handles.get_mut(&handle_id) {
                handle.object = object;
            }
        }
        Ok(())
    }

    fn observed_resources<'a>(
        &'a self,
        job: &'a FfxGpuJobDescription,
    ) -> Vec<FfxWgpuObservedResource<'a>> {
        let mut out = Vec::new();
        let mut add = |binding: &'a str, resource: FfxResourceInternal, mip: u32, writable| {
            if let Ok(r) = self.resource(resource) {
                out.push(FfxWgpuObservedResource {
                    binding,
                    name: &r.handle.name,
                    description: r.handle.description,
                    object: r.handle.object.observed(),
                    mip: r.handle.object.uav_mip(mip),
                    writable,
                });
            }
        };
        match &job.descriptor {
            FfxGpuJobDescriptor::ClearFloat(clear) => add("target", clear.target, 0, true),
            FfxGpuJobDescriptor::Copy(copy) => {
                add("src", copy.src, 0, false);
                add("dst", copy.dst, 0, true);
            }
            FfxGpuJobDescriptor::Compute(compute) => {
                let state = &compute.pipeline;
                for (b, r) in state.srv_texture_bindings.iter().zip(&compute.srv_textures) {
                    add(&b.name, r.resource, 0, false);
                }
                for (b, r) in state.uav_texture_bindings.iter().zip(&compute.uav_textures) {
                    add(&b.name, r.resource, r.mip, true);
                }
                for (b, r) in state.srv_buffer_bindings.iter().zip(&compute.srv_buffers) {
                    add(&b.name, r.resource, 0, false);
                }
                for (b, r) in state.uav_buffer_bindings.iter().zip(&compute.uav_buffers) {
                    add(&b.name, r.resource, 0, true);
                }
                if state.cmd_signature != 0 {
                    add("cmdArgument", compute.cmd_argument, 0, false);
                }
            }
        }
        out
    }

    fn observe(
        &self,
        observer: &mut Option<FfxWgpuJobObserver>,
        encoder: &mut wgpu::CommandEncoder,
        job: &FfxGpuJobDescription,
        after: bool,
    ) {
        let Some(observer) = observer else {
            return;
        };
        let pass = match &job.descriptor {
            FfxGpuJobDescriptor::Compute(compute) => self
                .pipeline(&compute.pipeline)
                .ok()
                .map(|p| (p.effect, p.pass)),
            _ => None,
        };
        observer(&mut FfxWgpuJobObservation {
            device: &self.device,
            encoder,
            job,
            pass,
            after,
            resources: self.observed_resources(job),
        });
    }
}
